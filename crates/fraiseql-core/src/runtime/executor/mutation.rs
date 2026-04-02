//! Mutation execution.

use super::{Executor, resolve_inject_value};
use crate::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    error::{FraiseQLError, Result},
    runtime::{
        ResultProjector,
        mutation_result::{MutationOutcome, parse_mutation_row, populate_error_fields},
        suggest_similar,
    },
    security::SecurityContext,
};

/// Compile-time enforcement: `SqliteAdapter` must NOT implement `SupportsMutations`.
///
/// Calling `execute_mutation` on an `Executor<SqliteAdapter>` must not compile
/// because `SqliteAdapter` does not implement the `SupportsMutations` marker trait.
///
/// ```compile_fail
/// use fraiseql_core::runtime::Executor;
/// use fraiseql_core::db::sqlite::SqliteAdapter;
/// use fraiseql_core::schema::CompiledSchema;
/// use std::sync::Arc;
/// async fn _wont_compile() {
///     let adapter = Arc::new(SqliteAdapter::new_in_memory().await.unwrap());
///     let executor = Executor::new(CompiledSchema::new(), adapter);
///     executor.execute_mutation("createUser", None).await.unwrap();
/// }
/// ```
impl<A: DatabaseAdapter + SupportsMutations> Executor<A> {
    /// Execute a GraphQL mutation directly, with compile-time capability enforcement.
    ///
    /// Unlike `execute()` (which accepts raw GraphQL strings and performs a runtime
    /// `supports_mutations()` check), this method is only available on adapters that
    /// implement [`SupportsMutations`].  The capability is enforced at **compile time**:
    /// attempting to call this method with `SqliteAdapter` results in a compiler error.
    ///
    /// # Arguments
    ///
    /// * `mutation_name` - The GraphQL mutation field name (e.g. `"createUser"`)
    /// * `variables` - Optional JSON object of GraphQL variable values
    ///
    /// # Returns
    ///
    /// A JSON-encoded GraphQL response string on success.
    ///
    /// # Errors
    ///
    /// Same as `execute_mutation_query`, minus the adapter capability check.
    pub async fn execute_mutation(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // No runtime supports_mutations() check: the SupportsMutations bound
        // guarantees at compile time that this adapter supports mutations.
        self.execute_mutation_query_with_security(mutation_name, variables, None).await
    }
}

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute a GraphQL mutation by calling the configured database function.
    ///
    /// Looks up the `MutationDefinition` in the compiled schema, calls
    /// `execute_function_call` on the database adapter, parses the returned
    /// `mutation_response` row, and builds a GraphQL response containing either the
    /// success entity or a populated error-type object (when the function returns a
    /// `"failed:*"` / `"conflict:*"` / `"error"` status).
    ///
    /// This is the **unauthenticated** variant. It delegates to
    /// `execute_mutation_query_with_security` with `security_ctx = None`, which means
    /// any `inject` params on the mutation definition will cause a
    /// [`FraiseQLError::Validation`] error at runtime (inject requires a security
    /// context).
    ///
    /// # Arguments
    ///
    /// * `mutation_name` - The GraphQL mutation field name (e.g. `"createUser"`)
    /// * `variables` - Optional JSON object of GraphQL variable values
    ///
    /// # Returns
    ///
    /// A JSON-encoded GraphQL response string on success.
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — mutation name not found in the compiled schema
    /// * [`FraiseQLError::Validation`] — mutation definition has no `sql_source` configured
    /// * [`FraiseQLError::Validation`] — mutation requires `inject` params (needs security ctx)
    /// * [`FraiseQLError::Validation`] — the database function returned no rows
    /// * [`FraiseQLError::Database`] — the adapter's `execute_function_call` returned an error
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: live database adapter with SupportsMutations implementation.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # use fraiseql_core::runtime::Executor;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let schema: CompiledSchema = unimplemented!();
    /// # let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
    /// # let executor = Executor::new(schema, Arc::new(adapter));
    /// let vars = serde_json::json!({ "name": "Alice", "email": "alice@example.com" });
    /// // Returns {"data":{"createUser":{"id":"...", "name":"Alice"}}}
    /// // or      {"data":{"createUser":{"__typename":"UserAlreadyExistsError", "email":"..."}}}
    /// let result = executor.execute_mutation("createUser", Some(&vars)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub(super) async fn execute_mutation_query(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Runtime guard: verify this adapter supports mutations.
        // Note: this is a runtime check, not compile-time enforcement.
        // The common execute() entry point accepts raw GraphQL strings and
        // determines the operation type at runtime, which precludes compile-time
        // mutation gating. A future API revision (separate execute_mutation() method)
        // would move this to a compile-time bound (see roadmap.md).
        if !self.adapter.supports_mutations() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Mutation '{mutation_name}' cannot be executed: the configured database \
                     adapter does not support mutations. Use PostgresAdapter, MySqlAdapter, \
                     or SqlServerAdapter for mutation operations."
                ),
                path:    None,
            });
        }
        self.execute_mutation_query_with_security(mutation_name, variables, None).await
    }

    /// Internal implementation shared by `execute_mutation_query` and the
    /// security-aware path in `execute_with_security_internal`.
    ///
    /// Callers provide an optional [`SecurityContext`]:
    /// - `None` — unauthenticated path; mutations with `inject` params will fail.
    /// - `Some(ctx)` — authenticated path; `inject` param values are resolved from `ctx`'s JWT
    ///   claims and appended to the positional argument list after the client-supplied variables.
    ///
    /// # Arguments
    ///
    /// * `mutation_name` - The GraphQL mutation field name (e.g. `"deletePost"`)
    /// * `variables` - Optional JSON object of client-supplied variable values
    /// * `security_ctx` - Optional authenticated user context; required when the mutation
    ///   definition has one or more `inject` params
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — mutation not found, no `sql_source`, missing security
    ///   context for `inject` params, or database function returned no rows.
    /// * [`FraiseQLError::Database`] — the adapter's `execute_function_call` failed.
    pub(super) async fn execute_mutation_query_with_security(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        security_ctx: Option<&SecurityContext>,
    ) -> Result<String> {
        // 1. Locate the mutation definition
        let mutation_def = self.schema.find_mutation(mutation_name).ok_or_else(|| {
            let candidates: Vec<&str> =
                self.schema.mutations.iter().map(|m| m.name.as_str()).collect();
            let suggestion = suggest_similar(mutation_name, &candidates);
            let message = match suggestion.as_slice() {
                [s] => {
                    format!("Mutation '{mutation_name}' not found in schema. Did you mean '{s}'?")
                },
                [a, b] => format!(
                    "Mutation '{mutation_name}' not found in schema. Did you mean '{a}' or \
                         '{b}'?"
                ),
                [a, b, c, ..] => format!(
                    "Mutation '{mutation_name}' not found in schema. Did you mean '{a}', \
                         '{b}', or '{c}'?"
                ),
                _ => format!("Mutation '{mutation_name}' not found in schema"),
            };
            FraiseQLError::Validation {
                message,
                path: None,
            }
        })?;

        // 2. Require a sql_source (PostgreSQL function name).
        //
        // Fall back to the operation's table field when sql_source is absent.
        // The CLI compiler stores the SQL function name in both places
        // (sql_source and operation.{Insert|Update|Delete}.table), but older or
        // alternate compilation paths (e.g. fraiseql-core's own codegen) may only
        // populate operation.table and leave sql_source as None.
        let sql_source_owned: String;
        let sql_source: &str = if let Some(src) = mutation_def.sql_source.as_deref() {
            src
        } else {
            use crate::schema::MutationOperation;
            match &mutation_def.operation {
                MutationOperation::Insert { table }
                | MutationOperation::Update { table }
                | MutationOperation::Delete { table }
                    if !table.is_empty() =>
                {
                    sql_source_owned = table.clone();
                    &sql_source_owned
                },
                _ => {
                    return Err(FraiseQLError::Validation {
                        message: format!("Mutation '{mutation_name}' has no sql_source configured"),
                        path:    None,
                    });
                },
            }
        };

        // 3. Build positional args Vec from variables in ArgumentDefinition order. Validate that
        //    every required (non-nullable, no default) argument is present.
        let vars_obj = variables.and_then(|v| v.as_object());

        let mut missing_required: Vec<&str> = Vec::new();
        let total_args = mutation_def.arguments.len() + mutation_def.inject_params.len();
        let mut args: Vec<serde_json::Value> = Vec::with_capacity(total_args);
        args.extend(mutation_def.arguments.iter().map(|arg| {
            let value = vars_obj.and_then(|obj| obj.get(&arg.name)).cloned();
            if let Some(v) = value {
                v
            } else {
                if !arg.nullable && arg.default_value.is_none() {
                    missing_required.push(&arg.name);
                }
                arg.default_value.as_ref().map_or(serde_json::Value::Null, |v| v.to_json())
            }
        }));

        if !missing_required.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Mutation '{mutation_name}' is missing required argument(s): {}",
                    missing_required.join(", ")
                ),
                path:    None,
            });
        }

        // 3a. Append server-injected parameters (after client args, in injection order).
        //
        // CONTRACT: inject params are always the *last* positional parameters of the SQL
        // function, in the order they appear in `inject_params` (insertion-ordered IndexMap).
        // The SQL function signature in the database MUST declare injected parameters after
        // all client-supplied parameters. Violating this order silently passes inject values
        // to the wrong SQL parameters. The CLI compiler (`fraiseql-cli compile`) validates
        // inject key names and source syntax when producing `schema.compiled.json`, but
        // cannot verify SQL function arity — that remains a developer responsibility.
        if !mutation_def.inject_params.is_empty() {
            let ctx = security_ctx.ok_or_else(|| FraiseQLError::Validation {
                message: format!(
                    "Mutation '{}' requires inject params but no security context is available \
                     (unauthenticated request)",
                    mutation_name
                ),
                path:    None,
            })?;
            for (param_name, source) in &mutation_def.inject_params {
                args.push(resolve_inject_value(param_name, source, ctx)?);
            }
        }

        // 4. Call the database function
        let rows = self.adapter.execute_function_call(sql_source, &args).await?;

        // 5. Expect at least one row
        let row = rows.into_iter().next().ok_or_else(|| FraiseQLError::Validation {
            message: format!("Mutation '{mutation_name}': function returned no rows"),
            path:    None,
        })?;

        // 6. Parse the mutation_response row
        let outcome = parse_mutation_row(&row)?;

        // 6a. Bump fact table versions after a successful mutation.
        //
        // This invalidates cached aggregation results for any fact tables listed
        // in `MutationDefinition.invalidates_fact_tables`.  We bump versions on
        // Success only — an Error outcome means no data was written, so caches
        // remain valid.  Non-cached adapters return Ok(()) from the default trait
        // implementation (no-op); only `CachedDatabaseAdapter` performs actual work.
        if matches!(outcome, MutationOutcome::Success { .. })
            && !mutation_def.invalidates_fact_tables.is_empty()
        {
            self.adapter
                .bump_fact_table_versions(&mutation_def.invalidates_fact_tables)
                .await?;
        }

        // Invalidate query result cache for views/entities touched by this mutation.
        //
        // Strategy:
        // - UPDATE/DELETE with entity_id: entity-aware eviction only (precise, no false positives).
        //   Evicts only the cache entries that actually contain the mutated entity UUID.
        // - CREATE or explicit invalidates_views: view-level flush. For CREATE the new entity isn't
        //   in any existing cache entry, so entity-aware is a no-op. View-level ensures list
        //   queries return the new row.
        // - No entity_id and no views declared: infer view from return type (backward-compat).
        if let MutationOutcome::Success {
            entity_type,
            entity_id,
            ..
        } = &outcome
        {
            // Entity-aware path: precise eviction for UPDATE/DELETE.
            if let (Some(etype), Some(eid)) = (entity_type.as_deref(), entity_id.as_deref()) {
                self.adapter.invalidate_by_entity(etype, eid).await?;
            }

            // View-level path: needed when entity_id is absent (CREATE) or when the developer
            // explicitly declared invalidates_views to also refresh list queries.
            if entity_id.is_none() || !mutation_def.invalidates_views.is_empty() {
                let views_to_invalidate = if mutation_def.invalidates_views.is_empty() {
                    self.schema
                        .types
                        .iter()
                        .find(|t| t.name == mutation_def.return_type)
                        .filter(|t| !t.sql_source.as_str().is_empty())
                        .map(|t| t.sql_source.to_string())
                        .into_iter()
                        .collect::<Vec<_>>()
                } else {
                    mutation_def.invalidates_views.clone()
                };
                if !views_to_invalidate.is_empty() {
                    self.adapter.invalidate_views(&views_to_invalidate).await?;
                }
            }
        }

        // Clone name and return_type to avoid borrow issues after schema lookups
        let mutation_return_type = mutation_def.return_type.clone();
        let mutation_name_owned = mutation_name.to_string();

        let result_json = match outcome {
            MutationOutcome::Success {
                entity,
                entity_type,
                ..
            } => {
                // Determine the GraphQL __typename
                let typename = entity_type
                    .or_else(|| {
                        // Fall back to first non-error union member
                        self.schema
                            .find_union(&mutation_return_type)
                            .and_then(|u| {
                                u.member_types.iter().find(|t| {
                                    self.schema.find_type(t).is_none_or(|td| !td.is_error)
                                })
                            })
                            .cloned()
                    })
                    .unwrap_or_else(|| mutation_return_type.clone());

                let mut obj = entity.as_object().cloned().unwrap_or_default();
                obj.insert("__typename".to_string(), serde_json::Value::String(typename));
                serde_json::Value::Object(obj)
            },
            MutationOutcome::Error {
                status, metadata, ..
            } => {
                // Find the matching error type from the return union
                let error_type = self.schema.find_union(&mutation_return_type).and_then(|u| {
                    u.member_types.iter().find_map(|t| {
                        let td = self.schema.find_type(t)?;
                        if td.is_error { Some(td) } else { None }
                    })
                });

                match error_type {
                    Some(td) => {
                        let mut fields = populate_error_fields(&td.fields, &metadata);
                        fields.insert(
                            "__typename".to_string(),
                            serde_json::Value::String(td.name.to_string()),
                        );
                        // Include status so the client can act on it
                        fields.insert("status".to_string(), serde_json::Value::String(status));
                        serde_json::Value::Object(fields)
                    },
                    None => {
                        // No error type defined: surface the status as a plain object
                        serde_json::json!({ "__typename": mutation_return_type, "status": status })
                    },
                }
            },
        };

        let response = ResultProjector::wrap_in_data_envelope(result_json, &mutation_name_owned);
        Ok(serde_json::to_string(&response)?)
    }
}
