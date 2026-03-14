#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Golden fixture tests — verify every field of every struct in `CompiledSchema`.
//!
//! Each fixture is a canonical JSON file that exercises a specific set of features.
//! These tests assert that:
//!   1. Every non-default field value parses correctly.
//!   2. The schema round-trips without data loss (`to_json` → `from_json` → equality).
//!
//! If any of these tests fail on a newly added field, it means the field was
//! silently lost or wrong — exactly the class of bug described in issue #53.

use std::path::{Path, PathBuf};

use fraiseql_core::schema::{
    CompiledSchema, CursorType, GraphQLValue, MutationOperation, RetryConfig, SqlProjectionHint,
};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/golden")
}

fn load_golden(name: &str) -> CompiledSchema {
    let path = fixtures_dir().join(name);
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Cannot read fixture {name}: {e}"));
    CompiledSchema::from_json(&json).unwrap_or_else(|e| panic!("Cannot parse fixture {name}: {e}"))
}

fn load_golden_json(name: &str) -> String {
    let path = fixtures_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Cannot read fixture {name}: {e}"))
}

// =============================================================================
// Fixture 01 — Basic queries and mutations with all field variants
// =============================================================================

#[test]
fn golden_01_types_all_fields() {
    let schema = load_golden("01-basic-query-mutation.json");
    assert_eq!(schema.types.len(), 1);

    let user = &schema.types[0];
    assert_eq!(user.name, "User");
    assert_eq!(user.sql_source, "v_user");
    assert_eq!(user.jsonb_column, "payload"); // non-default ("data")
    assert_eq!(user.description.as_deref(), Some("A registered user in the system"));
    assert!(!user.is_error);
    assert!(!user.relay);
    assert!(user.implements.is_empty());
    assert!(user.requires_role.is_none());

    let hint = user.sql_projection_hint.as_ref().expect("sql_projection_hint must be present");
    assert_eq!(hint.database, "postgresql");
    assert!(hint.projection_template.contains("jsonb_build_object"));
    assert_eq!(hint.estimated_reduction_percent, 60);

    // Fields cover all scalar FieldType variants
    let field_names: Vec<&str> = user.fields.iter().map(|f| f.name.as_str()).collect();
    assert!(field_names.contains(&"id"));
    assert!(field_names.contains(&"email"));
    assert!(field_names.contains(&"name"));
    assert!(field_names.contains(&"uid"));
    assert!(field_names.contains(&"meta"));
    assert!(field_names.contains(&"amount"));
    assert!(field_names.contains(&"created_at"));
    assert!(field_names.contains(&"email_addr"));
    assert!(field_names.contains(&"tags"));

    // Alias field
    let email_field = user.fields.iter().find(|f| f.name == "email_addr").unwrap();
    assert_eq!(email_field.alias.as_deref(), Some("emailAddress"));

    // Nullable variation
    let name_field = user.fields.iter().find(|f| f.name == "name").unwrap();
    assert!(name_field.nullable);
    assert_eq!(name_field.description.as_deref(), Some("Display name"));

    let id_field = user.fields.iter().find(|f| f.name == "id").unwrap();
    assert!(!id_field.nullable);
}

#[test]
fn golden_01_queries_all_fields() {
    let schema = load_golden("01-basic-query-mutation.json");
    assert_eq!(schema.queries.len(), 2);

    let users_q = schema.queries.iter().find(|q| q.name == "users").unwrap();
    assert_eq!(users_q.return_type, "User");
    assert!(users_q.returns_list);
    assert!(!users_q.nullable);
    assert_eq!(users_q.sql_source.as_deref(), Some("v_user"));
    assert_eq!(users_q.description.as_deref(), Some("List all users with optional filtering"));
    assert_eq!(users_q.jsonb_column, "payload");
    assert!(users_q.auto_params.has_where);
    assert!(users_q.auto_params.has_order_by);
    assert!(users_q.auto_params.has_limit);
    assert!(users_q.auto_params.has_offset);
    assert!(!users_q.relay);
    assert!(users_q.relay_cursor_column.is_none());
    assert_eq!(users_q.relay_cursor_type, CursorType::Int64);
    assert!(users_q.inject_params.is_empty());
    assert!(users_q.cache_ttl_seconds.is_none());
    assert!(users_q.additional_views.is_empty());
    assert!(users_q.requires_role.is_none());
    assert!(users_q.deprecation.is_none());
    assert_eq!(users_q.arguments.len(), 2);

    // Argument with default_value
    let limit_arg = users_q.arguments.iter().find(|a| a.name == "limit").unwrap();
    assert!(!limit_arg.nullable);
    assert_eq!(limit_arg.default_value, Some(GraphQLValue::Int(10)));

    // nullable argument
    let email_arg = users_q.arguments.iter().find(|a| a.name == "email").unwrap();
    assert!(email_arg.nullable);
    assert_eq!(email_arg.description.as_deref(), Some("Filter by email address"));

    // Single-item nullable query
    let user_q = schema.queries.iter().find(|q| q.name == "user").unwrap();
    assert!(!user_q.returns_list);
    assert!(user_q.nullable);
}

#[test]
fn golden_01_mutations_all_operations() {
    let schema = load_golden("01-basic-query-mutation.json");
    assert_eq!(schema.mutations.len(), 4);

    let create = schema.mutations.iter().find(|m| m.name == "createUser").unwrap();
    assert_eq!(create.sql_source.as_deref(), Some("fn_create_user"));
    assert_eq!(create.description.as_deref(), Some("Create a new user account"));
    assert_eq!(create.arguments.len(), 2);
    assert!(create.inject_params.is_empty());
    assert!(create.invalidates_fact_tables.is_empty());
    assert!(create.invalidates_views.is_empty());
    assert!(create.deprecation.is_none());
    match &create.operation {
        MutationOperation::Insert { table } => assert_eq!(table, "users"),
        other => panic!("Expected Insert, got {other:?}"),
    }

    let update = schema.mutations.iter().find(|m| m.name == "updateUser").unwrap();
    match &update.operation {
        MutationOperation::Update { table } => assert_eq!(table, "users"),
        other => panic!("Expected Update, got {other:?}"),
    }

    let delete = schema.mutations.iter().find(|m| m.name == "deleteUser").unwrap();
    match &delete.operation {
        MutationOperation::Delete { table } => assert_eq!(table, "users"),
        other => panic!("Expected Delete, got {other:?}"),
    }

    let custom = schema.mutations.iter().find(|m| m.name == "customOp").unwrap();
    assert_eq!(custom.operation, MutationOperation::Custom);
}

// =============================================================================
// Fixture 02 — Enums, input types, subscriptions
// =============================================================================

#[test]
fn golden_02_enums() {
    let schema = load_golden("02-enum-input-subscription.json");
    assert_eq!(schema.enums.len(), 2);

    let status = schema.enums.iter().find(|e| e.name == "OrderStatus").unwrap();
    assert_eq!(status.description.as_deref(), Some("Possible states of a customer order"));
    assert_eq!(status.values.len(), 5);

    let cancelled = status.values.iter().find(|v| v.name == "CANCELLED").unwrap();
    let dep = cancelled.deprecation.as_ref().expect("CANCELLED must be deprecated");
    assert_eq!(dep.reason.as_deref(), Some("Use REFUNDED instead"));

    let pending = status.values.iter().find(|v| v.name == "PENDING").unwrap();
    assert!(pending.deprecation.is_none());
    assert_eq!(pending.description.as_deref(), Some("Order has been placed"));
}

#[test]
fn golden_02_input_types() {
    let schema = load_golden("02-enum-input-subscription.json");
    assert_eq!(schema.input_types.len(), 1);

    let filter = &schema.input_types[0];
    assert_eq!(filter.name, "OrderFilter");
    assert_eq!(filter.description.as_deref(), Some("Filter criteria for orders"));
    assert_eq!(filter.fields.len(), 3);
    assert!(filter.metadata.is_some());

    let min_amount = filter.fields.iter().find(|f| f.name == "min_amount").unwrap();
    assert_eq!(min_amount.default_value.as_deref(), Some("0"));

    let created_after = filter.fields.iter().find(|f| f.name == "created_after").unwrap();
    let dep = created_after.deprecation.as_ref().expect("created_after must be deprecated");
    assert_eq!(dep.reason.as_deref(), Some("Use dateRange instead"));
}

#[test]
fn golden_02_subscriptions() {
    let schema = load_golden("02-enum-input-subscription.json");
    assert_eq!(schema.subscriptions.len(), 2);

    let order_created = schema.subscriptions.iter().find(|s| s.name == "orderCreated").unwrap();
    assert_eq!(order_created.return_type, "Order");
    assert_eq!(order_created.description.as_deref(), Some("Fires when a new order is placed"));
    assert_eq!(order_created.topic.as_deref(), Some("orders.created"));
    assert!(!order_created.fields.is_empty());
    assert!(order_created.fields.contains(&"id".to_string()));
    assert!(!order_created.filter_fields.is_empty());
    assert!(order_created.filter_fields.contains(&"userId".to_string()));
    assert!(order_created.deprecation.is_none());
    assert_eq!(order_created.arguments.len(), 1);

    let filter = order_created.filter.as_ref().expect("filter must be present");
    assert!(filter.argument_paths.contains_key("userId"));
    assert_eq!(filter.argument_paths["userId"], "/user_id");
    assert_eq!(filter.static_filters.len(), 2);
    assert_eq!(filter.static_filters[0].path, "/source");

    // Deprecated subscription
    let legacy = schema.subscriptions.iter().find(|s| s.name == "legacyOrderUpdated").unwrap();
    assert!(legacy.deprecation.is_some());
    let dep = legacy.deprecation.as_ref().unwrap();
    assert_eq!(dep.reason.as_deref(), Some("Use orderUpdated subscription instead"));

    // subscriptions_config present
    assert!(schema.subscriptions_config.is_some());
}

// =============================================================================
// Fixture 03 — Interfaces, unions, custom directives
// =============================================================================

#[test]
fn golden_03_interfaces() {
    let schema = load_golden("03-interface-union-directive.json");
    assert_eq!(schema.interfaces.len(), 2);

    let node = schema.interfaces.iter().find(|i| i.name == "Node").unwrap();
    assert_eq!(node.description.as_deref(), Some("An object with a globally unique ID"));
    assert_eq!(node.fields.len(), 1);
    assert_eq!(node.fields[0].name, "id");
}

#[test]
fn golden_03_unions() {
    let schema = load_golden("03-interface-union-directive.json");
    assert_eq!(schema.unions.len(), 2);

    let search = schema.unions.iter().find(|u| u.name == "SearchResult").unwrap();
    assert_eq!(search.description.as_deref(), Some("Possible types returned by a search query"));
    assert_eq!(search.member_types.len(), 2);
    assert!(search.member_types.contains(&"BlogPost".to_string()));
    assert!(search.member_types.contains(&"Video".to_string()));
}

#[test]
fn golden_03_directives() {
    let schema = load_golden("03-interface-union-directive.json");
    assert_eq!(schema.directives.len(), 2);

    let rate_limit = schema.directives.iter().find(|d| d.name == "rateLimit").unwrap();
    assert_eq!(
        rate_limit.description.as_deref(),
        Some("Apply rate limiting to a field or operation")
    );
    assert_eq!(rate_limit.locations.len(), 2);
    assert_eq!(rate_limit.arguments.len(), 2);
    assert!(!rate_limit.is_repeatable);

    let auth = schema.directives.iter().find(|d| d.name == "auth").unwrap();
    assert!(auth.is_repeatable);
    assert_eq!(auth.locations.len(), 4);
}

#[test]
fn golden_03_type_implements() {
    let schema = load_golden("03-interface-union-directive.json");

    let post = schema.types.iter().find(|t| t.name == "BlogPost").unwrap();
    assert_eq!(post.implements.len(), 2);
    assert!(post.implements.contains(&"Node".to_string()));
    assert!(post.implements.contains(&"Searchable".to_string()));
}

// =============================================================================
// Fixture 04 — Error types (is_error: true)
// =============================================================================

#[test]
fn golden_04_error_types() {
    let schema = load_golden("04-error-type.json");

    let dup = schema.types.iter().find(|t| t.name == "DuplicateEmailError").unwrap();
    assert!(dup.is_error);
    assert_eq!(dup.description.as_deref(), Some("Error returned when email already exists"));
    assert_eq!(dup.fields.len(), 5);

    let val = schema.types.iter().find(|t| t.name == "ValidationError").unwrap();
    assert!(val.is_error);

    // Non-error type
    let success = schema.types.iter().find(|t| t.name == "CreateUserSuccess").unwrap();
    assert!(!success.is_error);
}

// =============================================================================
// Fixture 05 — Security, inject_params, cache, observers, fact_tables
// =============================================================================

#[test]
fn golden_05_inject_params_and_cache() {
    let schema = load_golden("05-security-inject-cache.json");

    let orders_q = schema.queries.iter().find(|q| q.name == "orders").unwrap();
    assert_eq!(orders_q.inject_params.len(), 1);
    assert!(orders_q.inject_params.contains_key("tenant_id"));
    assert_eq!(orders_q.cache_ttl_seconds, Some(300));
    assert_eq!(orders_q.additional_views, vec!["v_order_summary", "v_order_items"]);
    assert_eq!(orders_q.requires_role.as_deref(), Some("admin"));

    let summary_q = schema.queries.iter().find(|q| q.name == "orderSummary").unwrap();
    assert_eq!(summary_q.inject_params.len(), 2);
    assert!(summary_q.inject_params.contains_key("user_id"));
    assert!(summary_q.inject_params.contains_key("tenant_id"));
    assert_eq!(summary_q.cache_ttl_seconds, Some(0));
}

#[test]
fn golden_05_mutation_inject_and_invalidates() {
    let schema = load_golden("05-security-inject-cache.json");

    let create_order = schema.mutations.iter().find(|m| m.name == "createOrder").unwrap();
    assert_eq!(create_order.inject_params.len(), 2);
    assert!(create_order.inject_params.contains_key("user_id"));
    assert!(create_order.inject_params.contains_key("tenant_id"));
    assert_eq!(create_order.invalidates_fact_tables, vec!["tf_sales", "tf_order_count"]);
    assert_eq!(create_order.invalidates_views, vec!["v_order_summary", "v_order_items"]);
}

#[test]
fn golden_05_fact_tables() {
    let schema = load_golden("05-security-inject-cache.json");
    assert_eq!(schema.fact_tables.len(), 2);
    assert!(schema.fact_tables.contains_key("tf_sales"));
    assert!(schema.fact_tables.contains_key("tf_order_count"));
}

#[test]
fn golden_05_observers() {
    let schema = load_golden("05-security-inject-cache.json");
    assert_eq!(schema.observers.len(), 2);

    let high_value = schema.observers.iter().find(|o| o.name == "onHighValueOrder").unwrap();
    assert_eq!(high_value.entity, "Order");
    assert_eq!(high_value.event, "INSERT");
    assert_eq!(high_value.condition.as_deref(), Some("amount > 1000"));
    assert_eq!(high_value.actions.len(), 2);
    assert_eq!(high_value.retry.max_attempts, 5);
    assert_eq!(high_value.retry.backoff_strategy, "exponential");
    assert_eq!(high_value.retry.initial_delay_ms, 500);
    assert_eq!(high_value.retry.max_delay_ms, 30_000);

    let deleted = schema.observers.iter().find(|o| o.name == "onOrderDeleted").unwrap();
    assert_eq!(deleted.event, "DELETE");
    assert!(deleted.condition.is_none());
    assert_eq!(deleted.retry.backoff_strategy, "linear");
    assert_ne!(deleted.retry, RetryConfig::default()); // non-default retry
}

#[test]
fn golden_05_security_config() {
    let schema = load_golden("05-security-inject-cache.json");
    assert!(schema.security.is_some());

    let config = schema.security_config().expect("security_config must parse");
    assert_eq!(config.role_definitions.len(), 2);

    let admin = config.role_definitions.iter().find(|r| r.name == "admin").unwrap();
    assert_eq!(admin.description.as_deref(), Some("Full system administrator"));
    assert_eq!(admin.scopes.len(), 3);
    assert!(admin.scopes.iter().any(|s| s == "admin:*"));

    assert_eq!(config.default_role.as_deref(), Some("viewer"));
}

#[test]
fn golden_05_observers_config_and_validation_config() {
    let schema = load_golden("05-security-inject-cache.json");
    assert!(schema.observers_config.is_some());
    assert!(schema.validation_config.is_some());

    let obs_cfg = schema.observers_config.as_ref().expect("observers_config present");
    assert_eq!(obs_cfg.backend, "nats");
    assert!(obs_cfg.nats_url.is_some());
}

#[test]
fn golden_05_type_requires_role() {
    let schema = load_golden("05-security-inject-cache.json");
    let order_type = schema.types.iter().find(|t| t.name == "Order").unwrap();
    assert_eq!(order_type.requires_role.as_deref(), Some("admin"));
}

// =============================================================================
// Fixture 06 — Relay pagination with Int64 cursor (default)
// =============================================================================

#[test]
fn golden_06_relay_int64() {
    let schema = load_golden("06-relay-int64-cursor.json");

    let product_type = schema.types.iter().find(|t| t.name == "Product").unwrap();
    assert!(product_type.relay);

    let products_q = schema.queries.iter().find(|q| q.name == "products").unwrap();
    assert!(products_q.relay);
    assert_eq!(products_q.relay_cursor_column.as_deref(), Some("pk_product"));
    assert_eq!(products_q.relay_cursor_type, CursorType::Int64);

    let product_q = schema.queries.iter().find(|q| q.name == "product").unwrap();
    assert!(product_q.relay);
    assert_eq!(product_q.relay_cursor_column.as_deref(), Some("pk_product"));
    assert_eq!(product_q.relay_cursor_type, CursorType::Int64);
}

// =============================================================================
// Fixture 07 — Relay pagination with UUID cursor + federation/debug/mcp/sdl
// =============================================================================

#[test]
fn golden_07_relay_uuid_cursor() {
    let schema = load_golden("07-relay-uuid-cursor.json");

    let item_type = schema.types.iter().find(|t| t.name == "Item").unwrap();
    assert!(item_type.relay);

    let items_q = schema.queries.iter().find(|q| q.name == "items").unwrap();
    assert!(items_q.relay);
    assert_eq!(items_q.relay_cursor_column.as_deref(), Some("id"));
    assert_eq!(items_q.relay_cursor_type, CursorType::Uuid);
    assert_eq!(
        items_q.description.as_deref(),
        Some("Relay-paginated item list using UUID cursor")
    );
}

#[test]
fn golden_07_federation_debug_mcp_sdl() {
    let schema = load_golden("07-relay-uuid-cursor.json");

    let fed = schema.federation.as_ref().expect("federation must be present");
    assert!(fed.enabled);
    assert_eq!(fed.service_name.as_deref(), Some("items-service"));
    assert!(fed.circuit_breaker.is_some());

    let debug = schema.debug_config.as_ref().expect("debug_config must be present");
    assert!(debug.enabled);
    assert!(debug.database_explain);

    let mcp = schema.mcp_config.as_ref().expect("mcp_config must be present");
    assert!(mcp.enabled);
    assert_eq!(mcp.path, "/mcp");
    assert_eq!(mcp.include, ["items"]);

    let sdl = schema.schema_sdl.as_ref().expect("schema_sdl must be present");
    assert!(sdl.contains("type Item"));
    assert!(sdl.contains("ItemConnection"));
}

// =============================================================================
// Fixture 08 — Deprecation on queries, mutations, enums, fields, arguments, subscriptions
// =============================================================================

#[test]
fn golden_08_deprecated_query_and_args() {
    let schema = load_golden("08-deprecation-advanced.json");

    let legacy_q = schema.queries.iter().find(|q| q.name == "legacyUsers").unwrap();
    let dep = legacy_q.deprecation.as_ref().expect("legacyUsers must be deprecated");
    assert_eq!(dep.reason.as_deref(), Some("Use 'users' query with new schema instead"));

    let old_limit = legacy_q.arguments.iter().find(|a| a.name == "oldLimit").unwrap();
    let arg_dep = old_limit.deprecation.as_ref().expect("oldLimit must be deprecated");
    assert_eq!(arg_dep.reason.as_deref(), Some("Use 'first' argument instead"));

    // Deprecation with no reason (empty object)
    let legacy_single = schema.queries.iter().find(|q| q.name == "legacyUser").unwrap();
    assert!(legacy_single.deprecation.is_some());
    let dep2 = legacy_single.deprecation.as_ref().unwrap();
    assert!(dep2.reason.is_none());
}

#[test]
fn golden_08_deprecated_mutation_and_args() {
    let schema = load_golden("08-deprecation-advanced.json");

    let create = schema.mutations.iter().find(|m| m.name == "createLegacyUser").unwrap();
    let dep = create.deprecation.as_ref().expect("createLegacyUser must be deprecated");
    assert_eq!(dep.reason.as_deref(), Some("Use createUser mutation instead"));

    let password_arg = create.arguments.iter().find(|a| a.name == "password").unwrap();
    let arg_dep = password_arg.deprecation.as_ref().expect("password must be deprecated");
    assert_eq!(arg_dep.reason.as_deref(), Some("Credentials managed externally via OIDC"));
}

#[test]
fn golden_08_deprecated_enum_value_and_field() {
    let schema = load_golden("08-deprecation-advanced.json");

    let legacy_status = schema.enums.iter().find(|e| e.name == "LegacyStatus").unwrap();
    let suspended = legacy_status.values.iter().find(|v| v.name == "SUSPENDED").unwrap();
    let dep = suspended.deprecation.as_ref().expect("SUSPENDED must be deprecated");
    assert_eq!(dep.reason.as_deref(), Some("Use DISABLED instead"));

    let legacy_user = schema.types.iter().find(|t| t.name == "LegacyUser").unwrap();
    let login_field = legacy_user.fields.iter().find(|f| f.name == "login").unwrap();
    let field_dep = login_field.deprecation.as_ref().expect("login field must be deprecated");
    assert_eq!(field_dep.reason.as_deref(), Some("Use 'email' field instead"));
}

#[test]
fn golden_08_deprecated_subscription() {
    let schema = load_golden("08-deprecation-advanced.json");

    let legacy_sub = schema.subscriptions.iter().find(|s| s.name == "legacyUserEvents").unwrap();
    let dep = legacy_sub.deprecation.as_ref().expect("legacyUserEvents must be deprecated");
    assert_eq!(dep.reason.as_deref(), Some("Use userEvents subscription on new schema"));
}

// =============================================================================
// Round-trip invariant — all 8 fixtures survive serialize → deserialize unchanged
// =============================================================================

const FIXTURE_NAMES: &[&str] = &[
    "01-basic-query-mutation.json",
    "02-enum-input-subscription.json",
    "03-interface-union-directive.json",
    "04-error-type.json",
    "05-security-inject-cache.json",
    "06-relay-int64-cursor.json",
    "07-relay-uuid-cursor.json",
    "08-deprecation-advanced.json",
];

#[test]
fn roundtrip_all_fixtures() {
    for name in FIXTURE_NAMES {
        let original_json = load_golden_json(name);
        let schema1 = CompiledSchema::from_json(&original_json)
            .unwrap_or_else(|e| panic!("Parse failed for {name}: {e}"));

        let reserialised =
            schema1.to_json().unwrap_or_else(|e| panic!("Serialise failed for {name}: {e}"));

        let schema2 = CompiledSchema::from_json(&reserialised)
            .unwrap_or_else(|e| panic!("Re-parse failed for {name}: {e}"));

        // PartialEq covers all structural fields (types, queries, mutations,
        // subscriptions, enums, input_types, interfaces, unions, directives,
        // fact_tables, observers, federation, security, observers_config,
        // subscriptions_config, schema_sdl).
        assert_eq!(schema1, schema2, "Round-trip equality failed for fixture: {name}");

        // Manually verify fields excluded from PartialEq are preserved.
        assert_eq!(
            schema1.validation_config, schema2.validation_config,
            "validation_config lost in round-trip for {name}"
        );
        assert_eq!(
            schema1.debug_config, schema2.debug_config,
            "debug_config lost in round-trip for {name}"
        );
        assert_eq!(
            schema1.mcp_config, schema2.mcp_config,
            "mcp_config lost in round-trip for {name}"
        );
    }
}

#[test]
fn sql_projection_hint_roundtrip() {
    let schema = load_golden("01-basic-query-mutation.json");
    let hint: &SqlProjectionHint = schema.types[0].sql_projection_hint.as_ref().unwrap();

    let json = serde_json::to_string(hint).unwrap();
    let hint2: SqlProjectionHint = serde_json::from_str(&json).unwrap();
    assert_eq!(hint.database, hint2.database);
    assert_eq!(hint.projection_template, hint2.projection_template);
    assert_eq!(hint.estimated_reduction_percent, hint2.estimated_reduction_percent);
}
