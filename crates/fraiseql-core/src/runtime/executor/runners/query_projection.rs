//! Projection field builders and type-mapping utilities for query execution.
//!
//! These pure functions transform GraphQL selection sets into SQL projection
//! hints and enrich ORDER BY clauses with schema-derived type information.

use std::sync::Arc;

use crate::{
    backend::{
        OrderByClause, ProjectionField, ScalarFieldType,
        projection_generator::FieldKind,
        where_clause::{SharedFieldTypes, WhereFieldSchema},
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
/// An object field is projected through its sub-selection at every depth, and one with
/// no sub-selection projects to `{}`: no depth or shape falls back to the stored
/// sub-object. The recursion follows the selection, which `max_query_depth` bounds. A
/// cap of 4 used to hand everything below it back as stored — every key of an object
/// five levels down, gated or not.
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
) -> Vec<ProjectionField> {
    let type_def = schema.find_type(parent_type_name);
    selections
        .iter()
        // Skip __typename — it is a GraphQL meta-field not stored in the JSONB column.
        // Including it would generate `data->>'__typename'` (always NULL) in the SQL
        // projection and then overwrite the value already injected by `with_typename`.
        .filter(|sel| sel.name != "__typename")
        .map(|sel| {
            // A localized field's translations sibling reads the field's stored map (#1513).
            if let Some(base) = type_def.and_then(|td| td.translations_of(&sel.name)) {
                return ProjectionField {
                    name:       sel.response_key().to_string(),
                    source:     base.name.to_string(),
                    kind:       FieldKind::Composite,
                    sub_fields: None,
                    localized:  crate::runtime::translations_read(schema, sel),
                    computed:   None,
                };
            }
            let field_def =
                type_def.and_then(|td| td.fields.iter().find(|f| f.name == sel.name.as_str()));

            let is_composite = field_def.is_some_and(|fd| !fd.field_type.is_scalar());
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

            // Recurse into a single object — an enum is composite too, and is its stored
            // value. A list falls back to its stored array, which `project_nested_lists`
            // re-projects element by element.
            let sub_fields = field_def
                .filter(|fd| {
                    matches!(
                        fd.field_type,
                        crate::schema::FieldType::Object(_)
                            | crate::schema::FieldType::Interface(_)
                            | crate::schema::FieldType::Union(_)
                    )
                })
                .and_then(|fd| fd.field_type.type_name())
                .map(|child_type| {
                    build_typed_projection_fields(&sel.nested_fields, schema, child_type)
                });

            ProjectionField {
                // Output under the response key (alias when present)…
                name: sel.response_key().to_string(),
                // …but read the JSONB column from the *source* field name so an
                // aliased field reads the right column (#418).
                source: sel.name.clone(),
                kind,
                sub_fields,
                // #1513: a localized field reads its selection's chain.
                localized: field_def
                    .filter(|fd| fd.localized)
                    .and_then(|_| crate::runtime::selection_chain(schema, sel))
                    .map(fraiseql_db::LocalizedRead::Label),
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
/// Only the top level is mapped **here**. A nested relation path
/// (`machine.installed_at`) gets its cast during the parse descent instead
/// (#1157): `WhereFieldInfo` carries the declared cast at every level, and the
/// parser records the dotted path it actually walked. Resolving per level
/// rather than enumerating paths up front is what keeps a relation cycle
/// (`Order → Customer → Order`) from making the path set unbounded — the set
/// is bounded by the query's own depth.
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

    // Whether `{field: {sub: …}}` is a legitimate nested predicate is decided by
    // the same function that decides whether the published `{Entity}WhereInput`
    // gives that field a nested filter type. Answering it twice is how a schema
    // ends up advertising a filter the engine refuses — or accepting one the
    // schema says is not there.
    let known = crate::schema::derived_inputs::where_keys_of(schema, type_def);

    let fields =
        WhereFieldSchema::with_relations(casts, known, Arc::clone(&schema.where_relation_fields));
    // #1513: a filter on a localized field reads the request locale's label, collated.
    match (
        crate::runtime::localization_chain(schema),
        crate::runtime::request_locale(schema),
    ) {
        (Some(chain), Some(locale)) => {
            fields.with_locale(fraiseql_db::where_clause::LocaleReading {
                chain,
                collation: schema.locale.as_ref().and_then(|l| l.collation(&locale)),
            })
        },
        _ => fields,
    }
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
pub const fn field_type_to_where_type(ft: &crate::schema::FieldType) -> ScalarFieldType {
    use crate::schema::FieldType as FT;
    match ft {
        // Identity fields compare case-insensitively over the two renderings of
        // a UUID. `ScalarFieldType::Uuid` emits **no cast** — it marks the
        // comparison semantics, not a SQL type. The ORDER BY sibling keeps
        // `Text`, which is why the two were split before this landed.
        FT::Id | FT::Uuid => ScalarFieldType::Uuid,
        // The four pgvector field types, spelled out rather than left to
        // `scalar_cast_hint`'s `_ => Text` (#1117). Like `Uuid` above this emits
        // no cast; it tells the distance predicates that the backing view is
        // contractually required to expose a native column, so the operand is
        // `"embedding"` and not `(data->>'embedding')::vector` — a 122×
        // difference, measured, because the JSONB form re-parses every row's
        // embedding out of text.
        //
        // ⚠ All four are named here, and the arm below is still a wildcard — so
        // a *fifth* pgvector type added to `FieldType` would fall through to
        // `Text` and silently keep the slow operand rather than failing to
        // compile. `vector_field_types_are_not_absorbed_by_the_wildcard` is what
        // catches that instead; it asserts over the list, not over one member.
        FT::Vector | FT::BitVector | FT::HalfVector | FT::SparseVector => ScalarFieldType::Vector,
        _ => scalar_cast_hint(ft),
    }
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
///
/// # Sorting by a field that does not exist
///
/// An unknown sort key used to keep `ScalarFieldType`'s default and lower to a
/// JSONB extraction of a key that is not there — all-NULL, which orders nothing.
/// The client received rows in whatever order the plan happened to produce, with
/// no signal that its sort had been discarded.
///
/// A key is refused unless it is one of the query's
/// [`sortable_keys`](crate::schema::derived_inputs::sortable_keys): a declared field that
/// orders meaningfully, or a native column (a sort key can be legitimate without being a
/// declared field, since the statement below routes a native column straight to its
/// column). The derived `…OrderByField` enum is the same set (#1159).
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the field and the type. Only
/// when the schema can adjudicate: an unknown type, or one carrying no fields,
/// passes unchanged (#939).
pub fn enrich_order_by_clauses(
    mut clauses: Vec<OrderByClause>,
    schema: &CompiledSchema,
    return_type: &str,
    native_columns: &std::collections::HashMap<String, String>,
    security_context: Option<&crate::security::SecurityContext>,
) -> crate::error::Result<Vec<OrderByClause>> {
    let type_def = schema.find_type(return_type);
    // #1159: the keys the derived `…OrderByField` enum lists, and no other. `None` when the
    // schema cannot adjudicate (the type is absent or carries no fields): rejecting on an
    // absence of evidence is what #939 forbids.
    let sortable =
        crate::schema::derived_inputs::sortable_keys(schema, return_type, native_columns);
    // #1512: a text key sorts under the request locale's collation, when there is one.
    let collation = schema
        .locale
        .as_ref()
        .zip(crate::runtime::request_locale(schema))
        .and_then(|(config, locale)| config.collation(&locale));

    for clause in &mut clauses {
        if let Some(keys) = &sortable {
            if !keys.contains(&clause.field) {
                return Err(unknown_sort_field(&clause.field, return_type, keys));
            }
        }

        // Look up the field type from the schema definition.
        if let Some(td) = type_def {
            if let Some(field_def) = td.find_field(&clause.field) {
                // Ordering by a field the caller may not read ranks rows by it (ruling AA 3).
                super::super::support::security::refuse_unreadable_reference(
                    schema,
                    return_type,
                    field_def,
                    security_context,
                    "order by",
                )?;
                clause.field_type = field_type_to_order_by_type(&field_def.field_type);
                if matches!(field_def.field_type, crate::schema::FieldType::String)
                    && native_columns
                        .get(&clause.storage_key())
                        .is_none_or(|cast| sorts_as_text(cast))
                {
                    clause.collation.clone_from(&collation);
                }
                // #1513: a localized field sorts by its label, not its stored map.
                if field_def.localized {
                    clause.localized = crate::runtime::localization_chain(schema);
                }
            }
        }

        // Check if the query definition has a native column mapping for this field.
        // `native_columns` keys are the GraphQL argument names (camelCase).
        let storage_key = clause.storage_key();
        if let Some(native_type) = native_columns.get(&storage_key) {
            // #1521: the keyset binds the cursor's value for this key as this type.
            clause.native_type = Some(native_type.clone());
            clause.native_column = Some(storage_key);
        }
    }
    Ok(clauses)
}

/// Whether a native column of PostgreSQL type `cast` is text, and so takes a `COLLATE`
/// (#1512). An empty cast is an untyped column read as text. Anything else (a number, a
/// uuid, a timestamp) sorts by its own type, which a collation does not apply to.
pub(super) fn sorts_as_text(cast: &str) -> bool {
    matches!(
        cast.to_ascii_lowercase().as_str(),
        "" | "text" | "varchar" | "character varying" | "bpchar" | "character" | "citext"
    )
}

/// The error for a sort key the query does not accept, suggesting the nearest it does.
fn unknown_sort_field(
    field: &str,
    return_type: &str,
    sortable: &[String],
) -> crate::error::FraiseQLError {
    let candidates: Vec<&str> = sortable.iter().map(String::as_str).collect();
    let subject = format!("Cannot sort by '{field}' on type '{return_type}'.");
    let message = match super::super::super::suggest_similar(field, &candidates).as_slice() {
        [s] => format!("{subject} Did you mean '{s}'?"),
        [a, b] => format!("{subject} Did you mean '{a}' or '{b}'?"),
        [a, b, c, ..] => format!("{subject} Did you mean '{a}', '{b}', or '{c}'?"),
        _ => subject,
    };
    crate::error::FraiseQLError::Validation {
        message,
        path: Some(format!("orderBy.{field}")),
    }
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
        let expr = crate::backend::order_by::vector_distance_expr(
            clause,
            crate::backend::DatabaseType::PostgreSQL,
        )?
        .ok_or_else(|| {
            crate::error::FraiseQLError::internal("a nearest clause carried no vector operand")
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
    /// The WHERE and ORDER BY hints **have** diverged, and only for identities.
    ///
    /// This test was written to fail when the split stopped being cosmetic, so
    /// the divergence had to be written down rather than discovered. It has now
    /// fired once, for exactly `ID` and `UUID`: the filter compares them
    /// case-insensitively while the sort keeps text ordering, which is correct
    /// today and must not change.
    #[test]
    fn the_where_and_order_by_cast_hints_diverge_only_for_identity_fields() {
        for ft in representative_field_types() {
            let where_ty = field_type_to_where_type(&ft);
            let order_ty = field_type_to_order_by_type(&ft);
            if matches!(ft, FT::Id | FT::Uuid) {
                assert_eq!(where_ty, ScalarFieldType::Uuid, "identity filters compare as identity");
                assert_eq!(order_ty, ScalarFieldType::Text, "identity sorts stay text ordering");
            } else {
                assert_eq!(
                    where_ty, order_ty,
                    "only identities may diverge; {ft:?} did — intended? then update this"
                );
            }
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
    /// Was `id_equality_is_case_sensitive_text_equality` — the defect, pinned
    /// where it is decided. It is now fixed: identity fields carry their own
    /// comparison semantics rather than sharing `Text` with strings.
    #[test]
    fn identity_fields_compare_as_identities_not_as_text() {
        assert_eq!(field_type_to_where_type(&FT::Id), ScalarFieldType::Uuid);
        assert_eq!(field_type_to_where_type(&FT::Uuid), ScalarFieldType::Uuid);
        assert_ne!(
            field_type_to_where_type(&FT::String),
            ScalarFieldType::Uuid,
            "a plain string that happens to hold a UUID must keep text semantics"
        );
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

#[cfg(test)]
mod order_by_validation {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        backend::OrderByClause,
        schema::{FieldDefinition, FieldType, TypeDefinition},
    };

    fn schema_with_order() -> CompiledSchema {
        let mut schema = CompiledSchema::default();
        let mut order = TypeDefinition::new("Order", "v_order");
        order.fields.push(FieldDefinition::new("reference", FieldType::String));
        order.fields.push(FieldDefinition::new("total", FieldType::Decimal));
        schema.types.push(order);
        schema
    }

    fn clause(field: &str) -> Vec<OrderByClause> {
        vec![OrderByClause::new(
            field.to_string(),
            crate::backend::OrderDirection::Asc,
        )]
    }

    #[test]
    fn an_unknown_sort_field_is_refused() {
        let err = enrich_order_by_clauses(
            clause("totallyBogusField"),
            &schema_with_order(),
            "Order",
            &HashMap::new(),
            None,
        )
        .expect_err("an unknown sort key silently ordered nothing");
        let msg = err.to_string();
        assert!(msg.contains("totallyBogusField"), "must name the field: {msg}");
        assert!(msg.contains("Order"), "must name the type: {msg}");
    }

    #[test]
    fn a_near_miss_sort_field_gets_a_did_you_mean_hint() {
        let err = enrich_order_by_clauses(
            clause("referense"),
            &schema_with_order(),
            "Order",
            &HashMap::new(),
            None,
        )
        .expect_err("a typo must be refused");
        assert!(err.to_string().contains("Did you mean 'reference'?"), "got: {err}");
    }

    #[test]
    fn a_declared_sort_field_passes_and_keeps_its_cast() {
        let enriched = enrich_order_by_clauses(
            clause("total"),
            &schema_with_order(),
            "Order",
            &HashMap::new(),
            None,
        )
        .expect("a declared field is a legitimate sort key");
        assert_eq!(
            enriched[0].field_type,
            ScalarFieldType::Numeric,
            "the declared type must still drive the cast"
        );
    }

    /// A sort key can be legitimate **without** being a declared type field: the
    /// statement right below the check routes a native column to a real column.
    /// Rejecting on "not a type field" alone would break those deployments.
    #[test]
    fn a_native_column_that_is_not_a_type_field_passes() {
        let mut native = HashMap::new();
        native.insert("pk_order".to_string(), "int4".to_string());
        let enriched = enrich_order_by_clauses(
            clause("pk_order"),
            &schema_with_order(),
            "Order",
            &native,
            None,
        )
        .expect("a native column is a legitimate sort key");
        assert_eq!(
            enriched[0].native_column.as_deref(),
            Some("pk_order"),
            "the native mapping must still be applied"
        );
    }

    #[test]
    fn an_unknown_type_cannot_adjudicate_and_passes() {
        enrich_order_by_clauses(
            clause("anything"),
            &schema_with_order(),
            "NoSuchType",
            &HashMap::new(),
            None,
        )
        .expect("no type information — a rejection cannot be justified");
    }

    #[test]
    fn a_type_with_no_fields_cannot_adjudicate_and_passes() {
        let mut schema = CompiledSchema::default();
        schema.types.push(TypeDefinition::new("Order", "v_order"));
        enrich_order_by_clauses(clause("anything"), &schema, "Order", &HashMap::new(), None)
            .expect("an empty field list is absence of evidence, not evidence of absence");
    }
}
