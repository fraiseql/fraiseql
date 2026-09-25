//! The read gates of a write's payload selection.
//!
//! A mutation's payload is the document its function returned, and every entity in it — the
//! payload itself, a cascade payload's `entity`, each `cascade.updated[].entity`, an error
//! outcome's detail — is served as a read of its type would serve it. The read path's gates
//! apply, through the read path's classifier (`query_nested`):
//!
//! * `requires_scope`, at every level, the root included — a `Mask` nulls the field, a `Reject`
//!   refuses the request;
//! * a nested level's read gates — its type's `requires_role` and `requires_actor`, and the #422
//!   authorizer, asked of it as a read of its type;
//! * a nested level's row security, over the documents the write returned ([`DocumentRowFilter`]).
//!
//! Everything that can refuse is decided **before the write**, from the selection and the
//! principal: a refused selection never runs the function. The concrete type of an entity is
//! stamped by the database, so it is not known then — the selection is classified against
//! every type the position can hold ([`PayloadGates::classify`]), which is the direction
//! that is safe. An entity the write stamps with a type no position anticipated is
//! classified when it arrives ([`PayloadGates::late`]). A refusal then still takes the write
//! with it: a schema or configuration with any gate `late` could meet runs every write in a
//! transaction (`core::write_refusal_gate`), and one with none cannot refuse there.
//!
//! The root entity is not row-filtered: the write function is the authority over what it
//! returns. Masking and the row filter run after the write, on the returned document.

use std::{borrow::Cow, collections::HashSet};

use super::super::{
    super::context::ExecutorContext,
    query_nested::{DocumentRowFilter, LevelAuthz, SelectionAccess},
};
use crate::{
    error::Result,
    graphql::FieldSelection,
    runtime::{project_entity, projection::effective_selections},
    schema::CompiledSchema,
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

/// What the static gates decided for a payload selection, before the write.
pub(super) struct PayloadGates {
    access:     SelectionAccess,
    rows:       DocumentRowFilter,
    /// The `(position, type)` pairs classified.
    classified: HashSet<(PayloadPosition, String)>,
    /// The #422 authorizer's input: the request's variables, as the write binds them.
    input:      Option<serde_json::Value>,
}

impl PayloadGates {
    /// Classify `selections`, the payload selection of a mutation returning `return_type`,
    /// at every position and against every type each position can hold.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` for what the read path refuses — a `Reject` field, a
    /// nested level whose read the caller may not make, one the #422 authorizer denies — and
    /// for a nested level whose row security cannot be evaluated over the returned document.
    pub(super) fn classify(
        ctx: &ExecutorContext,
        security_ctx: Option<&SecurityContext>,
        variables: Option<&serde_json::Value>,
        return_type: &str,
        is_cascade: bool,
        selections: &[FieldSelection],
    ) -> Result<Self> {
        let roots = payload_roots(&ctx.schema, return_type, is_cascade, selections);
        Self::classify_roots(ctx, security_ctx, variables.cloned(), &roots)
    }

    fn classify_roots(
        ctx: &ExecutorContext,
        security_ctx: Option<&SecurityContext>,
        input: Option<serde_json::Value>,
        roots: &[(PayloadPosition, String, &[FieldSelection])],
    ) -> Result<Self> {
        let typed: Vec<(&str, &[FieldSelection])> =
            roots.iter().map(|(_, t, sels)| (t.as_str(), *sels)).collect();
        let access = SelectionAccess::classify_roots(
            &ctx.schema,
            &typed,
            security_ctx,
            LevelAuthz::from_config(&ctx.config, input.as_ref()),
        )?;
        let mut rows = DocumentRowFilter::default();
        for (_, type_name, sels) in roots {
            rows.plan(ctx, type_name, sels, security_ctx)?;
        }
        Ok(Self {
            access,
            rows,
            classified: roots.iter().map(|(p, t, _)| (*p, t.clone())).collect(),
            input,
        })
    }

    /// The gates of an entity the write stamped `type_name` at `position`, when
    /// [`Self::classify`] did not anticipate it: classified now. `None` when it did.
    ///
    /// # Errors
    ///
    /// As [`Self::classify`].
    pub(super) fn late(
        &self,
        ctx: &ExecutorContext,
        security_ctx: Option<&SecurityContext>,
        position: PayloadPosition,
        type_name: &str,
        selections: &[FieldSelection],
    ) -> Result<Option<Self>> {
        if self.classified.contains(&(position, type_name.to_string())) {
            return Ok(None);
        }
        Self::classify_roots(
            ctx,
            security_ctx,
            self.input.clone(),
            &[(position, type_name.to_string(), selections)],
        )
        .map(Some)
    }

    /// Project `entity`, a `type_name` at `position`, through `selections`: its nested
    /// levels row-filtered, then masked. Returns the document projected from — the parent
    /// the #423 authorizer decides over — the projection, and the fields statically masked.
    ///
    /// # Errors
    ///
    /// What [`Self::late`] returns for a type no position anticipated.
    pub(super) fn project<'e>(
        &self,
        ctx: &ExecutorContext,
        security_ctx: Option<&SecurityContext>,
        position: PayloadPosition,
        type_name: &str,
        selections: &[FieldSelection],
        entity: &'e serde_json::Value,
    ) -> Result<GatedEntity<'e>> {
        let late = self.late(ctx, security_ctx, position, type_name, selections)?;
        let gates = late.as_ref().unwrap_or(self);
        let mut source = Cow::Borrowed(entity);
        if !gates.rows.is_empty() {
            gates.rows.apply(source.to_mut(), type_name, selections, &ctx.schema);
        }
        let mut projected = project_entity(&source, type_name, selections, &ctx.schema);
        gates.access.null_masked(&mut projected, type_name, selections, &ctx.schema);
        Ok(GatedEntity {
            source,
            projected,
            masked: gates.access.masked_fields(type_name),
        })
    }
}

/// An entity projected through its payload position's gates.
pub(super) struct GatedEntity<'e> {
    /// The document projected from, nested levels row-filtered.
    pub(super) source:    Cow<'e, serde_json::Value>,
    /// The response value.
    pub(super) projected: serde_json::Value,
    /// The root fields `requires_scope` masked, by name.
    pub(super) masked:    Vec<String>,
}

/// Every `(position, type, selections)` a payload selection can be served as.
fn payload_roots<'s>(
    schema: &CompiledSchema,
    return_type: &str,
    is_cascade: bool,
    selections: &'s [FieldSelection],
) -> Vec<(PayloadPosition, String, &'s [FieldSelection])> {
    let mut roots = Vec::new();
    // An error outcome is served as the stamped error type, or the union's error member:
    // any declared error type.
    for error_type in schema.types.iter().filter(|t| t.is_error) {
        push_root(&mut roots, (PayloadPosition::Root, error_type.name.to_string(), selections));
    }
    if is_cascade {
        let payload_type = super::resolve_payload_type(return_type, schema);
        for sel in effective_selections(selections, &payload_type, schema) {
            match sel.name.as_str() {
                "entity" => {
                    let entity_types = super::payload_entity_type(&payload_type, schema)
                        .map_or_else(|| cascade_node_types(schema), |t| vec![t]);
                    for entity_type in entity_types {
                        push_root(
                            &mut roots,
                            (PayloadPosition::CascadeEntity, entity_type, &sel.nested_fields),
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
        // The success entity: a member of the returned union, or the returned type.
        match schema.find_union(return_type) {
            Some(union) => {
                for member in &union.member_types {
                    push_root(&mut roots, (PayloadPosition::Root, member.clone(), selections));
                }
            },
            None => {
                push_root(&mut roots, (PayloadPosition::Root, return_type.to_string(), selections));
            },
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

/// The types a cascade's entity can be: every one implementing `CascadeNode`. A
/// framework-`internal` type never does (the compiler excludes it from cascade
/// classification), and the runner refuses one named in a cascade before serving it — so
/// `internal` is not read here (`tools/check-internal-flag-sites.sh`).
fn cascade_node_types(schema: &CompiledSchema) -> Vec<String> {
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
