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
    // Reason: one arm of the awaited `_service`/`_entities` dispatch above; SDL
    // generation happens to be in-memory while entity resolution is not.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
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
        // #423: the *dynamic* per-row field authorizer (`authorize = true`) is not wired
        // into the subgraph resolver, so a schema declaring any such field shuts
        // `_entities` down entirely rather than serving it unevaluated (tracked
        // follow-up: thread an authorizer into the subgraph resolver). This check is
        // schema-global and says nothing about the static `requires_scope` RBAC below,
        // which #1030 found this path had simply never run.
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

        // #1030: field-level RBAC, through the SAME classifier the query path uses.
        // `_entities` ran its own two checks and this was not one of them, so a
        // `requires_scope` field that answers 403 on `query { employee(id:…) { salary } }`
        // came back in full through `_entities` — and `on_deny = Mask` came back
        // *unmasked*, the quieter and worse half. Calling `classify_fields_for_read` is
        // the point of the fix, not an implementation detail: a check added to the query
        // path is now added to this one by construction, which is what stopped being
        // true when this resolver grew a second copy of the gates.
        //
        // Runs before the read, so a `Reject` field denies without touching the database.
        let masked_by_type =
            self.classify_entities_fields(&representations, &selection, security_context)?;

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
        let mut entities = crate::federation::batch_load_entities_enforced(
            &representations,
            &fed_resolver,
            Arc::clone(&self.ctx.adapter),
            &selection,
            Some(trace_context),
            &row_filters,
            &session_pairs,
        )
        .await?;

        // #1030: null the fields the classifier masked, per entity. The resolver returns
        // `Vec<Option<Value>>` positionally aligned with `representations` (the federation
        // spec requires that alignment), so the type is read from the representation
        // rather than from the payload's `__typename` — a selection that omits
        // `__typename` still masks.
        if masked_by_type.values().any(|masked| !masked.is_empty()) {
            for (entity, rep) in entities.iter_mut().zip(representations.iter()) {
                let (Some(entity), Some(masked)) =
                    (entity.as_mut(), masked_by_type.get(&rep.typename))
                else {
                    continue;
                };
                if !masked.is_empty() {
                    super::super::null_masked_fields(entity, masked);
                }
            }
        }

        // Return federation response format
        let response = serde_json::json!({
            "data": {
                "_entities": entities
            }
        });

        Ok(response)
    }

    /// Classify the requested selection for every requested entity type through the
    /// **same** helper the query path uses (#1030).
    ///
    /// Returns each type's masked-field list; a `Reject` field the caller cannot read
    /// returns `Err(FraiseQLError::Authorization)` instead, before any SQL runs.
    ///
    /// [`crate::federation::selection_parser::parse_field_selection`] flattens every
    /// inline fragment into one list — `... on Employee { salary }` and
    /// `... on Department { name }` arrive as `["salary", "name"]` with no record of
    /// which condition each came from — so each requested type is classified against
    /// the whole list. That is sound, not sloppy: both classifiers pass through a name
    /// absent from the type's definition (their existing rule for built-ins such as
    /// `__typename`), so another fragment's fields never deny here. The residue is a
    /// field name declared on two requested types and gated on one, which is checked
    /// against both — an over-denial, the direction that fails closed.
    ///
    /// The wildcard fallback needs no special case, which is worth stating because it looks
    /// like it should. When `parse_field_selection` fails the caller substitutes `["*"]`,
    /// which no type declares as a field, so nothing here is masked or rejected — and
    /// nothing needs to be: `build_select_list` drops `*` (`is_safe_sql_identifier` admits
    /// only `[A-Za-z0-9_]`) and emits the type's key fields alone. A gated non-key field
    /// cannot be read on that path at all, and the keys are the values the caller supplied
    /// in the representation, echoed back.
    fn classify_entities_fields(
        &self,
        representations: &[crate::federation::EntityRepresentation],
        selection: &crate::federation::FieldSelection,
        security_context: Option<&SecurityContext>,
    ) -> Result<std::collections::HashMap<String, Vec<String>>> {
        let mut masked_by_type: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for rep in representations {
            if masked_by_type.contains_key(&rep.typename) {
                continue;
            }
            let access = super::security::classify_fields_for_read(
                &self.ctx.schema,
                &rep.typename,
                selection.fields.clone(),
                security_context,
            )?;
            masked_by_type.insert(rep.typename.clone(), access.masked);
        }
        Ok(masked_by_type)
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
    /// backing read query still has its **type-level** `requires_role` enforced (#1030):
    /// #677's lowering onto operations cannot reach an entity no operation returns, so
    /// the declaration is read from the type directly. Such a type declares no
    /// `inject_params` — it has nowhere to — and the global RLS gate above plus the
    /// caller's session variables still cover it.
    ///
    /// This gate is role/row-shaped only. **Field**-level RBAC is
    /// [`classify_entities_fields`](Self::classify_entities_fields), which delegates to the
    /// query path's classifier rather than restating it here — the two checks living in
    /// one function is how they drifted apart in the first place.
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
            let qdef = self
                .ctx
                .schema
                .queries
                .iter()
                .find(|q| q.return_type == rep.typename && q.sql_source.is_some());

            // requires_role: deny unless the request holds the role (anonymous or not).
            //
            // #1030(b): read the *type's* declaration too, not only the query's. #677
            // lowers a type-level `requires_role` onto the operations that return the
            // type — deliberately, so the five operation-level gates enforce it with no
            // sixth enforcement site. But an entity reachable only through `_entities`
            // has no operation to receive the lowering (an owner-split `extend type` with
            // a type-level `sql_source` per #507, and the Python SDK synthesizes one for
            // every non-embedded type), so this loop used to `continue` past it and
            // `entity_sources()` served it ungated. Load-time validation
            // (`CompiledSchema::type_role_violations`) rejects a query/type pair that
            // disagrees, so at most one distinct role is ever in play here.
            let required_role = qdef.and_then(|q| q.requires_role.as_deref()).or_else(|| {
                self.ctx
                    .schema
                    .types
                    .iter()
                    .find(|t| t.name.as_str() == rep.typename)
                    .and_then(|t| t.requires_role.as_deref())
            });
            // NOT `role_gate::enforce_requires_role`: `_entities` answers a refusal as
            // an authorization denial with the federation error shape, not as the
            // operation's absence — a subgraph's caller is a gateway that already
            // knows the type exists, so there is nothing to hide from it.
            if let Some(required_role) = required_role {
                let has_role =
                    security_context.is_some_and(|sc| sc.roles.iter().any(|r| r == required_role));
                if !has_role {
                    return Err(entities_authz_denied(&format!(
                        "type '{}' requires a role the _entities request does not hold",
                        rep.typename
                    )));
                }
            }

            // requires_actor (#966): `_entities` is the second door around the
            // operation gates (#1030), so the backing query's actor allow-list is
            // enforced here too. Unlike `requires_role` there is no type-level
            // declaration to fall back to — `requires_actor` is operation-only by
            // design, precisely so there is no lowering that can fail to reach a
            // queryless entity. An entity with no backing query is therefore
            // unrestricted by this gate and covered by `requires_role` above.
            if let Some(q) = qdef {
                crate::security::actor_type::enforce_requires_actor(
                    "Query",
                    &q.name,
                    &q.requires_actor,
                    security_context,
                )
                .map_err(|_| {
                    entities_authz_denied(&format!(
                        "type '{}' is restricted to actor types the _entities request is not",
                        rep.typename
                    ))
                })?;
            }

            // inject_params (tenant/owner scoping): fail closed for anonymous callers —
            // the resolver cannot apply the per-row filter. A queryless entity declares
            // no inject params (it has nowhere to); its session variables are still
            // applied, so DB-native `current_setting()` RLS covers it.
            if qdef.is_some_and(|q| !q.inject_params.is_empty()) && security_context.is_none() {
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
