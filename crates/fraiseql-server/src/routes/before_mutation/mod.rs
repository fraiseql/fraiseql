//! `before:mutation` chain dispatch (#1327, #1328).
//!
//! The bridge between the compiled schema's `before:mutation` function chain and
//! the engine's enforcement point. [`FunctionChainGate`] implements
//! [`BeforeMutationGate`], so the engine consults the chain from
//! `execute_mutation_impl` — the single place every mutation entry path converges
//! on — rather than from one HTTP handler.
//!
//! That move is the fix for #1327. The chain used to run in the GraphQL handler,
//! once per request, keyed on `parse_query(…).root_field` and handed
//! `request.variables`, which left three ways to execute a mutation without
//! running its chain: a second root field (only the first one's chain ran), inline
//! arguments (invisible to the chain, and a rewrite reached only variables), and
//! the REST write route (which dispatched `after:mutation` only). At the
//! chokepoint there is nothing left to route around: the chain runs per executed
//! root, in document order, with the arguments the write will bind from.
//!
//! The gate is installed on the executor's
//! [`RuntimeConfig`](fraiseql_core::runtime::RuntimeConfig) at serve time, once
//! the function modules are loaded — see `Server::prepare_functions_runtime`.
//!
//! # What the chain may do (#1328)
//!
//! It runs on a `fraiseql_functions::host::before_mutation::BeforeMutationHost` —
//! named in prose, not linked, because it lives behind
//! `fraiseql-functions/host-live` and an intra-doc link to an item this build
//! compiles out is a `-D warnings` rustdoc failure:
//! a read-only `fraiseql_query` bridge executed **as the requesting principal**,
//! plus logging and the caller's auth context. Every side-effecting op refuses by
//! name. And it runs inside a [`BeforeMutationBudget`] — because it is
//! synchronous, on the write path, and running user-supplied `JavaScript`.

use std::{sync::Arc, time::Duration};

use fraiseql_core::{
    error::{FraiseQLError, Result},
    security::{BeforeMutationGate, BeforeMutationOutcome, BeforeMutationRequest},
};

use crate::subsystems::BeforeMutationHooks;

/// The wall-clock ceiling on one mutation's whole `before:mutation` chain.
///
/// The chain is **synchronous on the write path**: every millisecond it spends is
/// a millisecond the client waits and the write has not happened. Since #1328 it
/// can also issue database reads, so "how long can a hook take?" stopped being a
/// question about CPU in an isolate and became a question about the request's
/// tail latency.
///
/// The default is **500 ms** for the whole chain. That number was already written
/// down — `fraiseql-functions`'s trigger docs have claimed a "500ms default,
/// shorter than the general function timeout of 5s, because before-hooks are on
/// the critical mutation path" since the chain was written — and nothing enforced
/// it: the gate passed `ResourceLimits::default()`, so each hook got the general
/// 5 s and a chain of *n* hooks got 5*n* seconds. This makes the documented number
/// true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeforeMutationBudget(Duration);

impl BeforeMutationBudget {
    /// The default ceiling: 500 ms for the whole chain.
    pub const DEFAULT_MS: u64 = 500;
    /// The environment variable that overrides it, in milliseconds. `0` disables
    /// the ceiling — the same convention `query_timeout_ms` uses.
    pub const ENV: &'static str = "FRAISEQL_FUNCTIONS_BEFORE_MUTATION_BUDGET_MS";

    /// Build a budget from a millisecond count. `0` means "no ceiling".
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(Duration::from_millis(millis))
    }

    /// Resolve the budget from an arbitrary key→value getter.
    ///
    /// Factored out of [`from_env`](Self::from_env) so the override is unit
    /// testable without mutating global process state — the same shape
    /// `DispatchDefaults::from_getter` uses. An unset or unparseable value leaves
    /// the default in place.
    #[must_use]
    pub fn from_getter(get: impl Fn(&str) -> Option<String>) -> Self {
        Self::from_millis(
            get(Self::ENV).and_then(|value| value.parse().ok()).unwrap_or(Self::DEFAULT_MS),
        )
    }

    /// Resolve the budget from the process environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_getter(|key| std::env::var(key).ok())
    }

    /// The ceiling as a `Duration`.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }

    /// Whether a ceiling is in force at all.
    #[must_use]
    pub const fn is_enforced(self) -> bool {
        !self.0.is_zero()
    }
}

impl Default for BeforeMutationBudget {
    fn default() -> Self {
        Self::from_millis(Self::DEFAULT_MS)
    }
}

/// Runs the compiled schema's `before:mutation` function chain as the engine's
/// enforcement point.
///
/// A mutation with no registered `before:mutation` trigger costs one
/// `HashMap::get` and returns [`BeforeMutationOutcome::Proceed`], so the gate is
/// effectively free for every write that declares no rule.
pub struct FunctionChainGate {
    hooks:  Arc<BeforeMutationHooks>,
    budget: BeforeMutationBudget,
}

impl FunctionChainGate {
    /// Wrap the prepared hook bundle as the engine's `before:mutation` gate, with
    /// the default 500 ms chain budget.
    #[must_use]
    pub fn new(hooks: Arc<BeforeMutationHooks>) -> Self {
        Self {
            hooks,
            budget: BeforeMutationBudget::default(),
        }
    }

    /// Override the chain's wall-clock ceiling.
    #[must_use]
    pub const fn with_budget(mut self, budget: BeforeMutationBudget) -> Self {
        self.budget = budget;
        self
    }
}

#[async_trait::async_trait]
impl BeforeMutationGate for FunctionChainGate {
    async fn before_mutation(
        &self,
        request: &BeforeMutationRequest<'_>,
    ) -> Result<BeforeMutationOutcome> {
        // Keyed on the field name the client wrote — never `response_key`. Two roots
        // calling the same mutation differ only by alias.
        let Some(chain) = self.hooks.trigger_registry.before_chain(request.mutation) else {
            return Ok(BeforeMutationOutcome::Proceed);
        };

        // The budget wraps the *whole* chain, here rather than inside `run_chain`,
        // so it binds on every build — including one with no function runtime — and
        // stays covered by the plain `--lib` invocations.
        let outcome =
            run_within_budget(request.mutation, self.budget, self.run_chain(&chain, request)).await;

        map_chain_outcome(request.mutation, outcome)
    }
}

impl FunctionChainGate {
    /// Run the chain on the `before:mutation` host surface.
    ///
    /// Feature-gated because the host surface it runs on lives behind
    /// `fraiseql-functions/host-live` (it reads through the engine's bridge, so it
    /// names `fraiseql-core`'s `MutationHookReader`). Everything around it — the
    /// chain lookup, the budget, and the outcome mapping — stays ungated, and so
    /// stays covered by the plain `--lib` test invocations.
    #[cfg(feature = "functions-runtime")]
    async fn run_chain(
        &self,
        chain: &fraiseql_functions::BeforeMutationChain,
        request: &BeforeMutationRequest<'_>,
    ) -> Result<fraiseql_functions::BeforeMutationResult> {
        let input = request.arguments.clone();
        let host = fraiseql_functions::host::before_mutation::BeforeMutationHost::new(
            fraiseql_functions::EventPayload {
                trigger_type: format!("before:mutation:{}", request.mutation),
                entity:       request.mutation.to_string(),
                event_kind:   "before".to_string(),
                data:         input.clone(),
                timestamp:    chrono::Utc::now(),
            },
            request.reader.clone(),
            request.principal,
        );

        // The per-hook isolate watchdog gets the same ceiling as the whole chain,
        // so a single runaway guest is stopped by its own runtime rather than only
        // by the outer timeout — which cannot reclaim a spinning isolate, only
        // stop waiting for it.
        let limits = fraiseql_functions::ResourceLimits {
            max_duration: self.budget.duration(),
            ..fraiseql_functions::ResourceLimits::default()
        };

        chain
            .execute(
                input,
                &self.hooks.module_registry,
                &self.hooks.observer,
                Arc::new(host),
                limits,
            )
            .await
    }

    /// A build with no function runtime cannot run a declared chain, and must not
    /// pretend the write was approved.
    ///
    /// Unreachable in the product: #1326 makes a build that cannot serve a
    /// declared `[functions]` section refuse to boot, and with no section the gate
    /// is never installed. It is reachable by an embedder that assembles
    /// [`BeforeMutationHooks`] by hand, and for them a named refusal beats the
    /// runtime's generic "No runtime registered for Deno".
    #[cfg(not(feature = "functions-runtime"))]
    #[allow(clippy::unused_async)] // Reason: the gated sibling is async; the signatures must match.
    async fn run_chain(
        &self,
        _chain: &fraiseql_functions::BeforeMutationChain,
        request: &BeforeMutationRequest<'_>,
    ) -> Result<fraiseql_functions::BeforeMutationResult> {
        Err(FraiseQLError::Unsupported {
            message: format!(
                "mutation `{}` declares a before:mutation chain, but this build has no \
                 function runtime (feature `functions-runtime`) — the write is refused rather \
                 than run unadjudicated",
                request.mutation
            ),
        })
    }
}

/// Run `chain` under the budget, turning an overrun into a refusal.
///
/// Generic over the future so both sides of the threshold are testable without a
/// guest runtime: a chain that overruns needs nothing more than a future that
/// takes too long, and a guard tested only on the side that passes is not tested.
///
/// A zero budget means no ceiling, and the future is awaited directly.
async fn run_within_budget<F>(
    mutation: &str,
    budget: BeforeMutationBudget,
    chain: F,
) -> Result<fraiseql_functions::BeforeMutationResult>
where
    F: std::future::Future<Output = Result<fraiseql_functions::BeforeMutationResult>>,
{
    if !budget.is_enforced() {
        return chain.await;
    }
    match tokio::time::timeout(budget.duration(), chain).await {
        Ok(outcome) => outcome,
        // Fail-closed, like every other way the chain can fail to decide: a hook
        // that ran out of time approved nothing.
        Err(_) => Err(FraiseQLError::Timeout {
            timeout_ms: u64::try_from(budget.duration().as_millis()).unwrap_or(u64::MAX),
            query:      Some(format!("before:mutation:{mutation}")),
        }),
    }
}

/// Map what the `before:mutation` chain returned onto what the engine does next.
///
/// Split out of [`FunctionChainGate::before_mutation`] so every arm is testable
/// without a guest runtime: executing a real chain needs a Deno/WASM runtime and a
/// loaded module, which would leave the `Abort` and fail-closed arms — the ones
/// that matter — pinned by nothing.
fn map_chain_outcome(
    mutation: &str,
    outcome: Result<fraiseql_functions::BeforeMutationResult>,
) -> Result<BeforeMutationOutcome> {
    match outcome {
        // The chain threads the input through every trigger and returns what the last
        // one left, so this is the argument view the write must bind from — identical
        // to the input when no trigger rewrote it. Returned as a rewrite
        // unconditionally rather than compared: the old handler treated a `null`
        // result as "no change" and dropped the arguments entirely, which ran the
        // write with no input instead of failing the required-argument check.
        Ok(fraiseql_functions::BeforeMutationResult::Proceed(arguments)) => {
            Ok(BeforeMutationOutcome::ProceedWith { arguments })
        },
        Ok(fraiseql_functions::BeforeMutationResult::Abort(reason)) => {
            Ok(BeforeMutationOutcome::Abort { reason })
        },
        // Reason: `BeforeMutationResult` is `#[non_exhaustive]`, so this arm is
        // required from outside its crate. It fails **closed**: a decision this build
        // does not understand must not execute the write. The previous handler-side
        // wildcard proceeded with the original input, which is the one thing an
        // enforcement hook must never do.
        Ok(unknown) => {
            tracing::error!(
                mutation = %mutation,
                result = ?unknown,
                "before:mutation chain returned an unrecognised decision — refusing the write"
            );
            Err(FraiseQLError::Internal {
                message: "before:mutation hook returned an unrecognised decision".to_string(),
                source:  None,
            })
        },
        // A chain that ran out of its budget already carries the diagnosis that says
        // so; folding it into the generic "execution failed" message would hide the
        // one cause an operator can act on by raising a limit.
        Err(error @ FraiseQLError::Timeout { .. }) => {
            tracing::error!(
                error = %error,
                mutation = %mutation,
                "before:mutation chain exceeded its budget — refusing the write"
            );
            Err(error)
        },
        Err(error) => {
            tracing::error!(
                error = %error,
                mutation = %mutation,
                "before:mutation chain failed"
            );
            Err(FraiseQLError::Internal {
                message: "before:mutation hook execution failed".to_string(),
                source:  None,
            })
        },
    }
}

#[cfg(test)]
mod tests;
