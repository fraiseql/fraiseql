//! Federation query execution (_service and _entities).

use std::sync::Arc;

use super::super::Executor;
use crate::{
    db::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    security::SecurityContext,
};

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute a federation query (_service or _entities).
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — the query name is not `_service` or `_entities`, or
    ///   federation is not enabled in the compiled schema.
    /// * [`FraiseQLError::Database`] — the `_entities` lookup query fails.
    pub(in crate::runtime::executor) async fn execute_federation_query(
        &self,
        query_name: &str,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        match query_name {
            "_service" => self.execute_service_query().await,
            "_entities" => self.execute_entities_query(query, variables, security_context).await,
            _ => Err(FraiseQLError::Validation {
                message: format!("Unknown federation query: {}", query_name),
                path:    None,
            }),
        }
    }

    /// Execute _service query returning federation SDL.
    async fn execute_service_query(&self) -> Result<serde_json::Value> {
        // Get federation metadata from schema
        let fed_metadata =
            self.ctx.schema.federation_metadata().ok_or_else(|| FraiseQLError::Validation {
                message: "Federation not enabled in schema".to_string(),
                path:    None,
            })?;

        // Generate SDL with federation directives
        let raw_schema = self.ctx.schema.raw_schema();
        let sdl = crate::federation::generate_service_sdl(&raw_schema, &fed_metadata);

        // Return federation response format
        let response = serde_json::json!({
            "data": {
                "_service": {
                    "sdl": sdl
                }
            }
        });

        Ok(response)
    }

    /// Execute _entities query resolving federation entities.
    async fn execute_entities_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        // #423: the federation `_entities` resolver has no SecurityContext and resolves
        // entities by `__typename`; it does not run per-row field authorization. Fail
        // closed if the schema declares any policy-gated field (tracked follow-up:
        // thread an authorizer into the subgraph resolver).
        crate::security::field_authorizer::deny_if_schema_has_gated_field(
            &self.ctx.schema,
            "federation _entities",
        )?;

        // Get federation metadata from schema
        let fed_metadata =
            self.ctx.schema.federation_metadata().ok_or_else(|| FraiseQLError::Validation {
                message: "Federation not enabled in schema".to_string(),
                path:    None,
            })?;

        // Extract representations from variables
        let representations_value =
            variables.and_then(|v| v.get("representations")).ok_or_else(|| {
                FraiseQLError::Validation {
                    message: "_entities query requires 'representations' variable".to_string(),
                    path:    None,
                }
            })?;

        // Parse representations
        let representations =
            crate::federation::parse_representations(representations_value, &fed_metadata)?;

        // Phase 03 (C1b): fail-closed authorization for RLS-/inject-/role-gated entity
        // types. Returns before any SQL runs when the request is not allowed to resolve
        // the requested entities.
        self.enforce_entities_authz(&representations, security_context)?;

        // Validate representations
        crate::federation::validate_representations(&representations, &fed_metadata)?;

        // Create federation resolver
        let fed_resolver = crate::federation::FederationResolver::new(fed_metadata);

        // Extract actual field selection from GraphQL query AST.
        // __typename is NOT added to the SQL field list — it is a GraphQL meta-field
        // not stored in the database. The database_resolver injects it into results.
        let selection = match crate::federation::selection_parser::parse_field_selection(query) {
            Ok(sel) if !sel.fields.is_empty() => {
                let fields: Vec<String> =
                    sel.fields.into_iter().filter(|f| f != "__typename").collect();
                crate::federation::FieldSelection::new(fields)
            },
            _ => {
                // Fallback to wildcard if parsing fails or no fields extracted
                crate::federation::FieldSelection::new(vec![
                    "*".to_string(), // Wildcard for all fields (will be expanded by resolver)
                ])
            },
        };

        // Extract or create trace context for federation operations
        // Note: Trace context should ideally be passed from HTTP headers via ExecutionContext,
        // but for now we create a new context for tracing federation operations.
        // The trace context could be injected through the query variables or a request-scoped store
        // in future versions to correlate with the incoming HTTP trace headers.
        let trace_context = crate::federation::FederationTraceContext::new();

        // Batch load entities from database with tracing support
        let entities = crate::federation::batch_load_entities_with_tracing(
            &representations,
            &fed_resolver,
            Arc::clone(&self.ctx.adapter),
            &selection,
            Some(trace_context),
        )
        .await?;

        // Return federation response format
        let response = serde_json::json!({
            "data": {
                "_entities": entities
            }
        });

        Ok(response)
    }

    /// Fail-closed authorization gate for the federation `_entities` path (Phase 03 C1b).
    ///
    /// The `_entities` resolver builds its own SQL in `fraiseql-federation` with no slot
    /// to inject the per-row RLS / `inject_params` predicate the regular query path
    /// applies, so it cannot compose those filters. Rather than serve entities with no
    /// row-level enforcement, it fails **closed**:
    ///
    /// * **Row-level security configured + unauthenticated request** → deny. An RLS-protected
    ///   deployment must never resolve federation entities for an anonymous caller (the resolver
    ///   applies no per-row predicate).
    /// * **A representation's backing query declares `requires_role`** → deny unless the request
    ///   holds that role (enforced for authenticated and anonymous callers alike).
    /// * **A representation's backing query declares `inject_params` (tenant/owner scoping) +
    ///   unauthenticated request** → deny.
    ///
    /// When the request **is** authenticated, RLS-/inject-backed types are resolved under
    /// the *trusted-gateway* assumption: the federation gateway forwarded an
    /// authenticated principal, and the entity references it passes were themselves
    /// produced by a row-filtered parent query on the originating subgraph. Composing
    /// per-row RLS / `inject_params` into this subgraph resolver (to also defend against a
    /// caller hitting `_entities` directly with arbitrary ids) is a tracked follow-up.
    ///
    /// The type→gate association uses the same first-wins rule as the Relay `node` path
    /// (the query that exposes the type via a SQL view). A representation type with no
    /// backing read query has no role/inject gate to enforce here; the global RLS gate
    /// above still covers it.
    fn enforce_entities_authz(
        &self,
        representations: &[crate::federation::EntityRepresentation],
        security_context: Option<&SecurityContext>,
    ) -> Result<()> {
        // Type-independent gate: an RLS-configured deployment must not resolve entities
        // for an anonymous caller — the resolver applies no per-row RLS predicate.
        if self.ctx.config.rls_policy.is_some() && security_context.is_none() {
            return Err(entities_authz_denied(
                "row-level security is configured but the _entities request is unauthenticated",
            ));
        }

        for rep in representations {
            let Some(qdef) = self
                .ctx
                .schema
                .queries
                .iter()
                .find(|q| q.return_type == rep.typename && q.sql_source.is_some())
            else {
                continue;
            };

            // requires_role: deny unless the request holds the role (anonymous or not).
            if let Some(ref required_role) = qdef.requires_role {
                let has_role =
                    security_context.is_some_and(|sc| sc.roles.iter().any(|r| r == required_role));
                if !has_role {
                    return Err(entities_authz_denied(&format!(
                        "type '{}' requires a role the _entities request does not hold",
                        rep.typename
                    )));
                }
            }

            // inject_params (tenant/owner scoping): fail closed for anonymous callers —
            // the resolver cannot apply the per-row filter.
            if !qdef.inject_params.is_empty() && security_context.is_none() {
                return Err(entities_authz_denied(&format!(
                    "type '{}' is tenant/owner-scoped but the _entities request is unauthenticated",
                    rep.typename
                )));
            }
        }

        Ok(())
    }
}

/// The fail-closed `_entities` denial: a 403 that does not echo the requested ids.
fn entities_authz_denied(reason: &str) -> FraiseQLError {
    FraiseQLError::Authorization {
        message:  format!("federation _entities denied: {reason}"),
        action:   Some("read".to_string()),
        resource: Some("_entities".to_string()),
    }
}
