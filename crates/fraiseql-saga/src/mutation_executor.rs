//! Saga mutation dispatch.
//!
//! A saga step's **local** arm is a client of the engine's mutation chokepoint.
//! Its **remote** arm dispatches to a peer subgraph over HTTPS
//! ([`crate::HttpMutationClient`]).

use std::sync::Arc;

use fraiseql_core::{runtime::Executor, schema::RunAs, security::SecurityContext, types::TenantId};
use fraiseql_db::traits::{DatabaseAdapter, SupportsMutations};
use fraiseql_error::Result;
use fraiseql_federation::types::FederationMetadata;
use serde_json::Value;

/// Executes saga mutations.
///
/// Generic over an adapter that is **statically** write-capable: the engine's
/// write entries live on `impl<A: DatabaseAdapter + SupportsMutations>`, so a
/// read-only adapter cannot reach them, and a saga cannot be built over one.
#[derive(Clone)]
pub struct FederationMutationExecutor<A: DatabaseAdapter + SupportsMutations> {
    /// The engine. A saga step dispatches *through* it, never around it.
    engine:   Arc<Executor<A>>,
    /// Federation metadata. Used by the **remote** arm to build the outgoing
    /// GraphQL mutation and project its response; the local arm resolves the
    /// mutation from the compiled schema instead, so it needs none of this.
    metadata: FederationMetadata,
    /// Names this saga in the principal it mints (`system_job:<job_id>`), so an
    /// audit row says which orchestrator wrote.
    job_id:   String,
    /// The background authority every step of this saga writes with.
    ///
    /// There is no default and no fallback: an application that orchestrates a
    /// saga states the roles, scopes and tenant its steps act with, and an empty
    /// `RunAs` is a real answer meaning "no authority" — under which any mutation
    /// declaring `requires_role` is refused. Inferring it would be the only way
    /// back to the fail-open write this crate exists to remove (#1354).
    run_as:   RunAs,
}

impl<A: DatabaseAdapter + SupportsMutations> FederationMutationExecutor<A> {
    /// Create a saga mutation executor over an engine.
    ///
    /// `job_id` names the saga in the minted principal; `run_as` is the authority
    /// its steps write with — see [`Self::run_as`](Self#structfield.run_as) for why
    /// it has no default.
    #[must_use]
    pub fn new(
        engine: Arc<Executor<A>>,
        metadata: FederationMetadata,
        job_id: impl Into<String>,
        run_as: RunAs,
    ) -> Self {
        Self {
            engine,
            metadata,
            job_id: job_id.into(),
            run_as,
        }
    }

    /// The federation metadata this executor resolves entity types against.
    ///
    /// Used by the saga remote-dispatch path
    /// ([`SagaExecutor::dispatch_step`](crate::saga_executor::SagaExecutor)) to
    /// build the outgoing GraphQL mutation and project its response.
    #[must_use]
    pub(crate) const fn metadata(&self) -> &FederationMetadata {
        &self.metadata
    }

    /// The principal one dispatch runs under.
    ///
    /// [`SecurityContext::system_job`] marks its own principal
    /// `EnrichmentMark::Exempt` at the construction site: the orchestrator acting
    /// as itself has no subject an identity resolver could look up. `request_id`
    /// is the step's id, so each write correlates to the step that issued it and
    /// a crash-recovery replay carries the same correlation as the original.
    fn identity(&self, request_id: &str) -> SecurityContext {
        SecurityContext::system_job(
            self.job_id.as_str(),
            request_id,
            self.run_as.roles.clone(),
            self.run_as.scopes.clone(),
            self.run_as.tenant.clone().map(TenantId::from),
        )
    }

    /// Execute a locally-owned step's mutation **through the engine**.
    ///
    /// `mutation_name` is the step's full persisted operation name (e.g.
    /// `createOrder`) — the same name any other caller of the chokepoint passes,
    /// resolved against the compiled schema.
    ///
    /// This used to be a second write path. It built `INSERT`/`UPDATE`/`DELETE`
    /// SQL as a string from the entity metadata, guessed the statement kind from
    /// the operation name's leading verb, and dispatched it with
    /// `DatabaseAdapter::execute_raw_query` — taking no `SecurityContext` at all.
    /// So the operation `Authorizer` (#422), `requires_role`, `requires_actor`
    /// (#966), the `before:mutation` chain (#1327), argument validation, the RLS
    /// session variables, the change-log outbox row and the field authorizer
    /// (#423) were all skipped, on every saga write (#1354). They run now,
    /// because this is [`Executor::execute_mutation_with_security`] and nothing
    /// else.
    ///
    /// The verb sniffing is gone with the SQL: which statement a mutation issues
    /// is the compiled schema's to say, not a prefix match on its name.
    ///
    /// # Errors
    ///
    /// Returns whatever the chokepoint returns: an unknown mutation name, a
    /// refusal from any gate above for this saga's authority, an argument that
    /// does not validate, or the database's own error.
    pub async fn execute_local_mutation(
        &self,
        mutation_name: &str,
        variables: &Value,
        request_id: &str,
    ) -> Result<Value> {
        let principal = self.identity(request_id);
        self.engine
            .execute_mutation_with_security(mutation_name, variables, Some(&principal))
            .await
    }

    /// Execute a mutation on an extended (non-owned) entity — **not
    /// implemented**, and honest about it (#785).
    ///
    /// This method used to fabricate success: it echoed the input variables
    /// back with `_remote_execution: true` without contacting any subgraph, so
    /// a caller believed the owning service applied a write it never heard of.
    /// It now fails loud. Real cross-subgraph mutation propagation exists —
    /// register the owning subgraph on a `SagaCoordinator` (or use
    /// [`crate::HttpMutationClient`] directly), which dispatches the mutation
    /// over HTTPS with SSRF validation and an idempotency key.
    ///
    /// # Errors
    ///
    /// Always returns [`fraiseql_error::FraiseQLError::Internal`].
    // Reason: part of the executor's awaitable mutation surface, alongside the
    // variants that do dial a subgraph. Callers dispatch across all of them.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    pub async fn execute_extended_mutation(
        &self,
        typename: &str,
        mutation_name: &str,
        _variables: &Value,
    ) -> Result<Value> {
        Err(fraiseql_error::FraiseQLError::Internal {
            message: format!(
                "extended mutation '{mutation_name}' on non-owned entity '{typename}' is not \
                 implemented on this executor; dispatch it to the owning subgraph via \
                 HttpMutationClient (or a SagaCoordinator step with the subgraph registered) \
                 instead — this API previously fabricated a success response without \
                 contacting any subgraph"
            ),
            source:  None,
        })
    }
}

#[cfg(test)]
mod tests;
