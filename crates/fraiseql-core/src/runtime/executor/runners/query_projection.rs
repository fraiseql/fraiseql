//! Projection field builders and type-mapping utilities for query execution.
//!
//! These pure functions transform GraphQL selection sets into SQL projection
//! hints and enrich ORDER BY clauses with schema-derived type information.

use std::sync::Arc;

use crate::{
    db::{
        OrderByClause, ProjectionField, ScalarFieldType,
        projection_generator::FieldKind,
        where_clause::{SharedFieldTypes, WhereFieldInfo, WhereFieldSchema},
    },
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
                // Output under the response key (alias when present)…
                name: sel.response_key().to_string(),
                // …but read the JSONB column from the *source* field name so an
                // aliased field reads the right column (#418).
                source: sel.name.clone(),
                kind,
                sub_fields,
                computed: None,
            }
        })
        .collect()
}

/// Declared scalar type of every filterable field on `return_type`, keyed by
/// the JSONB storage key.
///
/// Both ORDER BY and WHERE need this: a JSON extraction is `text`, so the SQL
/// generator has to be told what the field really is before it can compare or
/// sort it. ORDER BY has consulted the schema since it was written; WHERE never
/// did, which is why every date, UUID and string range filter was a hard SQL
/// error (#798) and `in: [19.9]` silently missed rows `eq: 19.9` matched (#800).
///
/// Only the top level is mapped. A nested relation path (`machine.id`) has no
/// entry, and the generator falls back to the JSON value's shape.
///
/// # Adjudication
///
/// The returned [`WhereFieldSchema`] distinguishes *"this type declares these
/// keys"* from *"the schema could not tell us"*. A type that is **not found**,
/// or one that carries **no fields**, yields a carrier that cannot adjudicate —
/// so an unknown `where` key passes rather than being rejected on an absence of
/// evidence (#939). Encoding that as a distinct state, rather than as an empty
/// map, is what stops the allowlist failing open on a missing type or failing
/// closed on every schema without field metadata.
#[must_use]
pub fn where_field_types(schema: &CompiledSchema, return_type: &str) -> WhereFieldSchema {
    let Some(type_def) = schema.find_type(return_type) else {
        return WhereFieldSchema::default();
    };
    if type_def.fields.is_empty() {
        return WhereFieldSchema::default();
    }

    let casts: SharedFieldTypes = Arc::new(
        type_def
            .fields
            .iter()
            .map(|f| {
                (
                    crate::utils::to_snake_case(f.name.as_str()),
                    field_type_to_where_type(&f.field_type),
                )
            })
            .collect(),
    );

    let known = type_def
        .fields
        .iter()
        .map(|f| {
            (
                crate::utils::to_snake_case(f.name.as_str()),
                WhereFieldInfo {
                    declared_name: f.name.to_string(),
                    // A composite field is a relation, so `{field: {sub: …}}` is
                    // a legitimate nested filter on it; on a scalar it is not.
                    is_relation:   !f.field_type.is_scalar(),
                },
            )
        })
        .collect();

    WhereFieldSchema::with_known_keys(casts, known)
}

/// Map a schema [`FieldType`] to the ORDER BY cast hint.
///
/// Returns [`ScalarFieldType::Text`] for types that sort correctly as text
/// (strings, UUIDs, enums) or for composite/container types where a cast
/// would be meaningless.
const fn field_type_to_order_by_type(ft: &crate::schema::FieldType) -> ScalarFieldType {
    scalar_cast_hint(ft)
}

/// Map a schema [`FieldType`] to the WHERE comparison cast hint.
///
/// **Currently byte-identical to [`field_type_to_order_by_type`], deliberately
/// so — this is a decoupling, not a behaviour change.** The two were one
/// function serving two jobs, and that shared identity is what makes the `ID`
/// equality defect (F4) dangerous to fix: the natural repair is to cast a
/// UUID-backed field, but `cast_type_name` is read by *both* the WHERE
/// generator and the ORDER BY renderer (`dialect/trait_def.rs`, pinned by
/// `dialect/capability/tests.rs`), so changing the filter cast silently retypes
/// sorting too.
///
/// Splitting them first means whichever repair is chosen touches one job. The
/// sort behaviour is correct today and must stay byte-identical; the filter
/// behaviour is the one under review.
///
/// # The defect this exists to make fixable
///
/// `ID`/`UUID` land on [`ScalarFieldType::Text`], so equality is case-sensitive
/// text equality against the JSONB rendering — which PostgreSQL emits
/// lower-case. `{"id":{"eq":"0000000a-…-b"}}` matches; the same UUID
/// upper-cased returns **zero rows**, which is indistinguishable from "no rows
/// matched". See `docs/adr/0017-entity-identity-contract.md` for why
/// `FieldType::Id` cannot be assumed UUID-backed: it *intentionally* spans
/// uuid / integer / text keys.
const fn field_type_to_where_type(ft: &crate::schema::FieldType) -> ScalarFieldType {
    scalar_cast_hint(ft)
}

/// The shared type→cast mapping both jobs start from.
const fn scalar_cast_hint(ft: &crate::schema::FieldType) -> ScalarFieldType {
    use crate::schema::FieldType as FT;
    match ft {
        FT::Int => ScalarFieldType::Integer,
        FT::Float | FT::Decimal => ScalarFieldType::Numeric,
        FT::Boolean => ScalarFieldType::Boolean,
        FT::DateTime => ScalarFieldType::DateTime,
        FT::Date => ScalarFieldType::Date,
        FT::Time => ScalarFieldType::Time,
        // String, ID, UUID, Json, Enum, Scalar, and container types sort as text.
        _ => ScalarFieldType::Text,
    }
}

/// Enrich parsed `OrderByClause` values with schema-derived type information
/// and native column mappings.
///
/// For each clause, looks up the field in the compiled schema's type definition
/// to determine the correct `ScalarFieldType` (so the SQL generator emits a
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
pub fn selections_contain_field(selections: &[FieldSelection], field_name: &str) -> bool {
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

#[cfg(test)]
#[path = "query_projection_tests.rs"]
mod query_projection_tests;

/// The computed projection fields for the vector-distance fields a selection
/// asks for (#959).
///
/// A field declaring `vector_distance = "embedding"` is not stored anywhere: its
/// value is the distance expression the `nearest` search ordered by, taken from
/// the very clause that ordered it, so the number a row reports and the position
/// it occupies cannot come from two different computations.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] when such a field is selected on a
/// query that ran no `nearest` search, or one that searched a different vector
/// field. Both are answered rather than nulled: a null here reads as "distance
/// unknown" from a query that looks like it succeeded, and the caller cannot
/// tell it from a row that genuinely has no distance.
pub fn vector_distance_projection_fields(
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    parent_type_name: &str,
    nearest: Option<&OrderByClause>,
) -> crate::error::Result<Vec<ProjectionField>> {
    let Some(type_def) = schema.find_type(parent_type_name) else {
        return Ok(Vec::new());
    };

    let mut fields = Vec::new();
    for sel in selections {
        let Some(field_def) = type_def.fields.iter().find(|f| f.name == sel.name.as_str()) else {
            continue;
        };
        let Some(measures) = field_def.vector_distance.as_deref() else {
            continue;
        };

        let Some(clause) = nearest else {
            return Err(crate::error::FraiseQLError::validation(format!(
                "'{parent_type_name}.{}' is the distance to '{measures}' and is only defined \
                 on a query that searches it; add a `nearest` argument or drop the field",
                field_def.name
            )));
        };
        if clause.field != measures {
            return Err(crate::error::FraiseQLError::validation(format!(
                "'{parent_type_name}.{}' is the distance to '{measures}', but this query \
                 searched '{}'",
                field_def.name, clause.field
            )));
        }
        let expr =
            crate::db::order_by::vector_distance_expr(clause, crate::db::DatabaseType::PostgreSQL)?
                .ok_or_else(|| {
                    crate::error::FraiseQLError::internal(
                        "a nearest clause carried no vector operand",
                    )
                })?;
        fields.push(ProjectionField::computed(sel.response_key(), expr));
    }
    Ok(fields)
}

/// Merge computed distance fields into a typed projection, replacing the
/// stored-key read the selection would otherwise have produced.
///
/// Replacement is by response key and in place, because GraphQL requires the
/// response to keep the query's field order — appending would move the distance
/// to the end of every row.
pub fn merge_computed_fields(typed: &mut Vec<ProjectionField>, computed: Vec<ProjectionField>) {
    for field in computed {
        if let Some(slot) = typed.iter_mut().find(|f| f.name == field.name) {
            *slot = field;
        } else {
            typed.push(field);
        }
    }
}

#[cfg(test)]
mod cast_hint_characterisation {
    use super::*;
    use crate::schema::FieldType as FT;

    /// Every `FieldType` that reaches either cast-hint mapping, so the pinning
    /// below cannot quietly stop covering a variant.
    fn representative_field_types() -> Vec<FT> {
        vec![
            FT::String,
            FT::Int,
            FT::Float,
            FT::Boolean,
            FT::Id,
            FT::DateTime,
            FT::Date,
            FT::Time,
            FT::Json,
            FT::Uuid,
            FT::Decimal,
        ]
    }

    /// The WHERE and ORDER BY hints were one function until this split, and the
    /// split is deliberately behaviour-preserving. This test is the record of
    /// that: when a repair makes them diverge, **this test is what fails**, and
    /// the divergence has to be written down rather than discovered.
    #[test]
    fn the_where_and_order_by_cast_hints_have_not_diverged() {
        for ft in representative_field_types() {
            assert_eq!(
                field_type_to_where_type(&ft),
                field_type_to_order_by_type(&ft),
                "cast hints diverged for {ft:?} — intended? then update this characterisation"
            );
        }
    }

    /// The root cause of F4, pinned where it is decided rather than where it is
    /// observed.
    ///
    /// `ID` and `UUID` map to `Text`, so a filter compares the JSONB text
    /// rendering — which PostgreSQL emits lower-case. The consequence is that
    /// `{"id":{"eq":"0000000A-…"}}` returns **zero rows** for a row that exists,
    /// and zero rows is indistinguishable from "nothing matched".
    ///
    /// Named as a decision, not an accident: see
    /// `docs/adr/0017-entity-identity-contract.md` for why `FieldType::Id`
    /// cannot simply be cast — it intentionally spans uuid / integer / text keys.
    #[test]
    fn id_equality_is_case_sensitive_text_equality() {
        assert_eq!(field_type_to_where_type(&FT::Id), ScalarFieldType::Text);
        assert_eq!(field_type_to_where_type(&FT::Uuid), ScalarFieldType::Text);
    }

    /// The sort side is **correct today** and must stay byte-identical through
    /// any F4 repair: UUIDs sort correctly as text, and a `::uuid` cast in
    /// ORDER BY would both retype the sort and error on any row whose `id` is
    /// not a valid UUID.
    #[test]
    fn identity_fields_sort_as_text_and_that_is_correct() {
        assert_eq!(field_type_to_order_by_type(&FT::Id), ScalarFieldType::Text);
        assert_eq!(field_type_to_order_by_type(&FT::Uuid), ScalarFieldType::Text);
    }

    #[test]
    fn strongly_typed_scalars_keep_their_casts_on_both_sides() {
        assert_eq!(field_type_to_where_type(&FT::Int), ScalarFieldType::Integer);
        assert_eq!(field_type_to_where_type(&FT::Decimal), ScalarFieldType::Numeric);
        assert_eq!(field_type_to_where_type(&FT::DateTime), ScalarFieldType::DateTime);
        assert_eq!(field_type_to_order_by_type(&FT::Int), ScalarFieldType::Integer);
        assert_eq!(field_type_to_order_by_type(&FT::Decimal), ScalarFieldType::Numeric);
        assert_eq!(field_type_to_order_by_type(&FT::DateTime), ScalarFieldType::DateTime);
    }
}
