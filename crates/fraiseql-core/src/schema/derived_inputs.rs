//! The filter and sort input types FraiseQL derives from a compiled schema.
//!
//! `where` and `orderBy` are auto-wired arguments: a query enables them through
//! [`AutoParams`](super::AutoParams) and the runtime reads their raw value off
//! the argument map, so they are deliberately absent from
//! [`QueryDefinition::arguments`](super::QueryDefinition::arguments). Until
//! v2.15.0 they were also *typed* as the `JSON` scalar, which made the
//! conventional names every client writes — `OrderWhereInput`,
//! `OrderOrderByInput` — resolve against nothing, and § 5.8.2 refused documents
//! that were correct in every other respect.
//!
//! This module makes those names real. It is the **only** place they are
//! spelled, so the name a query publishes and the type a schema defines cannot
//! drift: [`CompiledSchema::build_indexes`](super::CompiledSchema::build_indexes)
//! materialises what [`derive()`] returns, and
//! [`QueryDefinition::graphql_arguments`](super::QueryDefinition::graphql_arguments)
//! types an argument **iff** the schema carries the type it would name.
//!
//! # What is not derived, and why
//!
//! Deriving a filter is advertising an operator. #869 deleted a 48-type
//! `<RichType>WhereInput` surface that advertised 35 operator names the WHERE
//! parser could not serve, two of which silently bound to unrelated operators.
//! Every rule below is the same rule: **do not advertise what the engine cannot
//! run, and do not advertise a name no type backs.**
//!
//! * The **fulltext, network and ltree** operator families are not bucketed onto any leaf.
//!   `matches` needs a `tsvector`, `is_private` an `inet`, `ancestor_of` an `ltree`; a declared
//!   field type cannot say whether the column behind it is one. They stay executable — nothing
//!   coerces argument values — but they are not promised.
//! * A **list of relations** gets a list filter, not a nested entity filter. The engine lowers
//!   `{lines: {sku: {eq: …}}}` to `data->'lines'->>'sku'`, which cannot index into an array and so
//!   matches nothing, silently.
//! * An `Object(T)` that **no type declares** is an opaque leaf, not a relation. Authoring layers
//!   emit these — one real schema carries `datetime`, `date`, `dict`, `IPAddress`, `Hostname`,
//!   `LTree`, `MACAddress` and `CIDR` — and the runtime already treats them as text.
//! * A name the **author declared** is never derived over.
//!
//! # This is advertisement, not enforcement
//!
//! Per-scalar filters are stricter than the engine, which restricts no operator
//! by field type. Nothing coerces a query argument's value against its declared
//! input type (`arg_type` is read at runtime only on the mutation path), so a
//! stricter published type refuses nothing at execution — it constrains clients
//! that validate locally against introspection.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use fraiseql_db::where_clause::{RelationFieldMaps, WhereFieldInfo};

use super::{
    CompiledSchema, EnumDefinition, EnumValueDefinition, FieldType, InputFieldDefinition,
    InputObjectDefinition,
};

/// The enum a derived `orderBy` item's `direction` is typed with.
pub const SORT_DIRECTION_ENUM: &str = "SortDirection";

/// Comparison operators, advertised on every leaf.
pub const COMPARISON_OPERATORS: &[&str] = &["eq", "neq", "gt", "gte", "lt", "lte", "in", "nin"];

/// NULL checks, advertised on every leaf.
pub const NULL_OPERATORS: &[&str] = &["isnull", "is_not_null"];

/// LIKE/ILIKE/regex operators, advertised on text-backed leaves.
pub const STRING_OPERATORS: &[&str] = &[
    "contains",
    "icontains",
    "startswith",
    "istartswith",
    "endswith",
    "iendswith",
    "like",
    "ilike",
    "nlike",
    "nilike",
    "regex",
    "iregex",
    "nregex",
    "niregex",
];

/// JSONB array containment and length operators, advertised on lists and `JSON`.
pub const ARRAY_OPERATORS: &[&str] = &[
    "array_contains",
    "array_contained_by",
    "array_overlaps",
    "len_eq",
    "len_neq",
    "len_gt",
    "len_gte",
    "len_lt",
    "len_lte",
];

/// JSONB containment, advertised on `JSON`.
pub const CONTAINMENT_OPERATORS: &[&str] = &["strictly_contains"];

/// pgvector distance operators, advertised on the vector leaves.
pub const VECTOR_OPERATORS: &[&str] = &[
    "cosine_distance",
    "l2_distance",
    "l1_distance",
    "hamming_distance",
    "inner_product",
    "jaccard_distance",
];

/// The combinators [`WhereClause::from_graphql_json`] implements — and the only
/// ones it implements.
///
/// v1 emitted `AND`/`OR`/`NOT`. Publishing that spelling against this engine
/// would advertise three fields no request can execute.
///
/// [`WhereClause::from_graphql_json`]: fraiseql_db::where_clause::WhereClause::from_graphql_json
pub const COMBINATORS: [&str; 3] = ["_and", "_or", "_not"];

/// The input objects and enums a schema's auto-wired filter/sort surface needs.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DerivedInputs {
    /// Entity filters, scalar filters and `orderBy` items, in a deterministic order.
    pub input_types: Vec<InputObjectDefinition>,

    /// [`SORT_DIRECTION_ENUM`], when a sort surface was derived and the schema
    /// does not already declare it.
    pub enums: Vec<EnumDefinition>,
}

/// The type name a `where` argument on `entity` publishes.
#[must_use]
pub fn where_input_type_name(entity: &str) -> String {
    format!("{entity}WhereInput")
}

/// The item type name an `orderBy` argument on `entity` publishes.
#[must_use]
pub fn order_by_input_type_name(entity: &str) -> String {
    format!("{entity}OrderByInput")
}

/// Everything the schema's `where`/`orderBy` surface needs and does not already
/// declare.
///
/// Deterministic: entity filters follow the breadth-first walk from the queries
/// in declaration order, `orderBy` items follow query order, and scalar filters
/// are sorted by name.
#[must_use]
pub fn derive(schema: &CompiledSchema) -> DerivedInputs {
    let mut leaves: BTreeMap<String, LeafFilter> = BTreeMap::new();
    let mut entities: Vec<InputObjectDefinition> = Vec::new();
    let mut walked: Vec<String> = Vec::new();

    // Breadth-first from every entity a `where`-enabled query returns, following
    // single-object relations. The walk continues through an entity whose filter
    // the author declared — their type is kept, but the entities it can reach
    // still need theirs.
    let mut queue: Vec<String> = filterable_entities(schema);
    while let Some(entity) = pop_front(&mut queue) {
        if walked.contains(&entity) {
            continue;
        }
        let Some(type_def) = adjudicable_type(schema, &entity) else {
            continue;
        };
        walked.push(entity.clone());

        let mut fields = Vec::new();
        for field in &type_def.fields {
            match classify(schema, &field.field_type) {
                Slot::Relation(target) => {
                    queue.push(target.clone());
                    fields.push(
                        InputFieldDefinition::new(
                            field.name.as_str(),
                            where_input_type_name(&target),
                        )
                        .with_description(format!("Filter by the related `{target}`.")),
                    );
                },
                Slot::Leaf(leaf) => {
                    let type_name = leaf.type_name();
                    fields.push(
                        InputFieldDefinition::new(field.name.as_str(), type_name.clone())
                            .with_description(format!("Filter on `{}`.", field.name)),
                    );
                    leaves.entry(type_name).or_insert(leaf);
                },
                // Omission is the safe direction: an interface- or union-typed
                // field has no filter shape this can justify, and inventing one
                // would advertise a predicate the engine cannot honour.
                Slot::Unfilterable => {},
            }
        }

        let name = where_input_type_name(&entity);
        if schema.find_input_type(&name).is_some() {
            continue;
        }
        fields.extend(combinator_fields(&name));
        entities.push(InputObjectDefinition::new(name).with_fields(fields).with_description(
            format!(
                "Filter predicate over `{entity}`. Top-level keys are combined with AND; \
                     `_and`/`_or`/`_not` nest explicitly."
            ),
        ));
    }

    let mut input_types = entities;
    input_types.extend(order_by_inputs(schema));
    // A leaf filter is collected while walking an entity, including an entity
    // whose own filter is already present — so the "already declared" check has
    // to happen here rather than at collection time, or a second `build_indexes`
    // would append every leaf again.
    input_types.extend(
        leaves
            .into_values()
            .filter(|leaf| schema.find_input_type(&leaf.type_name()).is_none())
            .map(LeafFilter::into_input_object),
    );

    let mut enums = Vec::new();
    if input_types.iter().any(|i| i.name.ends_with("OrderByInput"))
        && schema.find_enum(SORT_DIRECTION_ENUM).is_none()
    {
        enums.push(
            EnumDefinition::new(SORT_DIRECTION_ENUM)
                .with_value(EnumValueDefinition::new("ASC"))
                .with_value(EnumValueDefinition::new("DESC"))
                .with_description("Sort direction."),
        );
    }

    DerivedInputs { input_types, enums }
}

/// The `orderBy` item type for every entity a sortable query returns.
///
/// Follows queries rather than the relation closure: `orderBy` applies at the
/// query's own level and never nests.
fn order_by_inputs(schema: &CompiledSchema) -> Vec<InputObjectDefinition> {
    let mut out: Vec<InputObjectDefinition> = Vec::new();
    for query in &schema.queries {
        if query.relay || !query.auto_params.has_order_by {
            continue;
        }
        let name = order_by_input_type_name(&query.return_type);
        if schema.find_input_type(&name).is_some() || out.iter().any(|i| i.name == name) {
            continue;
        }
        if adjudicable_type(schema, &query.return_type).is_none() {
            continue;
        }
        out.push(
            InputObjectDefinition::new(name)
                .with_fields(vec![
                    // `String!`, not an enum of sortable fields:
                    // `enrich_order_by_clauses` accepts a declared field *or* a
                    // native column, and a native column need not be a declared
                    // field. An enum would advertise a narrower sort surface
                    // than the engine's on exactly the queries compiled with
                    // `--database`.
                    InputFieldDefinition::new("field", "String")
                        .with_nullable(false)
                        .with_description("Field to sort by."),
                    InputFieldDefinition::new("direction", SORT_DIRECTION_ENUM)
                        .with_default_value("ASC")
                        .with_description("Sort direction; ascending when omitted."),
                ])
                .with_description(format!("One sort key over `{}`.", query.return_type)),
        );
    }
    out
}

/// Entities a `where`-enabled query returns.
///
/// Relay connections are excluded: their argument surface is owned by each
/// renderer's relay path, and `graphql_arguments` returns them unchanged.
fn filterable_entities(schema: &CompiledSchema) -> Vec<String> {
    schema
        .queries
        .iter()
        .filter(|q| !q.relay && q.auto_params.has_where)
        .map(|q| q.return_type.clone())
        .collect()
}

/// The type definition backing `name`, when the schema can adjudicate its
/// fields.
///
/// A type that is absent, or present with no fields, means the compiler emitted
/// no field information — not that the entity has no fields. Deriving a filter
/// from that absence would publish an input object that positively forbids every
/// key the engine still accepts (#939).
fn adjudicable_type<'a>(
    schema: &'a CompiledSchema,
    name: &str,
) -> Option<&'a super::TypeDefinition> {
    schema.find_type(name).filter(|t| !t.fields.is_empty())
}

fn combinator_fields(self_name: &str) -> Vec<InputFieldDefinition> {
    vec![
        InputFieldDefinition::new("_and", format!("[{self_name}!]"))
            .with_description("Every predicate in the list must match."),
        InputFieldDefinition::new("_or", format!("[{self_name}!]"))
            .with_description("At least one predicate in the list must match."),
        // Typed, not `JSON`: v1 left `NOT` untyped while typing `AND`/`OR`, and
        // an untyped hole in a typed surface is where the next silent wrong
        // answer lives.
        InputFieldDefinition::new("_not", self_name)
            .with_description("The predicate must not match."),
    ]
}

/// Pop from the front, keeping the walk breadth-first and its output stable.
fn pop_front(queue: &mut Vec<String>) -> Option<String> {
    if queue.is_empty() {
        return None;
    }
    Some(queue.remove(0))
}

/// What a field of a given type contributes to its entity's filter.
enum Slot {
    /// A nested entity predicate: `{T}WhereInput`.
    Relation(String),
    /// An operator bag over a leaf type.
    Leaf(LeafFilter),
    /// No filter shape this can justify; the field is left out of the surface.
    Unfilterable,
}

/// Which operator families a leaf advertises beyond comparison and NULL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Ops {
    /// LIKE/ILIKE/regex — text-backed leaves.
    string:      bool,
    /// JSONB array containment and length.
    array:       bool,
    /// JSONB containment.
    containment: bool,
    /// pgvector distances.
    vector:      bool,
}

impl Ops {
    const fn none() -> Self {
        Self {
            string:      false,
            array:       false,
            containment: false,
            vector:      false,
        }
    }

    const fn string() -> Self {
        Self {
            string: true,
            ..Self::none()
        }
    }

    const fn array() -> Self {
        Self {
            array: true,
            ..Self::none()
        }
    }

    const fn json() -> Self {
        Self {
            array: true,
            containment: true,
            ..Self::none()
        }
    }

    const fn vector() -> Self {
        Self {
            vector: true,
            ..Self::none()
        }
    }

    /// Every operator name this leaf advertises, in a fixed family order.
    fn operator_names(self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = COMPARISON_OPERATORS.to_vec();
        names.extend(NULL_OPERATORS);
        if self.string {
            names.extend(STRING_OPERATORS);
        }
        if self.array {
            names.extend(ARRAY_OPERATORS);
        }
        if self.containment {
            names.extend(CONTAINMENT_OPERATORS);
        }
        if self.vector {
            names.extend(VECTOR_OPERATORS);
        }
        names
    }
}

/// An operator bag over one leaf type.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LeafFilter {
    /// Names the generated type: `{key}Filter`.
    key:     String,
    /// GraphQL type of a single operand — what `eq` takes.
    operand: String,
    ops:     Ops,
}

impl LeafFilter {
    fn type_name(&self) -> String {
        format!("{}Filter", self.key)
    }

    fn into_input_object(self) -> InputObjectDefinition {
        let fields = self
            .ops
            .operator_names()
            .into_iter()
            .map(|op| {
                InputFieldDefinition::new(op, operand_type(op, &self.operand))
                    .with_description(operator_description(op))
            })
            .collect();
        InputObjectDefinition::new(self.type_name())
            .with_fields(fields)
            .with_description(format!(
                "Filter operators over `{}`. Every key is ANDed.",
                self.operand
            ))
    }
}

/// The GraphQL type an operator's operand takes.
///
/// Follows the operator, not the field: `in` takes a list of the field's type,
/// a NULL check takes a `Boolean`, a LIKE pattern is always a `String`, a length
/// comparison an `Int`, and JSONB containment a `JSON` document.
fn operand_type(op: &str, operand: &str) -> String {
    if matches!(op, "in" | "nin") {
        return format!("[{operand}!]");
    }
    if NULL_OPERATORS.contains(&op) {
        return "Boolean".to_string();
    }
    if STRING_OPERATORS.contains(&op) {
        return "String".to_string();
    }
    if op.starts_with("len_") {
        return "Int".to_string();
    }
    if ARRAY_OPERATORS.contains(&op) || CONTAINMENT_OPERATORS.contains(&op) {
        return "JSON".to_string();
    }
    operand.to_string()
}

/// One line of prose per operator, taken from the SQL it lowers to.
fn operator_description(op: &str) -> String {
    fraiseql_db::where_clause::operator_spec(op)
        .map_or_else(|| format!("`{op}`"), |spec| format!("`{op}` — SQL `{}`.", spec.sql_op))
}

/// The entity a nested `where` predicate on a field of this type filters, if the
/// published surface gives that field a nested filter at all.
///
/// This is the runtime's half of the same question [`derive`] answers when it
/// gives a field `{Target}WhereInput` instead of an operator bag, and it is
/// deliberately the *same* function: `WhereClause::from_graphql_json` decides
/// whether `{field: {sub: …}}` is a legitimate nested predicate, and if it
/// decided differently from the published type the schema would advertise a
/// filter the engine refuses, or accept one the schema says is not there.
///
/// `None` for a scalar, an enum, an opaque leaf, and for a **list** — the engine
/// lowers a nested predicate on a list to a JSON path that cannot index into an
/// array, so it matches nothing, silently.
#[must_use]
pub fn nested_filter_entity(schema: &CompiledSchema, field_type: &FieldType) -> Option<String> {
    match classify(schema, field_type) {
        Slot::Relation(name) => Some(name),
        Slot::Leaf(_) | Slot::Unfilterable => None,
    }
}

/// The `where` keys of every type a nested predicate can descend into, keyed by
/// declared type name.
///
/// Built once per compiled schema and carried on it, because a nested level is
/// adjudicated against the *target* type's keys and rebuilding the closure per
/// request would put a map-per-type allocation in front of every filter.
#[must_use]
pub fn relation_field_maps(schema: &CompiledSchema) -> RelationFieldMaps {
    Arc::new(
        schema
            .types
            .iter()
            .filter(|t| !t.fields.is_empty())
            .map(|t| (t.name.to_string(), Arc::new(where_keys_of(schema, t))))
            .collect(),
    )
}

/// The `where` keys one type declares, in the shape the parser adjudicates
/// against.
pub(crate) fn where_keys_of(
    schema: &CompiledSchema,
    type_def: &super::TypeDefinition,
) -> HashMap<String, WhereFieldInfo> {
    type_def
        .fields
        .iter()
        .map(|f| {
            let relation_type = nested_filter_entity(schema, &f.field_type);
            (
                crate::utils::to_snake_case(f.name.as_str()),
                WhereFieldInfo {
                    declared_name: f.name.to_string(),
                    is_relation: relation_type.is_some(),
                    relation_type,
                },
            )
        })
        .collect()
}

/// The filter slot a field of `field_type` occupies.
fn classify(schema: &CompiledSchema, field_type: &FieldType) -> Slot {
    let leaf = |key: &str, operand: &str, ops: Ops| {
        Slot::Leaf(LeafFilter {
            key: key.to_string(),
            operand: operand.to_string(),
            ops,
        })
    };

    match field_type {
        FieldType::Int => leaf("Int", "Int", Ops::none()),
        FieldType::Float => leaf("Float", "Float", Ops::none()),
        FieldType::Decimal => leaf("Decimal", "Decimal", Ops::none()),
        FieldType::Boolean => leaf("Boolean", "Boolean", Ops::none()),
        FieldType::String => leaf("String", "String", Ops::string()),
        FieldType::Id => leaf("ID", "ID", Ops::none()),
        FieldType::Uuid => leaf("UUID", "UUID", Ops::none()),
        FieldType::DateTime => leaf("DateTime", "DateTime", Ops::none()),
        FieldType::Date => leaf("Date", "Date", Ops::none()),
        FieldType::Time => leaf("Time", "Time", Ops::none()),
        FieldType::Json => leaf("JSON", "JSON", Ops::json()),
        // A dense vector introspects as `JSON`; a bit or sparse vector as the
        // `String` that is its own text form (#959).
        FieldType::Vector => leaf("Vector", "JSON", Ops::vector()),
        FieldType::HalfVector => leaf("HalfVector", "JSON", Ops::vector()),
        FieldType::BitVector => leaf("BitVector", "String", Ops::vector()),
        FieldType::SparseVector => leaf("SparseVector", "String", Ops::vector()),
        // A rich scalar is stored as TEXT, so the text operators are the ones
        // that actually run against it.
        FieldType::Scalar(name) => leaf(name, name, Ops::string()),
        FieldType::Enum(name) => leaf(name, name, Ops::none()),
        FieldType::Object(name) => match adjudicable_type(schema, name) {
            Some(_) => Slot::Relation(name.clone()),
            // No type declares it, so it is not an entity. The runtime already
            // compares it as text.
            None => leaf(name, name, Ops::string()),
        },
        FieldType::List(inner) => match classify(schema, inner) {
            Slot::Leaf(inner_leaf) => Slot::Leaf(LeafFilter {
                key:     format!("{}List", inner_leaf.key),
                operand: format!("[{}!]", inner_leaf.operand),
                ops:     Ops::array(),
            }),
            // A list of a declared entity: a list filter, never a nested entity
            // filter (see the module header).
            Slot::Relation(name) => Slot::Leaf(LeafFilter {
                key:     format!("{name}List"),
                operand: "JSON".to_string(),
                ops:     Ops::array(),
            }),
            Slot::Unfilterable => Slot::Unfilterable,
        },
        // An interface or union field has no single filter shape, and an output
        // field typed as an *input* object is malformed to begin with. Omission
        // is the safe direction: it narrows what is advertised, never what the
        // engine accepts.
        FieldType::Interface(_) | FieldType::Union(_) | FieldType::Input(_) => Slot::Unfilterable,
    }
}

#[cfg(test)]
#[path = "derived_inputs_tests.rs"]
mod tests;
