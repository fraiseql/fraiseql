#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use serde_json::{Value, json};

use crate::schema::{CompiledSchema, IntrospectionBuilder, IntrospectionType, TypeKind};

/// A schema with one ordinary query and a fact table carrying one column of each kind the
/// published row types differently.
fn fixture() -> Value {
    json!({
        "types": [{"name": "Order", "sql_source": "v_order",
                   "fields": [{"name": "id", "field_type": "Int", "nullable": false}]}],
        "queries": [{"name": "orders", "return_type": "Order", "returns_list": true,
                     "sql_source": "v_order"}],
        "mutations": [],
        "fact_tables": {"tf_sales": {
            "table_name": "tf_sales",
            "measures": [
                {"name": "revenue", "sql_type": "Decimal", "nullable": false},
                {"name": "quantity", "sql_type": "Int", "nullable": false},
                {"name": "units", "sql_type": "BigInt", "nullable": false},
                {"name": "ratio", "sql_type": "Float", "nullable": false,
                 "additivity": {"kind": "non_additive"}}
            ],
            "dimensions": {"name": "data", "paths": [
                {"name": "category", "json_path": "data->>'category'", "data_type": "text"}
            ]},
            "denormalized_filters": [
                {"name": "occurred_at", "sql_type": "Timestamp", "indexed": true},
                {"name": "day", "sql_type": "Date", "indexed": true},
                {"name": "org_path", "sql_type": "Ltree", "indexed": true},
                {"name": "region", "sql_type": "Text", "indexed": true},
                {"name": "active", "sql_type": "Boolean", "indexed": false}
            ]
        }}
    })
}

fn load(doc: &Value) -> CompiledSchema {
    CompiledSchema::from_json(&doc.to_string(), false).expect("the fixture loads")
}

fn refusal(doc: &Value) -> String {
    CompiledSchema::from_json(&doc.to_string(), false)
        .expect_err("the schema must not load")
        .to_string()
}

/// `[T!]!` notation of an introspected type reference.
fn rendered(t: &IntrospectionType) -> String {
    let inner = || t.of_type.as_deref().map_or_else(String::new, rendered);
    match t.kind {
        TypeKind::NonNull => format!("{}!", inner()),
        TypeKind::List => format!("[{}]", inner()),
        _ => t.name.clone().unwrap_or_default(),
    }
}

fn introspected(schema: &CompiledSchema, name: &str) -> IntrospectionType {
    IntrospectionBuilder::build(schema)
        .types
        .into_iter()
        .find(|t| t.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("`{name}` is published"))
}

fn query_field(schema: &CompiledSchema, name: &str) -> (String, Vec<(String, String)>) {
    let query = introspected(schema, "Query");
    let field = query
        .fields
        .unwrap()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("`Query.{name}` is published"));
    let args = field.args.iter().map(|a| (a.name.clone(), rendered(&a.input_type))).collect();
    (rendered(&field.field_type), args)
}

fn field_types(t: &IntrospectionType) -> Vec<(String, String)> {
    t.fields
        .iter()
        .flatten()
        .map(|f| (f.name.clone(), rendered(&f.field_type)))
        .chain(
            t.input_fields
                .iter()
                .flatten()
                .map(|f| (f.name.clone(), rendered(&f.input_type))),
        )
        .collect()
}

fn field_type(t: &IntrospectionType, name: &str) -> Option<String> {
    field_types(t).into_iter().find(|(n, _)| n == name).map(|(_, ty)| ty)
}

#[test]
fn the_aggregate_root_is_published_with_typed_arguments() {
    let schema = load(&fixture());
    let (returns, args) = query_field(&schema, "sales_aggregate");
    assert_eq!(returns, "[SalesAggregateRow!]!");
    let expected: Vec<(String, String)> = [
        ("where", "SalesAggregateWhere"),
        ("groupBy", "SalesAggregateGroupBy"),
        ("aggregates", "[SalesAggregateSelection!]"),
        ("having", "SalesAggregateHaving"),
        ("orderBy", "SalesAggregateOrderBy"),
        ("limit", "Int"),
        ("offset", "Int"),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
    .collect();
    assert_eq!(args, expected);
}

#[test]
fn the_window_root_is_published_with_typed_arguments_and_json_rows() {
    let schema = load(&fixture());
    let (returns, args) = query_field(&schema, "sales_window");
    assert_eq!(returns, "[JSON!]!");
    let names: Vec<&str> = args.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["select", "windows", "where", "orderBy", "limit", "offset"]);
    assert_eq!(args[1].1, "[WindowSpec!]");
    assert_eq!(args[2].1, "SalesAggregateWhere");
}

/// The root keeps its name under a camelCase schema: the dispatch routes it by the
/// `_aggregate` suffix, so `salesAggregate` would reach nothing.
#[test]
fn a_root_is_published_under_its_routed_name_whatever_the_naming_convention() {
    let mut doc = fixture();
    doc["naming_convention"] = json!("camelCase");
    let schema = load(&doc);
    query_field(&schema, "sales_aggregate");
    query_field(&schema, "sales_window");
}

/// Each row field is typed by what the runtime decodes for its column.
#[test]
fn the_row_fields_are_typed_by_what_the_runtime_decodes() {
    let schema = load(&fixture());
    let row = introspected(&schema, "SalesAggregateRow");
    for (field, expected) in [
        ("category", "String"),
        ("region", "String"),
        ("active", "Boolean"),
        ("occurred_at_day", "DateTime"),
        ("day_month", "Date"),
        ("org_path_depth", "Int"),
        ("org_path_level", "String"),
        ("count", "Int"),
        ("revenue_sum", "Decimal"),
        ("revenue_avg", "Decimal"),
        ("quantity_sum", "Int"),
        ("quantity_avg", "Float"),
        ("units_sum", "Float"),
        ("units_max", "Int"),
        ("quantity_count_distinct", "Int"),
        ("active_bool_and", "Boolean"),
        ("region_array_agg", "[String]"),
        ("region_string_agg", "String"),
    ] {
        assert_eq!(field_type(&row, field).as_deref(), Some(expected), "`{field}`");
    }
}

/// What would decode to `null`, or be refused by the planner or the database, is not
/// published.
#[test]
fn a_combination_the_runtime_cannot_answer_is_not_published() {
    let schema = load(&fixture());
    let row = introspected(&schema, "SalesAggregateRow");
    for field in [
        "ratio_sum",
        "ratio_avg",
        "category_array_agg",
        "category_bool_and",
        "count_distinct",
        "occurred_at_array_agg",
        "region_bool_and",
    ] {
        assert_eq!(field_type(&row, field), None, "`{field}` is not published");
    }
}

#[test]
fn the_group_by_input_is_a_boolean_per_key_and_an_int_per_tree_level() {
    let schema = load(&fixture());
    let group_by = introspected(&schema, "SalesAggregateGroupBy");
    assert_eq!(field_type(&group_by, "category").as_deref(), Some("Boolean"));
    assert_eq!(field_type(&group_by, "occurred_at_week").as_deref(), Some("Boolean"));
    assert_eq!(field_type(&group_by, "org_path_depth").as_deref(), Some("Boolean"));
    assert_eq!(field_type(&group_by, "org_path_level").as_deref(), Some("Int"));
    assert_eq!(field_type(&group_by, "revenue_sum"), None, "an aggregate is no group key");
}

#[test]
fn the_where_input_publishes_the_operators_a_fact_table_filter_serves() {
    let schema = load(&fixture());
    let filter = introspected(&schema, "SalesAggregateWhere");
    for (field, expected) in [
        ("region_eq", "String"),
        ("region_in", "[String!]"),
        ("region_icontains", "String"),
        ("occurred_at_gte", "DateTime"),
        ("day_lt", "Date"),
        ("active_eq", "Boolean"),
        ("active_isnull", "Boolean"),
        ("org_path_descendant_of", "String"),
        ("org_path_depth_eq", "Int"),
        ("category_eq", "String"),
    ] {
        assert_eq!(field_type(&filter, field).as_deref(), Some(expected), "`{field}`");
    }
    // Refused by the fact-table SQL generator, so not advertised.
    for field in [
        "region_regex",
        "region_nlike",
        "active_gt",
        "org_path_descendant_of_id",
    ] {
        assert_eq!(field_type(&filter, field), None, "`{field}` is not published");
    }
}

#[test]
fn the_selection_having_and_order_by_inputs_follow_the_row() {
    let schema = load(&fixture());
    let selection = introspected(&schema, "SalesAggregateSelection");
    assert_eq!(field_type(&selection, "revenue_sum").as_deref(), Some("Boolean"));
    assert_eq!(field_type(&selection, "category"), None, "a group key is not an aggregate");
    let having = introspected(&schema, "SalesAggregateHaving");
    assert_eq!(field_type(&having, "revenue_sum_gt").as_deref(), Some("Decimal"));
    assert_eq!(field_type(&having, "count_gte").as_deref(), Some("Int"));
    let order_by = introspected(&schema, "SalesAggregateOrderBy");
    assert_eq!(field_type(&order_by, "revenue_sum").as_deref(), Some("SortDirection"));
    assert_eq!(field_type(&order_by, "category").as_deref(), Some("SortDirection"));
}

/// The SDL publishes what introspection does: the roots, the row and the inputs.
#[test]
fn the_sdl_publishes_the_roots_and_their_types() {
    let schema = load(&fixture());
    let sdl = schema.raw_schema();
    let tree = apollo_parser::Parser::new(&sdl).parse();
    assert_eq!(tree.errors().len(), 0, "the SDL parses:\n{sdl}");
    for needle in [
        "sales_aggregate(where: SalesAggregateWhere, groupBy: SalesAggregateGroupBy, \
         aggregates: [SalesAggregateSelection!], having: SalesAggregateHaving, orderBy: \
         SalesAggregateOrderBy, limit: Int, offset: Int): [SalesAggregateRow!]!",
        "type SalesAggregateRow {",
        "input SalesAggregateWhere {",
        "input WindowSpec {",
        "enum WindowFunctionKind {",
    ] {
        assert!(sdl.contains(needle), "the SDL carries `{needle}`:\n{sdl}");
    }
    // Each name is declared once: a row type is an object, never also a scalar.
    for scalar in sdl.lines().filter_map(|l| l.strip_prefix("scalar ")) {
        for kind in ["type", "input", "enum"] {
            assert!(
                !sdl.lines().any(|l| l.starts_with(&format!("{kind} {scalar} "))),
                "`{scalar}` is declared a scalar and a {kind}:\n{sdl}"
            );
        }
    }
}

/// A schema whose only roots are a fact table's still publishes a `Query` type.
#[test]
fn a_schema_with_only_fact_tables_publishes_a_query_type() {
    let mut doc = fixture();
    doc["queries"] = json!([]);
    let schema = load(&doc);
    let sdl = schema.raw_schema();
    assert!(sdl.contains("type Query {"), "{sdl}");
    query_field(&schema, "sales_aggregate");
}

#[test]
fn a_declared_type_named_like_a_derived_one_is_refused_at_load() {
    let mut doc = fixture();
    doc["types"].as_array_mut().unwrap().push(json!(
        {"name": "SalesAggregateRow", "sql_source": "v_x",
         "fields": [{"name": "id", "field_type": "Int", "nullable": false}]}
    ));
    let message = refusal(&doc);
    assert!(message.contains("`SalesAggregateRow`"), "{message}");
    assert!(message.contains("`tf_sales`"), "{message}");
    assert!(message.contains("recompile"), "{message}");
}

#[test]
fn a_declared_input_named_like_a_derived_one_is_refused_at_load() {
    let mut doc = fixture();
    doc["input_types"] = json!([{"name": "WindowSpec", "fields": [
        {"name": "x", "field_type": "Int"}
    ]}]);
    let message = refusal(&doc);
    assert!(message.contains("`WindowSpec`"), "{message}");
}

/// D1: a key that is not a GraphQL name cannot be a field; the load says what to call it.
#[test]
fn a_fact_table_key_that_is_not_a_graphql_name_is_refused_naming_one_that_is() {
    for (section, value, key, suggestion) in [
        (
            "native_dimension_mapping",
            json!({"dimensions.category.id": "region"}),
            "dimensions.category.id",
            "dimensions_category_id",
        ),
        (
            "native_measures",
            json!({"measures.volume": "volume"}),
            "measures.volume",
            "measures_volume",
        ),
    ] {
        let mut doc = fixture();
        doc["fact_tables"]["tf_sales"][section] = value;
        let message = refusal(&doc);
        assert!(message.contains(&format!("`{key}`")), "{message}");
        assert!(message.contains(&format!("`{suggestion}`")), "{message}");
        assert!(message.contains("recompile"), "{message}");
    }
    // A measure, a dimension path and a filter: each is a row field or a key.
    for (pointer, key, what, suggestion) in [
        ("/fact_tables/tf_sales/measures/0/name", "rev.enue", "measure", "rev_enue"),
        (
            "/fact_tables/tf_sales/dimensions/paths/0/name",
            "cat-egory",
            "dimension path",
            "cat_egory",
        ),
        (
            "/fact_tables/tf_sales/denormalized_filters/3/name",
            "2region",
            "denormalized filter",
            "_2region",
        ),
    ] {
        let mut doc = fixture();
        *doc.pointer_mut(pointer).unwrap() = json!(key);
        let message = refusal(&doc);
        assert!(message.contains(&format!("{what} `{key}` is not a GraphQL name")), "{message}");
        assert!(message.contains(&format!("`{suggestion}`")), "{message}");
    }
}

/// A table whose keys cannot be published publishes no roots, even in a schema built by hand
/// that the load never refuses.
#[test]
fn a_table_whose_keys_cannot_be_published_publishes_no_roots() {
    let mut doc = fixture();
    doc["fact_tables"]["tf_sales"]["measures"][0]["name"] = json!("rev.enue");
    let table = serde_json::from_value(doc["fact_tables"]["tf_sales"].clone()).unwrap();
    let mut schema = CompiledSchema::new();
    schema.add_fact_table("tf_sales".to_string(), table);
    schema.build_indexes();
    assert!(schema.fact_root("sales_aggregate").is_none());
    assert!(!schema.raw_schema().contains("SalesAggregateRow"));
}

/// Building the indexes again publishes nothing twice.
#[test]
fn building_the_indexes_twice_publishes_each_input_once() {
    let mut schema = load(&fixture());
    let inputs = schema.input_types.len();
    let enums = schema.enums.len();
    schema.build_indexes();
    assert_eq!(schema.input_types.len(), inputs);
    assert_eq!(schema.enums.len(), enums);
}

/// A declared enum named like one the roots publish, with other values, is refused.
#[test]
fn a_declared_enum_named_like_a_derived_one_is_refused_at_load() {
    let mut doc = fixture();
    doc["enums"] = json!([{"name": "WindowFrameType", "values": [{"name": "ROWS"}]}]);
    let message = refusal(&doc);
    assert!(message.contains("`WindowFrameType` is declared"), "{message}");
}

/// Two sources of one row field: a filter column spelled like a measure's aggregate.
#[test]
fn a_row_field_two_sources_produce_is_refused_naming_both() {
    let mut doc = fixture();
    doc["fact_tables"]["tf_sales"]["denormalized_filters"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "revenue_sum", "sql_type": "Decimal", "indexed": false}));
    let message = refusal(&doc);
    assert!(message.contains("`revenue_sum`"), "{message}");
    assert!(message.contains("measure `revenue`"), "{message}");
    assert!(message.contains("filter `revenue_sum`"), "{message}");
}

/// § 5.8.2: a variable typed with a published input names a defined type. The schema
/// declares an input of its own, so the check adjudicates: it skips one with none.
#[test]
fn a_variable_typed_with_a_published_input_is_accepted() {
    let mut doc = fixture();
    doc["input_types"] = json!([{"name": "Note", "fields": [{"name": "x", "field_type": "Int"}]}]);
    let schema = load(&doc);
    let types = |source: &str| {
        let document = crate::graphql::parse_query(source).unwrap();
        crate::runtime::validate_variable_types(&schema, &schema, None, &document.variables, false)
    };
    types("query($n: SalesAggregateNope) { sales_aggregate(groupBy: $n) { count } }")
        .expect_err("the control: an undefined input type is refused");
    types(
        "query($g: SalesAggregateGroupBy, $w: SalesAggregateWhere, $s: [WindowSpec!]) \
         { sales_aggregate(groupBy: $g, where: $w) { region count } }",
    )
    .expect("the published input types are defined");
}
