//! `before:mutation` chain dispatch (#1327).
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

use std::sync::Arc;

use fraiseql_core::{
    error::{FraiseQLError, Result},
    security::{BeforeMutationGate, BeforeMutationOutcome, BeforeMutationRequest},
};

use crate::subsystems::BeforeMutationHooks;

/// Runs the compiled schema's `before:mutation` function chain as the engine's
/// enforcement point.
///
/// A mutation with no registered `before:mutation` trigger costs one
/// `HashMap::get` and returns [`BeforeMutationOutcome::Proceed`], so the gate is
/// effectively free for every write that declares no rule.
pub struct FunctionChainGate {
    hooks: Arc<BeforeMutationHooks>,
}

impl FunctionChainGate {
    /// Wrap the prepared hook bundle as the engine's `before:mutation` gate.
    #[must_use]
    pub const fn new(hooks: Arc<BeforeMutationHooks>) -> Self {
        Self { hooks }
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

        let input = request.arguments.clone();
        let host = fraiseql_functions::NoopHostContext::new(fraiseql_functions::EventPayload {
            trigger_type: format!("before:mutation:{}", request.mutation),
            entity:       request.mutation.to_string(),
            event_kind:   "before".to_string(),
            data:         input.clone(),
            timestamp:    chrono::Utc::now(),
        });

        let outcome = chain
            .execute(
                input,
                &self.hooks.module_registry,
                &self.hooks.observer,
                &host,
                fraiseql_functions::ResourceLimits::default(),
            )
            .await;

        map_chain_outcome(request.mutation, outcome)
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
