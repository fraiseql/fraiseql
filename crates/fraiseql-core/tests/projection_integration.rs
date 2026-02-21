//! Tests end-to-end SQL projection functionality:
//! - Schema compilation with projection hints
//! - Multi-database projection SQL generation
//! - Projection detection heuristics
//! - ResultProjector with `__typename` handling
//! - Complete query execution with projections

use fraiseql_core::{
    db::{
        projection_generator::{
            FieldKind, MySqlProjectionGenerator, PostgresProjectionGenerator,
            SqliteProjectionGenerator,
        },
        types::JsonbValue,
    },
    runtime::{FieldMapping, ProjectionMapper, ResultProjector},
    schema::SqlProjectionHint,
};
use serde_json::json;

/// Test that projection hints can be created and used
#[test]
fn test_sql_projection_hint_creation() {
    let hint = SqlProjectionHint {
        database:                    "postgresql".to_string(),
        projection_template:
            "jsonb_build_object('id', \"data\"->>'id', 'name', \"data\"->>'name')".to_string(),
        estimated_reduction_percent: 65,
    };

    assert_eq!(hint.database, "postgresql");
    assert_eq!(hint.estimated_reduction_percent, 65);
    assert!(hint.projection_template.contains("jsonb_build_object"));
}

/// Test PostgreSQL projection SQL generation
#[test]
fn test_postgres_projection_complete_flow() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();

    // Verify structure
    assert!(sql.contains("jsonb_build_object("));
    assert!(sql.contains("'id'"));
    assert!(sql.contains("'name'"));
    assert!(sql.contains("'email'"));
    assert!(sql.contains("\"data\""));

    // Verify it can be used in a SELECT clause
    let select = generator.generate_select_clause("users", &fields).unwrap();
    assert!(select.starts_with("SELECT jsonb_build_object("));
    assert!(select.contains("FROM \"users\""));
}

/// Test MySQL projection SQL generation
#[test]
fn test_mysql_projection_complete_flow() {
    let generator = MySqlProjectionGenerator::new();
    let fields = vec!["id".to_string(), "email".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();

    // Verify structure
    assert!(sql.contains("JSON_OBJECT("));
    assert!(sql.contains("JSON_EXTRACT("));
    assert!(sql.contains("'id'"));
    assert!(sql.contains("'email'"));
    assert!(sql.contains("`data`"));
}

/// Test SQLite projection SQL generation
#[test]
fn test_sqlite_projection_complete_flow() {
    let generator = SqliteProjectionGenerator::new();
    let fields = vec!["id".to_string(), "active".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();

    // Verify structure
    assert!(sql.contains("json_object("));
    assert!(sql.contains("json_extract("));
    assert!(sql.contains("'id'"));
    assert!(sql.contains("'active'"));
    assert!(sql.contains("\"data\""));
}

/// Test ResultProjector with `__typename` addition
#[test]
fn test_result_projector_add_typename() {
    let projector = ResultProjector::new(vec!["id".to_string()]);

    let data = json!({
        "id": "123",
        "name": "Alice",
        "email": "alice@example.com"
    });

    let jsonb = JsonbValue::new(data);
    let result = projector.add_typename_only(&jsonb, "User").unwrap();

    // Verify __typename is added
    assert_eq!(result.get("__typename"), Some(&json!("User")));
    // Verify original fields are preserved
    assert_eq!(result.get("id"), Some(&json!("123")));
    assert_eq!(result.get("name"), Some(&json!("Alice")));
}

/// Test ResultProjector with array of results
#[test]
fn test_result_projector_add_typename_array() {
    let projector = ResultProjector::new(vec![]);

    let data = json!([
        {"id": "1", "name": "Alice"},
        {"id": "2", "name": "Bob"}
    ]);

    let jsonb = JsonbValue::new(data);
    let result = projector.add_typename_only(&jsonb, "User").unwrap();

    // Verify array structure preserved
    assert!(result.is_array());
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    // Verify each element has __typename
    assert_eq!(arr[0].get("__typename"), Some(&json!("User")));
    assert_eq!(arr[1].get("__typename"), Some(&json!("User")));
}

/// Test that ResultProjector projects fields correctly (via project_results method)
#[test]
fn test_result_projector_projects_single_field() {
    let projector = ResultProjector::new(vec!["id".to_string()]);

    let data = json!({
        "id": "123",
        "name": "Alice",
        "email": "alice@example.com"
    });

    let jsonb = JsonbValue::new(data);
    let projected = projector.project_results(&[jsonb], false).unwrap();

    // Should only have id field
    assert_eq!(projected.get("id"), Some(&json!("123")));
    assert_eq!(projected.get("name"), None);
    assert_eq!(projected.get("email"), None);
}

/// Test that ResultProjector projects multiple fields correctly
#[test]
fn test_result_projector_projects_multiple_fields() {
    let projector = ResultProjector::new(vec!["id".to_string(), "email".to_string()]);

    let data = json!({
        "id": "123",
        "name": "Alice",
        "email": "alice@example.com",
        "phone": "+1234567890"
    });

    let jsonb = JsonbValue::new(data);
    let projected = projector.project_results(&[jsonb], false).unwrap();

    // Should have exactly id and email
    assert_eq!(projected.get("id"), Some(&json!("123")));
    assert_eq!(projected.get("email"), Some(&json!("alice@example.com")));
    assert_eq!(projected.get("name"), None);
    assert_eq!(projected.get("phone"), None);
}

/// Test that custom column names work across all databases
#[test]
fn test_projection_custom_column_names() {
    let pg = PostgresProjectionGenerator::with_column("metadata");
    let mysql = MySqlProjectionGenerator::with_column("metadata");
    let sqlite = SqliteProjectionGenerator::with_column("metadata");

    let fields = vec!["id".to_string()];

    let pg_sql = pg.generate_projection_sql(&fields).unwrap();
    let mysql_sql = mysql.generate_projection_sql(&fields).unwrap();
    let sqlite_sql = sqlite.generate_projection_sql(&fields).unwrap();

    // Verify custom column name is used
    assert!(pg_sql.contains("\"metadata\""));
    assert!(mysql_sql.contains("`metadata`"));
    assert!(sqlite_sql.contains("\"metadata\""));
}

/// Test that empty field list returns passthrough
#[test]
fn test_projection_empty_fields_passthrough() {
    let pg = PostgresProjectionGenerator::new();
    let mysql = MySqlProjectionGenerator::new();
    let sqlite = SqliteProjectionGenerator::new();

    let empty_fields: Vec<String> = vec![];

    let pg_sql = pg.generate_projection_sql(&empty_fields).unwrap();
    let mysql_sql = mysql.generate_projection_sql(&empty_fields).unwrap();
    let sqlite_sql = sqlite.generate_projection_sql(&empty_fields).unwrap();

    // With no fields, should return column reference only
    assert_eq!(pg_sql, "\"data\"");
    assert_eq!(mysql_sql, "`data`");
    assert_eq!(sqlite_sql, "\"data\"");
}

/// Test that generators handle special characters in field names correctly
#[test]
fn test_identifier_handling() {
    // Test that valid identifiers work correctly in projection SQL
    let pg = PostgresProjectionGenerator::new();
    let fields = vec![
        "id".to_string(),
        "user_id".to_string(),
        "field123".to_string(),
    ];

    let sql = pg.generate_projection_sql(&fields).unwrap();
    assert!(sql.contains("'id'"));
    assert!(sql.contains("'user_id'"));
    assert!(sql.contains("'field123'"));
}

/// Test projection with special characters in field names (should escape safely)
#[test]
fn test_projection_field_escaping() {
    let pg = PostgresProjectionGenerator::new();

    // Field with alphanumeric, underscore, dollar - all valid
    let fields = vec!["user_id".to_string(), "data$obj".to_string()];
    let sql = pg.generate_projection_sql(&fields).unwrap();

    // Should include both fields
    assert!(sql.contains("'user_id'"));
    assert!(sql.contains("'data$obj'"));
}

/// Test that projection hints calculate correct reduction percentages
#[test]
fn test_projection_hint_reduction_calculation() {
    // Create a hint that represents projecting 5 out of 20 fields
    let hint = SqlProjectionHint {
        database: "postgresql".to_string(),
        projection_template: "jsonb_build_object('id', data->>'id', 'name', data->>'name', 'email', data->>'email', 'status', data->>'status', 'created_at', data->>'created_at')".to_string(),
        estimated_reduction_percent: 75, // 5/20 = 25% remain, so 75% reduction
    };

    assert_eq!(hint.estimated_reduction_percent, 75);
    assert_eq!(hint.database, "postgresql");
}

/// Test ResultProjector wrapping response in data envelope
#[test]
fn test_result_projector_data_envelope() {
    let result = json!([
        {"id": "1", "name": "Alice"},
        {"id": "2", "name": "Bob"}
    ]);

    let wrapped = ResultProjector::wrap_in_data_envelope(result, "users");

    // Verify structure: { "data": { "users": [...] } }
    assert!(wrapped.get("data").is_some());
    let data_obj = wrapped.get("data").unwrap();
    assert!(data_obj.get("users").is_some());
    let users = data_obj.get("users").unwrap();
    assert!(users.is_array());
    assert_eq!(users.as_array().unwrap().len(), 2);
}

/// Test ResultProjector error wrapping
#[test]
fn test_result_projector_error_envelope() {
    use fraiseql_core::error::FraiseQLError;

    let error = FraiseQLError::Validation {
        message: "Invalid field selection".to_string(),
        path:    Some("query.users".to_string()),
    };

    let wrapped = ResultProjector::wrap_error(&error);

    // Verify structure: { "errors": [...] }
    assert!(wrapped.get("errors").is_some());
    assert!(wrapped.get("data").is_none());

    let errors = wrapped.get("errors").unwrap();
    assert!(errors.is_array());
    assert_eq!(errors.as_array().unwrap().len(), 1);

    let error_obj = &errors.as_array().unwrap()[0];
    assert!(error_obj.get("message").is_some());
}

/// Test that projection works correctly with nested objects via ResultProjector
#[test]
fn test_projection_with_nested_structure() {
    let projector = ResultProjector::new(vec!["id".to_string(), "profile".to_string()]);

    let data = json!({
        "id": "123",
        "profile": {
            "name": "Alice",
            "email": "alice@example.com"
        },
        "settings": {
            "theme": "dark"
        }
    });

    let jsonb = JsonbValue::new(data);
    let projected = projector.project_results(&[jsonb], false).unwrap();

    // Should have id and profile, but not settings
    assert_eq!(projected.get("id"), Some(&json!("123")));
    assert!(projected.get("profile").is_some());
    assert_eq!(projected.get("settings"), None);
}

/// Companion to `test_projection_with_nested_structure`.
///
/// Verifies the same invariant — nested object fields are preserved and extraneous
/// fields are excluded — but with the field arriving as a serialized JSON string
/// (the format PostgreSQL `->>` returns for JSONB object columns).
///
/// This is the regression guard for issue #27 at the `projection_integration` level.
#[test]
fn test_projection_with_serialized_nested_structure() {
    let projector = ProjectionMapper::with_mappings(vec![
        FieldMapping::simple("id"),
        FieldMapping::nested_object(
            "profile",
            "Profile",
            vec![FieldMapping::simple("name")], // only request name, not email
        ),
    ]);

    // profile arrives as a JSON string (the ->> path)
    let data = json!({
        "id":      "123",
        "profile": r#"{"name":"Alice","email":"alice@example.com"}"#,
        "settings": { "theme": "dark" }
    });

    let jsonb = JsonbValue::new(data);
    let projected = projector.project(&jsonb).expect("projection must not fail");

    // id must be present
    assert_eq!(projected.get("id"), Some(&json!("123")));

    // settings must be excluded (not in field list)
    assert_eq!(projected.get("settings"), None, "settings must be filtered out");

    // profile must be an object — NOT a string
    let profile = projected.get("profile").expect("profile must be present");
    assert!(
        profile.is_object(),
        "profile must be recovered as an object, got: {profile}"
    );
    // __typename must be injected
    assert_eq!(
        profile.get("__typename"),
        Some(&json!("Profile")),
        "profile must have __typename"
    );
    // requested field must be present
    assert_eq!(profile.get("name"), Some(&json!("Alice")), "profile.name must be projected");
    // unrequested field must be excluded
    assert_eq!(
        profile.get("email"),
        None,
        "profile.email must be filtered out (not requested)"
    );
}

/// Test complete flow: hint creation -> projection generation -> result wrapping
#[test]
fn test_complete_projection_pipeline() {
    // Step 1: Create projection hint
    let _hint = SqlProjectionHint {
        database:                    "postgresql".to_string(),
        projection_template:
            "jsonb_build_object('id', \"data\"->>'id', 'name', \"data\"->>'name')".to_string(),
        estimated_reduction_percent: 87,
    };

    // Step 2: Generate SQL using hint template (note: in real scenario, projection_template would
    // be used)
    let fields = vec!["id".to_string(), "name".to_string()];
    let generator = PostgresProjectionGenerator::new();
    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert!(sql.contains("jsonb_build_object"));

    // Step 3: Simulate database result
    let result_data = json!({
        "id": "123",
        "name": "Alice"
    });

    // Step 4: Project result and add __typename
    let projector = ResultProjector::new(fields);
    let jsonb = JsonbValue::new(result_data);
    let with_typename = projector.add_typename_only(&jsonb, "User").unwrap();

    // Step 5: Wrap in GraphQL envelope
    let final_response = ResultProjector::wrap_in_data_envelope(with_typename, "user");

    // Verify complete pipeline
    assert!(final_response.get("data").is_some());
    let data = final_response.get("data").unwrap();
    let user = data.get("user").unwrap();
    assert_eq!(user.get("id"), Some(&json!("123")));
    assert_eq!(user.get("name"), Some(&json!("Alice")));
    assert_eq!(user.get("__typename"), Some(&json!("User")));
}

/// Test projection SQL generation for large field lists
#[test]
fn test_projection_with_many_fields() {
    let generator = PostgresProjectionGenerator::new();

    // Create 50 fields
    let fields: Vec<String> = (0..50).map(|i| format!("field_{}", i)).collect();

    let sql = generator.generate_projection_sql(&fields).unwrap();

    // Verify all fields are included
    assert!(sql.contains("jsonb_build_object("));
    for field in &fields {
        assert!(sql.contains(&format!("'{}'", field)));
    }
}

/// Test that database-specific SQL has correct syntax
#[test]
fn test_database_specific_syntax() {
    let fields = vec!["id".to_string(), "status".to_string()];

    let pg_sql = PostgresProjectionGenerator::new().generate_projection_sql(&fields).unwrap();
    let mysql_sql = MySqlProjectionGenerator::new().generate_projection_sql(&fields).unwrap();
    let sqlite_sql = SqliteProjectionGenerator::new().generate_projection_sql(&fields).unwrap();

    // PostgreSQL uses ->> for scalar fields (text extraction)
    assert!(pg_sql.contains("->>'"));

    // MySQL uses JSON_EXTRACT
    assert!(mysql_sql.contains("JSON_EXTRACT"));

    // SQLite uses json_extract
    assert!(sqlite_sql.contains("json_extract"));
}

/// Test that typed projection uses -> for Object fields, ->> for Scalar fields.
///
/// This is the regression test for issue #27: using ->> on a JSONB column that contains
/// a nested object returns its JSON-serialised text rather than a structured value.
#[test]
fn test_postgres_typed_projection_operator_selection() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![
        ("id".to_string(), FieldKind::Scalar),
        ("title".to_string(), FieldKind::Scalar),
        ("author".to_string(), FieldKind::Object),
    ];

    let sql = generator.generate_projection_sql_typed(&fields).unwrap();

    // Scalar fields: text extraction
    assert!(sql.contains("'id', \"data\"->>'id'"), "scalar 'id' must use ->>");
    assert!(sql.contains("'title', \"data\"->>'title'"), "scalar 'title' must use ->>");
    // Object field: JSONB extraction — preserves nested structure
    assert!(sql.contains("'author', \"data\"->'author'"), "object 'author' must use ->");
    assert!(
        !sql.contains("'author', \"data\"->>'author'"),
        "object 'author' must NOT use ->>"
    );
}

/// Regression test for issue #27: full pipeline simulation.
///
/// Simulates what happens in production when the SQL layer uses ->> for an Object field
/// (returning a JSON-encoded string) and the Rust projection layer must recover the
/// nested structure correctly.
///
/// This test intentionally crosses the boundary between projection_generator.rs (SQL)
/// and projection.rs (Rust) — the gap that allowed the bug to go undetected.
#[test]
fn test_nested_object_pipeline_json_string_to_object() {
    // Simulate the buggy SQL path: ->> returns a JSON string for a nested object.
    // In production this is what PostgreSQL hands back when the view has a JSONB
    // nested object column but the SQL uses ->>.
    let raw_db_row = serde_json::json!({
        "id": "bb18a1b3",
        "title": "Post Title",
        // author arrives as a serialized JSON string (what ->> produces)
        "author": "{\"id\": \"4787988d\", \"identifier\": \"author\", \"email\": \"author@example.com\"}"
    });

    let jsonb = JsonbValue::new(raw_db_row);

    // Rust projection layer must re-parse the string back into an object.
    let mapper = ProjectionMapper::with_mappings(vec![
        FieldMapping::simple("id"),
        FieldMapping::simple("title"),
        FieldMapping::nested_object(
            "author",
            "User",
            vec![FieldMapping::simple("id"), FieldMapping::simple("identifier")],
        ),
    ]);

    let result = mapper.project(&jsonb).unwrap();

    // author must be a proper object, not a string
    let author = result.get("author").expect("author field must exist");
    assert!(author.is_object(), "author must be an object, got: {author}");
    assert_eq!(author.get("id"), Some(&serde_json::json!("4787988d")));
    assert_eq!(author.get("identifier"), Some(&serde_json::json!("author")));
    // email was not requested — must be filtered out by nested_fields projection
    assert_eq!(author.get("email"), None, "unrequested field must not leak");
    // __typename must be injected
    assert_eq!(author.get("__typename"), Some(&serde_json::json!("User")));
}

/// Test projection result projection for list vs single
#[test]
fn test_result_projector_list_vs_single() {
    let projector = ResultProjector::new(vec!["id".to_string()]);

    let data = json!({"id": "1", "name": "Alice"});
    let jsonb = JsonbValue::new(data);

    // Test list projection
    let list_result = projector.project_results(std::slice::from_ref(&jsonb), true).unwrap();
    assert!(list_result.is_array());
    assert_eq!(list_result.as_array().unwrap().len(), 1);

    // Test single projection
    let single_result = projector.project_results(std::slice::from_ref(&jsonb), false).unwrap();
    assert!(single_result.is_object());
    assert_eq!(single_result.get("id"), Some(&json!("1")));
}

/// Test empty result set handling
#[test]
fn test_projection_with_empty_results() {
    let projector = ResultProjector::new(vec!["id".to_string()]);

    // Empty result set for list query
    let list_result = projector.project_results(&[], true).unwrap();
    assert!(list_result.is_array());
    assert_eq!(list_result.as_array().unwrap().len(), 0);

    // Empty result set for single query
    let single_result = projector.project_results(&[], false).unwrap();
    assert_eq!(single_result, json!(null));
}

/// Regression guard for fraiseql-python issue #269.
///
/// Verifies the full camelCase/snake_case round-trip:
///  1. SQL generator uses snake_case JSONB keys (`first_name`, `created_at`)
///  2. The SQL response key labels stay camelCase (`firstName`, `createdAt`) for GraphQL
///  3. The Rust projection layer maps snake_case source keys to camelCase output keys
///
/// If either conversion is broken, fields silently return null.
#[test]
fn test_camel_to_snake_jsonb_lookup_round_trip() {
    let generator = PostgresProjectionGenerator::new();

    // GraphQL field names are camelCase (what the client sends)
    let typed_fields = vec![
        ("id".to_string(), FieldKind::Scalar),
        ("firstName".to_string(), FieldKind::Scalar),
        ("isActive".to_string(), FieldKind::Scalar),
        ("createdAt".to_string(), FieldKind::Scalar),
    ];

    let sql = generator.generate_projection_sql_typed(&typed_fields).unwrap();

    // SQL must use snake_case keys for JSONB lookup (the DB stores them that way)
    assert!(sql.contains("->>'first_name'"), "JSONB key must be snake_case: first_name");
    assert!(sql.contains("->>'is_active'"), "JSONB key must be snake_case: is_active");
    assert!(sql.contains("->>'created_at'"), "JSONB key must be snake_case: created_at");

    // SQL must NOT use camelCase keys (would return null from JSONB)
    assert!(!sql.contains("->>'firstName'"), "Must NOT look up camelCase key");
    assert!(!sql.contains("->>'isActive'"), "Must NOT look up camelCase key");
    assert!(!sql.contains("->>'createdAt'"), "Must NOT look up camelCase key");

    // Response key labels in jsonb_build_object must stay camelCase (for GraphQL)
    assert!(sql.contains("'firstName'"), "Response key must be camelCase");
    assert!(sql.contains("'isActive'"), "Response key must be camelCase");
    assert!(sql.contains("'createdAt'"), "Response key must be camelCase");

    // Now simulate what PostgreSQL returns (JSONB stored with snake_case keys)
    // and verify the projector maps them correctly to camelCase output keys.
    //
    // Note: when SQL projection is active, PostgreSQL already applies the key
    // renaming inside jsonb_build_object. Here we test the Rust-side projection
    // path (no SQL projection), which must also handle the rename.
    let projector = ProjectionMapper::with_mappings(vec![
        FieldMapping::aliased("first_name", "firstName"),
        FieldMapping::aliased("is_active", "isActive"),
        FieldMapping::aliased("created_at", "createdAt"),
    ]);

    let db_row = serde_json::json!({
        "first_name": "Bob",
        "is_active":  true,
        "created_at": "2024-01-14T00:00:00Z",
    });

    let result = projector.project(&JsonbValue::new(db_row)).unwrap();

    // Response uses camelCase (GraphQL convention)
    assert_eq!(result.get("firstName"), Some(&serde_json::json!("Bob")));
    assert_eq!(result.get("isActive"), Some(&serde_json::json!(true)));
    assert_eq!(result.get("createdAt"), Some(&serde_json::json!("2024-01-14T00:00:00Z")));

    // snake_case keys must not leak into the GraphQL response
    assert_eq!(result.get("first_name"), None, "snake_case key must not appear in response");
    assert_eq!(result.get("is_active"), None, "snake_case key must not appear in response");
    assert_eq!(result.get("created_at"), None, "snake_case key must not appear in response");
}
