//! Converter field-survival tests.
//!
//! These tests verify that every field set on `IntermediateQuery`,
//! `IntermediateMutation`, and `IntermediateType` survives
//! `SchemaConverter::convert()` into the final `CompiledSchema`.
//!
//! **Why this matters**: the converter is the last transformation in the CLI
//! pipeline before the compiled schema is serialised to disk.  A field silently
//! dropped here will not be present at runtime, causing subtle bugs that are
//! hard to attribute to a specific commit (exactly the class of issue #53).

use indexmap::IndexMap;

use fraiseql_cli::schema::{
    SchemaConverter,
    intermediate::{
        IntermediateField, IntermediateMutation, IntermediateQuery, IntermediateSchema,
        IntermediateType,
    },
};
use fraiseql_core::schema::InjectedParamSource;

/// Minimal `Order` type used as return type in most tests.
fn order_type() -> IntermediateType {
    IntermediateType {
        name:   "Order".to_string(),
        fields: vec![IntermediateField {
            field_type:    "ID".to_string(),
            name:          "id".to_string(),
            nullable:      false,
            description:   None,
            directives:    None,
            requires_scope: None,
            on_deny:       None,
        }],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Query field survival
// ---------------------------------------------------------------------------

#[test]
fn converter_threads_cache_ttl_seconds_on_query() {
    let schema = IntermediateSchema {
        types: vec![order_type()],
        queries: vec![IntermediateQuery {
            name:              "orders".to_string(),
            return_type:       "Order".to_string(),
            returns_list:      true,
            sql_source:        Some("v_order".to_string()),
            cache_ttl_seconds: Some(300),
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let q = compiled.find_query("orders").expect("'orders' query must be present");

    assert_eq!(
        q.cache_ttl_seconds,
        Some(300),
        "cache_ttl_seconds must survive SchemaConverter::convert()"
    );
}

#[test]
fn converter_threads_inject_params_on_query() {
    let mut inject = IndexMap::new();
    inject.insert("tenant_id".to_string(), "jwt:tenant_id".to_string());

    let schema = IntermediateSchema {
        types: vec![order_type()],
        queries: vec![IntermediateQuery {
            name:         "tenantOrders".to_string(),
            return_type:  "Order".to_string(),
            returns_list: true,
            sql_source:   Some("v_order".to_string()),
            inject,
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let q = compiled.find_query("tenantOrders").expect("'tenantOrders' query must be present");

    assert_eq!(q.inject_params.len(), 1, "inject_params must have one entry");

    let src = q.inject_params.get("tenant_id").expect("inject_params must contain 'tenant_id'");
    assert_eq!(
        *src,
        InjectedParamSource::Jwt("tenant_id".to_string()),
        "inject source must survive SchemaConverter::convert()"
    );
}

#[test]
fn converter_threads_requires_role_on_query() {
    let schema = IntermediateSchema {
        types: vec![order_type()],
        queries: vec![IntermediateQuery {
            name:         "adminQuery".to_string(),
            return_type:  "Order".to_string(),
            sql_source:   Some("v_order".to_string()),
            requires_role: Some("admin".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let q = compiled.find_query("adminQuery").expect("'adminQuery' must be present");

    assert_eq!(
        q.requires_role.as_deref(),
        Some("admin"),
        "requires_role must survive SchemaConverter::convert()"
    );
}

// ---------------------------------------------------------------------------
// Mutation field survival
// ---------------------------------------------------------------------------

#[test]
fn converter_threads_invalidates_views_on_mutation() {
    let schema = IntermediateSchema {
        types: vec![order_type()],
        mutations: vec![IntermediateMutation {
            name:             "placeOrder".to_string(),
            return_type:      "Order".to_string(),
            sql_source:       Some("fn_place_order".to_string()),
            invalidates_views: vec!["v_order_summary".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let m = compiled.find_mutation("placeOrder").expect("'placeOrder' mutation must be present");

    assert_eq!(
        m.invalidates_views,
        vec!["v_order_summary"],
        "invalidates_views must survive SchemaConverter::convert()"
    );
}

#[test]
fn converter_threads_inject_params_on_mutation() {
    let mut inject = IndexMap::new();
    inject.insert("user_id".to_string(), "jwt:sub".to_string());

    let schema = IntermediateSchema {
        types: vec![order_type()],
        mutations: vec![IntermediateMutation {
            name:       "createOrder".to_string(),
            return_type: "Order".to_string(),
            sql_source:  Some("fn_create_order".to_string()),
            inject,
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let m = compiled.find_mutation("createOrder").expect("'createOrder' mutation must be present");

    assert_eq!(m.inject_params.len(), 1, "inject_params must have one entry after conversion");

    let src = m.inject_params.get("user_id").expect("inject_params must contain 'user_id'");
    assert_eq!(
        *src,
        InjectedParamSource::Jwt("sub".to_string()),
        "jwt:sub must become Jwt(\"sub\") after SchemaConverter::convert()"
    );
}

#[test]
fn converter_threads_sql_source_on_mutation() {
    let schema = IntermediateSchema {
        types: vec![order_type()],
        mutations: vec![IntermediateMutation {
            name:       "doTheThing".to_string(),
            return_type: "Order".to_string(),
            sql_source:  Some("fn_do_the_thing".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let m = compiled.find_mutation("doTheThing").expect("'doTheThing' must be present");

    assert_eq!(
        m.sql_source.as_deref(),
        Some("fn_do_the_thing"),
        "sql_source must survive SchemaConverter::convert()"
    );
}

#[test]
fn converter_threads_invalidates_fact_tables_on_mutation() {
    let schema = IntermediateSchema {
        types: vec![order_type()],
        mutations: vec![IntermediateMutation {
            name:                    "createOrder".to_string(),
            return_type:             "Order".to_string(),
            sql_source:              Some("fn_create_order".to_string()),
            invalidates_fact_tables: vec!["tf_sales".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let m = compiled.find_mutation("createOrder").expect("'createOrder' must be present");

    assert_eq!(
        m.invalidates_fact_tables,
        vec!["tf_sales"],
        "invalidates_fact_tables must survive SchemaConverter::convert()"
    );
}

// ---------------------------------------------------------------------------
// Type field survival
// ---------------------------------------------------------------------------

#[test]
fn converter_threads_is_error_flag_on_type() {
    let schema = IntermediateSchema {
        types: vec![IntermediateType {
            name:     "UserNotFound".to_string(),
            is_error: true,
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let t = compiled.find_type("UserNotFound").expect("'UserNotFound' type must be present");

    assert!(t.is_error, "is_error flag must survive SchemaConverter::convert()");
}

#[test]
fn converter_threads_requires_role_on_type() {
    let schema = IntermediateSchema {
        types: vec![IntermediateType {
            name:         "SecretReport".to_string(),
            requires_role: Some("admin".to_string()),
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let t = compiled.find_type("SecretReport").expect("'SecretReport' type must be present");

    assert_eq!(
        t.requires_role.as_deref(),
        Some("admin"),
        "requires_role must survive SchemaConverter::convert() on types"
    );
}

// ---------------------------------------------------------------------------
// Multiple fields survive in a single call (composite regression guard)
// ---------------------------------------------------------------------------

#[test]
fn converter_all_critical_query_fields_survive_together() {
    let mut inject = IndexMap::new();
    inject.insert("tenant_id".to_string(), "jwt:org_id".to_string());

    let schema = IntermediateSchema {
        types: vec![order_type()],
        queries: vec![IntermediateQuery {
            name:              "fullQuery".to_string(),
            return_type:       "Order".to_string(),
            returns_list:      true,
            sql_source:        Some("v_full".to_string()),
            cache_ttl_seconds: Some(120),
            requires_role:     Some("viewer".to_string()),
            inject,
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let q = compiled.find_query("fullQuery").expect("'fullQuery' must be present");

    assert_eq!(q.sql_source.as_deref(), Some("v_full"), "sql_source");
    assert_eq!(q.cache_ttl_seconds, Some(120), "cache_ttl_seconds");
    assert_eq!(q.requires_role.as_deref(), Some("viewer"), "requires_role");
    assert_eq!(q.inject_params.len(), 1, "inject_params.len");
    let src = q.inject_params.get("tenant_id").expect("tenant_id must be in inject_params");
    assert_eq!(
        *src,
        InjectedParamSource::Jwt("org_id".to_string()),
        "inject source jwt:org_id must become Jwt(\"org_id\")"
    );
}
