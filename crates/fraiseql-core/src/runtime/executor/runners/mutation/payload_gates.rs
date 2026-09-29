//! The read gates of a write's payload selection.
//!
//! A mutation's payload is the document its function returned, and every entity in it — the
//! payload itself, a cascade payload's `entity`, each `cascade.updated[].entity`, an error
//! outcome's detail — is served as a read of its type would serve it: through the read plan
//! (`read_plan`), which carries the read path's gates, and the type of every entity served,
//! root included, its own `requires_role` — checked where the entity is served, since the
//! database stamps the type.
//!
//! Everything that can refuse is decided **before the write**, from the selection and the
//! principal: a refused selection never runs the function. The concrete type of an entity is
//! stamped by the database, so it is not known then — the selection is classified against
//! every type the position can hold ([`PayloadGates::classify`]), which is the direction
//! that is safe. That set is exact and derived from the schema (ruling AA 1): an entity
//! stamped with any other type broke the function's contract, and is refused where it would
//! be served ([`PayloadGates::project`]) — never classified on arrival, never served with
//! gates it was not classified under. Every write is adjudicated inside its transaction
//! (ruling Z 1), so the refusal takes the write with it.
//!
//! The root entity is not row-filtered: the write function is the authority over what it
//! returns. Masking and the row filter run after the write, on the returned document.

use std::{
    collections::HashSet,
    sync::atomic::{AtomicU64, Ordering},
};

use super::super::{super::context::ExecutorContext, read_plan::ReadPlan};
use crate::{
    error::Result,
    graphql::FieldSelection,
    runtime::projection::effective_selections,
    schema::{CompiledSchema, MutationDefinition},
    security::SecurityContext,
};

/// Where in a payload an entity sits. One type can sit at several, under different
/// selections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PayloadPosition {
    /// The payload itself: a plain mutation's entity, or any mutation's error detail.
    Root,
    /// A cascade payload's `entity`.
    CascadeEntity,
    /// An entity a cascade's `updated` reports.
    UpdatedEntity,
}

/// What a mutation's function may stamp in its `entity_type` column (rulings AA 1, AG 2).
///
/// The one derivation the runner's contract check reads, and the CLI's stamp lint
/// (`fraiseql compile --database`, `fraiseql doctor --against-db`) with it, so the two cannot
/// drift (ruling AJ 1).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StampContract {
    /// The types a success can be served as: for a cascade mutation, its payload's `entity`
    /// type (else every `CascadeNode` implementor); otherwise the non-error members of the
    /// returned union, the implementors of the returned interface, or the returned type.
    pub success: Vec<String>,
    /// The types a failure can be served as: the error members of the returned union, the
    /// error implementors of the returned interface, or — for an object return, where nothing
    /// narrower is declared — every error type of the schema.
    pub error:   Vec<String>,
}

impl StampContract {
    /// The contract of `mutation` in `schema`.
    #[must_use]
    pub fn of(schema: &CompiledSchema, mutation: &MutationDefinition) -> Self {
        Self::for_return(schema, &mutation.return_type, mutation.cascade)
    }

    /// The contract of a mutation returning `return_type`, a cascade one or not.
    pub(super) fn for_return(schema: &CompiledSchema, return_type: &str, is_cascade: bool) -> Self {
        let success = if is_cascade {
            let payload_type = super::resolve_payload_type(return_type, schema);
            super::payload_entity_type(&payload_type, schema)
                .map_or_else(|| cascade_node_types(schema), |t| vec![t])
        } else {
            success_types(schema, return_type)
        };
        Self {
            success,
            error: error_types(schema, return_type),
        }
    }
}

/// What the static gates decided for a payload selection, before the write.
pub(super) struct PayloadGates {
    plan:       ReadPlan,
    /// The `(position, type)` pairs classified: exactly what each position can hold.
    classified: HashSet<(PayloadPosition, String)>,
    /// What the function may stamp, per arm: the classified root and cascade-entity sets are
    /// drawn from it.
    contract:   StampContract,
}

impl PayloadGates {
    /// Classify `selections`, the payload selection of a mutation returning `return_type`,
    /// at every position and against every type each position can hold.
    ///
    /// # Errors
    ///
    /// What [`ReadPlan::classify`] refuses.
    pub(super) fn classify(
        ctx: &ExecutorContext,
        security_ctx: Option<&SecurityContext>,
        variables: Option<&serde_json::Value>,
        return_type: &str,
        is_cascade: bool,
        selections: &[FieldSelection],
    ) -> Result<Self> {
        let contract = StampContract::for_return(&ctx.schema, return_type, is_cascade);
        let roots = payload_roots(&ctx.schema, &contract, return_type, is_cascade, selections);
        let typed: Vec<(&str, &[FieldSelection])> =
            roots.iter().map(|(_, t, sels)| (t.as_str(), *sels)).collect();
        let plan = ReadPlan::classify(ctx, security_ctx, variables, &typed)?;
        Ok(Self {
            plan,
            classified: roots.iter().map(|(p, t, _)| (*p, t.clone())).collect(),
            contract,
        })
    }

    /// What the function may stamp (ruling AJ 1).
    pub(super) const fn contract(&self) -> &StampContract {
        &self.contract
    }

    /// Serve `entity`, a `type_name` at `position`, through `selections` under the read plan.
    ///
    /// # Errors
    ///
    /// `Validation` when `type_name` is not a type `position` can hold — the function broke its
    /// contract (ruling AA 1); `Authorization` when `type_name` requires a role the request
    /// does not hold; what [`ReadPlan::serve`] refuses.
    // Reason: the entity, where it sits, and the request's principal and variables are each
    // a separate input to one decision; a struct would only relocate them.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn serve(
        &self,
        ctx: &ExecutorContext,
        security_ctx: Option<&SecurityContext>,
        position: PayloadPosition,
        type_name: &str,
        selections: &[FieldSelection],
        entity: &serde_json::Value,
        variables: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        if !self.classified.contains(&(position, type_name.to_string())) {
            return Err(off_contract(position, type_name, &self.holdable(position)));
        }
        refuse_unless_type_readable(ctx, security_ctx, position, type_name)?;
        self.plan.serve(ctx, security_ctx, type_name, selections, entity, variables)
    }

    /// The types `position` can hold, sorted, for the contract error.
    fn holdable(&self, position: PayloadPosition) -> Vec<String> {
        let mut types: Vec<String> = self
            .classified
            .iter()
            .filter(|(p, _)| *p == position)
            .map(|(_, t)| t.clone())
            .collect();
        types.sort();
        types
    }
}

/// Contract errors raised since the process started (ruling AJ 3).
static CONTRACT_ERRORS: AtomicU64 = AtomicU64::new(0);

/// How many writes this process refused as a mutation contract error — an off-contract or
/// ambiguous `entity_type` stamp, on either outcome (rulings AA 1, AG 3) — each rolled back.
///
/// Exported by the server as `fraiseql_mutation_contract_errors_total` (ruling AJ 3): a
/// contract error is a bug in a mutation function, and this is where operators see it
/// happen. Aggregate on purpose; the per-operation error counter names the operation.
#[must_use]
pub fn mutation_contract_errors() -> u64 {
    CONTRACT_ERRORS.load(Ordering::Relaxed)
}

/// The `path` every contract error carries: the column the function got wrong.
const CONTRACT_ERROR_PATH: &str = "entity_type";

/// A contract error with `message`, counted (ruling AJ 3). Every contract error is built
/// here, so the count and [`is_contract_error`] cannot miss one.
pub(super) fn contract_error(message: String) -> crate::error::FraiseQLError {
    CONTRACT_ERRORS.fetch_add(1, Ordering::Relaxed);
    crate::error::FraiseQLError::Validation {
        message,
        path: Some(CONTRACT_ERROR_PATH.to_string()),
    }
}

/// Whether `error` is a contract error built by [`contract_error`].
pub(super) fn is_contract_error(error: &crate::error::FraiseQLError) -> bool {
    matches!(
        error,
        crate::error::FraiseQLError::Validation { path: Some(path), .. } if path == CONTRACT_ERROR_PATH
    )
}

/// The contract error for an entity stamped with a type its position cannot hold.
pub(super) fn off_contract(
    position: PayloadPosition,
    stamp: &str,
    holdable: &[String],
) -> crate::error::FraiseQLError {
    contract_error(format!(
        "the mutation function stamped '{stamp}' on a {position:?} entity, which that position \
         cannot hold (it can hold: {}); the write was rolled back",
        if holdable.is_empty() {
            "nothing".to_string()
        } else {
            holdable.join(", ")
        }
    ))
}

/// Refuse to serve an entity of `type_name` at `position` unless the caller may read the
/// type at all: its own read's `requires_role` (else the type's) and `requires_actor`, the
/// gates a nested level of the type applies.
///
/// #677 lowers a type's role onto an operation only when the operation returns exactly that
/// type, and the classifier checks it only below the root. A payload root is served as a read
/// of whatever type it holds — a union member, the type the function stamped, a cascade's
/// entity, an entity a cascade reports as updated — so the type's role is checked where it
/// is served. Every write is adjudicated in its transaction (ruling Z 1), so the refusal
/// takes the write with it.
///
/// A `403`, as at a nested level: the caller named the operation, so the type's existence
/// is not what the answer discloses.
///
/// # Errors
///
/// `FraiseQLError::Authorization` when the type's read requires a role or an actor type the
/// request does not hold.
fn refuse_unless_type_readable(
    ctx: &ExecutorContext,
    security_ctx: Option<&SecurityContext>,
    position: PayloadPosition,
    type_name: &str,
) -> Result<()> {
    // The type's own read's gates, as a nested level of the type applies them.
    match super::super::query_nested::type_read_refusal(&ctx.schema, type_name, security_ctx) {
        None => Ok(()),
        Some(why) => Err(crate::error::FraiseQLError::Authorization {
            message:  format!("the payload serves '{type_name}' ({position:?}), {why}"),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        }),
    }
}

/// Every `(position, type, selections)` a payload selection can be served as.
fn payload_roots<'s>(
    schema: &CompiledSchema,
    contract: &StampContract,
    return_type: &str,
    is_cascade: bool,
    selections: &'s [FieldSelection],
) -> Vec<(PayloadPosition, String, &'s [FieldSelection])> {
    let mut roots = Vec::new();
    // An error outcome is served as one of the error types this mutation can return
    // (ruling AG 2): classified here, so the classified set is the error arm's contract.
    for error_type in &contract.error {
        push_root(&mut roots, (PayloadPosition::Root, error_type.clone(), selections));
    }
    if is_cascade {
        let payload_type = super::resolve_payload_type(return_type, schema);
        for sel in effective_selections(selections, &payload_type, schema) {
            match sel.name.as_str() {
                "entity" => {
                    for entity_type in &contract.success {
                        push_root(
                            &mut roots,
                            (
                                PayloadPosition::CascadeEntity,
                                entity_type.clone(),
                                &sel.nested_fields,
                            ),
                        );
                    }
                },
                "cascade" => {
                    for arm in effective_selections(
                        &sel.nested_fields,
                        super::CASCADE_UPDATES_TYPE,
                        schema,
                    ) {
                        if arm.name != "updated" {
                            continue;
                        }
                        for field in effective_selections(
                            &arm.nested_fields,
                            super::UPDATED_ENTITY_TYPE,
                            schema,
                        ) {
                            if field.name == "entity" {
                                for node in cascade_node_types(schema) {
                                    push_root(
                                        &mut roots,
                                        (
                                            PayloadPosition::UpdatedEntity,
                                            node,
                                            &field.nested_fields,
                                        ),
                                    );
                                }
                            }
                        }
                    }
                },
                _ => {},
            }
        }
    } else {
        // The success entity: a non-error member of the returned union (its error members
        // are the error arm's, above), an implementor of the returned interface, or the
        // returned type.
        for success in &contract.success {
            push_root(&mut roots, (PayloadPosition::Root, success.clone(), selections));
        }
    }
    roots
}

/// Add a root unless it is already there: an error member of the returned union is also a
/// declared error type.
fn push_root<'s>(
    roots: &mut Vec<(PayloadPosition, String, &'s [FieldSelection])>,
    root: (PayloadPosition, String, &'s [FieldSelection]),
) {
    if !roots
        .iter()
        .any(|r| r.0 == root.0 && r.1 == root.1 && std::ptr::eq(r.2, root.2))
    {
        roots.push(root);
    }
}

/// The types a successful mutation returning `return_type` can produce: the non-error members
/// of a union, the implementors of an interface, or the type itself.
pub(super) fn success_types(schema: &CompiledSchema, return_type: &str) -> Vec<String> {
    if let Some(union) = schema.find_union(return_type) {
        return union
            .member_types
            .iter()
            .filter(|t| schema.find_type(t).is_none_or(|td| !td.is_error))
            .cloned()
            .collect();
    }
    if schema.find_interface(return_type).is_some() {
        return schema
            .types
            .iter()
            .filter(|t| t.implements.iter().any(|i| i == return_type))
            .map(|t| t.name.to_string())
            .collect();
    }
    vec![return_type.to_string()]
}

/// The error types a failed mutation returning `return_type` can be served as (ruling AG 2):
/// the `is_error` members of a union, the `is_error` implementors of an interface, or — for an
/// object return, where nothing narrower is declared — every error type of the schema.
pub(super) fn error_types(schema: &CompiledSchema, return_type: &str) -> Vec<String> {
    if let Some(union) = schema.find_union(return_type) {
        return union
            .member_types
            .iter()
            .filter(|t| schema.find_type(t).is_some_and(|td| td.is_error))
            .cloned()
            .collect();
    }
    let implements = schema.find_interface(return_type).is_some();
    schema
        .types
        .iter()
        .filter(|t| t.is_error && (!implements || t.implements.iter().any(|i| i == return_type)))
        .map(|t| t.name.to_string())
        .collect()
}

/// The types a cascade's entity can be: every one implementing `CascadeNode`. A
/// framework-`internal` type never does (the compiler excludes it from cascade
/// classification), and the runner refuses one named in a cascade before serving it — so
/// `internal` is not read here (`tools/check-internal-flag-sites.sh`).
pub(super) fn cascade_node_types(schema: &CompiledSchema) -> Vec<String> {
    schema
        .types
        .iter()
        .filter(|t| t.implements.iter().any(|i| i == "CascadeNode"))
        .map(|t| t.name.to_string())
        .collect()
}

#[cfg(test)]
#[path = "payload_gates_tests.rs"]
mod tests;
