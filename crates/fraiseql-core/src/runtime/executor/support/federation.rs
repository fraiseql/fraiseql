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

        // Create federation resolver, carrying each entity type's backing relation
        // and jsonb projection column so the `_entities` resolver reads from the real
        // view (`v_organization`) and projects its `data`-jsonb fields — instead of
        // `lower(typename)` selecting bare columns, which named a relation that does
        // not exist and could not read jsonb-backed fields, so view-backed
        // cross-subgraph joins silently returned null (#504).
        //
        // The backing relation is sourced from the *query* that returns the type
        // (owned entities), with a fallback to the type-level `sql_source` for an
        // owner-split `extend type` entity that has no local query (#507). See
        // [`CompiledSchema::entity_sources`].
        let fed_resolver = crate::federation::FederationResolver::new(fed_metadata)
            .with_entity_sources(self.ctx.schema.entity_sources());

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

        // Phase 03 (C1b/R1): compose per-row enforcement for authenticated requests.
        //  * `row_filters` — per entity type, the `inject_params` (tenant/owner) scoping rendered
        //    as a columnar predicate ANDed onto the key lookup, so a direct `_entities` hit with
        //    arbitrary ids is still row-filtered (no longer resolved "under the trusted-gateway
        //    assumption" for inject-scoped types).
        //  * `session_pairs` — the caller's session variables, applied transaction-locally so
        //    `current_setting()` DB-native RLS is enforced on this path (#329 parity).
        // App-level `rls_policy` stays trusted-gateway: its `WhereClause` targets the JSONB
        // `data->>` view shape and cannot be composed onto the columnar entity table.
        let row_filters = self.build_entities_row_filters(&representations, security_context)?;
        let resolved_session_vars = match security_context {
            Some(sc)
                if !self.ctx.schema.session_variables.variables.is_empty()
                    || self.ctx.schema.session_variables.inject_started_at =>
            {
                super::super::security::resolve_session_variables(
                    &self.ctx.schema.session_variables,
                    sc,
                )?
            },
            _ => Vec::new(),
        };
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        // Extract or create trace context for federation operations
        // Note: Trace context should ideally be passed from HTTP headers via ExecutionContext,
        // but for now we create a new context for tracing federation operations.
        // The trace context could be injected through the query variables or a request-scoped store
        // in future versions to correlate with the incoming HTTP trace headers.
        let trace_context = crate::federation::FederationTraceContext::new();

        // Batch load entities from database with tracing support + per-row enforcement.
        let entities = crate::federation::batch_load_entities_enforced(
            &representations,
            &fed_resolver,
            Arc::clone(&self.ctx.adapter),
            &selection,
            Some(trace_context),
            &row_filters,
            &session_pairs,
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
    /// This gate runs before any SQL and rejects requests that must never reach the
    /// resolver. It composes with the per-row enforcement applied afterwards by
    /// [`build_entities_row_filters`](Self::build_entities_row_filters) (C1b/R1):
    ///
    /// * **Row-level security configured + unauthenticated request** → deny. An RLS-protected
    ///   deployment must never resolve federation entities for an anonymous caller (the resolver
    ///   applies no per-row predicate for an absent principal).
    /// * **A representation's backing query declares `requires_role`** → deny unless the request
    ///   holds that role (enforced for authenticated and anonymous callers alike).
    /// * **A representation's backing query declares `inject_params` (tenant/owner scoping) +
    ///   unauthenticated request** → deny.
    ///
    /// When the request **is** authenticated, `inject_params`-scoped types are now row-filtered:
    /// `build_entities_row_filters` composes the tenant/owner predicate onto the resolver SQL and
    /// the caller's session variables drive `current_setting()` DB-native RLS, so a direct
    /// `_entities` hit with arbitrary ids is still scoped. An app-level `rls_policy` `WhereClause`
    /// remains under the *trusted-gateway* assumption — it targets the JSONB `data->>` view shape
    /// and cannot be composed onto the columnar federation entity table (a documented limitation).
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

    /// Build the per-typename per-row enforcement predicates for the `_entities`
    /// resolver (Phase 03 C1b/R1 follow-up).
    ///
    /// For each distinct requested entity type whose backing read query declares
    /// `inject_params` (tenant/owner scoping), this composes a columnar equality
    /// predicate — `WhereClause::NativeField` (`"tenant_id" = $N`) — from the
    /// caller's resolved inject values. The federation entity table is columnar
    /// (`SELECT … FROM "<type>"`), never the JSONB `data->>` view, so the predicate
    /// is built as a `NativeField` (with the cast from `native_columns` when known)
    /// and **never** a JSONB `Field`.
    ///
    /// Returns an empty map for an anonymous request: it has no principal to scope
    /// by, and [`enforce_entities_authz`](Self::enforce_entities_authz) has already
    /// denied any inject-/role-gated type for unauthenticated callers (ungated types
    /// carry no per-row filter). **Fail-closed:** when a backing query is
    /// inject-scoped, [`resolve_inject_value`](super::super::resolve_inject_value)
    /// errors if the required claim is absent, so the request is denied rather than
    /// resolved without the filter.
    fn build_entities_row_filters(
        &self,
        representations: &[crate::federation::EntityRepresentation],
        security_context: Option<&SecurityContext>,
    ) -> Result<std::collections::HashMap<String, crate::db::WhereClause>> {
        use crate::db::{WhereClause, WhereOperator};

        let mut filters = std::collections::HashMap::new();
        let Some(sc) = security_context else {
            return Ok(filters);
        };

        for rep in representations {
            if filters.contains_key(&rep.typename) {
                continue;
            }
            let Some(qdef) = self
                .ctx
                .schema
                .queries
                .iter()
                .find(|q| q.return_type == rep.typename && q.sql_source.is_some())
            else {
                continue;
            };
            if qdef.inject_params.is_empty() {
                continue;
            }

            let mut conditions: Vec<WhereClause> = Vec::with_capacity(qdef.inject_params.len());
            for (col, source) in &qdef.inject_params {
                let value = super::super::resolve_inject_value(col, source, sc)?;
                let pg_cast = qdef
                    .native_columns
                    .get(col)
                    .map(|t| crate::runtime::native_columns::pg_type_to_cast(t).to_string())
                    .unwrap_or_default();
                conditions.push(WhereClause::NativeField {
                    column: col.clone(),
                    pg_cast,
                    operator: WhereOperator::Eq,
                    value,
                });
            }
            let clause = if conditions.len() == 1 {
                conditions.remove(0)
            } else {
                WhereClause::And(conditions)
            };
            filters.insert(rep.typename.clone(), clause);
        }

        Ok(filters)
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
