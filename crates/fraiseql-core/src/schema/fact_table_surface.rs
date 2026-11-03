//! The published surface of the fact tables' `_aggregate` and `_window` roots (#1800).
//!
//! A fact table `tf_<name>` answers two roots, `<name>_aggregate` and `<name>_window`, which
//! the dispatch routes by name. This module derives what introspection and the SDL publish
//! for them, from the fact-table metadata alone:
//!
//! - `<Name>AggregateRow`, the aggregate root's row: every `groupBy` key and every aggregate the
//!   table can compute, each nullable, a key the request did not compute answering `null`;
//! - the aggregate root's argument inputs (`<Name>AggregateWhere`, `…GroupBy`, `…Selection`,
//!   `…Having`, `…OrderBy`), and the window root's, shared by every table (`WindowSpec`, …);
//! - the roots themselves.
//!
//! A row field is typed by what the runtime decodes for it, which follows the declared
//! column type: a `Decimal` measure's sums and averages are exact strings (`Decimal`), other
//! numeric results are numbers. A combination the runtime would answer `null` for, or the
//! database refuse, is not published: an aggregate of a dimension that is no column, an
//! `array_agg` or `string_agg` of a non-text column, `bool_and`/`bool_or` of a non-boolean
//! one, and every aggregate of a non-additive measure.
//!
//! A window row's keys are the aliases the request chooses, so it has no static type: its
//! rows are published as `JSON`, and its arguments are typed.
//!
//! Like the derived filter inputs, none of this is stored in the compiled artifact: it is
//! derived on load, so its content hash does not move.

use super::{
    ArgumentDefinition, CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition,
    FieldType, InputFieldDefinition, InputObjectDefinition, TypeDefinition,
    derived_inputs::{COMPARISON_OPERATORS, NULL_OPERATORS, SORT_DIRECTION_ENUM, pascal_case},
};
use crate::compiler::fact_table::{
    Additivity, FactTableMetadata, GroupKeySource, MeasureColumn, SqlType,
};

/// The LIKE operators a fact-table filter serves. The negated and regex forms are refused by
/// its SQL generator, so they are not published.
const LIKE_OPERATORS: &[&str] = &[
    "contains",
    "icontains",
    "startswith",
    "istartswith",
    "endswith",
    "iendswith",
    "like",
    "ilike",
];

/// The ltree operators a fact-table filter serves on an ltree column.
const LTREE_OPERATORS: &[&str] = &[
    "ancestor_of",
    "descendant_of",
    "matches_lquery",
    "matches_ltxtquery",
    "matches_any_lquery",
    "lca",
    "depth_eq",
    "depth_neq",
    "depth_gt",
    "depth_gte",
    "depth_lt",
    "depth_lte",
];

/// The node-id operators an ltree column with a declared hierarchy serves.
const NODE_ID_OPERATORS: &[&str] = &["descendant_of_id", "ancestor_of_id"];

/// The `having` operators, `<alias>_<op>`.
const HAVING_OPERATORS: &[&str] = &["gt", "gte", "lt", "lte", "eq", "neq"];

/// The measure functions, as their alias suffix spells them.
const MEASURE_FUNCTIONS: &[&str] = &["sum", "avg", "min", "max", "stddev", "variance"];

/// Everything the fact-table roots publish.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FactTableSurface {
    /// The argument inputs, per table and then the shared window inputs.
    pub input_types: Vec<InputObjectDefinition>,
    /// The enums the inputs name.
    pub enums:       Vec<EnumDefinition>,
    /// The aggregate rows, one per table.
    pub row_types:   Vec<TypeDefinition>,
    /// The roots, two per table, ordered by table.
    pub roots:       Vec<FactRoot>,
}

/// One published fact-table root.
#[derive(Debug, Clone, PartialEq)]
pub struct FactRoot {
    /// The root's name, published verbatim: the dispatch routes it by its suffix.
    pub name:       String,
    /// The fact table it reads.
    pub fact_table: String,
    /// Its arguments.
    pub arguments:  Vec<ArgumentDefinition>,
    /// The row type it returns a non-null list of: `None` for a window root, whose rows are
    /// `JSON`.
    pub row_type:   Option<String>,
}

/// A field of an aggregate row, and what produces it.
#[derive(Debug, Clone, PartialEq)]
pub struct RowField {
    /// The key the row answers under.
    pub name:       String,
    /// What computes it, for messages naming two sources of one key.
    pub source:     String,
    /// Its published type.
    pub field_type: FieldType,
    /// Whether it is a `groupBy` key, rather than an aggregate.
    pub group_key:  bool,
}

/// The `PascalCase` stem a fact table's types are named from: `tf_sales_order` →
/// `SalesOrder`. `None` for a table not named `tf_<name>`, which no root reaches.
#[must_use]
pub fn type_stem(table_name: &str) -> Option<String> {
    table_name.strip_prefix("tf_").map(pascal_case)
}

/// The aggregate row type of a fact table.
#[must_use]
pub fn row_type_name(stem: &str) -> String {
    format!("{stem}AggregateRow")
}

/// Every field an aggregate row over `table` can carry, the `groupBy` keys first. A name two
/// sources produce is listed twice, so [`row_field_collisions`] can name both.
#[must_use]
pub fn row_fields(table: &FactTableMetadata) -> Vec<RowField> {
    let mut fields: Vec<RowField> = table
        .group_keys()
        .into_iter()
        .map(|key| RowField {
            field_type: group_key_type(table, &key.source),
            source:     key.source.to_string(),
            name:       key.name,
            group_key:  true,
        })
        .collect();
    let mut mapped: Vec<(&String, &String)> = table.native_dimension_mapping.iter().collect();
    mapped.sort();
    for (key, column) in mapped {
        if fields.iter().any(|f| f.name == *key) {
            continue;
        }
        fields.push(RowField {
            name:       key.clone(),
            source:     format!("the dimension `{key}` mapped to column `{column}`"),
            field_type: filter_column_type(table, column),
            group_key:  true,
        });
    }

    fields.push(aggregate("count", "the count of rows", FieldType::Int));
    for measure in &table.measures {
        fields.extend(measure_fields(measure));
    }
    let mut native: Vec<&String> = table.native_measures.keys().collect();
    native.sort();
    for key in native {
        // A native measure is never read as `Decimal`: its results are numbers.
        for function in MEASURE_FUNCTIONS {
            fields.push(aggregate(
                &format!("{key}_{function}"),
                &format!("the `{function}` of native measure `{key}`"),
                FieldType::Float,
            ));
        }
    }
    for filter in &table.denormalized_filters {
        let name = &filter.name;
        let mut of = |function: &str, field_type: FieldType| {
            fields.push(aggregate(
                &format!("{name}_{function}"),
                &format!("the `{function}` of `{name}`"),
                field_type,
            ));
        };
        if filter.sql_type == SqlType::Boolean {
            of("bool_and", FieldType::Boolean);
            of("bool_or", FieldType::Boolean);
        }
        if filter.sql_type == SqlType::Text {
            of("array_agg", FieldType::list_of_nullable(FieldType::String));
            of("string_agg", FieldType::String);
        }
        of("json_agg", FieldType::Json);
        of("jsonb_agg", FieldType::Json);
    }
    fields
}

/// One message per row field name two sources produce.
#[must_use]
pub fn row_field_collisions(table: &FactTableMetadata) -> Vec<String> {
    let fields = row_fields(table);
    let mut violations = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        if let Some(first) = fields.iter().take(i).find(|f| f.name == field.name) {
            violations.push(format!(
                "fact table `{}`: row field `{}` is both {} and {}: rename the column and \
                 recompile",
                table.table_name, field.name, first.source, field.source
            ));
        }
    }
    violations
}

/// One message per key a fact table publishes that is not a GraphQL name, so cannot be an
/// argument field or a row field (#1800). Each suggests the `snake_case` name it could have.
#[must_use]
pub fn unpublishable_key_violations(table: &FactTableMetadata) -> Vec<String> {
    let mut keys: Vec<(&str, &str)> = table
        .native_dimension_mapping
        .keys()
        .map(|k| ("native dimension mapping key", k.as_str()))
        .chain(table.native_measures.keys().map(|k| ("native measure key", k.as_str())))
        .chain(table.dimensions.paths.iter().map(|p| ("dimension path", p.name.as_str())))
        .chain(table.measures.iter().map(|m| ("measure", m.name.as_str())))
        .chain(
            table
                .denormalized_filters
                .iter()
                .map(|f| ("denormalized filter", f.name.as_str())),
        )
        .filter(|(_, key)| !is_graphql_name(key))
        .collect();
    keys.sort_unstable();
    keys.into_iter()
        .map(|(what, key)| {
            format!(
                "fact table `{}`: {what} `{key}` is not a GraphQL name, so its roots cannot \
                 publish it: rename it `{}` and recompile",
                table.table_name,
                graphql_name_for(key)
            )
        })
        .collect()
}

/// Whether `name` is a GraphQL name: `[_A-Za-z][_0-9A-Za-z]*`.
fn is_graphql_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The `snake_case` GraphQL name a key could have: `dimensions.category.id` →
/// `dimensions_category_id`.
fn graphql_name_for(key: &str) -> String {
    let mut name: String = crate::utils::to_snake_case(key)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    name
}

/// The surface every reachable fact table publishes, tables in name order. A table whose
/// keys cannot be published is left out; the load refuses it.
#[must_use]
pub fn derive(schema: &CompiledSchema) -> FactTableSurface {
    let mut tables: Vec<&FactTableMetadata> = schema.fact_tables.values().collect();
    tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
    let mut surface = FactTableSurface::default();
    for table in tables {
        let Some(stem) = type_stem(&table.table_name) else {
            continue;
        };
        if !unpublishable_key_violations(table).is_empty()
            || !row_field_collisions(table).is_empty()
        {
            continue;
        }
        let root = &table.table_name["tf_".len()..];
        let fields = row_fields(table);
        let where_input = where_input(table, &stem);
        let where_name = where_input.as_ref().map(|i| i.name.clone());
        let inputs = aggregate_inputs(&stem, &fields);
        let mut arguments = Vec::new();
        if let Some(name) = &where_name {
            arguments.push(optional("where", FieldType::Input(name.clone())));
        }
        for (argument, input, list) in [
            ("groupBy", &inputs.group_by, false),
            ("aggregates", &inputs.selection, true),
            ("having", &inputs.having, false),
            ("orderBy", &inputs.order_by, false),
        ] {
            if let Some(input) = input {
                let input_type = FieldType::Input(input.name.clone());
                arguments.push(optional(
                    argument,
                    if list {
                        FieldType::list(input_type)
                    } else {
                        input_type
                    },
                ));
            }
        }
        arguments.extend(page_arguments());
        surface.roots.push(FactRoot {
            name: format!("{root}_aggregate"),
            fact_table: table.table_name.clone(),
            arguments,
            row_type: Some(row_type_name(&stem)),
        });

        let mut window_arguments = vec![
            optional("select", FieldType::list(FieldType::Input("WindowColumn".to_string()))),
            optional("windows", FieldType::list(FieldType::Input("WindowSpec".to_string()))),
        ];
        if let Some(name) = &where_name {
            window_arguments.push(optional("where", FieldType::Input(name.clone())));
        }
        window_arguments.push(optional(
            "orderBy",
            FieldType::list(FieldType::Input("WindowOrderBy".to_string())),
        ));
        window_arguments.extend(page_arguments());
        surface.roots.push(FactRoot {
            name:       format!("{root}_window"),
            fact_table: table.table_name.clone(),
            arguments:  window_arguments,
            row_type:   None,
        });

        surface.row_types.push(row_type(table, &stem, &fields));
        surface.input_types.extend(where_input);
        surface.input_types.extend(
            [
                inputs.group_by,
                inputs.selection,
                inputs.having,
                inputs.order_by,
            ]
            .into_iter()
            .flatten(),
        );
    }
    if !surface.roots.is_empty() {
        let (inputs, enums) = window_inputs();
        surface.input_types.extend(inputs);
        surface.enums.extend(enums);
        surface.enums.push(sort_direction());
    }
    surface
}

/// The names the surface derives, each with what it is, for the load's collision check.
#[must_use]
pub fn derived_names(surface: &FactTableSurface) -> Vec<(&str, &'static str)> {
    surface
        .row_types
        .iter()
        .map(|t| (t.name.as_str(), "object"))
        .chain(surface.input_types.iter().map(|i| (i.name.as_str(), "input")))
        .chain(surface.enums.iter().map(|e| (e.name.as_str(), "enum")))
        .collect()
}

/// The `SortDirection` enum, as the derived `orderBy` inputs publish it.
fn sort_direction() -> EnumDefinition {
    EnumDefinition::new(SORT_DIRECTION_ENUM)
        .with_value(EnumValueDefinition::new("ASC"))
        .with_value(EnumValueDefinition::new("DESC"))
        .with_description("Sort direction.")
}

fn optional(name: &str, arg_type: FieldType) -> ArgumentDefinition {
    ArgumentDefinition::optional(name, arg_type)
}

fn page_arguments() -> [ArgumentDefinition; 2] {
    [
        optional("limit", FieldType::Int).with_description("Maximum number of rows to return."),
        optional("offset", FieldType::Int).with_description("Number of rows to skip."),
    ]
}

fn aggregate(name: &str, source: &str, field_type: FieldType) -> RowField {
    RowField {
        name: name.to_string(),
        source: source.to_string(),
        field_type,
        group_key: false,
    }
}

/// The aggregates of one measure, typed by its column. A non-additive measure has none; a
/// measure reduced over time takes the plain functions only.
fn measure_fields(measure: &MeasureColumn) -> Vec<RowField> {
    if matches!(measure.additivity, Additivity::NonAdditive) {
        return Vec::new();
    }
    let name = &measure.name;
    let mut fields: Vec<RowField> = MEASURE_FUNCTIONS
        .iter()
        .map(|function| {
            aggregate(
                &format!("{name}_{function}"),
                &format!("the `{function}` of measure `{name}`"),
                measure_function_type(&measure.sql_type, function),
            )
        })
        .collect();
    if matches!(measure.additivity, Additivity::Additive) {
        fields.push(aggregate(
            &format!("{name}_count_distinct"),
            &format!("the distinct count of measure `{name}`"),
            FieldType::Int,
        ));
        for function in ["json_agg", "jsonb_agg"] {
            fields.push(aggregate(
                &format!("{name}_{function}"),
                &format!("the `{function}` of measure `{name}`"),
                FieldType::Json,
            ));
        }
    }
    fields
}

/// The type a measure function answers in, following what PostgreSQL returns for the column:
/// a `numeric` result of a `Decimal` measure is an exact string; `SUM` of an `int4` is an
/// `int8`, and `MIN`/`MAX` keep the column's type; every other result is a `numeric` or a
/// `float8`, read as a number.
fn measure_function_type(sql_type: &SqlType, function: &str) -> FieldType {
    match (sql_type, function) {
        (SqlType::Decimal, _) => FieldType::Decimal,
        (SqlType::Int, "sum" | "min" | "max") | (SqlType::BigInt, "min" | "max") => FieldType::Int,
        _ => FieldType::Float,
    }
}

/// The type a `groupBy` key answers in.
fn group_key_type(table: &FactTableMetadata, source: &GroupKeySource) -> FieldType {
    match source {
        GroupKeySource::Filter(column) => filter_column_type(table, column),
        GroupKeySource::Bucket { column, bucket } => {
            let from_calendar = table.calendar_dimensions.iter().any(|c| {
                c.source_column == *column
                    && c.granularities
                        .iter()
                        .any(|g| g.buckets.iter().any(|b| b.bucket_type == *bucket))
            });
            if from_calendar {
                // A calendar bucket is the text of its pre-computed key.
                FieldType::String
            } else {
                match table.denormalized_filters.iter().find(|f| f.name == *column) {
                    Some(f) if f.sql_type == SqlType::Date => FieldType::Date,
                    _ => FieldType::DateTime,
                }
            }
        },
        GroupKeySource::Depth(_) => FieldType::Int,
        // A dimension is read as text, and a tree level is an ltree path.
        _ => FieldType::String,
    }
}

/// The type a denormalized filter column is read in, as a group key.
fn filter_column_type(table: &FactTableMetadata, column: &str) -> FieldType {
    let Some(filter) = table.denormalized_filters.iter().find(|f| f.name == column) else {
        return FieldType::String;
    };
    scalar_of(&filter.sql_type)
}

const fn scalar_of(sql_type: &SqlType) -> FieldType {
    match sql_type {
        SqlType::Int | SqlType::BigInt => FieldType::Int,
        // A `numeric` group key is not one of the exact-string aliases: it is read as a number.
        SqlType::Decimal | SqlType::Float => FieldType::Float,
        SqlType::Uuid => FieldType::Uuid,
        SqlType::Timestamp => FieldType::DateTime,
        SqlType::Date => FieldType::Date,
        SqlType::Boolean => FieldType::Boolean,
        SqlType::Jsonb | SqlType::Json | SqlType::Other(_) => FieldType::Json,
        _ => FieldType::String,
    }
}

fn row_type(table: &FactTableMetadata, stem: &str, fields: &[RowField]) -> TypeDefinition {
    let mut row = TypeDefinition::new(row_type_name(stem), table.table_name.clone());
    row.fields = fields
        .iter()
        .map(|f| {
            FieldDefinition::nullable(f.name.as_str(), f.field_type.clone())
                .with_description(format!("{}.", capitalize(&f.source)))
        })
        .collect();
    row.description = Some(format!(
        "A row of an aggregate over fact table `{}`: its `groupBy` keys and the aggregates \
         requested. A field the request did not compute is `null`.",
        table.table_name
    ));
    row
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// The flat `<field>_<operator>` filter both roots take, or `None` when the table declares
/// nothing to filter on.
fn where_input(table: &FactTableMetadata, stem: &str) -> Option<InputObjectDefinition> {
    let mut fields = Vec::new();
    let mut push = |field: &str, operand: &str, operators: Vec<&str>| {
        for op in operators {
            let operand_type = match op {
                "in" | "nin" | "matches_any_lquery" | "lca" => format!("[{operand}!]"),
                "isnull" | "is_not_null" => "Boolean".to_string(),
                _ if op.starts_with("depth_") => "Int".to_string(),
                _ if LIKE_OPERATORS.contains(&op) => "String".to_string(),
                _ => operand.to_string(),
            };
            fields.push(InputFieldDefinition::new(format!("{field}_{op}"), operand_type));
        }
    };
    for filter in &table.denormalized_filters {
        let operand = scalar_of(&filter.sql_type).to_graphql_string();
        let mut operators: Vec<&str> = match filter.sql_type {
            SqlType::Boolean => vec!["eq", "neq"],
            SqlType::Uuid => vec!["eq", "neq", "in", "nin"],
            SqlType::Jsonb | SqlType::Json | SqlType::Other(_) => vec![],
            _ => COMPARISON_OPERATORS.to_vec(),
        };
        operators.extend(NULL_OPERATORS);
        if filter.sql_type == SqlType::Text {
            operators.extend(LIKE_OPERATORS);
        }
        if filter.sql_type == SqlType::Ltree {
            operators.extend(LTREE_OPERATORS);
            push(&filter.name, "String", operators);
            if filter.hierarchy.is_some() {
                push(&filter.name, "UUID", NODE_ID_OPERATORS.to_vec());
            }
            continue;
        }
        push(&filter.name, &operand, operators);
    }
    for path in &table.dimensions.paths {
        if table.denormalized_filters.iter().any(|f| f.name == path.name) {
            continue;
        }
        let mut operators = COMPARISON_OPERATORS.to_vec();
        operators.extend(NULL_OPERATORS);
        operators.extend(LIKE_OPERATORS);
        push(&path.name, "String", operators);
    }
    let mut mapped: Vec<(&String, &String)> = table.native_dimension_mapping.iter().collect();
    mapped.sort();
    for (key, column) in mapped {
        if table.denormalized_filters.iter().any(|f| f.name == *key)
            || table.dimensions.paths.iter().any(|p| p.name == *key)
        {
            continue;
        }
        let mut operators = COMPARISON_OPERATORS.to_vec();
        operators.extend(NULL_OPERATORS);
        push(key, &filter_column_type(table, column).to_graphql_string(), operators);
    }
    (!fields.is_empty()).then(|| {
        InputObjectDefinition::new(format!("{stem}AggregateWhere"))
            .with_fields(fields)
            .with_description(format!(
                "Filter over fact table `{}`: `<field>_<operator>` keys, all ANDed.",
                table.table_name
            ))
    })
}

/// The aggregate root's `groupBy`, `aggregates`, `having` and `orderBy` inputs.
struct AggregateInputs {
    group_by:  Option<InputObjectDefinition>,
    selection: Option<InputObjectDefinition>,
    having:    Option<InputObjectDefinition>,
    order_by:  Option<InputObjectDefinition>,
}

fn aggregate_inputs(stem: &str, fields: &[RowField]) -> AggregateInputs {
    let input = |suffix: &str, description: String, fields: Vec<InputFieldDefinition>| {
        (!fields.is_empty()).then(|| {
            InputObjectDefinition::new(format!("{stem}Aggregate{suffix}"))
                .with_fields(fields)
                .with_description(description)
        })
    };
    let group_by = fields
        .iter()
        .filter(|f| f.group_key)
        .map(|f| {
            let level = f.source.starts_with("the level of");
            InputFieldDefinition::new(f.name.as_str(), if level { "Int" } else { "Boolean" })
                .with_description(if level {
                    format!("Group by {}: the path cut to this many labels.", f.source)
                } else {
                    format!("Group by {}.", f.source)
                })
        })
        .collect();
    let selection = fields
        .iter()
        .filter(|f| !f.group_key)
        .map(|f| {
            InputFieldDefinition::new(f.name.as_str(), "Boolean")
                .with_description(format!("Compute {}.", f.source))
        })
        .collect();
    let mut having = Vec::new();
    for field in fields.iter().filter(|f| !f.group_key) {
        let operand = field.field_type.to_graphql_string();
        for op in HAVING_OPERATORS {
            having.push(InputFieldDefinition::new(format!("{}_{op}", field.name), operand.clone()));
        }
    }
    let order_by = fields
        .iter()
        .map(|f| InputFieldDefinition::new(f.name.as_str(), SORT_DIRECTION_ENUM))
        .collect();
    AggregateInputs {
        group_by:  input(
            "GroupBy",
            "The keys to group rows by; `true` groups by the key.".to_string(),
            group_by,
        ),
        selection: input(
            "Selection",
            "One aggregate to compute; `true` computes it.".to_string(),
            selection,
        ),
        having:    input(
            "Having",
            "Keep only the groups whose aggregates compare as stated, all ANDed.".to_string(),
            having,
        ),
        order_by:  input("OrderBy", "The row fields to sort by.".to_string(), order_by),
    }
}

/// The window root's inputs, the same for every fact table, and their enums.
fn window_inputs() -> (Vec<InputObjectDefinition>, Vec<EnumDefinition>) {
    let enumeration = |name: &str, description: &str, values: &[&str]| {
        EnumDefinition::new(name)
            .with_values(values.iter().map(|v| EnumValueDefinition::new(*v)).collect())
            .with_description(description)
    };
    let field = |name: &str, field_type: &str| InputFieldDefinition::new(name, field_type);
    let required = |name: &str, field_type: &str| {
        InputFieldDefinition::new(name, field_type).with_nullable(false)
    };
    let enums = vec![
        enumeration(
            "WindowColumnKind",
            "What a window column reads.",
            &["measure", "dimension", "filter"],
        ),
        enumeration(
            "WindowFunctionKind",
            "A window function.",
            &[
                "row_number",
                "rank",
                "dense_rank",
                "ntile",
                "percent_rank",
                "cume_dist",
                "lag",
                "lead",
                "first_value",
                "last_value",
                "nth_value",
                "running_sum",
                "running_avg",
                "running_count",
                "running_min",
                "running_max",
                "running_stddev",
                "running_variance",
            ],
        ),
        enumeration("WindowFrameType", "How a window frame counts.", &["ROWS", "RANGE", "GROUPS"]),
        enumeration(
            "WindowFrameBoundaryKind",
            "Where a window frame starts or ends.",
            &[
                "unbounded_preceding",
                "n_preceding",
                "current_row",
                "n_following",
                "unbounded_following",
            ],
        ),
        enumeration(
            "WindowFrameExclusion",
            "The rows a window frame leaves out.",
            &["current_row", "group", "ties", "no_others"],
        ),
    ];
    let inputs = vec![
        InputObjectDefinition::new("WindowColumn")
            .with_fields(vec![
                required("type", "WindowColumnKind"),
                field("name", "String"),
                field("path", "String"),
                required("alias", "String"),
            ])
            .with_description(
                "A column of a window row: a measure or filter by `name`, a dimension by \
                 `path`, answering under `alias`.",
            ),
        InputObjectDefinition::new("WindowPartition")
            .with_fields(vec![
                required("type", "WindowColumnKind"),
                field("name", "String"),
                field("path", "String"),
            ])
            .with_description("A column a window is partitioned by."),
        InputObjectDefinition::new("WindowFunction")
            .with_fields(vec![
                required("type", "WindowFunctionKind"),
                field("field", "String"),
                field("measure", "String"),
                field("n", "Int"),
                field("offset", "Int"),
                field("default", "JSON"),
            ])
            .with_description(
                "A window function and its operands: `field` for value functions, `measure` \
                 for running aggregates, `n` for `ntile` and `nth_value`, `offset` and \
                 `default` for `lag` and `lead`.",
            ),
        InputObjectDefinition::new("WindowFrameBoundary")
            .with_fields(vec![
                required("type", "WindowFrameBoundaryKind"),
                field("n", "Int"),
            ])
            .with_description("A window frame boundary; `n` for `n_preceding`/`n_following`."),
        InputObjectDefinition::new("WindowFrame")
            .with_fields(vec![
                required("frame_type", "WindowFrameType"),
                required("start", "WindowFrameBoundary"),
                required("end", "WindowFrameBoundary"),
                field("exclusion", "WindowFrameExclusion"),
            ])
            .with_description("A window frame."),
        InputObjectDefinition::new("WindowOrderBy")
            .with_fields(vec![
                required("field", "String"),
                field("direction", SORT_DIRECTION_ENUM).with_default_value("\"ASC\""),
            ])
            .with_description("A sort key of a window, or of the window rows."),
        InputObjectDefinition::new("WindowSpec")
            .with_fields(vec![
                required("function", "WindowFunction"),
                required("alias", "String"),
                field("partitionBy", "[WindowPartition!]"),
                field("orderBy", "[WindowOrderBy!]"),
                field("frame", "WindowFrame"),
            ])
            .with_description("A window function over the rows, answering under `alias`."),
    ];
    (inputs, enums)
}

#[cfg(test)]
#[path = "fact_table_surface_tests.rs"]
mod tests;
