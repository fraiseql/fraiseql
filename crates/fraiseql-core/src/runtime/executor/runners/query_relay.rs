//! Relay connection and node query execution methods for [`QueryRunner`].

use std::sync::Arc;

use super::super::resolve_inject_value;
use super::query::QueryRunner;
use super::query_params::{compute_projection_reduction, inject_param_where_clause};
use super::query_projection::{build_typed_projection_fields, enrich_order_by_clauses, selections_contain_field};
use crate::{
    db::{
        CursorValue, WhereClause,
        projection_generator::PostgresProjectionGenerator,
        traits::DatabaseAdapter,
    },
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::ResultProjector,
    schema::SqlProjectionHint,
    security::{RlsWhereClause, SecurityContext},
};

impl<A: DatabaseAdapter> QueryRunner<A> {
    /// Execute a Relay connection query with cursor-based (keyset) pagination.
    ///
    /// Reads `first`, `after`, `last`, `before` from `variables`, fetches a page
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
    ) -> Result<serde_json::Value> {
        use crate::{
            compiler::aggregation::OrderByClause,
            runtime::relay::{decode_edge_cursor, decode_uuid_cursor, encode_edge_cursor},
            schema::CursorType,
        };

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
        let rls_where_clause: Option<RlsWhereClause> = if let (Some(ref rls_policy), Some(ctx)) =
            (&self.ctx.config.rls_policy, security_context)
        {
            rls_policy.evaluate(ctx, &query_def.name)?
        } else {
            None
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
                    let value = resolve_inject_value(col, source, ctx)?;
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

        // Extract relay pagination arguments from variables.
        let vars = variables.and_then(|v| v.as_object());
        let first: Option<u32> = vars
            .and_then(|v| v.get("first"))
            .and_then(|v| v.as_u64())
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
        let last: Option<u32> = vars
            .and_then(|v| v.get("last"))
            .and_then(|v| v.as_u64())
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX));
        let after_cursor: Option<&str> = vars.and_then(|v| v.get("after")).and_then(|v| v.as_str());
        let before_cursor: Option<&str> =
            vars.and_then(|v| v.get("before")).and_then(|v| v.as_str());

        // Decode base64 cursors — type depends on relay_cursor_type.
        // If a cursor string is provided but fails to decode, return a validation
        // error immediately. Silently ignoring an invalid cursor would return a
        // full result set, violating the client's pagination intent.
        let (after_pk, before_pk) =
            match query_def.relay_cursor_type {
                CursorType::Int64 => {
                    let after = match after_cursor {
                        Some(s) => Some(decode_edge_cursor(s).map(CursorValue::Int64).ok_or_else(
                            || FraiseQLError::Validation {
                                message: format!("invalid relay cursor for `after`: {s:?}"),
                                path:    Some("after".to_string()),
                            },
                        )?),
                        None => None,
                    };
                    let before = match before_cursor {
                        Some(s) => Some(decode_edge_cursor(s).map(CursorValue::Int64).ok_or_else(
                            || FraiseQLError::Validation {
                                message: format!("invalid relay cursor for `before`: {s:?}"),
                                path:    Some("before".to_string()),
                            },
                        )?),
                        None => None,
                    };
                    (after, before)
                },
                CursorType::Uuid => {
                    let after = match after_cursor {
                        Some(s) => {
                            Some(decode_uuid_cursor(s).map(CursorValue::Uuid).ok_or_else(|| {
                                FraiseQLError::Validation {
                                    message: format!("invalid relay cursor for `after`: {s:?}"),
                                    path:    Some("after".to_string()),
                                }
                            })?)
                        },
                        None => None,
                    };
                    let before = match before_cursor {
                        Some(s) => {
                            Some(decode_uuid_cursor(s).map(CursorValue::Uuid).ok_or_else(|| {
                                FraiseQLError::Validation {
                                    message: format!("invalid relay cursor for `before`: {s:?}"),
                                    path:    Some("before".to_string()),
                                }
                            })?)
                        },
                        None => None,
                    };
                    (after, before)
                },
            };

        // Determine direction and limit.
        // Forward pagination takes priority; fallback to 20 if neither first/last given.
        let (forward, page_size) = if last.is_some() && first.is_none() {
            (false, last.unwrap_or(20))
        } else {
            (true, first.unwrap_or(20))
        };

        // Fetch page_size + 1 rows to detect hasNextPage/hasPreviousPage.
        let fetch_limit = page_size + 1;

        // Parse optional `where` filter from variables.
        let user_where_clause = if query_def.auto_params.has_where {
            vars.and_then(|v| v.get("where"))
                .map(WhereClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        // Compose final WHERE: security (RLS + inject) AND user-supplied WHERE.
        // Security conditions always come first so they cannot be bypassed.
        let combined_where = match (security_where, user_where_clause) {
            (None, None) => None,
            (Some(sec), None) => Some(sec),
            (None, Some(user)) => Some(user),
            (Some(sec), Some(user)) => Some(WhereClause::And(vec![sec, user])),
        };

        // Parse optional `orderBy` from variables, enriched with schema type info.
        let order_by = if query_def.auto_params.has_order_by {
            vars.and_then(|v| v.get("orderBy"))
                .map(OrderByClause::from_graphql_json)
                .transpose()?
                .map(|clauses| {
                    enrich_order_by_clauses(
                        clauses,
                        &self.ctx.schema,
                        &query_def.return_type,
                        &query_def.native_columns,
                    )
                })
        } else {
            None
        };

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

        let result = relay
            .execute_relay_page(
                sql_source,
                cursor_column,
                after_pk,
                before_pk,
                fetch_limit,
                forward,
                combined_where.as_ref(),
                order_by.as_deref(),
                include_total_count,
            )
            .await?;

        // Detect whether there are more pages.
        let has_extra = result.rows.len() > page_size as usize;
        let rows: Vec<_> = result.rows.into_iter().take(page_size as usize).collect();

        let (has_next_page, has_previous_page) = if forward {
            (has_extra, had_after)
        } else {
            (had_before, has_extra)
        };

        // Build edges: each edge has { cursor, node }.
        let mut edges = Vec::with_capacity(rows.len());
        let mut start_cursor_str: Option<String> = None;
        let mut end_cursor_str: Option<String> = None;

        for (i, row) in rows.iter().enumerate() {
            let data = &row.data;

            let col_val = data.as_object().and_then(|obj| obj.get(cursor_column));

            let cursor_str = match query_def.relay_cursor_type {
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

            if i == 0 {
                start_cursor_str = Some(cursor_str.clone());
            }
            end_cursor_str = Some(cursor_str.clone());

            edges.push(serde_json::json!({
                "cursor": cursor_str,
                "node": data,
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
            if let Some(count) = result.total_count {
                connection["totalCount"] = serde_json::json!(count);
            } else {
                connection["totalCount"] = serde_json::Value::Null;
            }
        }

        let response = ResultProjector::wrap_in_data_envelope(connection, &query_def.name);
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
    ) -> Result<serde_json::Value> {
        use crate::{
            db::{WhereClause, where_clause::WhereOperator},
            runtime::relay::decode_node_id,
        };

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

        // 2. Decode base64("TypeName:uuid") → (type_name, uuid).
        let (type_name, uuid) =
            decode_node_id(&raw_id).ok_or_else(|| FraiseQLError::Validation {
                message: format!("node query: invalid node ID '{raw_id}'"),
                path:    Some("node.id".to_string()),
            })?;

        // 3. Find the SQL view for this type (O(1) index lookup built at startup).
        let sql_source: Arc<str> =
            self.ctx.node_type_index.get(&type_name).cloned().ok_or_else(|| {
                FraiseQLError::Validation {
                    message: format!("node query: no registered SQL view for type '{type_name}'"),
                    path:    Some("node.id".to_string()),
                }
            })?;

        // 4. Build WHERE clause: data->>'id' = uuid
        let where_clause = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::Value::String(uuid),
        };

        // 5. Build projection hint from selections (mirrors regular query path).
        let projection_hint = if !selections.is_empty() {
            let typed_fields =
                build_typed_projection_fields(selections, &self.ctx.schema, &type_name, 0);
            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_typed_projection_sql(&typed_fields)
                .unwrap_or_else(|_| "data".to_string());
            Some(SqlProjectionHint::new(
                self.ctx.adapter.database_type(),
                projection_sql,
                compute_projection_reduction(typed_fields.len()),
            ))
        } else {
            None
        };

        // 6. Execute the query (limit 1) with projection.
        let rows = self
            .ctx
            .adapter
            .execute_with_projection_arc(
                &sql_source,
                projection_hint.as_ref(),
                Some(&where_clause),
                Some(1),
                None,
                None,
            )
            .await?;

        // 7. Return the first matching row (or null).
        // When the Arc is exclusively owned (uncached path, refcount = 1) we can move the
        // data out without copying.  When the cache also holds a reference (refcount ≥ 2)
        // we clone the single `serde_json::Value` for this one-row lookup.
        let node_value = Arc::try_unwrap(rows).map_or_else(
            |arc| arc.first().map_or(serde_json::Value::Null, |row| row.data.clone()),
            |v| v.into_iter().next().map_or(serde_json::Value::Null, |row| row.data),
        );

        let response = ResultProjector::wrap_in_data_envelope(node_value, "node");
        Ok(response)
    }
}
