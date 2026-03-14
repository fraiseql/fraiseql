//! External integration tests for `fraiseql-test-utils`.
//!
//! Complements the inline unit tests by exercising the public API from an
//! outside-crate perspective and covering builder methods not exercised inline.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)] // Reason: test code

use fraiseql_core::schema::{CursorType, FieldDenyPolicy, FieldType};
use fraiseql_test_utils::{
    TestFieldBuilder, TestMutationBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
    assert_field_path, assert_graphql_error_code, assert_graphql_error_contains,
    assert_graphql_success, assert_has_data, assert_no_graphql_errors, create_temp_dir,
    fixtures::{sample_error_response, sample_query_response, sample_user},
    get_test_id, mock_db::{MockDb, MockDbError}, setup_test_env,
};
use serde_json::json;

// ── TestQueryBuilder — methods not covered inline ─────────────────────────────

#[test]
fn query_builder_with_description() {
    let q = TestQueryBuilder::new("users", "User")
        .with_description("Fetch all users")
        .build();
    assert_eq!(q.description.as_deref(), Some("Fetch all users"));
}

#[test]
fn query_builder_no_sql_source_sets_none() {
    let q = TestQueryBuilder::new("custom", "Result").no_sql_source().build();
    assert!(q.sql_source.is_none());
}

#[test]
fn query_builder_default_sql_source_is_v_name() {
    let q = TestQueryBuilder::new("orders", "Order").build();
    assert_eq!(q.sql_source.as_deref(), Some("v_orders"));
}

#[test]
fn query_builder_relay_flag_true() {
    let q = TestQueryBuilder::new("posts", "Post").relay(true).build();
    assert!(q.relay);
}

#[test]
fn query_builder_relay_flag_false_by_default() {
    let q = TestQueryBuilder::new("posts", "Post").build();
    assert!(!q.relay);
}

#[test]
fn query_builder_relay_cursor_column_implies_relay_true() {
    let q = TestQueryBuilder::new("items", "Item").relay_cursor_column("pk_item").build();
    assert!(q.relay, "relay should be set when cursor column is specified");
    assert_eq!(q.relay_cursor_column.as_deref(), Some("pk_item"));
}

#[test]
fn query_builder_relay_cursor_type_uuid() {
    let q = TestQueryBuilder::new("nodes", "Node")
        .relay_cursor_column("id")
        .relay_cursor_type(CursorType::Uuid)
        .build();
    assert_eq!(q.relay_cursor_type, CursorType::Uuid);
}

#[test]
fn query_builder_relay_cursor_type_int64_default() {
    let q = TestQueryBuilder::new("nodes", "Node").relay(true).build();
    assert_eq!(q.relay_cursor_type, CursorType::Int64);
}

#[test]
fn query_builder_with_additional_views() {
    let views = vec!["v_user_stats".to_string(), "v_user_roles".to_string()];
    let q = TestQueryBuilder::new("users", "User").with_additional_views(views.clone()).build();
    assert_eq!(q.additional_views, views);
}

#[test]
fn query_builder_additional_views_empty_by_default() {
    let q = TestQueryBuilder::new("users", "User").build();
    assert!(q.additional_views.is_empty());
}

#[test]
fn query_builder_returns_list_false_by_default() {
    let q = TestQueryBuilder::new("user", "User").build();
    assert!(!q.returns_list);
}

// ── TestMutationBuilder — methods not covered inline ──────────────────────────

#[test]
fn mutation_builder_default_sql_source_is_fn_name() {
    let m = TestMutationBuilder::new("createPost", "Post").build();
    assert_eq!(m.sql_source.as_deref(), Some("fn_createPost"));
}

#[test]
fn mutation_builder_with_description() {
    let m = TestMutationBuilder::new("deleteUser", "DeleteResult")
        .with_description("Permanently remove a user account")
        .build();
    assert_eq!(m.description.as_deref(), Some("Permanently remove a user account"));
}

#[test]
fn mutation_builder_deprecated() {
    let m = TestMutationBuilder::new("oldCreate", "Thing")
        .deprecated("Use createThing2 instead")
        .build();
    let dep = m.deprecation.unwrap();
    assert_eq!(dep.reason.as_deref(), Some("Use createThing2 instead"));
}

#[test]
fn mutation_builder_no_description_by_default() {
    let m = TestMutationBuilder::new("doThing", "Thing").build();
    assert!(m.description.is_none());
}

// ── TestTypeBuilder — methods not covered inline ──────────────────────────────

#[test]
fn type_builder_relay_node() {
    let t = TestTypeBuilder::new("Post", "v_post").relay_node().build();
    assert!(t.relay);
}

#[test]
fn type_builder_relay_false_by_default() {
    let t = TestTypeBuilder::new("Post", "v_post").build();
    assert!(!t.relay);
}

#[test]
fn type_builder_with_implements() {
    let t = TestTypeBuilder::new("User", "v_user").with_implements(&["Node", "Auditable"]).build();
    assert_eq!(t.implements, vec!["Node".to_string(), "Auditable".to_string()]);
}

#[test]
fn type_builder_implements_empty_by_default() {
    let t = TestTypeBuilder::new("User", "v_user").build();
    assert!(t.implements.is_empty());
}

#[test]
fn type_builder_requires_role() {
    let t = TestTypeBuilder::new("AdminView", "v_admin").requires_role("superadmin").build();
    assert_eq!(t.requires_role.as_deref(), Some("superadmin"));
}

#[test]
fn type_builder_no_role_by_default() {
    let t = TestTypeBuilder::new("User", "v_user").build();
    assert!(t.requires_role.is_none());
}

#[test]
fn type_builder_with_description() {
    let t = TestTypeBuilder::new("Product", "v_product")
        .with_description("A purchasable product")
        .build();
    assert_eq!(t.description.as_deref(), Some("A purchasable product"));
}

#[test]
fn type_builder_no_description_by_default() {
    let t = TestTypeBuilder::new("Item", "v_item").build();
    assert!(t.description.is_none());
}

#[test]
fn type_builder_with_scoped_field_sets_scope() {
    let t = TestTypeBuilder::new("Employee", "v_employee")
        .with_scoped_field("salary", FieldType::Int, "read:salary")
        .build();
    assert_eq!(t.fields[0].requires_scope.as_deref(), Some("read:salary"));
}

// ── TestFieldBuilder — methods not covered inline ─────────────────────────────

#[test]
fn field_builder_with_description() {
    let f = TestFieldBuilder::new("email", FieldType::String)
        .with_description("User's email address")
        .build();
    assert_eq!(f.description.as_deref(), Some("User's email address"));
}

#[test]
fn field_builder_deprecated() {
    let f = TestFieldBuilder::new("legacyId", FieldType::Int)
        .deprecated("Use id instead")
        .build();
    let dep = f.deprecation.unwrap();
    assert_eq!(dep.reason.as_deref(), Some("Use id instead"));
}

#[test]
fn field_builder_not_deprecated_by_default() {
    let f = TestFieldBuilder::new("id", FieldType::Int).build();
    assert!(f.deprecation.is_none());
}

#[test]
fn field_builder_deny_policy_mask() {
    let f = TestFieldBuilder::new("ssn", FieldType::String)
        .requires_scope("pii:read")
        .on_deny(FieldDenyPolicy::Mask)
        .build();
    assert_eq!(f.on_deny, FieldDenyPolicy::Mask);
}

#[test]
fn field_builder_nullable_is_nullable() {
    let f = TestFieldBuilder::nullable("bio", FieldType::String).build();
    assert!(f.nullable);
}

#[test]
fn field_builder_new_is_not_nullable() {
    let f = TestFieldBuilder::new("id", FieldType::Int).build();
    assert!(!f.nullable);
}

#[test]
fn field_builder_scope_not_set_by_default() {
    let f = TestFieldBuilder::new("name", FieldType::String).build();
    assert!(f.requires_scope.is_none());
}

// ── TestSchemaBuilder — methods not covered inline ────────────────────────────

#[test]
fn schema_builder_with_empty_type() {
    let schema = TestSchemaBuilder::new().with_empty_type("Tag", "v_tag").build();
    assert_eq!(schema.types.len(), 1);
    assert_eq!(schema.types[0].name, "Tag");
    assert_eq!(schema.types[0].sql_source, "v_tag");
    assert!(schema.types[0].fields.is_empty());
}

#[test]
fn schema_builder_with_federation() {
    let schema = TestSchemaBuilder::new()
        .with_federation(json!({"serviceUrl": "http://auth.svc/graphql"}))
        .build();
    assert!(schema.federation.is_some());
}

#[test]
fn schema_builder_multiple_queries_all_indexed() {
    let schema = TestSchemaBuilder::new()
        .with_simple_query("users", "User", true)
        .with_simple_query("user", "User", false)
        .with_simple_query("adminUsers", "User", true)
        .build();
    assert_eq!(schema.queries.len(), 3);
    assert!(schema.find_query("users").is_some());
    assert!(schema.find_query("user").is_some());
    assert!(schema.find_query("adminUsers").is_some());
    assert!(schema.find_query("nonexistent").is_none());
}

#[test]
fn schema_builder_multiple_mutations_all_present() {
    let schema = TestSchemaBuilder::new()
        .with_simple_mutation("createUser", "User")
        .with_simple_mutation("deleteUser", "DeleteResult")
        .build();
    assert_eq!(schema.mutations.len(), 2);
}

#[test]
fn schema_builder_with_query_overrides_default_source() {
    let q = TestQueryBuilder::new("items", "Item").with_sql_source("v_custom_items").build();
    let schema = TestSchemaBuilder::new().with_query(q).build();
    assert_eq!(schema.queries[0].sql_source.as_deref(), Some("v_custom_items"));
}

// ── MockDb — edge cases not covered inline ────────────────────────────────────

#[tokio::test]
async fn mock_db_default_is_same_as_new() {
    let db: MockDb = MockDb::default();
    assert!(db.keys().await.is_empty());
}

#[tokio::test]
async fn mock_db_clone_shares_state() {
    let db = MockDb::new();
    let db_clone = db.clone();

    db.insert("shared_key".to_string(), json!({"value": 42})).await;

    // Clone should see the insert
    assert!(db_clone.exists("shared_key").await);
    let v = db_clone.get("shared_key").await.unwrap();
    assert_eq!(v["value"], 42);
}

#[tokio::test]
async fn mock_db_insert_overwrites_existing_key() {
    let db = MockDb::new();
    db.insert("key".to_string(), json!("first")).await;
    db.insert("key".to_string(), json!("second")).await;

    let v = db.get("key").await.unwrap();
    assert_eq!(v, json!("second"));
}

#[tokio::test]
async fn mock_db_exists_returns_false_before_insert() {
    let db = MockDb::new();
    assert!(!db.exists("never_inserted").await);
}

#[tokio::test]
async fn mock_db_exists_returns_false_after_clear() {
    let db = MockDb::new();
    db.insert("k".to_string(), json!(1)).await;
    db.clear().await;
    assert!(!db.exists("k").await);
}

#[tokio::test]
async fn mock_db_keys_empty_after_clear() {
    let db = MockDb::new();
    db.insert("a".to_string(), json!(1)).await;
    db.insert("b".to_string(), json!(2)).await;
    db.clear().await;
    assert!(db.keys().await.is_empty());
}

// ── MockDbError Display ───────────────────────────────────────────────────────

#[test]
fn mock_db_error_query_error_display() {
    let e = MockDbError::QueryError("syntax error".into());
    assert!(e.to_string().contains("syntax error"));
}

#[test]
fn mock_db_error_connection_error_display() {
    let e = MockDbError::ConnectionError("refused".into());
    assert!(e.to_string().contains("refused"));
}

#[test]
fn mock_db_error_not_found_display() {
    let e = MockDbError::NotFound;
    assert!(!e.to_string().is_empty());
}

#[test]
fn mock_db_error_implements_std_error() {
    let e = MockDbError::NotFound;
    let _: &dyn std::error::Error = &e;
}

// ── Assertion helpers — public API from external crate ────────────────────────

#[test]
fn assert_no_graphql_errors_passes_when_no_errors_field() {
    assert_no_graphql_errors(&json!({"data": {"user": {"id": 1}}}));
}

#[test]
fn assert_no_graphql_errors_passes_when_empty_errors_array() {
    assert_no_graphql_errors(&json!({"data": {}, "errors": []}));
}

#[test]
fn assert_graphql_success_passes_on_clean_response() {
    assert_graphql_success(&json!({"data": {"user": {"id": 1}}}));
}

#[test]
fn assert_has_data_returns_data_field() {
    let response = json!({"data": {"id": 42}});
    let data = assert_has_data(&response);
    assert_eq!(data["id"], 42);
}

#[test]
fn assert_graphql_error_contains_finds_message() {
    let response = sample_error_response("something went wrong");
    assert_graphql_error_contains(&response, "something went wrong");
}

#[test]
fn assert_graphql_error_code_finds_code() {
    let response = sample_error_response("oops");
    assert_graphql_error_code(&response, "INTERNAL_ERROR");
}

#[test]
fn assert_field_path_navigates_nested_json() {
    let response = sample_query_response();
    assert_field_path(&response, "data.user.name", &json!("John Doe"));
}

// ── Fixtures ──────────────────────────────────────────────────────────────────

#[test]
fn sample_user_has_expected_id() {
    let user = sample_user();
    assert_eq!(user["id"], "user_123");
}

#[test]
fn sample_user_has_created_at() {
    let user = sample_user();
    assert!(user["created_at"].is_string());
}

#[test]
fn sample_query_response_has_data_user_email() {
    let response = sample_query_response();
    assert_eq!(response["data"]["user"]["email"], "john@example.com");
}

#[test]
fn sample_error_response_has_errors_array() {
    let response = sample_error_response("test error");
    let errors = response["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
}

// ── Utilities ─────────────────────────────────────────────────────────────────

#[test]
fn get_test_id_returns_unique_strings() {
    let id1 = get_test_id();
    let id2 = get_test_id();
    assert_ne!(id1, id2);
}

#[test]
fn get_test_id_is_valid_uuid_format() {
    let id = get_test_id();
    // UUID v4 has 36 chars: xxxxxxxx-xxxx-4xxx-xxxx-xxxxxxxxxxxx
    assert_eq!(id.len(), 36);
    assert!(id.contains('-'));
}

#[test]
fn setup_test_env_is_idempotent() {
    setup_test_env();
    setup_test_env(); // should not panic
}

#[test]
fn create_temp_dir_returns_existing_directory() {
    let dir = create_temp_dir();
    assert!(dir.path().exists());
    assert!(dir.path().is_dir());
}

#[test]
fn create_temp_dir_returns_unique_paths() {
    let d1 = create_temp_dir();
    let d2 = create_temp_dir();
    assert_ne!(d1.path(), d2.path());
}
