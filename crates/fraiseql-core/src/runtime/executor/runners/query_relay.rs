//! Relay connection and node query execution methods for [`QueryRunner`].

use std::sync::Arc;

use super::{
    super::resolve_inject_value,
    query::QueryRunner,
    query_params::{
        client_where_argument, coerce_pagination_arg, compute_projection_reduction,
        enforce_max_page_size, inject_param_where_clause,
    },
    query_projection::{
        build_typed_projection_fields, enrich_order_by_clauses, selections_contain_field,
    },
};
use crate::{
    backend::{CursorValue, WhereClause, projection_generator::PostgresProjectionGenerator},
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::ResultProjector,
    schema::SqlProjectionHint,
    security::{RlsWhereClause, SecurityContext, rls_policy::RlsTarget},
};

impl QueryRunner {
    /// Execute a Relay connection query with cursor-based (keyset) pagination.
    ///
    /// Reads `first`, `after`, `last`, `before` from the match's merged arguments
    /// (inline arguments under request variables, variables winning), fetches a page
    /// of rows using `pk_{type}` keyset ordering, and wraps the result in the
    /// Relay `XxxConnection` format:
    /// ```json
    /// {
    ///   "data": {
    ///     "users": {
    ///       "edges": [{ "cursor": "NDI=", "node": { "id": "...", ... } }],
    ///       "pageInfo": {
    ///         "hasNextPage": true, "hasPreviousPage": false,
    ///         "startCursor": "NDI=", "endCursor": "Mw=="
    ///       }
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if required pagination variables are
    /// missing or contain invalid cursor values.
    /// Returns [`FraiseQLError::Database`] if the SQL execution or result projection fails.
    pub(super) async fn execute_relay_query(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        session_vars: &[(&str, &str)],
    ) -> Result<serde_json::Value> {
        use crate::{
            compiler::aggregation::OrderByClause,
            runtime::relay::{
                KEYSET_CURSOR_VERSION, KeysetCursor, encode_edge_cursor, encode_keyset_cursor,
                ordering_fingerprint,
            },
            schema::CursorType,
        };

        // #423: the Relay path emits the entity `node` blob directly and does not yet
        // run per-row field authorization. Fail closed if the entity type has any
        // policy-gated field (tracked follow-up: enforce per-edge).
        if self.ctx.schema.type_has_gated_field(&query_match.query_def.return_type) {
            return Err(FraiseQLError::Authorization {
                message:  format!(
                    "Field-level authorization is not enforced on the Relay path, but type \
                     '{}' declares a policy-gated field",
                    query_match.query_def.return_type
                ),
                action:   Some("read".to_string()),
                resource: Some(query_match.query_def.return_type.clone()),
            });
        }

        let query_def = &query_match.query_def;

        // Guard: queries with inject params require a security context.
        if !query_def.inject_params.is_empty() && security_context.is_none() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but was called without a security context",
                    query_def.name
                ),
                path:    None,
            });
        }

        let sql_source =
            query_def.sql_source.as_deref().ok_or_else(|| FraiseQLError::Validation {
                message: format!("Relay query '{}' has no sql_source configured", query_def.name),
                path:    None,
            })?;

        let cursor_column =
            query_def
                .relay_cursor_column
                .as_deref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: format!(
                        "Relay query '{}' has no relay_cursor_column derived",
                        query_def.name
                    ),
                    path:    None,
                })?;

        // Guard: relay pagination requires the executor to have been constructed
        // via `Executor::new_with_relay` with a `RelayDatabaseAdapter`.
        let relay = self.ctx.relay.as_ref().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Relay pagination is not supported by the {} adapter. \
                 Use a relay-capable adapter (e.g. PostgreSQL) and construct \
                 the executor with `Executor::new_with_relay`.",
                self.ctx.adapter.database_type()
            ),
            path:    None,
        })?;

        // --- RLS + inject_params evaluation (same logic as execute_from_match) ---
        // Evaluate RLS policy to generate security WHERE clause.
        let rls_where_clause: Option<RlsWhereClause> =
            match (&self.ctx.config.rls_policy, security_context) {
                (Some(rls_policy), Some(ctx)) => rls_policy
                    .evaluate(ctx, &RlsTarget::query(&query_def.name, &query_def.return_type))?,
                (Some(_), None) => {
                    // Fail closed: an RLS-protected deployment must not serve a relay page to an
                    // anonymous caller. Previously this fell through to `None` (no RLS clause),
                    // leaking every row to unauthenticated relay queries.
                    return Err(FraiseQLError::Validation {
                        message: format!("Query '{}' not found in schema", query_def.name),
                        path:    None,
                    });
                },
                (None, _) => None,
            };

        // Resolve inject_params from JWT claims and compose with RLS.
        let security_where: Option<WhereClause> = if query_def.inject_params.is_empty() {
            rls_where_clause.map(RlsWhereClause::into_where_clause)
        } else {
            let ctx = security_context.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but was called without a security context",
                    query_def.name
                ),
                path:    None,
            })?;
            let mut conditions: Vec<WhereClause> = query_def
                .inject_params
                .iter()
                .map(|(col, source)| {
                    let value =
                        resolve_inject_value(col, source, ctx, self.ctx.schema.tenant_claim())?;
                    Ok(inject_param_where_clause(col, value, &query_def.native_columns))
                })
                .collect::<Result<Vec<_>>>()?;

            if let Some(rls) = rls_where_clause {
                conditions.insert(0, rls.into_where_clause());
            }
            match conditions.len() {
                0 => None,
                1 => Some(conditions.remove(0)),
                _ => Some(WhereClause::And(conditions)),
            }
        };

        // Field-level RBAC over what each edge's `node` selects, at every level, each
        // against its own type — and each nested level's `requires_role` /
        // `requires_actor` (`query_nested`). Before the read, so a `Reject` anywhere never
        // reaches the database. A connection used to serve each row's stored `data` as its
        // `node`: every stored key, selected or not, the gated ones included.
        let node_fields = connection_node_fields(query_match);
        let selection_access = super::query_nested::SelectionAccess::classify(
            &self.ctx.schema,
            &query_def.return_type,
            &node_fields,
            Vec::new(),
            security_context,
            super::query_nested::LevelAuthz::from_config(&self.ctx.config, variables),
        )?;

        // The nested levels whose type scopes its rows (`query_nested`), each read with that
        // type's predicate as a level of a composed statement whose root is the keyset page
        // below — the page is cut before any level is joined to it. None: the page is read
        // by the relay adapter, as it always was.
        let nested_reads = self.plan_nested_reads(
            &query_def.return_type,
            &node_fields,
            security_context,
            &selection_access,
        )?;

        // Extract relay pagination arguments from the matcher's merged argument map
        // (#904). Reading the raw request `variables` instead dropped every argument
        // written inline in the document — silently, because the query still returned
        // a page. `where:` is the sharp case: a dropped filter widens the result set.
        // The matcher merges inline arguments under the request variables, variables
        // winning, so this is the same source every other query path reads.
        let args = &query_match.arguments;
        // `nearest` (#386) has no relay lowering; ignoring it here would return
        // an unordered page that reads as a successful similarity search.
        if args.contains_key("nearest") {
            return Err(FraiseQLError::validation(
                "`nearest` is not supported on relay (connection) queries; use a plain \
                 list query for similarity search",
            ));
        }
        // A mistyped cursor window is refused, not dropped (#1197). `first: "2"`
        // used to answer `None` here and fall through to the default page size
        // of 20 below — ten times what the client asked for, under a 200 and
        // with `pageInfo` describing the page it did not request.
        let first = coerce_pagination_arg("first", args.get("first"))?;
        let last = coerce_pagination_arg("last", args.get("last"))?;
        // Cap the requested page size before it reaches SQL (#421: unbounded-pagination DoS guard).
        let first = enforce_max_page_size(first, self.ctx.config.max_page_size, "first")?;
        let last = enforce_max_page_size(last, self.ctx.config.max_page_size, "last")?;
        let after_cursor: Option<&str> = args.get("after").and_then(serde_json::Value::as_str);
        let before_cursor: Option<&str> = args.get("before").and_then(serde_json::Value::as_str);

        // Determine direction and limit.
        // Forward pagination takes priority; fallback to 20 if neither first/last given.
        let (forward, page_size) = if last.is_some() && first.is_none() {
            (false, last.unwrap_or(20))
        } else {
            (true, first.unwrap_or(20))
        };

        // Fetch page_size + 1 rows to detect hasNextPage/hasPreviousPage. Saturating so an
        // unbounded `first` (when max_page_size is disabled) cannot overflow to LIMIT 0.
        let fetch_limit = page_size.saturating_add(1);

        // The client's `where`, refused rather than dropped when this query does not
        // accept one (#1283). A relay query always declares `has_where`, so the refusal
        // is unreachable from here — the call is what keeps this path from being a
        // sixth private copy of the rule when that stops being true.
        let user_where_clause = client_where_argument(
            &self.ctx.schema,
            query_def,
            args,
            self.ctx.config.rls_policy.as_deref(),
            security_context,
        )?;

        // Compose final WHERE: security (RLS + inject) AND user-supplied WHERE.
        // Security conditions always come first so they cannot be bypassed.
        let combined_where = match (security_where, user_where_clause) {
            (None, None) => None,
            (Some(sec), None) => Some(sec),
            (None, Some(user)) => Some(user),
            (Some(sec), Some(user)) => Some(WhereClause::And(vec![sec, user])),
        };

        // Parse optional `orderBy`, enriched with schema type info.
        let order_by = if query_def.auto_params.has_order_by {
            // An explicit `orderBy: null` is the argument's absence (GraphQL §2.9.5).
            args.get("orderBy")
                .filter(|value| !value.is_null())
                .map(OrderByClause::from_graphql_json)
                .transpose()?
                .map(|clauses| {
                    enrich_order_by_clauses(
                        clauses,
                        &self.ctx.schema,
                        &query_def.return_type,
                        &query_def.native_columns,
                        security_context,
                    )
                })
                .transpose()?
        } else {
            None
        };
        // #1521: under an `orderBy` the page resumes past the cursor row's sort-key values,
        // which its cursor carries along with the fingerprint of the ordering they belong to.
        let ordering = {
            let keys = crate::backend::keyset::keyset_keys(order_by.as_deref())?;
            (!keys.is_empty())
                .then(|| ordering_fingerprint(&crate::backend::keyset::keyset_signature(&keys)))
        };
        let decode = |argument: &str, cursor: Option<&str>| {
            cursor
                .map(|s| {
                    decode_cursor(s, &query_def.relay_cursor_type, ordering.as_deref()).map_err(
                        |reason| FraiseQLError::Validation {
                            message: format!("invalid relay cursor for `{argument}`: {reason}"),
                            path:    Some(argument.to_string()),
                        },
                    )
                })
                .transpose()
        };
        let after_pk = decode("after", after_cursor)?;
        let before_pk = decode("before", before_cursor)?;

        // Detect whether the client selected `totalCount` inside the connection.
        // Named fragment spreads are already expanded by the matcher's FragmentResolver.
        // Inline fragments (`... on UserConnection { totalCount }`) remain as FieldSelection
        // entries with a name starting with "..." — we recurse one level into those.
        let include_total_count = query_match
            .selections
            .iter()
            .find(|sel| sel.name == query_def.name)
            .is_some_and(|connection_field| {
                selections_contain_field(&connection_field.nested_fields, "totalCount")
            });

        // Capture before the move into execute_relay_page.
        let had_after = after_pk.is_some();
        let had_before = before_pk.is_some();

        // Pin session variables to the page/count queries' connection so
        // RLS-protected relay pagination returns the correct tenant's rows (#329).
        let (page, result_total_count) = if nested_reads.is_empty() {
            let result = relay
                .execute_relay_page_with_session(
                    sql_source,
                    cursor_column,
                    after_pk,
                    before_pk,
                    fetch_limit,
                    forward,
                    combined_where.as_ref(),
                    order_by.as_deref(),
                    include_total_count,
                    session_vars,
                    query_def.read_routing,
                )
                .await?;
            let total = result.total_count();
            (result.into_rows(), total)
        } else {
            // The same page — cursor, ordering, direction, `fetch_limit` — as the root of a
            // composed read, so each row-gated level carries its type's predicate. An
            // adapter that cannot compose refuses (501) rather than serving them ungated.
            let root = crate::backend::ComposedLevel {
                view:         sql_source.to_string(),
                projection:   None,
                where_clause: combined_where.clone(),
                order_by:     order_by.clone(),
                limit:        Some(fetch_limit),
                offset:       None,
                keyset:       Some(crate::backend::ComposedKeyset {
                    cursor_column: cursor_column.to_string(),
                    cursor: if forward { after_pk } else { before_pk },
                    forward,
                }),
                keys:         super::query_nested::root_keys(&nested_reads),
                embeds:       nested_reads,
            };
            let documents = self
                .execute_composed_document_read(root, session_vars, query_def.read_routing)
                .await?;
            // `totalCount` ignores the cursor (Relay): the relay adapter's own count, asked
            // for with an empty page, so it is the count the flat path returns.
            let total = if include_total_count {
                relay
                    .execute_relay_page_with_session(
                        sql_source,
                        cursor_column,
                        None,
                        None,
                        0,
                        forward,
                        combined_where.as_ref(),
                        order_by.as_deref(),
                        true,
                        session_vars,
                        query_def.read_routing,
                    )
                    .await?
                    .total_count()
            } else {
                None
            };
            (std::sync::Arc::unwrap_or_clone(documents), total)
        };

        // Detect whether there are more pages.
        let has_extra = page.len() > page_size as usize;
        // The adapter returns a page in connection order, so a backward page's extra row,
        // the one past the page, is its first: dropping the last instead lost the row next
        // to the `before` cursor on every backward page that had a previous one.
        let extra = if forward {
            0
        } else {
            page.len().saturating_sub(page_size as usize)
        };
        let rows: Vec<_> = page.into_iter().skip(extra).take(page_size as usize).collect();

        let (has_next_page, has_previous_page) = if forward {
            (has_extra, had_after)
        } else {
            (had_before, has_extra)
        };

        // Build edges: each edge has { cursor, node }.
        let mut edges = Vec::with_capacity(rows.len());
        let mut start_cursor_str: Option<String> = None;
        let mut end_cursor_str: Option<String> = None;

        for (i, mut row) in rows.into_iter().enumerate() {
            // The row's sort-key values, which the adapter adds to the document of a page
            // with an ordering; never part of the node.
            let sort_keys = row
                .data
                .as_object_mut()
                .and_then(|obj| obj.remove(crate::backend::keyset::SORT_KEYS_KEY));
            let data = &row.data;

            let col_val = data.as_object().and_then(|obj| obj.get(cursor_column));

            let position = match query_def.relay_cursor_type {
                CursorType::Int64 => col_val
                    .and_then(|v| v.as_i64())
                    .map(encode_edge_cursor)
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: format!(
                            "Relay query '{}': cursor column '{}' not found or not an integer in \
                             result JSONB. Ensure the view exposes this column inside the `data` object.",
                            query_def.name, cursor_column
                        ),
                        path: None,
                    })?,
                CursorType::Uuid => col_val
                    .and_then(|v| v.as_str())
                    .map(crate::runtime::relay::encode_uuid_cursor)
                    .ok_or_else(|| FraiseQLError::Validation {
                        message: format!(
                            "Relay query '{}': cursor column '{}' not found or not a string in \
                             result JSONB. Ensure the view exposes this column inside the `data` object.",
                            query_def.name, cursor_column
                        ),
                        path: None,
                    })?,
            };

            let cursor_str = match &ordering {
                None => position,
                Some(ordering) => encode_keyset_cursor(&KeysetCursor {
                    version:   KEYSET_CURSOR_VERSION,
                    ordering:  ordering.clone(),
                    sort_keys: sort_key_values(sort_keys, &query_def.name)?,
                    position:  col_val.cloned().unwrap_or_default(),
                }),
            };

            if i == 0 {
                start_cursor_str = Some(cursor_str.clone());
            }
            end_cursor_str = Some(cursor_str.clone());

            let mut node = crate::runtime::project_entity(
                data,
                &query_def.return_type,
                &node_fields,
                &self.ctx.schema,
            );
            selection_access.null_masked(
                &mut node,
                &query_def.return_type,
                &node_fields,
                &self.ctx.schema,
            );
            edges.push(serde_json::json!({
                "cursor": cursor_str,
                "node": node,
            }));
        }

        let page_info = serde_json::json!({
            "hasNextPage": has_next_page,
            "hasPreviousPage": has_previous_page,
            "startCursor": start_cursor_str,
            "endCursor": end_cursor_str,
        });

        let mut connection = serde_json::json!({
            "edges": edges,
            "pageInfo": page_info,
        });

        // Include totalCount when the client requested it and the adapter provided it.
        if include_total_count {
            if let Some(count) = result_total_count {
                connection["totalCount"] = serde_json::json!(count);
            } else {
                connection["totalCount"] = serde_json::Value::Null;
            }
        }

        let response =
            ResultProjector::wrap_in_data_envelope(connection, query_match.response_key());
        Ok(response)
    }

    /// Execute a Relay global `node(id: ID!)` query.
    ///
    /// Decodes the opaque node ID (`base64("TypeName:uuid")`), locates the
    /// appropriate SQL view by searching the compiled schema for a query that
    /// returns that type, and fetches the matching row.
    ///
    /// Returns `{ "data": { "node": <object> } }` on success, or
    /// `{ "data": { "node": null } }` when the object is not found.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` when:
    /// - The `id` argument is missing or malformed
    /// - No SQL view is registered for the requested type
    pub(in super::super) async fn execute_node_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        selections: &[FieldSelection],
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        use crate::{
            backend::{WhereClause, where_clause::WhereOperator},
            graphql::selection_set,
            runtime::relay::decode_node_id,
        };

        // 0. Evaluate `@skip`/`@include` against the request variables. Named fragment spreads were
        //    already expanded at classification time, where the document's fragment definitions are
        //    available — the same split as the mutation path. Before this the node runner evaluated
        //    no directives at all, so `node(id:) { secret @skip(if: true) }` projected and returned
        //    the field.
        let filtered_selections =
            selection_set::filter(selections, &selection_set::variables_map(variables))?;
        let selections: &[FieldSelection] = &filtered_selections;

        // 1. Extract the raw opaque ID. Priority: $variables.id > inline literal in query text.
        let raw_id: String = if let Some(id_val) = variables
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("id"))
            .and_then(|v| v.as_str())
        {
            id_val.to_string()
        } else {
            // Fall back to extracting inline literal, e.g. node(id: "NDI=")
            Self::extract_inline_node_id(query).ok_or_else(|| FraiseQLError::Validation {
                message: "node query: missing or unresolvable 'id' argument".to_string(),
                path:    Some("node.id".to_string()),
            })?
        };

        // 2. Resolve the id to (type_name, uuid). An object's `id` is its bare UUID (the Relay
        //    global id, ADR-0017), so that is what `node(id: x.id)` receives; its type is resolved
        //    among the Node types (#1398). `base64("Type:uuid")` still names the type outright —
        //    the way to refetch an id two Node types share.
        let (type_name, uuid) = if let Some(typed) = decode_node_id(&raw_id) {
            typed
        } else if uuid::Uuid::parse_str(&raw_id).is_ok() {
            match self.resolve_bare_node_type(&raw_id, security_context, variables).await? {
                Some(type_name) => (type_name, raw_id.clone()),
                None => {
                    return Ok(ResultProjector::wrap_in_data_envelope(
                        serde_json::Value::Null,
                        "node",
                    ));
                },
            }
        } else {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "node query: invalid node ID '{raw_id}' — expected an object's id (a UUID) \
                     or base64(\"Type:uuid\")"
                ),
                path:    Some("node.id".to_string()),
            });
        };

        // 2b. #939: the selection set is scoped to the type the opaque id resolved,
        //     so field-existence can only be checked here — the matcher never sees
        //     this path. An undeclared field would otherwise be projected as
        //     `data->>'name'`, i.e. a `null` under a 200.
        crate::graphql::validate_selection_set(&self.ctx.schema, &type_name, selections)?;

        // #423: the Relay `node` lookup has no SecurityContext and emits the entity blob
        // directly; fail closed if the resolved type has any policy-gated field.
        if self.ctx.schema.type_has_gated_field(&type_name) {
            return Err(FraiseQLError::Authorization {
                message:  format!(
                    "Field-level authorization is not enforced on the Relay node path, but type \
                     '{type_name}' declares a policy-gated field"
                ),
                action:   Some("read".to_string()),
                resource: Some(type_name.clone()),
            });
        }

        // 3. Find the SQL view for this type (O(1) index lookup built at startup).
        let sql_source: Arc<str> =
            self.ctx.node_type_index.get(&type_name).cloned().ok_or_else(|| {
                FraiseQLError::Validation {
                    message: format!("node query: no registered SQL view for type '{type_name}'"),
                    path:    Some("node.id".to_string()),
                }
            })?;

        // 3b. Authorization (H2). The Relay `node(id:)` lookup resolves an arbitrary type by
        //     opaque global id, so — like the regular query path — it must apply the backing
        //     query's `requires_role`, RLS, and `inject_params` gates. Without this it is an
        //     IDOR: a leaked node id returns the row with no access control.
        //
        //     The type is exposed by the query that registered its node view (same first-wins
        //     rule as `node_type_index`). A type with no backing read query is not resolvable.
        let node_qdef = self
            .ctx
            .schema
            .queries
            .iter()
            .find(|q| q.return_type == type_name && q.sql_source.is_some())
            .ok_or_else(|| FraiseQLError::Validation {
                message: format!("node query: no registered SQL view for type '{type_name}'"),
                path:    Some("node.id".to_string()),
            })?;

        // requires_role: invisible (enumeration-hiding "not found") unless the context holds it.
        crate::security::role_gate::enforce_requires_role(
            "Query",
            &node_qdef.name,
            node_qdef.requires_role.as_deref(),
            security_context,
        )?;

        // requires_actor (#966): the `node(id:)` lookup is a second door onto the
        // rows its backing query guards, so it inherits that query's actor
        // allow-list exactly as it inherits `requires_role` above.
        crate::security::actor_type::enforce_requires_actor(
            "Query",
            &node_qdef.name,
            &node_qdef.requires_actor,
            security_context,
        )?;

        // Build the security WHERE (RLS ∧ inject_params). Fail closed when a policy is
        // configured but no security context is present: such a type is never resolvable by
        // opaque id without a principal, so return "not found" (null) — never the raw row.
        let security_where = match self.node_scope(node_qdef, security_context)? {
            NodeScope::Readable(scope) => scope,
            NodeScope::Hidden => {
                return Ok(ResultProjector::wrap_in_data_envelope(serde_json::Value::Null, "node"));
            },
        };

        // 3c. Field-level RBAC at every level of the selection, each against its own type,
        //     and each nested level's `requires_role` / `requires_actor` — the classifier
        //     the GraphQL root runs (`query_nested`). Before the read, so a `Reject`
        //     anywhere never reaches the database.
        //
        //     #422: `node` was put to the authorizer at the operation gate, before the id
        //     was decoded. Now the type is known, the read is asked again as what it is.
        if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
            let op = crate::security::AuthzOperation::root(
                crate::security::OperationKind::Query,
                "node",
                Some(type_name.as_str()),
            );
            crate::security::authorizer::enforce_authz(
                authorizer.as_ref(),
                security_context,
                &[op],
                variables,
            )?;
        }
        let selection_access = super::query_nested::SelectionAccess::classify(
            &self.ctx.schema,
            &type_name,
            selections,
            Vec::new(),
            security_context,
            super::query_nested::LevelAuthz::from_config(&self.ctx.config, variables),
        )?;

        // 3d. The nested levels whose type scopes its rows, each read with that type's
        //     predicate. The node is one row by id: the same composed document read as the
        //     GraphQL root, rooted at that row.
        let nested_reads =
            self.plan_nested_reads(&type_name, selections, security_context, &selection_access)?;

        // 4. Build WHERE clause: data->>'id' = uuid, AND'd under the security filter (which always
        //    comes first so it cannot be bypassed).
        let id_where = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::Value::String(uuid),
        };
        let where_clause = match security_where {
            Some(sec) => WhereClause::And(vec![sec, id_where]),
            None => id_where,
        };

        // 5. Read the row (limit 1), pinning session variables to the read's connection so a
        //    PostgreSQL `current_setting()`-backed RLS policy constrains the node lookup the same
        //    way it constrains regular queries and Relay pages (#610). The FraiseQL `rls_policy`
        //    and `inject_params` gates are enforced above as an explicit WHERE filter.
        //
        //    `selections` was reduced above to exactly what the client asked for, and every
        //    branch projects it; there is deliberately no path that returns the untouched row.
        //    The two that used to exist were the reason #827 was an over-disclosure bug rather
        //    than a plain wrong-field-set one: an empty selection left `projection_hint` as
        //    `None`, and a projection-generation failure fell back to the literal `"data"` —
        //    both of which serve every column in the view.
        let resolved_session_vars = self.resolve_session_vars(security_context)?;
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let mut node_value = if nested_reads.is_empty() {
            // Flat: projected in SQL (mirrors the regular query path), then the nested
            // lists the SQL projection left as their stored sub-blob — every stored key,
            // whatever the sub-selection named — re-projected at their own type.
            let typed_fields =
                build_typed_projection_fields(selections, &self.ctx.schema, &type_name);
            let projection_sql = PostgresProjectionGenerator::new()
                .generate_typed_projection_sql(&typed_fields)
                .map_err(|e| FraiseQLError::Internal {
                    message: format!(
                        "node query: could not build a projection for type '{type_name}': {e}"
                    ),
                    source:  None,
                })?;
            let projection_hint = SqlProjectionHint::new(
                self.ctx.adapter.database_type(),
                projection_sql,
                compute_projection_reduction(typed_fields.len()),
            );
            let rows = self
                .ctx
                .adapter
                .execute_with_projection_arc_with_session(
                    &crate::backend::ProjectionRequest {
                        view:          &sql_source,
                        projection:    Some(&projection_hint),
                        where_clause:  Some(&where_clause),
                        order_by:      None,
                        limit:         Some(1),
                        offset:        None,
                        matched_up_to: None,
                    },
                    &session_pairs,
                    node_qdef.read_routing,
                )
                .await?;
            self.charge_node_budget(&rows)?;
            // When the Arc is exclusively owned (uncached path, refcount = 1) the data is
            // moved out; when the cache also holds it, this one row is cloned.
            let mut value = Arc::try_unwrap(rows).map_or_else(
                |arc| arc.first().map_or(serde_json::Value::Null, |row| row.data.clone()),
                |v| v.into_iter().next().map_or(serde_json::Value::Null, |row| row.data),
            );
            crate::runtime::project_nested_lists(
                &mut value,
                &type_name,
                selections,
                &self.ctx.schema,
            );
            crate::runtime::stamp_nested_typenames(
                &mut value,
                &type_name,
                selections,
                &self.ctx.schema,
            );
            value
        } else {
            // Composed: the same document read as the GraphQL root, rooted at this row,
            // its gated levels merged in and projected in Rust at every depth.
            let rows = self
                .execute_composed_document_read(
                    crate::backend::ComposedLevel {
                        view:         sql_source.to_string(),
                        projection:   None,
                        where_clause: Some(where_clause),
                        order_by:     None,
                        limit:        Some(1),
                        offset:       None,
                        keyset:       None,
                        keys:         super::query_nested::root_keys(&nested_reads),
                        embeds:       nested_reads,
                    },
                    &session_pairs,
                    node_qdef.read_routing,
                )
                .await?;
            self.charge_node_budget(&rows)?;
            super::query_nested::project_documents(
                &rows,
                &type_name,
                selections,
                &self.ctx.schema,
                false,
            )
        };

        // 6. Null what field-level RBAC masked, at every level.
        selection_access.null_masked(&mut node_value, &type_name, selections, &self.ctx.schema);

        let response = ResultProjector::wrap_in_data_envelope(node_value, "node");
        Ok(response)
    }

    /// The rows of a Node type's view this caller may read: its backing query's RLS and
    /// `inject_params`, or [`NodeScope::Hidden`] for an anonymous caller on a policy-gated
    /// type. One definition for the `node` lookup and the bare-id probe (#1398).
    fn node_scope(
        &self,
        node_qdef: &crate::schema::QueryDefinition,
        security_context: Option<&SecurityContext>,
    ) -> Result<NodeScope> {
        use crate::backend::WhereClause;

        let scope: Option<WhereClause> = match security_context {
            Some(sc) => {
                let rls = if let Some(ref rls_policy) = self.ctx.config.rls_policy {
                    rls_policy
                        .evaluate(sc, &RlsTarget::query(&node_qdef.name, &node_qdef.return_type))?
                        .map(RlsWhereClause::into_where_clause)
                } else {
                    None
                };
                let mut conditions: Vec<WhereClause> = node_qdef
                    .inject_params
                    .iter()
                    .map(|(col, source)| {
                        let value =
                            resolve_inject_value(col, source, sc, self.ctx.schema.tenant_claim())?;
                        Ok(inject_param_where_clause(col, value, &node_qdef.native_columns))
                    })
                    .collect::<Result<Vec<_>>>()?;
                if let Some(rls) = rls {
                    conditions.insert(0, rls);
                }
                match conditions.len() {
                    0 => None,
                    1 => Some(conditions.remove(0)),
                    _ => Some(WhereClause::And(conditions)),
                }
            },
            None if self.ctx.config.rls_policy.is_some() || !node_qdef.inject_params.is_empty() => {
                // Fail closed: anonymous lookup of a policy-gated type yields nothing.
                return Ok(NodeScope::Hidden);
            },
            None => None,
        };
        Ok(NodeScope::Readable(scope))
    }

    /// The Node type a bare UUID belongs to, among those this caller may read (#1398).
    ///
    /// An object's `id` is its UUID, so `node(id: x.id)` must find `x` without a type
    /// prefix. Every `relay = true` type with a node view is probed for the id under the
    /// gates the lookup itself applies — `requires_role`, `requires_actor`, the
    /// operation authorizer, RLS and `inject_params` — so a type the caller cannot read
    /// contributes nothing, and nothing below reveals that an id exists there.
    ///
    /// Several types can expose one entity (`User` and `UserSummary` over one table);
    /// an id two readable Node types share is refused rather than resolved by guess.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Validation` when the id is ambiguous; a database or authorizer
    /// outage propagates.
    async fn resolve_bare_node_type(
        &self,
        uuid: &str,
        security_context: Option<&SecurityContext>,
        variables: Option<&serde_json::Value>,
    ) -> Result<Option<String>> {
        use crate::backend::{WhereClause, where_clause::WhereOperator};

        let resolved_session_vars = self.resolve_session_vars(security_context)?;
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        let mut probes = Vec::new();
        for type_def in self.ctx.schema.types.iter().filter(|t| t.relay) {
            let type_name = type_def.name.to_string();
            let Some(view) = self.ctx.node_type_index.get(&type_name).cloned() else {
                continue;
            };
            let Some(node_qdef) = self
                .ctx
                .schema
                .queries
                .iter()
                .find(|q| q.return_type == type_name && q.sql_source.is_some())
            else {
                continue;
            };
            // A gate that refuses the caller removes the type from the candidates.
            if crate::security::role_gate::enforce_requires_role(
                "Query",
                &node_qdef.name,
                node_qdef.requires_role.as_deref(),
                security_context,
            )
            .is_err()
                || crate::security::actor_type::enforce_requires_actor(
                    "Query",
                    &node_qdef.name,
                    &node_qdef.requires_actor,
                    security_context,
                )
                .is_err()
            {
                continue;
            }
            if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
                let op = crate::security::AuthzOperation::root(
                    crate::security::OperationKind::Query,
                    "node",
                    Some(type_name.as_str()),
                );
                match crate::security::authorizer::enforce_authz(
                    authorizer.as_ref(),
                    security_context,
                    &[op],
                    variables,
                ) {
                    Ok(()) => {},
                    Err(FraiseQLError::Authorization { .. }) => continue,
                    Err(other) => return Err(other),
                }
            }
            let scope = match self.node_scope(node_qdef, security_context)? {
                NodeScope::Readable(scope) => scope,
                NodeScope::Hidden => continue,
            };
            let id_where = WhereClause::Field {
                path:     vec!["id".to_string()],
                operator: WhereOperator::Eq,
                value:    serde_json::Value::String(uuid.to_string()),
            };
            let where_clause = match scope {
                Some(sec) => WhereClause::And(vec![sec, id_where]),
                None => id_where,
            };
            let routing = node_qdef.read_routing;
            let session_pairs = &session_pairs;
            probes.push(async move {
                let rows = self
                    .ctx
                    .adapter
                    .execute_where_query_arc_with_session(
                        &view,
                        Some(&where_clause),
                        Some(1),
                        None,
                        None,
                        session_pairs,
                        routing,
                    )
                    .await?;
                Ok::<_, FraiseQLError>((!rows.is_empty()).then_some(type_name))
            });
        }

        let mut found: Vec<String> =
            futures::future::try_join_all(probes).await?.into_iter().flatten().collect();
        match found.len() {
            0 => Ok(None),
            1 => Ok(found.pop()),
            _ => {
                found.sort();
                Err(FraiseQLError::Validation {
                    message: format!(
                        "node query: id '{uuid}' identifies objects of more than one type ({}). \
                         Pass base64(\"Type:{uuid}\") to choose the type.",
                        found.join(", ")
                    ),
                    path:    Some("node.id".to_string()),
                })
            },
        }
    }

    /// The response-bytes ceiling, on the `node(id:)` lookup. One row, but one row of a
    /// materialised document is exactly the shape whose size the request cannot predict.
    fn charge_node_budget(&self, rows: &[crate::backend::JsonbValue]) -> Result<()> {
        if let Some(budget) =
            crate::security::ResponseBudget::new(self.ctx.config.max_response_bytes)
        {
            budget.charge_jsonb_rows(rows)?;
        }
        Ok(())
    }
}

/// A client's `after`/`before` cursor as the page it resumes: the row's position, and its
/// sort-key values when the connection is read under an `ordering` (its fingerprint).
///
/// A cursor resumes only the ordering it was issued under (#1521). Under another one, its
/// values would be compared with other keys' and the page would skip and repeat rows, so a
/// mismatch is refused, saying how to recover: request the first page again.
fn decode_cursor(
    cursor: &str,
    cursor_type: &crate::schema::CursorType,
    ordering: Option<&str>,
) -> std::result::Result<crate::backend::RelayCursor, String> {
    use crate::{
        runtime::relay::{
            KEYSET_CURSOR_VERSION, decode_edge_cursor, decode_keyset_cursor, decode_uuid_cursor,
        },
        schema::CursorType,
    };

    const RESTART: &str = "request the first page again, without `after`/`before`";
    let keyset = decode_keyset_cursor(cursor);
    let (position, sort_keys) = match (ordering, keyset) {
        (None, None) => {
            let position = match cursor_type {
                CursorType::Int64 => decode_edge_cursor(cursor).map(CursorValue::Int64),
                CursorType::Uuid => decode_uuid_cursor(cursor).map(CursorValue::Uuid),
            };
            return position
                .map(crate::backend::RelayCursor::at)
                .ok_or_else(|| format!("{cursor:?} is not a cursor of this connection"));
        },
        (None, Some(_)) => {
            return Err(format!(
                "it was issued under an `orderBy`, and this page has none: repeat that \
                 `orderBy`, or {RESTART}"
            ));
        },
        (Some(_), None) => {
            return Err(format!(
                "it was issued without an `orderBy` (or before 2.17), and this page has one: \
                 {RESTART}"
            ));
        },
        (_, Some(keyset)) if keyset.version != KEYSET_CURSOR_VERSION => {
            return Err(format!(
                "its format (version {}) is not one this server reads: {RESTART}",
                keyset.version
            ));
        },
        (Some(ordering), Some(keyset)) if keyset.ordering != ordering => {
            return Err(format!(
                "it was issued under another `orderBy` or locale than this page's: {RESTART}"
            ));
        },
        (Some(_), Some(keyset)) => (keyset.position, keyset.sort_keys),
    };
    let position = match cursor_type {
        CursorType::Int64 => position.as_i64().map(CursorValue::Int64),
        CursorType::Uuid => position.as_str().map(|s| CursorValue::Uuid(s.to_string())),
    }
    .ok_or_else(|| format!("{cursor:?} is not a cursor of this connection"))?;
    Ok(crate::backend::RelayCursor {
        position,
        sort_keys,
    })
}

/// A page row's sort-key values, as the adapter added them to its document.
fn sort_key_values(values: Option<serde_json::Value>, query: &str) -> Result<Vec<Option<String>>> {
    let missing = || FraiseQLError::Database {
        message:    format!(
            "Relay query '{query}': the page row carries no sort-key values to build its cursor \
             from"
        ),
        sql_state:  None,
        constraint: None,
    };
    let serde_json::Value::Array(values) = values.ok_or_else(missing)? else {
        return Err(missing());
    };
    values
        .into_iter()
        .map(|value| match value {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::String(s) => Ok(Some(s)),
            _ => Err(missing()),
        })
        .collect()
}

/// What a connection's `node` selects: the sub-selection of every `node` under every
/// `edges` the connection field selects, inline fragments included. The response always
/// writes `edges` and `node` under those names, so their selections are merged.
fn connection_node_fields(
    query_match: &crate::runtime::matcher::QueryMatch,
) -> Vec<FieldSelection> {
    let connection = query_match
        .selections
        .iter()
        .find(|sel| sel.name == query_match.query_def.name)
        .map_or(&[][..], |sel| sel.nested_fields.as_slice());
    let edges = sub_selections_named(connection, "edges");
    sub_selections_named(&edges, "node")
}

/// The sub-selections of every field named `name`, looking through inline fragments.
fn sub_selections_named(selections: &[FieldSelection], name: &str) -> Vec<FieldSelection> {
    let mut out = Vec::new();
    for sel in selections {
        if sel.name == name {
            out.extend(sel.nested_fields.iter().cloned());
        } else if sel.name.starts_with("...") {
            out.extend(sub_selections_named(&sel.nested_fields, name));
        }
    }
    out
}

/// What a `node` lookup may read of a Node type's view.
enum NodeScope {
    /// Readable, under this security predicate (`None`: unscoped).
    Readable(Option<crate::backend::WhereClause>),
    /// Never resolvable for this caller: an anonymous lookup of a policy-gated type.
    Hidden,
}
