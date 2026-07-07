//! Shared `id: ID!` conformance enforcement for the Node-style interfaces that the
//! compiler auto-implements on entities: `CascadeNode` (graphql-cascade) and `Node`
//! (Relay).
//!
//! Both interfaces declare `id: ID!` and are force-added to entities by their
//! synthesis passes ([`super::cascade_types`], [`super::relay`]). An entity that
//! does not expose `id: ID!` cannot back the interface, so the pass must fail fast
//! with one aggregated, actionable error — rather than emit a schema that later
//! fails the compiled-schema validator with a swallowed "missing field 'id'" bail
//! (the invariant: no synthesis pass may produce IR that `validate()` rejects).

use anyhow::bail;
use fraiseql_core::schema::{FieldType, TypeDefinition};

/// Why a type fails the Node-style `id: ID!` contract.
enum IdDefect {
    /// Has an `id` field, but not of type `ID` (e.g. `UUID`).
    WrongType(String),
    /// No `id` field at all.
    Missing,
}

/// A type conforms to the Node-style interface field `id: ID!` iff it has a field
/// named `id` of type [`FieldType::Id`].
///
/// Nullability is intentionally *not* required — this mirrors the compiled-schema
/// validator's own interface-conformance rule, which compares only field name and
/// type. Tightening it here would reject schemas that compile today.
fn id_defect(ty: &TypeDefinition) -> Option<IdDefect> {
    match ty.fields.iter().find(|f| f.name == "id") {
        Some(f) if f.field_type == FieldType::Id => None,
        Some(f) => Some(IdDefect::WrongType(f.field_type.to_graphql_string())),
        None => Some(IdDefect::Missing),
    }
}

/// Fail with one aggregated, actionable error if any `entities` cannot back a
/// Node-style interface (`id: ID!`).
///
/// `lead` is the opening sentence (names the feature + the interface contract);
/// `remedy` completes the `Fix:` line's "or …" clause. Entities are reported in
/// iteration order so the message is deterministic. Returns `Ok(())` when every
/// entity conforms.
pub(super) fn enforce_node_id_conformance<'a>(
    entities: impl Iterator<Item = &'a TypeDefinition>,
    lead: &str,
    remedy: &str,
) -> anyhow::Result<()> {
    let offenders: Vec<String> = entities
        .filter_map(|ty| id_defect(ty).map(|defect| (ty.name.to_string(), defect)))
        .map(|(name, defect)| match defect {
            IdDefect::WrongType(actual) => format!("  - Type '{name}': `id` is {actual}, not ID"),
            IdDefect::Missing => format!("  - Type '{name}': no `id` field"),
        })
        .collect();

    if offenders.is_empty() {
        return Ok(());
    }

    bail!(
        "{lead}:\n{}\nFix: expose `id: ID!` on each type, or {remedy}.",
        offenders.join("\n"),
    );
}
