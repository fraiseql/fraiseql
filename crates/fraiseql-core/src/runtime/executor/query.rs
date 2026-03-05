//! Regular and relay query execution.

use crate::{
    db::{
        CursorValue, WhereClause, WhereOperator,
        projection_generator::PostgresProjectionGenerator,
    },
    error::{FraiseQLError, Result},
    schema::SqlProjectionHint,
    security::SecurityContext,
};

use super::{Executor, null_masked_fields, resolve_inject_value};
use crate::{
    db::traits::DatabaseAdapter,
    runtime::{JsonbStrategy, ResultProjector},
};

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute a regular query with row-level security (RLS) filtering.
    ///
    /// This method:
    /// 1. Validates the user's security context (token expiration, etc.)
    /// 2. Evaluates RLS policies to determine what rows the user can access
    /// 3. Composes RLS filters with user-provided WHERE clauses
    /// 4. Passes the composed filter to the database adapter for SQL-level filtering
    ///
    /// RLS filtering happens at the database level, not in Rust, ensuring:
    /// - High performance (database can optimize filters)
    /// - Correct handling of pagination (LIMIT applied after RLS filtering)
    /// - Type-safe composition via WhereClause enum
    pub(super) async fn execute_regular_query_with_security(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        security_context: &SecurityContext,
    ) -> Result<String> {
        // 1. Validate security context (check expiration, etc.)
        if security_context.is_expired() {
            return Err(FraiseQLError::Validation {
                message: "Security token has expired".to_string(),
                path:    Some("request.authorization".to_string()),
            });
        }

        // 2. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

        // 2b. Enforce requires_role — return "not found" (not "forbidden") to prevent enumeration
        if let Some(ref required_role) = query_match.query_def.requires_role {
            if !security_context.roles.iter().any(|r| r == required_role) {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Query '{}' not found in schema",
                        query_match.query_def.name
                    ),
                    path: None,
                });
            }
        }

        // 3. Create execution plan
        let plan = self.planner.plan(&query_match)?;

        // 4. Evaluate RLS policy and build WHERE clause filter
        let rls_where_clause: Option<WhereClause> =
            if let Some(ref rls_policy) = self.config.rls_policy {
                // Evaluate RLS policy with user's security context
                rls_policy.evaluate(security_context, &query_match.query_def.name)?
            } else {
                // No RLS policy configured, allow all access
                None
            };

        // 5. Get SQL source from query definition
        let sql_source =
            query_match
                .query_def
                .sql_source
                .as_ref()
                .ok_or_else(|| FraiseQLError::Validation {
                    message: "Query has no SQL source".to_string(),
                    path:    None,
                })?;

        // 6. Generate SQL projection hint for requested fields (optimization)
        // Strategy selection: Project (extract fields) vs Stream (return full JSONB)
        let projection_hint = if !plan.projection_fields.is_empty()
            && plan.jsonb_strategy == JsonbStrategy::Project
        {
            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_projection_sql(&plan.projection_fields)
                .unwrap_or_else(|_| "data".to_string());

            Some(SqlProjectionHint {
                database:                    "postgresql".to_string(),
                projection_template:         projection_sql,
                estimated_reduction_percent: 50,
            })
        } else {
            // Stream strategy: return full JSONB, no projection hint
            None
        };

        // 7. AND inject conditions onto the RLS WHERE clause.
        //    Inject conditions always come after RLS so they cannot bypass it.
        let combined_where: Option<WhereClause> =
            if query_match.query_def.inject_params.is_empty() {
                rls_where_clause // common path: no-op
            } else {
                let mut conditions: Vec<WhereClause> = query_match
                    .query_def
                    .inject_params
                    .iter()
                    .map(|(col, source)| {
                        let value = resolve_inject_value(col, source, security_context)?;
                        Ok(WhereClause::Field {
                            path:     vec![col.clone()],
                            operator: WhereOperator::Eq,
                            value,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                if let Some(rls) = rls_where_clause {
                    conditions.insert(0, rls);
                }
                match conditions.len() {
                    0 => None,
                    1 => Some(conditions.remove(0)),
                    _ => Some(WhereClause::And(conditions)),
                }
            };

        // 8. Execute query with combined WHERE clause filter
        let results = self
            .adapter
            .execute_with_projection(
                sql_source,
                projection_hint.as_ref(),
                combined_where.as_ref(),
                None,
            )
            .await?;

        // 9. Apply field-level RBAC filtering (reject / mask / allow)
        let access = self.apply_field_rbac_filtering(
            &query_match.query_def.return_type,
            plan.projection_fields,
            security_context,
        )?;

        // 10. Project results — include both allowed and masked fields in projection
        let mut all_projection_fields = access.allowed;
        all_projection_fields.extend(access.masked.iter().cloned());
        let projector = ResultProjector::new(all_projection_fields);
        let mut projected =
            projector.project_results(&results, query_match.query_def.returns_list)?;

        // 11. Null out masked fields in the projected result
        if !access.masked.is_empty() {
            null_masked_fields(&mut projected, &access.masked);
        }

        // 12. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 13. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }

    pub(super) async fn execute_regular_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // 1. Match query to compiled template
        let query_match = self.matcher.match_query(query, variables)?;

        // Guard: role-restricted queries are invisible to unauthenticated users
        if query_match.query_def.requires_role.is_some() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Query '{}' not found in schema",
                    query_match.query_def.name
                ),
                path: None,
            });
        }

        // Guard: queries with inject params require a security context.
        if !query_match.query_def.inject_params.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Query '{}' has inject params but was called without a security context",
                    query_match.query_def.name
                ),
                path: None,
            });
        }

        // Route relay queries to dedicated handler.
        if query_match.query_def.relay {
            return self.execute_relay_query(&query_match, variables).await;
        }

        // 2. Create execution plan
        let plan = self.planner.plan(&query_match)?;

        // 3. Execute SQL query
        let sql_source = query_match.query_def.sql_source.as_ref().ok_or_else(|| {
            crate::error::FraiseQLError::Validation {
                message: "Query has no SQL source".to_string(),
                path:    None,
            }
        })?;

        // 3a. Generate SQL projection hint for requested fields (optimization)
        // Strategy selection: Project (extract fields) vs Stream (return full JSONB)
        // This reduces payload by 40-55% by projecting only requested fields at the database level
        let projection_hint = if !plan.projection_fields.is_empty()
            && plan.jsonb_strategy == JsonbStrategy::Project
        {
            let generator = PostgresProjectionGenerator::new();
            let projection_sql = generator
                .generate_projection_sql(&plan.projection_fields)
                .unwrap_or_else(|_| "data".to_string());

            Some(SqlProjectionHint {
                database:                    "postgresql".to_string(),
                projection_template:         projection_sql,
                estimated_reduction_percent: 50,
            })
        } else {
            // Stream strategy: return full JSONB, no projection hint
            None
        };

        let results = self
            .adapter
            .execute_with_projection(sql_source, projection_hint.as_ref(), None, None)
            .await?;

        // 4. Project results
        let projector = ResultProjector::new(plan.projection_fields);
        let projected = projector.project_results(&results, query_match.query_def.returns_list)?;

        // 5. Wrap in GraphQL data envelope
        let response =
            ResultProjector::wrap_in_data_envelope(projected, &query_match.query_def.name);

        // 6. Serialize to JSON string
        Ok(serde_json::to_string(&response)?)
    }

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
    pub(super) async fn execute_relay_query(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        use crate::{
            compiler::aggregation::OrderByClause,
            runtime::relay::{decode_edge_cursor, decode_uuid_cursor, encode_edge_cursor},
            schema::CursorType,
        };

        let query_def = &query_match.query_def;

        let sql_source = query_def.sql_source.as_deref().ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!(
                    "Relay query '{}' has no sql_source configured",
                    query_def.name
                ),
                path: None,
            }
        })?;

        let cursor_column = query_def.relay_cursor_column.as_deref().ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!(
                    "Relay query '{}' has no relay_cursor_column derived",
                    query_def.name
                ),
                path: None,
            }
        })?;

        // Guard: relay pagination requires the executor to have been constructed
        // via `Executor::new_with_relay` with a `RelayDatabaseAdapter`.
        let relay = self.relay.as_ref().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Relay pagination is not supported by the {} adapter. \
                 Use a relay-capable adapter (e.g. PostgreSQL) and construct \
                 the executor with `Executor::new_with_relay`.",
                self.adapter.database_type()
            ),
            path: None,
        })?;

        // Extract relay pagination arguments from variables.
        let vars = variables.and_then(|v| v.as_object());
        let first: Option<u32> = vars
            .and_then(|v| v.get("first"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let last: Option<u32> = vars
            .and_then(|v| v.get("last"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let after_cursor: Option<&str> =
            vars.and_then(|v| v.get("after")).and_then(|v| v.as_str());
        let before_cursor: Option<&str> =
            vars.and_then(|v| v.get("before")).and_then(|v| v.as_str());

        // Decode base64 cursors — type depends on relay_cursor_type.
        // If a cursor string is provided but fails to decode, return a validation
        // error immediately. Silently ignoring an invalid cursor would return a
        // full result set, violating the client's pagination intent.
        let (after_pk, before_pk) = match query_def.relay_cursor_type {
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
                    Some(s) => Some(decode_uuid_cursor(s).map(CursorValue::Uuid).ok_or_else(
                        || FraiseQLError::Validation {
                            message: format!("invalid relay cursor for `after`: {s:?}"),
                            path:    Some("after".to_string()),
                        },
                    )?),
                    None => None,
                };
                let before = match before_cursor {
                    Some(s) => Some(decode_uuid_cursor(s).map(CursorValue::Uuid).ok_or_else(
                        || FraiseQLError::Validation {
                            message: format!("invalid relay cursor for `before`: {s:?}"),
                            path:    Some("before".to_string()),
                        },
                    )?),
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
        let where_clause = if query_def.auto_params.has_where {
            vars.and_then(|v| v.get("where"))
                .map(WhereClause::from_graphql_json)
                .transpose()?
        } else {
            None
        };

        // Parse optional `orderBy` from variables.
        let order_by = if query_def.auto_params.has_order_by {
            vars.and_then(|v| v.get("orderBy"))
                .map(OrderByClause::from_graphql_json)
                .transpose()?
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
            .map(|connection_field| selections_contain_field(&connection_field.nested_fields, "totalCount"))
            .unwrap_or(false);

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
                where_clause.as_ref(),
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
        Ok(serde_json::to_string(&response)?)
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
    pub(super) async fn execute_node_query(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        use crate::{
            db::{WhereClause, where_clause::WhereOperator},
            runtime::relay::decode_node_id,
        };

        // 1. Extract the raw opaque ID.
        //    Priority: $variables.id > inline literal in query text.
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
        let (type_name, uuid) = decode_node_id(&raw_id).ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!("node query: invalid node ID '{raw_id}'"),
                path:    Some("node.id".to_string()),
            }
        })?;

        // 3. Find the SQL view for this type.
        //    Convention: look for the first query whose return_type matches.
        let sql_source = self
            .schema
            .queries
            .iter()
            .find(|q| q.return_type == type_name && q.sql_source.is_some())
            .and_then(|q| q.sql_source.as_deref())
            .ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "node query: no registered SQL view for type '{type_name}'"
                ),
                path: Some("node.id".to_string()),
            })?
            .to_string();

        // 4. Build WHERE clause: data->>'id' = uuid
        let where_clause = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::Value::String(uuid),
        };

        // 5. Execute the query (limit 1).
        let rows = self
            .adapter
            .execute_where_query(&sql_source, Some(&where_clause), Some(1), None)
            .await?;

        // 6. Return the first matching row (or null).
        let node_value = rows
            .into_iter()
            .next()
            .map(|row| row.data)
            .unwrap_or(serde_json::Value::Null);

        let response = ResultProjector::wrap_in_data_envelope(node_value, "node");
        Ok(serde_json::to_string(&response)?)
    }
}

/// Return `true` if `field_name` appears in `selections`, including inside inline
/// fragment entries (`FieldSelection` whose name starts with `"..."`).
///
/// Named fragment spreads are already flattened by [`FragmentResolver`] before this
/// is called, so we only need to recurse one level into inline fragments.
fn selections_contain_field(selections: &[crate::graphql::FieldSelection], field_name: &str) -> bool {
    for sel in selections {
        if sel.name == field_name {
            return true;
        }
        // Inline fragment: name starts with "..." (e.g. "...on UserConnection")
        if sel.name.starts_with("...") && selections_contain_field(&sel.nested_fields, field_name) {
            return true;
        }
    }
    false
}
