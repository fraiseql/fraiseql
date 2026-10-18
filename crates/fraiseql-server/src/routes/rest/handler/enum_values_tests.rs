//! #1580: a REST filter names an enum member, and reaches SQL as the value it is stored as.
//!
//! The GraphQL path is pinned end to end on PostgreSQL (`fraiseql-cli`'s
//! `authored_contract_pg`); REST builds its `QueryMatch` itself, so this pins that it
//! coerces as the matcher does.

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::http::HeaderMap;
use fraiseql_core::{
    runtime::Executor,
    schema::{CompiledSchema, EnumDefinition, EnumValueDefinition, FieldType, RestConfig},
};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestFieldBuilder, TestQueryBuilder, TestTypeBuilder},
};
use serde_json::json;

use crate::routes::rest::{handler::RestHandler, resource::RestRouteTable};

fn orders_schema() -> CompiledSchema {
    let orders = TestQueryBuilder::new("orders", "Order")
        .returns_list(true)
        .with_sql_source("v_order")
        .build();
    let order = TestTypeBuilder::new("Order", "v_order")
        .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
        .with_field(TestFieldBuilder::new("status", FieldType::Enum("OrderStatus".into())).build())
        .build();
    let mut schema = CompiledSchema::new();
    schema.queries.push(orders);
    schema.types.push(order);
    schema.enums.push(EnumDefinition::new("OrderStatus").with_values(vec![
        EnumValueDefinition::new("PENDING").stored_as("pending"),
        EnumValueDefinition::new("SHIPPED").stored_as("shipped"),
    ]));
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: false,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

#[test]
fn a_rest_filter_on_a_valued_enum_reaches_sql_as_the_stored_value() {
    let schema = orders_schema();
    let adapter = Arc::new(FailingAdapter::new());
    let executor = Arc::new(Executor::new(schema.clone(), adapter));
    let route_table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let rest_config = schema.rest_config.clone().unwrap();
    let handler = RestHandler::new(&executor, &schema, &rest_config, &route_table);

    let resolved = handler
        .resolve_get_query("/orders", &[("status[eq]", "SHIPPED")], &HeaderMap::new(), None)
        .unwrap();

    assert_eq!(
        resolved.query_match.arguments["where"],
        json!({"status": {"eq": "shipped"}}),
        "{:?}",
        resolved.query_match.arguments
    );
}
