//! Mutation execution — thin wrappers on `Executor<A>`.
//!
//! The core mutation logic lives in
//! [`runners::mutation::execute_mutation_impl`](super::runners::mutation::execute_mutation_impl).
//! This module contains:
//!
//! - The compile-time-enforced public API ([`Executor::execute_mutation`], bounded on
//!   [`SupportsMutations`]).
//! - The runtime-guarded internal dispatch entry point (`Executor::execute_mutation_query`,
//!   bounded only on [`DatabaseAdapter`]).
//! - Convenience wrappers used by the REST transport ([`execute_mutation_with_security`],
//!   [`execute_mutation_batch`], [`execute_bulk_by_filter`]).

use std::collections::HashMap;

use super::{Executor, runners};
use crate::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    error::{FraiseQLError, Result},
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
    /// Construct a mutation runner on demand.
    ///
    /// Zero-cost: `Arc::clone` is one atomic increment, no allocation.
    fn mutation_runner(&self) -> runners::mutation::MutationRunner<A> {
        runners::mutation::MutationRunner::new(std::sync::Arc::clone(&self.ctx))
    }

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
    /// * `type_selections` - Per-type field selections for projection
    ///
    /// # Returns
    ///
    /// A JSON-encoded GraphQL response value on success.
    ///
    /// # Errors
    ///
    /// Same as `execute_mutation_query`, minus the adapter
    /// capability check.
    pub async fn execute_mutation(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        type_selections: &HashMap<String, Vec<String>>,
    ) -> Result<serde_json::Value> {
        // No runtime supports_mutations() check: the SupportsMutations bound
        // guarantees at compile time that this adapter supports mutations.
        self.mutation_runner()
            .execute_mutation(mutation_name, variables, type_selections)
            .await
    }
}

impl<A: DatabaseAdapter> Executor<A> {
    /// Execute a GraphQL mutation by calling the configured database function.
    ///
    /// This is the **runtime-guarded** entry point called from [`execute_internal`] when the
    /// query is classified as a mutation. It checks `adapter.supports_mutations()` at runtime
    /// (because `execute_internal` is bounded only on `DatabaseAdapter`) and delegates to the
    /// shared [`execute_mutation_impl`](runners::mutation::execute_mutation_impl) function.
    ///
    /// # Errors
    ///
    /// * [`FraiseQLError::Validation`] — the adapter does not support mutations, mutation name not
    ///   found in the compiled schema, no `sql_source` configured, or the database function
    ///   returned no rows.
    /// * [`FraiseQLError::Database`] — the adapter's `execute_function_call` returned an error.
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
    /// # let schema: CompiledSchema = panic!("example");
    /// # let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
    /// # let executor = Executor::new(schema, Arc::new(adapter));
    /// let vars = serde_json::json!({ "name": "Alice", "email": "alice@example.com" });
    /// let selections = std::collections::HashMap::new(); // no filtering
    /// // Returns {"data":{"createUser":{"id":"...", "name":"Alice"}}}
    /// // or      {"data":{"createUser":{"__typename":"UserAlreadyExistsError", "email":"..."}}}
    /// let result = executor.execute_mutation("createUser", Some(&vars), &selections).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub(super) async fn execute_mutation_query(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        type_selections: &HashMap<String, Vec<String>>,
    ) -> Result<serde_json::Value> {
        // Runtime guard: verify this adapter supports mutations.
        // Note: this is a runtime check, not compile-time enforcement.
        // The common execute() entry point accepts raw GraphQL strings and
        // determines the operation type at runtime, which precludes compile-time
        // mutation gating. The direct execute_mutation() API provides compile-time
        // enforcement via the SupportsMutations bound on MutationRunner.
        if !self.ctx.adapter.supports_mutations() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Mutation '{mutation_name}' cannot be executed: the configured database \
                     adapter does not support mutations. Use PostgresAdapter, MySqlAdapter, \
                     or SqlServerAdapter for mutation operations."
                ),
                path:    None,
            });
        }
        runners::mutation::execute_mutation_impl(
            &self.ctx,
            mutation_name,
            variables,
            None,
            type_selections,
        )
        .await
    }

    /// Execute a mutation with security context for REST transport.
    ///
    /// Delegates to the standard mutation execution path with RLS enforcement.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    /// Returns `FraiseQLError::Validation` if inject params require a missing security context.
    pub async fn execute_mutation_with_security(
        &self,
        mutation_name: &str,
        arguments: &serde_json::Value,
        security_context: Option<&SecurityContext>,
    ) -> crate::error::Result<serde_json::Value> {
        // Build a synthetic GraphQL mutation query and delegate to execute()
        let args_str = if let Some(obj) = arguments.as_object() {
            obj.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join(", ")
        } else {
            String::new()
        };
        let query = if args_str.is_empty() {
            format!("mutation {{ {mutation_name} {{ status entity_id message }} }}")
        } else {
            format!("mutation {{ {mutation_name}({args_str}) {{ status entity_id message }} }}")
        };

        if let Some(ctx) = security_context {
            self.execute_with_security(&query, None, ctx).await
        } else {
            self.execute(&query, None).await
        }
    }

    /// Execute a batch of mutations (for REST bulk insert).
    ///
    /// Executes each mutation individually and collects results into a `BulkResult`.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered during batch execution.
    pub async fn execute_mutation_batch(
        &self,
        mutation_name: &str,
        items: &[serde_json::Value],
        security_context: Option<&SecurityContext>,
    ) -> crate::error::Result<crate::runtime::BulkResult> {
        let mut entities = Vec::with_capacity(items.len());
        for item in items {
            let result = self
                .execute_mutation_with_security(mutation_name, item, security_context)
                .await?;
            entities.push(result);
        }
        Ok(crate::runtime::BulkResult {
            affected_rows: entities.len() as u64,
            entities:      Some(entities),
        })
    }

    /// Execute a bulk operation (collection-level PATCH/DELETE) by filter.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` if the adapter returns an error.
    pub async fn execute_bulk_by_filter(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        mutation_name: &str,
        body: Option<&serde_json::Value>,
        _id_field: &str,
        _max_affected: u64,
        security_context: Option<&SecurityContext>,
    ) -> crate::error::Result<crate::runtime::BulkResult> {
        // Execute the filter query to find matching rows.
        let filter_result = self
            .query_runner()
            .execute_query_direct(query_match, None, security_context)
            .await?;

        let args = body.cloned().unwrap_or(serde_json::json!({}));
        let result = self
            .execute_mutation_with_security(mutation_name, &args, security_context)
            .await?;

        let count = filter_result
            .get("data")
            .and_then(|d| d.as_object())
            .and_then(|o| o.values().next())
            .and_then(|v| v.as_array())
            .map_or(1, |a| a.len() as u64);

        Ok(crate::runtime::BulkResult {
            affected_rows: count,
            entities:      Some(vec![result]),
        })
    }
}
