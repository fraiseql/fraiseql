//! Pipeline 5 — Stage B integration tests: CLI converter inject params.
//!
//! Verifies that `SchemaConverter::convert()` correctly parses every documented
//! inject-source string (`"jwt:<claim>"`) into the corresponding
//! `InjectedParamSource` variant, covering all special-alias claims and
//! arbitrary attribute names.
//!
//! **Why these tests exist**: `parse_inject_source` is private, but it is the
//! only logic that transforms the raw `"jwt:claim"` strings emitted by all six
//! authoring SDKs into the strongly-typed `InjectedParamSource` that the runtime
//! executor consumes.  A mis-parse (e.g. wrong claim name, wrong enum variant)
//! would silently break tenant isolation at runtime.

use indexmap::IndexMap;

use fraiseql_cli::schema::{
    SchemaConverter,
    intermediate::{IntermediateField, IntermediateQuery, IntermediateSchema, IntermediateType},
};
use fraiseql_core::schema::InjectedParamSource;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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

fn schema_with_inject(inject: IndexMap<String, String>) -> IntermediateSchema {
    IntermediateSchema {
        types:   vec![order_type()],
        queries: vec![IntermediateQuery {
            name:         "orders".to_string(),
            return_type:  "Order".to_string(),
            returns_list: true,
            sql_source:   Some("v_order".to_string()),
            inject,
            ..Default::default()
        }],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// JWT claim alias tests
// ---------------------------------------------------------------------------

/// Stage B: `"jwt:sub"` → `InjectedParamSource::Jwt("sub")`.
///
/// `sub` is the canonical OIDC subject claim, aliased to
/// `SecurityContext.user_id` by the executor.
#[test]
fn parse_inject_source_jwt_sub() {
    let mut inject = IndexMap::new();
    inject.insert("user_id".to_string(), "jwt:sub".to_string());

    let compiled = SchemaConverter::convert(schema_with_inject(inject))
        .expect("convert must succeed");
    let q = compiled.find_query("orders").expect("'orders' must be present");

    assert_eq!(q.inject_params.len(), 1);
    assert_eq!(
        *q.inject_params.get("user_id").unwrap(),
        InjectedParamSource::Jwt("sub".to_string()),
        "jwt:sub must produce Jwt(\"sub\")"
    );
}

/// Stage B: `"jwt:tenant_id"` → `InjectedParamSource::Jwt("tenant_id")`.
///
/// `tenant_id` is the primary multi-tenancy claim alias.
#[test]
fn parse_inject_source_jwt_tenant_id() {
    let mut inject = IndexMap::new();
    inject.insert("tenant_id".to_string(), "jwt:tenant_id".to_string());

    let compiled = SchemaConverter::convert(schema_with_inject(inject))
        .expect("convert must succeed");
    let q = compiled.find_query("orders").expect("'orders' must be present");

    assert_eq!(
        *q.inject_params.get("tenant_id").unwrap(),
        InjectedParamSource::Jwt("tenant_id".to_string()),
        "jwt:tenant_id must produce Jwt(\"tenant_id\")"
    );
}

/// Stage B: `"jwt:org_id"` → `InjectedParamSource::Jwt("org_id")`.
///
/// `org_id` is an alternative multi-tenancy claim alias.
#[test]
fn parse_inject_source_jwt_org_id() {
    let mut inject = IndexMap::new();
    inject.insert("org_id".to_string(), "jwt:org_id".to_string());

    let compiled = SchemaConverter::convert(schema_with_inject(inject))
        .expect("convert must succeed");
    let q = compiled.find_query("orders").expect("'orders' must be present");

    assert_eq!(
        *q.inject_params.get("org_id").unwrap(),
        InjectedParamSource::Jwt("org_id".to_string()),
        "jwt:org_id must produce Jwt(\"org_id\")"
    );
}

/// Stage B: arbitrary claim name → `InjectedParamSource::Jwt("<claim>")`.
///
/// Any claim not matching a special alias is looked up in
/// `SecurityContext.attributes` at runtime.
#[test]
fn parse_inject_source_jwt_arbitrary_claim() {
    let mut inject = IndexMap::new();
    inject.insert("department".to_string(), "jwt:department".to_string());

    let compiled = SchemaConverter::convert(schema_with_inject(inject))
        .expect("convert must succeed");
    let q = compiled.find_query("orders").expect("'orders' must be present");

    assert_eq!(
        *q.inject_params.get("department").unwrap(),
        InjectedParamSource::Jwt("department".to_string()),
        "arbitrary jwt claim must produce Jwt(\"department\")"
    );
}

/// Stage B: claim names with underscores and mixed chars are preserved exactly.
#[test]
fn parse_inject_source_jwt_claim_with_underscores() {
    let mut inject = IndexMap::new();
    inject.insert("custom_attr".to_string(), "jwt:custom_attr".to_string());

    let compiled = SchemaConverter::convert(schema_with_inject(inject))
        .expect("convert must succeed");
    let q = compiled.find_query("orders").expect("'orders' must be present");

    assert_eq!(
        *q.inject_params.get("custom_attr").unwrap(),
        InjectedParamSource::Jwt("custom_attr".to_string()),
    );
}

// ---------------------------------------------------------------------------
// Error paths — invalid inject source format
// ---------------------------------------------------------------------------

/// Stage B error: unknown prefix must be rejected.
///
/// Only `"jwt:<claim>"` is a valid source prefix.  Any other format should
/// cause `SchemaConverter::convert` to return `Err`.
#[test]
fn parse_inject_source_invalid_prefix_returns_error() {
    let mut inject = IndexMap::new();
    inject.insert("tenant_id".to_string(), "not_jwt:sub".to_string());

    let result = SchemaConverter::convert(schema_with_inject(inject));
    assert!(result.is_err(), "unknown prefix must return Err");
    // Use {:#} to display the full anyhow error chain.
    let msg = format!("{:#}", result.unwrap_err());
    assert!(
        msg.contains("jwt:") || msg.contains("Unknown inject source"),
        "error must mention the supported format: {msg}"
    );
}

/// Stage B error: `"jwt:"` with empty claim name must be rejected.
#[test]
fn parse_inject_source_empty_claim_returns_error() {
    let mut inject = IndexMap::new();
    inject.insert("tenant_id".to_string(), "jwt:".to_string());

    let result = SchemaConverter::convert(schema_with_inject(inject));
    assert!(result.is_err(), "empty claim name must return Err");
    let msg = format!("{:#}", result.unwrap_err());
    assert!(
        msg.contains("claim") || msg.contains("jwt:"),
        "error must describe the problem: {msg}"
    );
}

/// Stage B error: bare string without any prefix must be rejected.
#[test]
fn parse_inject_source_bare_string_returns_error() {
    let mut inject = IndexMap::new();
    inject.insert("tenant_id".to_string(), "just_a_string".to_string());

    let result = SchemaConverter::convert(schema_with_inject(inject));
    assert!(result.is_err(), "bare string without prefix must return Err");
}

/// Stage B error: inject param name conflicts with an explicit argument.
///
/// The converter must reject schemas where `inject` key shadows an argument,
/// preventing silent overrides of user-supplied values.
#[test]
fn parse_inject_source_arg_conflict_returns_error() {
    use fraiseql_cli::schema::intermediate::IntermediateArgument;

    let mut inject = IndexMap::new();
    inject.insert("filter_id".to_string(), "jwt:sub".to_string());

    let schema = IntermediateSchema {
        types:   vec![order_type()],
        queries: vec![IntermediateQuery {
            name:         "filteredOrders".to_string(),
            return_type:  "Order".to_string(),
            returns_list: true,
            sql_source:   Some("v_order".to_string()),
            // `filter_id` appears both as an argument and as an inject param
            arguments: vec![IntermediateArgument {
                name:       "filter_id".to_string(),
                arg_type:   "ID".to_string(),
                nullable:   false,
                default:    None,
                deprecated: None,
            }],
            inject,
            ..Default::default()
        }],
        ..Default::default()
    };

    let result = SchemaConverter::convert(schema);
    assert!(result.is_err(), "arg/inject name conflict must return Err");
    let msg = format!("{:#}", result.unwrap_err());
    assert!(
        msg.contains("filter_id") || msg.contains("conflict"),
        "error must name the conflicting param: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Multi-inject round-trip (Stage A → B)
// ---------------------------------------------------------------------------

/// Stage A → B: query with multiple inject params — all survive the converter.
///
/// Mirrors the real-world pattern where a tenant-scoped query injects both
/// `user_id` (from `sub`) and `tenant_id` simultaneously.
#[test]
fn converter_multi_inject_params_all_survive() {
    let mut inject = IndexMap::new();
    inject.insert("tenant_id".to_string(), "jwt:tenant_id".to_string());
    inject.insert("user_id".to_string(), "jwt:sub".to_string());
    inject.insert("org_id".to_string(), "jwt:org_id".to_string());

    let compiled = SchemaConverter::convert(schema_with_inject(inject))
        .expect("convert must succeed");
    let q = compiled.find_query("orders").expect("'orders' must be present");

    assert_eq!(
        q.inject_params.len(),
        3,
        "all three inject params must survive conversion"
    );

    assert_eq!(
        *q.inject_params.get("tenant_id").unwrap(),
        InjectedParamSource::Jwt("tenant_id".to_string())
    );
    assert_eq!(
        *q.inject_params.get("user_id").unwrap(),
        InjectedParamSource::Jwt("sub".to_string())
    );
    assert_eq!(
        *q.inject_params.get("org_id").unwrap(),
        InjectedParamSource::Jwt("org_id".to_string())
    );
}

/// Stage A → B: query with no inject params produces empty `inject_params` map.
#[test]
fn converter_empty_inject_produces_empty_map() {
    let schema = IntermediateSchema {
        types:   vec![order_type()],
        queries: vec![IntermediateQuery {
            name:         "allOrders".to_string(),
            return_type:  "Order".to_string(),
            returns_list: true,
            sql_source:   Some("v_order".to_string()),
            // No inject field — relies on Default
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let q = compiled.find_query("allOrders").expect("'allOrders' must be present");

    assert!(
        q.inject_params.is_empty(),
        "query with no inject must have empty inject_params"
    );
}

/// Stage A → B: inject params on mutations are also preserved.
#[test]
fn converter_inject_params_on_mutation_survive() {
    use fraiseql_cli::schema::intermediate::IntermediateMutation;

    let mut inject = IndexMap::new();
    inject.insert("user_id".to_string(), "jwt:sub".to_string());

    let schema = IntermediateSchema {
        types: vec![order_type()],
        mutations: vec![IntermediateMutation {
            name:       "placeOrder".to_string(),
            return_type: "Order".to_string(),
            sql_source: Some("fn_place_order".to_string()),
            inject,
            ..Default::default()
        }],
        ..Default::default()
    };

    let compiled = SchemaConverter::convert(schema).expect("convert must succeed");
    let m = compiled.find_mutation("placeOrder").expect("'placeOrder' must be present");

    assert_eq!(m.inject_params.len(), 1);
    assert_eq!(
        *m.inject_params.get("user_id").unwrap(),
        InjectedParamSource::Jwt("sub".to_string()),
        "inject on mutation must survive SchemaConverter::convert()"
    );
}
