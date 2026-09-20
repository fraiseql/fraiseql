//! Mutation execution — thin wrappers on `Executor<A>`.
//!
//! The core mutation logic lives in
//! [`runners::mutation::execute_mutation_impl`](super::runners::mutation::execute_mutation_impl).
//! This module contains:
//!
//! - The typed public API ([`Executor::execute_mutation`] and friends), which a transport uses when
//!   it already holds structured arguments.
//! - The internal dispatch entry point for a client document (`Executor::execute_mutation_query`).
//! - Convenience wrappers used by the REST transport ([`execute_mutation_with_security`],
//!   [`execute_mutation_batch`], [`execute_bulk_by_ids`]).
//!
//! None of them carries a capability bound. All converge on `execute_mutation_impl`, whose
//! step 0 resolves the executor's write slot — see [`Executor::new`] for where the
//! compile-time half of that decision is made.

use super::{Executor, runners};
use crate::{
    backend::traits::DatabaseAdapter,
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    security::SecurityContext,
};

/// The typed write entries.
///
/// These used to sit on an `impl<A: DatabaseAdapter + SupportsMutations>` block, and the
/// bound was described as the thing keeping a read-only adapter out. It kept out an
/// adapter without the *marker*; it could not speak for `supports_mutations()`, the
/// runtime gate `execute_function_call` is keyed on, and these entries skipped that check
/// on the strength of the bound. An adapter carrying one and not the other was dispatched.
///
/// The question is now asked once, at step 0 of the chokepoint, against a slot resolved at
/// construction from *both* gates. The compile-time refusal did not disappear: it moved to
/// `Executor::new`, which is bounded on `SupportsMutations` and is where the
/// `compile_fail` pair now lives. An adapter that declares nothing still cannot be built
/// into a write-capable executor.
impl<A: DatabaseAdapter> Executor<A> {
    /// Construct a mutation runner on demand.
    ///
    /// Zero-cost: `Arc::clone` is one atomic increment, no allocation.
    fn mutation_runner(&self) -> runners::mutation::MutationRunner<A> {
        runners::mutation::MutationRunner::new(std::sync::Arc::clone(&self.ctx))
    }

    /// Execute a GraphQL mutation directly, by name.
    ///
    /// Unlike `execute()`, which accepts a raw GraphQL string, this takes the mutation
    /// name and variables directly. Capability is settled by the executor's write slot:
    /// an executor built by [`Executor::read_only`], or from an adapter whose
    /// `supports_mutations()` returns `false`, refuses here before any other gate runs.
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
        selections: WriteSelections<'_>,
    ) -> Result<serde_json::Value> {
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
        selections: WriteSelections<'_>,
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
            WriteSelections::new(&selections)?,
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
    /// # use fraiseql_core::graphql::FieldSelection;
    /// # use fraiseql_core::schema::CompiledSchema;
    /// # use fraiseql_core::runtime::{Executor, WriteSelections};
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let schema: CompiledSchema = panic!("example");
    /// # let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
    /// # let executor = Executor::new(schema, Arc::new(adapter));
    /// let vars = serde_json::json!({ "name": "Alice", "email": "alice@example.com" });
    ///
    /// // The selection set is required and cannot be empty: an empty one reads as
    /// // "no field filtering", which returns the stored entity whole and skips
    /// // field-level authorization.
    /// let fields = vec![FieldSelection {
    ///     name:          "id".to_string(),
    ///     alias:         None,
    ///     arguments:     vec![],
    ///     nested_fields: vec![],
    ///     directives:    vec![],
    /// }];
    /// let selections = WriteSelections::new(&fields)?;
    ///
    /// // Returns {"data":{"createUser":{"id":"..."}}}
    /// // or      {"data":{"createUser":{"__typename":"UserAlreadyExistsError", "email":"..."}}}
    /// let result = executor.execute_mutation("createUser", Some(&vars), selections).await?;
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
        // The capability question is asked at step 0 of `execute_mutation_impl`, which
        // every path below reaches — including the § 5.3.3 refusal underneath, which is a
        // *validation* answer and must not depend on write capability to be given. The
        // check that used to stand here was the document path's own copy of a decision the
        // typed entries were meanwhile not making at all; there is one copy now.
        // The one place a *client document's* selection set becomes a write's.
        //
        // § 5.3.3 (#1357) runs **here, before the conversion**, not only at step 1e
        // of the chokepoint. `WriteSelections::new` would otherwise refuse the empty
        // set first, and its message is the backstop's — "a write needs a selection
        // set" — where § 5.3.3's names the offending type and is the answer the
        // GraphQL spec gives. Constructing the type ahead of the validator made the
        // better diagnosis unreachable on the one path that can actually produce the
        // shape from a client document.
        //
        // An unknown mutation is left to `execute_mutation_impl`, which has the
        // did-you-mean suggestion for it.
        if let Some(def) = self.schema().find_mutation(mutation_name) {
            crate::graphql::validate_leaf_field_selections(
                self.schema(),
                &def.return_type,
                selections,
            )?;
        }

        // Every mutation's return type is composite (#1358) and § 5.3.3 has just
        // adjudicated the set, so a compiled schema cannot deliver an empty one here.
        // Both of those are compiler- and validator-side, though, and
        // `schema.compiled.json` can be hand-authored — so this is where a schema
        // that skipped them fails closed rather than projecting the entity whole.
        self.execute_mutation_detailed(
            mutation_name,
            response_key,
            variables,
            security_context,
            WriteSelections::new(selections)?,
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
        selections: WriteSelections<'_>,
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

/// A write's result selection set, **guaranteed non-empty**.
///
/// # Why this is a type and not a `&[FieldSelection]`
///
/// An empty selection set is the *permissive* shape at the write entries, not a
/// neutral one. It is the input to two security decisions at once:
///
/// * [`project_entity`](crate::runtime::project_entity) filters the returned entity to it — and an
///   empty slice means "no field filtering", so the whole stored entity is returned;
/// * `selection_set_selects_gated_field` decides whether the #423 field authorizer runs at all —
///   and it is false for an empty slice, so the authorizer takes **zero calls**.
///
/// So `&[]` is not a value a write entry should be able to receive by accident.
/// REST's anonymous arm passed one (#1352); gRPC reached the same slice a different
/// way, through `unwrap_or_default()`; and an ordinary `mutation { createUser }`
/// carried one in from a client document (#1357). Three transports, three spellings,
/// one shape — which is what a type, rather than a fourth grep gate, is for.
///
/// # Why "non-empty" is sound
///
/// Every mutation's return type is composite (#1358), and GraphQL § 5.3.3 refuses a
/// composite field named without a selection set (#1357), so a document that reaches
/// a write entry always carries one. A transport with no document of its own uses
/// [`mutation_return_selections`], which never returns empty.
///
/// [`new`](Self::new) is still fallible rather than an assertion: the compiler is
/// only a gate for schemas that go *through* the compiler, and
/// `schema.compiled.json` can be hand-authored. This is the runtime backstop for
/// one that declares a leaf-returning mutation anyway.
#[derive(Clone, Copy, Debug)]
pub struct WriteSelections<'a>(&'a [FieldSelection]);

impl<'a> WriteSelections<'a> {
    /// Adopt a selection set for a write.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] when `selections` is empty. See the type
    /// docs for why that is a refusal rather than a permissive default.
    pub fn new(selections: &'a [FieldSelection]) -> Result<Self> {
        if selections.is_empty() {
            return Err(FraiseQLError::Validation {
                message: "A write needs a selection set: an empty one is read as \"no field \
                          filtering\", which returns the stored entity whole and skips \
                          field-level authorization."
                    .to_string(),
                path:    None,
            });
        }
        Ok(Self(selections))
    }

    /// The selection set, for the projector and the field authorizer.
    #[must_use]
    pub const fn as_slice(self) -> &'a [FieldSelection] {
        self.0
    }
}

/// A minimal non-empty selection set, for tests that do not exercise projection.
///
/// `__typename` is valid on every composite type and is never policy-gated, so it is
/// the smallest thing a write entry can legitimately be handed. Test-only on purpose:
/// production code either has a client's selection set or derives one from the return
/// type, and a public constructor for "the smallest set that compiles" would be a
/// third answer for a transport to reach for.
#[cfg(test)]
pub fn any_write_selections() -> WriteSelections<'static> {
    static SET: std::sync::OnceLock<Vec<FieldSelection>> = std::sync::OnceLock::new();
    let set = SET.get_or_init(|| {
        vec![FieldSelection {
            name:          "__typename".to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }]
    });
    WriteSelections::new(set).expect("a one-field selection set is not empty")
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
