//! Mutation execution — thin wrappers on `Executor<A>`.
//!
//! The core mutation logic lives in
//! [`runners::mutation::execute_mutation_impl`](super::runners::mutation::execute_mutation_impl).
//! This module contains:
//!
//! - The compile-time-enforced public API ([`Executor::execute_mutation`], bounded on
//!   [`SupportsMutations`]).
//! - The runtime-guarded internal dispatch entry point (`Executor::execute_mutation_query`, bounded
//!   only on [`DatabaseAdapter`]).
//! - Convenience wrappers used by the REST transport ([`execute_mutation_with_security`],
//!   [`execute_mutation_batch`], [`execute_bulk_by_ids`]).

use super::{Executor, runners};
use crate::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
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
///     executor.execute_mutation("createUser", None, &[]).await.unwrap();
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
    /// * `selections` - The result selection set (inline fragments intact) used to project the
    ///   response; pass `&[]` for no field filtering.
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
        selections: &[FieldSelection],
    ) -> Result<serde_json::Value> {
        // No runtime supports_mutations() check: the SupportsMutations bound
        // guarantees at compile time that this adapter supports mutations.
        self.mutation_runner()
            .execute_mutation(mutation_name, variables, selections)
            .await
    }

    /// Execute a mutation **with a principal**, binding arguments by name from
    /// `variables` (#1330).
    ///
    /// This is [`execute_mutation`](Self::execute_mutation) plus the caller's
    /// identity, and it is the entry a non-GraphQL transport should use when it
    /// already holds structured arguments. It converges at
    /// `execute_mutation_impl` like every other write, so `requires_role`,
    /// `requires_actor`, the `Authorizer`, argument validation, `before:mutation`
    /// and the change-log write all run.
    ///
    /// [`execute_mutation_with_security`](Self::execute_mutation_with_security) is
    /// the same call with the selection set derived from the mutation's return type
    /// rather than supplied by the caller — the entry a transport uses when it has no
    /// selection set of its own. It used to reach the engine by formatting a GraphQL
    /// document out of the arguments, a round-trip through text that could not
    /// represent every JSON value faithfully; that is fixed (#1331).
    ///
    /// # Errors
    ///
    /// Same as [`execute_mutation`](Self::execute_mutation), plus the refusals the
    /// gates raise for this principal.
    ///
    /// Returns the projection **and** the parsed `mutation_response` envelope: a
    /// transport whose own wire format is that envelope cannot reconstruct
    /// `entity_id` or a failure message from the projection alone.
    pub async fn execute_mutation_as(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        selections: &[FieldSelection],
    ) -> Result<crate::runtime::MutationExecution> {
        self.execute_mutation_detailed(
            mutation_name,
            mutation_name,
            variables,
            security_context,
            selections,
            &[],
        )
        .await
    }

    /// Execute a mutation for a transport that holds **structured arguments**, with an
    /// optional principal.
    ///
    /// Both arms of the REST write come here — authenticated and anonymous — so both are
    /// projected through the same selection set and face the same gates (#1352).
    ///
    /// This used to reach the engine by **formatting a GraphQL document** out of its
    /// arguments: `format!("{k}: {v}")` renders a `serde_json::Value` through `Display`,
    /// which emits JSON, and JSON quotes object keys where GraphQL does not. Any argument
    /// that was — or contained — an object produced a document the parser refused, so an
    /// authenticated REST write carrying a nested body failed outright, and under the
    /// JSONB `data`-column model a nested object is the ordinary body shape (#1331).
    /// Arguments are bound as **values** now; nothing round-trips through text.
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
        let selections = mutation_return_selections(self.schema(), mutation_name);
        self.execute_mutation_detailed(
            mutation_name,
            mutation_name,
            Some(arguments),
            security_context,
            &selections,
            &[],
        )
        .await
        .map(|execution| execution.data)
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

    /// Execute a mutation once per identified row — the engine behind a collection-level
    /// `PATCH`/`DELETE`.
    ///
    /// `ids` are the primary-key values the caller's filter selected; each is merged into
    /// the request body under `id_field` so the mutation function receives the row it is
    /// meant to act on. `affected_rows` is the number of mutations that actually ran.
    ///
    /// This replaces `execute_bulk_by_filter`, which ran the filter query, **discarded
    /// the matched rows**, invoked the mutation exactly once with the body and no row
    /// identity, and then reported `affected_rows` as the *filter's* row count — a
    /// fabricated success on a write path (`#913`). Its `_id_field` and `_max_affected`
    /// parameters were both unused.
    ///
    /// Row selection and the `max_affected` cap now live in the caller (the REST bulk
    /// handler), where the filter guard and the HTTP status for "too many rows" belong.
    /// Keeping them there is deliberate: this function can no longer run without a
    /// caller having decided which rows it applies to.
    ///
    /// # Errors
    ///
    /// Returns whatever the underlying mutation returns; the first failure aborts and
    /// propagates, so a partially-applied bulk reports the error rather than a count.
    pub async fn execute_bulk_by_ids(
        &self,
        mutation_name: &str,
        id_field: &str,
        ids: &[serde_json::Value],
        body: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> crate::error::Result<crate::runtime::BulkResult> {
        let mut entities = Vec::with_capacity(ids.len());

        for id in ids {
            let mut args = body.and_then(|b| b.as_object().cloned()).unwrap_or_default();
            // The row identity wins over anything the client put in the body under the
            // same key: a bulk request must not be able to redirect a per-row mutation.
            args.insert(id_field.to_string(), id.clone());

            let result = self
                .execute_mutation_with_security(
                    mutation_name,
                    &serde_json::Value::Object(args),
                    security_context,
                )
                .await?;
            entities.push(result);
        }

        Ok(crate::runtime::BulkResult {
            affected_rows: u64::try_from(entities.len()).unwrap_or(u64::MAX),
            entities:      Some(entities),
        })
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
    /// // Returns {"data":{"createUser":{"id":"...", "name":"Alice"}}}
    /// // or      {"data":{"createUser":{"__typename":"UserAlreadyExistsError", "email":"..."}}}
    /// let result = executor.execute_mutation("createUser", Some(&vars), &[]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub(super) async fn execute_mutation_query(
        &self,
        mutation_name: &str,
        response_key: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        selections: &[FieldSelection],
        inline_arguments: &[crate::graphql::GraphQLArgument],
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
        self.execute_mutation_detailed(
            mutation_name,
            response_key,
            variables,
            security_context,
            selections,
            inline_arguments,
        )
        .await
        .map(|execution| execution.data)
    }

    /// [`execute_mutation_query`](Self::execute_mutation_query), keeping the parsed
    /// `mutation_response` envelope alongside the projection (#1330).
    pub(super) async fn execute_mutation_detailed(
        &self,
        mutation_name: &str,
        response_key: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        selections: &[FieldSelection],
        inline_arguments: &[crate::graphql::GraphQLArgument],
    ) -> Result<runners::mutation::MutationExecution> {
        runners::mutation::execute_mutation_impl(
            &self.ctx,
            mutation_name,
            response_key,
            variables,
            security_context,
            selections,
            inline_arguments,
        )
        .await
    }
}

/// The selection set a transport with **no selection set of its own** projects a write
/// through: every scalar field the mutation's declared return type names.
///
/// ⚠ **Never empty, and scalars only.** Both are load-bearing, not stylistic.
///
/// An empty selection set is the *permissive* shape rather than a neutral one:
/// [`project_entity`](crate::runtime::project_entity) returns the whole entity
/// unfiltered, and `selection_set_selects_gated_field` is false for `&[]`, so the #423
/// field authorizer short-circuits with **zero calls**. The REST anonymous write arm
/// passed `&[]`, and so served policy-gated fields to unauthenticated callers that an
/// authenticated caller is refused (#1352).
///
/// Scalars only for the same reason one layer down: `project_field_value` returns a
/// *sub-selection-less object* verbatim, and `selection_field_has_gated_descendant` is
/// false when `nested_fields` is empty — so naming an object field without expanding it
/// would hand back whatever gated field is nested inside it.
///
/// The field list was built as **text** until #1331. Before that it was the literal
/// `status entity_id message` — the `app.mutation_response` envelope's field names rather
/// than the return type's — so every REST mutation answered `{"data":{"createItem":{}}}`,
/// a 201 naming no field the caller could read: the write-path twin of #886. Falling back
/// to those names when the return type is unknown keeps a schema that genuinely returns
/// the envelope working, and keeps this function's promise never to return an empty set.
#[must_use]
pub fn mutation_return_selections(
    schema: &crate::schema::CompiledSchema,
    mutation_name: &str,
) -> Vec<FieldSelection> {
    fn named(name: &str) -> FieldSelection {
        FieldSelection {
            name:          name.to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }
    }

    schema
        .find_mutation(mutation_name)
        .and_then(|m| schema.find_type(&m.return_type))
        .map(|t| {
            t.fields
                .iter()
                .filter(|f| f.field_type.is_scalar())
                .map(|f| named(f.output_name()))
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| vec![named("status"), named("entity_id"), named("message")])
}
