//! The FraiseQL entity-identity contract (ADR-0017): every entity's `id` field is a
//! global GraphQL `ID`.
//!
//! The Trinity pattern (`examples/_TEMPLATE`) backs each entity with three columns —
//! `pk_<entity>` (internal `INTEGER` join key, never exposed), **`id UUID`** (the
//! external, stable identity), and `identifier TEXT` (an optional human-readable
//! business key). The external identity is the `id UUID` column.
//!
//! GraphQL's `ID` scalar is the spec's opaque, stringified identifier — and a `UUID`
//! serializes as the *same* JSON string. So an authored `id: UUID` is canonicalized
//! to `id: ID` at compile time: **wire-transparent**, but it lets every Trinity
//! entity satisfy the `Node` / `CascadeNode` interfaces uniformly (cascade, Relay,
//! federation `@key(fields: "id")`) instead of each id-consuming subsystem
//! rediscovering the type heterogeneity. Non-UUID `id` shapes (`Int`, custom) are
//! left as authored; an entity that then opts into a Node-style interface must
//! expose `id: ID` (enforced by [`super::interface_conformance`]).

use fraiseql_core::schema::{CompiledSchema, FieldType};

/// Canonicalize each output object type's `id` field from `UUID` to `ID` — the
/// Trinity external identity, wire-identical to `ID`.
///
/// Runs before Relay/cascade synthesis so the interfaces they auto-implement see a
/// conformant `id: ID`. A no-op for an `id` already typed `ID` (or any non-`UUID`
/// shape) and for types with no `id` field.
pub(super) fn normalize_entity_identity(schema: &mut CompiledSchema) {
    for ty in &mut schema.types {
        for field in &mut ty.fields {
            if field.name == "id" && field.field_type == FieldType::Uuid {
                field.field_type = FieldType::Id;
            }
        }
    }
}
