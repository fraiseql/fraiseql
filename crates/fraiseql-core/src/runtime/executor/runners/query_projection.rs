//! Projection field builders and type-mapping utilities for query execution.
//!
//! These pure functions transform GraphQL selection sets into SQL projection
//! hints and enrich ORDER BY clauses with schema-derived type information.

use crate::{
    db::{OrderByClause, OrderByFieldType, ProjectionField},
    db::projection_generator::FieldKind,
    graphql::FieldSelection,
    schema::CompiledSchema,
};

/// Build a recursive [`ProjectionField`] tree from a GraphQL selection set.
///
/// For each field in `selections`, consults the compiled schema to determine
/// whether the field is composite (Object) or scalar, and — for Object fields —
/// recurses into the requested sub-fields to produce a nested
/// `jsonb_build_object(...)` at the SQL level instead of returning the full blob.
///
/// List fields always fall back to `data->'field'` (full blob) because
/// sub-projection inside aggregated JSONB arrays is out of scope.
///
/// Recursion is capped at 4 levels, matching `MAX_PROJECTION_DEPTH` in the
/// projection generator.
///
/// Filter `__typename` from SQL projection fields.
/// `__typename` is a GraphQL meta-field not stored in JSONB.
/// The `ResultProjector` handles injection — see `projection.rs`.
/// Removing this filter causes `data->>'__typename'` (NULL) to overwrite
/// the value injected by `with_typename()`, depending on field iteration order.
pub fn build_typed_projection_fields(
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    parent_type_name: &str,
    depth: usize,
) -> Vec<ProjectionField> {
    const MAX_DEPTH: usize = 4;

    let type_def = schema.find_type(parent_type_name);
    selections
        .iter()
        // Skip __typename — it is a GraphQL meta-field not stored in the JSONB column.
        // Including it would generate `data->>'__typename'` (always NULL) in the SQL
        // projection and then overwrite the value already injected by `with_typename`.
        .filter(|sel| sel.name != "__typename")
        .map(|sel| {
            let field_def =
                type_def.and_then(|td| td.fields.iter().find(|f| f.name == sel.name.as_str()));

            let is_composite = field_def.is_some_and(|fd| !fd.field_type.is_scalar());
            let is_list = field_def.is_some_and(|fd| fd.field_type.is_list());
            let is_text = field_def.is_some_and(|fd| {
                matches!(
                    fd.field_type,
                    crate::schema::FieldType::String | crate::schema::FieldType::Id
                )
            });

            let kind = if is_composite {
                FieldKind::Composite
            } else if is_text {
                FieldKind::Text
            } else {
                FieldKind::Native
            };

            // Recurse into Object types only — List fields fall back to full blob
            let sub_fields =
                if is_composite && !is_list && !sel.nested_fields.is_empty() && depth < MAX_DEPTH {
                    let child_type =
                        field_def.and_then(|fd| fd.field_type.type_name()).unwrap_or("");
                    if child_type.is_empty() {
                        None
                    } else {
                        Some(build_typed_projection_fields(
                            &sel.nested_fields,
                            schema,
                            child_type,
                            depth + 1,
                        ))
                    }
                } else {
                    None
                };

            ProjectionField {
                name: sel.response_key().to_string(),
                kind,
                sub_fields,
            }
        })
        .collect()
}

/// Map a schema [`FieldType`] to the ORDER BY cast hint.
///
/// Returns [`OrderByFieldType::Text`] for types that sort correctly as text
/// (strings, UUIDs, enums) or for composite/container types where a cast
/// would be meaningless.
const fn field_type_to_order_by_type(ft: &crate::schema::FieldType) -> OrderByFieldType {
    use crate::schema::FieldType as FT;
    match ft {
        FT::Int => OrderByFieldType::Integer,
        FT::Float | FT::Decimal => OrderByFieldType::Numeric,
        FT::Boolean => OrderByFieldType::Boolean,
        FT::DateTime => OrderByFieldType::DateTime,
        FT::Date => OrderByFieldType::Date,
        FT::Time => OrderByFieldType::Time,
        // String, ID, UUID, Json, Enum, Scalar, and container types sort as text.
        _ => OrderByFieldType::Text,
    }
}

/// Enrich parsed `OrderByClause` values with schema-derived type information
/// and native column mappings.
///
/// For each clause, looks up the field in the compiled schema's type definition
/// to determine the correct `OrderByFieldType` (so the SQL generator emits a
/// typed cast), and checks `native_columns` for a direct column mapping (so the
/// SQL generator can bypass JSONB extraction entirely).
pub fn enrich_order_by_clauses(
    mut clauses: Vec<OrderByClause>,
    schema: &CompiledSchema,
    return_type: &str,
    native_columns: &std::collections::HashMap<String, String>,
) -> Vec<OrderByClause> {
    let type_def = schema.find_type(return_type);
    for clause in &mut clauses {
        // Look up the field type from the schema definition.
        if let Some(td) = type_def {
            if let Some(field_def) = td.find_field(&clause.field) {
                clause.field_type = field_type_to_order_by_type(&field_def.field_type);
            }
        }

        // Check if the query definition has a native column mapping for this field.
        // `native_columns` keys are the GraphQL argument names (camelCase).
        let storage_key = clause.storage_key();
        if native_columns.contains_key(&storage_key) {
            clause.native_column = Some(storage_key);
        }
    }
    clauses
}

/// Return `true` if `field_name` appears in `selections`, including inside inline
/// fragment entries (`FieldSelection` whose name starts with `"..."`).
///
/// Named fragment spreads are already flattened by [`FragmentResolver`] before this
/// is called, so we only need to recurse one level into inline fragments.
pub fn selections_contain_field(
    selections: &[FieldSelection],
    field_name: &str,
) -> bool {
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
