//! Serving a function-backed root query field (#1329).
//!
//! The server half of `request:query`. `fraiseql-core` decides *whether* a field is
//! function-backed and owns everything around the value — the role and actor gates,
//! field-level RBAC, the selection projection, the response cache; this is the part
//! that actually runs a guest, and it is the only part that knows what a guest is.
//!
//! It is the mirror of [`before_mutation`](super::before_mutation): one
//! trait object installed on the executor's `RuntimeConfig`, so the engine reaches
//! a function through a seam rather than through a handler, and every transport that
//! reaches the executor gets the same behaviour.
//!
//! # The budget
//!
//! A request-serving invocation is on the read path, so the question "how long may
//! it take?" is the client's tail latency. Two knobs, and each does exactly one
//! thing:
//!
//! * a function's declared `timeout_ms` is the **author's** statement about their own function and
//!   is honoured as written — an LLM-backed field legitimately takes seconds;
//! * [`QueryFunctionBudget`] is the **operator's** default for functions that declare none.
//!
//! The env override replaces the default rather than capping the declaration,
//! because a cap would silently ignore a number the author wrote down. The request
//! is bounded regardless by the executor's own `query_timeout_ms`.

use std::{sync::Arc, time::Duration};

use fraiseql_core::{
    error::{FraiseQLError, Result},
    runtime::{QueryFunctionRequest, QueryFunctionResolver},
};

use crate::subsystems::BeforeMutationHooks;

/// The default wall-clock ceiling on one request-serving invocation.
///
/// Five seconds: the general function timeout `fraiseql-functions` has documented
/// since the runtime was written, and the right default for a field that may call
/// out. It is not 500 ms like
/// [`BeforeMutationBudget`](super::before_mutation::BeforeMutationBudget) because nothing is
/// waiting on a write — but it is a ceiling, because a guest that never returns would otherwise
/// hold its request until the executor's own query timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryFunctionBudget(Duration);

impl QueryFunctionBudget {
    /// The default ceiling for a function that declares no `timeout_ms`.
    pub const DEFAULT_MS: u64 = 5_000;
    /// The environment variable that overrides it, in milliseconds. `0` disables
    /// the ceiling — the same convention `query_timeout_ms` uses.
    pub const ENV: &'static str = "FRAISEQL_FUNCTIONS_REQUEST_QUERY_BUDGET_MS";

    /// Build a budget from a millisecond count. `0` means "no ceiling".
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(Duration::from_millis(millis))
    }

    /// Resolve the budget from an arbitrary key→value getter.
    ///
    /// Factored out of [`from_env`](Self::from_env) so the override is unit testable
    /// without mutating global process state. An unset or unparseable value leaves
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

    /// The ceiling for one function: its declared `timeout_ms` when it has one,
    /// this budget otherwise.
    ///
    /// The declaration wins, deliberately. A cap would let an operator's default
    /// silently overrule a number the author wrote next to the function — the
    /// silently-ignored-setting failure this codebase keeps removing — and the
    /// request is bounded either way by the executor's `query_timeout_ms`.
    #[must_use]
    pub fn for_function(self, declared_ms: Option<u64>) -> Self {
        declared_ms.map_or(self, Self::from_millis)
    }
}

impl Default for QueryFunctionBudget {
    fn default() -> Self {
        Self::from_millis(Self::DEFAULT_MS)
    }
}

/// Answers a function-backed root query field by invoking its declared function.
pub struct FunctionQueryResolver {
    hooks:  Arc<BeforeMutationHooks>,
    budget: QueryFunctionBudget,
}

impl FunctionQueryResolver {
    /// Build the resolver over the loaded function modules.
    #[must_use]
    pub fn new(hooks: Arc<BeforeMutationHooks>) -> Self {
        Self {
            hooks,
            budget: QueryFunctionBudget::default(),
        }
    }

    /// Override the default ceiling for functions that declare no `timeout_ms`.
    #[must_use]
    pub const fn with_budget(mut self, budget: QueryFunctionBudget) -> Self {
        self.budget = budget;
        self
    }
}

impl QueryFunctionResolver for FunctionQueryResolver {
    fn resolve<'a>(
        &'a self,
        request: QueryFunctionRequest<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send + 'a>>
    {
        Box::pin(async move {
            let field = request.field.to_string();
            let budget = self.budget.for_function(self.declared_timeout_ms(request.function));
            run_within_budget(&field, budget, self.invoke(request)).await
        })
    }
}

impl FunctionQueryResolver {
    /// The `timeout_ms` the schema declares for this function, if any.
    fn declared_timeout_ms(&self, function: &str) -> Option<u64> {
        self.hooks.request_query_timeouts.get(function).copied().flatten()
    }

    /// Invoke the guest on the request-serving host surface.
    ///
    /// Feature-gated because the host surface lives behind
    /// `fraiseql-functions/host-live`. Everything around it — the budget, the
    /// diagnosis — stays ungated, and so stays covered by the plain `--lib` test
    /// invocations.
    #[cfg(feature = "functions-runtime")]
    async fn invoke(&self, request: QueryFunctionRequest<'_>) -> Result<serde_json::Value> {
        use fraiseql_functions::host::request_query::{
            RequestQueryHost, interpret_query_answer, request_query_payload,
        };

        let module = self.hooks.module_registry.get(request.function).ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!(
                    "`{}` is backed by the function `{}`, which is not in the module registry — \
                     the module was not loaded from `[functions] module_dir` at boot",
                    request.field, request.function
                ),
                path:    Some(request.field.to_string()),
            }
        })?;

        let payload = request_query_payload(request.field, request.arguments.clone());
        let host = RequestQueryHost::new(
            payload.clone(),
            Some(Arc::clone(&request.reader)),
            request.principal,
            super::after_mutation::host_context_config(),
        );

        // The isolate's own watchdog gets the same ceiling as the outer budget, so a
        // runaway guest is stopped by its own runtime rather than only by a timeout
        // that can stop waiting but cannot reclaim a spinning isolate.
        let limits = fraiseql_functions::ResourceLimits {
            max_duration: self
                .budget
                .for_function(self.declared_timeout_ms(request.function))
                .duration(),
            ..fraiseql_functions::ResourceLimits::default()
        };

        let result = self
            .hooks
            .observer
            .invoke_with_context(module, payload, Arc::new(host), limits)
            .await
            .map_err(|error| attribute(request.field, request.function, &error))?;
        Ok(interpret_query_answer(result.value))
    }

    /// A build with no function runtime cannot answer a field it has no runtime for,
    /// and must not answer it with nothing.
    ///
    /// Unreachable in the product: #1326 makes a build that cannot serve a declared
    /// `[functions]` section refuse to boot, and with no section no resolver is
    /// installed. Reachable by an embedder assembling the hooks by hand, and for
    /// them a named refusal beats "No runtime registered for Deno".
    #[cfg(not(feature = "functions-runtime"))]
    #[allow(clippy::unused_async)] // Reason: the gated sibling is async; the signatures must match.
    async fn invoke(&self, request: QueryFunctionRequest<'_>) -> Result<serde_json::Value> {
        Err(FraiseQLError::Unsupported {
            message: format!(
                "`{}` is backed by the function `{}`, but this build has no function runtime \
                 (feature `functions-runtime`) — the field is refused rather than answered with \
                 nothing",
                request.field, request.function
            ),
        })
    }
}

/// Name the field and its function in a failure that came from the runtime.
///
/// A guest exception, an invalid module or a refused host op arrives as a message
/// about the guest — "Failed to load WASM component: expected `(`" — with nothing
/// tying it to a GraphQL field. That is unreadable in a client's `errors` array and
/// almost as bad in a log: a server may serve several function-backed fields, and
/// the only thing distinguishing their failures is which one was asked for.
///
/// The original message is kept whole rather than replaced. A wrapper that said
/// "the function failed" would remove the only part an author can act on.
///
/// Gated with `invoke`, its only caller: a build with no function runtime never
/// reaches a runtime error to attribute.
#[cfg(feature = "functions-runtime")]
fn attribute(field: &str, function: &str, error: &FraiseQLError) -> FraiseQLError {
    FraiseQLError::Validation {
        message: format!("`{field}` (function `{function}`): {error}"),
        path:    Some(field.to_string()),
    }
}

/// Run one invocation under the budget, turning an overrun into an error that names
/// the field.
///
/// Generic over the future so both sides of the threshold are testable without a
/// guest runtime: an invocation that overruns needs nothing more than a future that
/// takes too long, and a guard tested only on the side that passes is not tested.
///
/// A zero budget means no ceiling, and the future is awaited directly.
///
/// The overrun keeps its **own** diagnosis — a `Timeout` naming the field — rather
/// than being flattened into "the function failed". A field that is slow and a field
/// that is broken need different answers from the same error message.
async fn run_within_budget<F>(
    field: &str,
    budget: QueryFunctionBudget,
    invocation: F,
) -> Result<serde_json::Value>
where
    F: std::future::Future<Output = Result<serde_json::Value>>,
{
    if !budget.is_enforced() {
        return invocation.await;
    }
    match tokio::time::timeout(budget.duration(), invocation).await {
        Ok(answer) => answer,
        Err(_) => Err(FraiseQLError::Timeout {
            timeout_ms: u64::try_from(budget.duration().as_millis()).unwrap_or(u64::MAX),
            query:      Some(field.to_string()),
        }),
    }
}

#[cfg(test)]
mod tests;
