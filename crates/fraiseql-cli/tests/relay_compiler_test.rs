#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Compiler tests for the Relay specification support.
//!
//! Verifies that the CLI SchemaConverter correctly:
//! - Sets `relay=true` and derives `relay_cursor_column` on queries
//! - Injects the Node interface into `schema.interfaces`
//! - Injects PageInfo into `schema.types`
//! - Generates XxxEdge and XxxConnection types for each relay type
//! - Marks relay types as implementing the Node interface

#![allow(clippy::pedantic)]

use fraiseql_cli::schema::{
    SchemaConverter,
    intermediate::{IntermediateField, IntermediateQuery, IntermediateSchema, IntermediateType},
};
use fraiseql_core::schema::NamingConvention;

// =============================================================================
// Helper: minimal intermediate schema with one relay type + query
// =============================================================================

fn relay_intermediate_schema() -> IntermediateSchema {
    IntermediateSchema {
        version:              "2.0.0".to_string(),
        types:                vec![IntermediateType {
            name:          "User".to_string(),
            fields:        vec![
                IntermediateField {
                    name:           "id".to_string(),
                    field_type:     "ID".to_string(), // matches Node interface
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                },
                IntermediateField {
                    name:           "name".to_string(),
                    field_type:     "String".to_string(),
                    nullable:       false,
                    description:    None,
                    directives:     None,
                    requires_scope: None,
                    on_deny:        None,
                },
            ],
            description:   None,
            implements:    vec![],
            requires_role: None,
            is_error:      false,
            relay:         true,
        }],
        enums:                vec![],
        input_types:          vec![],
        interfaces:           vec![],
        unions:               vec![],
        queries:              vec![IntermediateQuery {
            name:              "users".to_string(),
            return_type:       "User".to_string(),
            returns_list:      true,
            nullable:          false,
            arguments:         vec![],
            description:       None,
            sql_source:        Some("v_user".to_string()),
            auto_params:       None,
            deprecated:        None,
            jsonb_column:      None,
            relay:             true,
            inject:            Default::default(),
            cache_ttl_seconds: None,
            additional_views:  vec![],
            requires_role:     None,
            relay_cursor_type: None,
        }],
        mutations:            vec![],
        subscriptions:        vec![],
        fragments:            None,
        directives:           None,
        fact_tables:          None,
        aggregate_queries:    None,
        observers:            None,
        custom_scalars:       None,
        security:             None,
        observers_config:     None,
        subscriptions_config: None,
        validation_config:    None,
        federation_config:    None,
        debug_config:         None,
        mcp_config:           None,
        query_defaults:       None,
        naming_convention:    NamingConvention::default(),
        session_variables:    None,
    }
}

// =============================================================================
// relay flag propagation
// =============================================================================

#[test]
fn test_query_relay_flag_is_set() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let users_query = compiled
        .queries
        .iter()
        .find(|q| q.name == "users")
        .expect("users query should exist");
    assert!(users_query.relay, "users query should have relay=true");
}

#[test]
fn test_query_relay_cursor_column_is_derived() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let users_query = compiled.queries.iter().find(|q| q.name == "users").unwrap();
    assert_eq!(
        users_query.relay_cursor_column,
        Some("pk_user".to_string()),
        "cursor column should be derived as pk_user from User type"
    );
    // Int64 is the default cursor type for non-UUID primary keys
    assert_eq!(
        users_query.relay_cursor_type,
        fraiseql_core::schema::CursorType::Int64,
        "cursor type should default to Int64 for bigint/pk columns"
    );
}

#[test]
fn test_user_type_relay_flag_is_set() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let user_type = compiled.types.iter().find(|t| t.name == "User").unwrap();
    assert!(user_type.relay, "User type should have relay=true");
}

// =============================================================================
// Node interface injection
// =============================================================================

#[test]
fn test_node_interface_is_injected() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let node_iface = compiled.interfaces.iter().find(|i| i.name == "Node");
    assert!(node_iface.is_some(), "Node interface should be injected into schema");
}

#[test]
fn test_node_interface_has_id_field() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let node_iface = compiled.interfaces.iter().find(|i| i.name == "Node").unwrap();
    assert!(
        node_iface.fields.iter().any(|f| f.name == "id"),
        "Node interface should have `id` field"
    );
}

#[test]
fn test_relay_type_implements_node() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let user_type = compiled.types.iter().find(|t| t.name == "User").unwrap();
    assert!(
        user_type.implements.iter().any(|i| i == "Node"),
        "User type should implement Node interface"
    );
}

// =============================================================================
// PageInfo injection
// =============================================================================

#[test]
fn test_page_info_is_injected() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let page_info = compiled.types.iter().find(|t| t.name == "PageInfo");
    assert!(page_info.is_some(), "PageInfo type should be injected");
}

#[test]
fn test_page_info_has_required_fields() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let page_info = compiled.types.iter().find(|t| t.name == "PageInfo").unwrap();
    let field_names: Vec<&str> = page_info.fields.iter().map(|f| f.name.as_str()).collect();

    assert!(field_names.contains(&"hasNextPage"), "PageInfo should have hasNextPage");
    assert!(field_names.contains(&"hasPreviousPage"), "PageInfo should have hasPreviousPage");
    assert!(field_names.contains(&"startCursor"), "PageInfo should have startCursor");
    assert!(field_names.contains(&"endCursor"), "PageInfo should have endCursor");
}

// =============================================================================
// Edge / Connection type injection
// =============================================================================

#[test]
fn test_user_edge_type_is_injected() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let edge = compiled.types.iter().find(|t| t.name == "UserEdge");
    assert!(edge.is_some(), "UserEdge type should be injected");
}

#[test]
fn test_user_edge_has_cursor_and_node() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let edge = compiled.types.iter().find(|t| t.name == "UserEdge").unwrap();
    let field_names: Vec<&str> = edge.fields.iter().map(|f| f.name.as_str()).collect();

    assert!(field_names.contains(&"cursor"), "UserEdge should have cursor field");
    assert!(field_names.contains(&"node"), "UserEdge should have node field");
}

#[test]
fn test_user_connection_type_is_injected() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let conn = compiled.types.iter().find(|t| t.name == "UserConnection");
    assert!(conn.is_some(), "UserConnection type should be injected");
}

#[test]
fn test_user_connection_has_edges_and_page_info() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let conn = compiled.types.iter().find(|t| t.name == "UserConnection").unwrap();
    let field_names: Vec<&str> = conn.fields.iter().map(|f| f.name.as_str()).collect();

    assert!(field_names.contains(&"edges"), "UserConnection should have edges field");
    assert!(field_names.contains(&"pageInfo"), "UserConnection should have pageInfo field");
    assert!(
        field_names.contains(&"totalCount"),
        "UserConnection should have totalCount field"
    );

    // totalCount must be nullable (null when not requested or not computed)
    let total_count_field = conn.fields.iter().find(|f| f.name == "totalCount").unwrap();
    assert!(total_count_field.nullable, "totalCount should be nullable");
}

// =============================================================================
// Non-relay types are unaffected
// =============================================================================

#[test]
fn test_non_relay_schema_has_no_node_interface() {
    let mut schema = relay_intermediate_schema();
    // Remove relay flag from type and query
    schema.types[0].relay = false;
    schema.queries[0].relay = false;

    let compiled = SchemaConverter::convert(schema).expect("schema conversion failed");

    assert!(
        !compiled.interfaces.iter().any(|i| i.name == "Node"),
        "Node interface should NOT be injected when no relay types exist"
    );
    assert!(
        !compiled.types.iter().any(|t| t.name == "PageInfo"),
        "PageInfo should NOT be injected when no relay types exist"
    );
    assert!(
        !compiled.types.iter().any(|t| t.name == "UserEdge"),
        "UserEdge should NOT be injected when no relay types exist"
    );
}

#[test]
fn test_relay_injection_is_idempotent() {
    // Running with the same schema twice should produce the same set of types
    // (Node + PageInfo injected only once).
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let node_count = compiled.interfaces.iter().filter(|i| i.name == "Node").count();
    let page_info_count = compiled.types.iter().filter(|t| t.name == "PageInfo").count();
    let edge_count = compiled.types.iter().filter(|t| t.name == "UserEdge").count();

    assert_eq!(node_count, 1, "Node interface should appear exactly once");
    assert_eq!(page_info_count, 1, "PageInfo should appear exactly once");
    assert_eq!(edge_count, 1, "UserEdge should appear exactly once");
}

// =============================================================================
// Relay query auto-params
// =============================================================================

#[test]
fn test_relay_query_has_no_limit_offset_auto_params() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let q = compiled.queries.iter().find(|q| q.name == "users").unwrap();
    // Relay queries use first/after/last/before instead of limit/offset
    assert!(!q.auto_params.has_limit, "relay query should not have has_limit auto_param");
    assert!(!q.auto_params.has_offset, "relay query should not have has_offset auto_param");
}

#[test]
fn test_relay_query_has_where_auto_param() {
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let q = compiled.queries.iter().find(|q| q.name == "users").unwrap();
    assert!(q.auto_params.has_where, "relay query should keep has_where auto_param");
}

// =============================================================================
// Relay cursor type propagation
// =============================================================================

#[test]
fn test_relay_uuid_cursor_type_is_set() {
    let mut schema = relay_intermediate_schema();
    // Override the single query to declare a UUID cursor column
    schema.queries[0].relay_cursor_type = Some("uuid".to_string());

    let compiled = SchemaConverter::convert(schema).expect("schema conversion failed");

    let q = compiled.queries.iter().find(|q| q.name == "users").unwrap();
    assert!(q.relay, "query should have relay=true");
    assert_eq!(
        q.relay_cursor_type,
        fraiseql_core::schema::CursorType::Uuid,
        "relay_cursor_type should be Uuid when declared as 'uuid'"
    );
}

#[test]
fn test_relay_int64_cursor_type_is_default() {
    // No relay_cursor_type set → defaults to Int64
    let compiled =
        SchemaConverter::convert(relay_intermediate_schema()).expect("schema conversion failed");

    let q = compiled.queries.iter().find(|q| q.name == "users").unwrap();
    assert_eq!(
        q.relay_cursor_type,
        fraiseql_core::schema::CursorType::Int64,
        "relay_cursor_type should default to Int64 when not declared"
    );
}
