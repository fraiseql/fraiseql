#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_postgres_projection_single_field() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec!["id".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert_eq!(sql, "jsonb_build_object('id', \"data\"->>'id' )");
}

#[test]
fn test_postgres_projection_multiple_fields() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert!(sql.contains("jsonb_build_object("));
    assert!(sql.contains("'id', \"data\"->>'id'"));
    assert!(sql.contains("'name', \"data\"->>'name'"));
    assert!(sql.contains("'email', \"data\"->>'email'"));
}

#[test]
fn test_postgres_projection_empty_fields() {
    let generator = PostgresProjectionGenerator::new();
    let fields: Vec<String> = vec![];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    // Empty projection should pass through the JSONB column
    assert_eq!(sql, "\"data\"");
}

#[test]
fn test_postgres_projection_custom_column() {
    let generator = PostgresProjectionGenerator::with_column("metadata");
    let fields = vec!["id".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert_eq!(sql, "jsonb_build_object('id', \"metadata\"->>'id' )");
}

#[test]
fn test_postgres_select_clause() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec!["id".to_string(), "name".to_string()];

    let sql = generator.generate_select_clause("users", &fields).unwrap();
    assert!(sql.starts_with("SELECT jsonb_build_object("));
    assert!(sql.contains("as \"data\""));
    assert!(sql.contains("FROM \"users\""));
}

#[test]
fn test_escape_identifier_quoting() {
    // Simple identifiers are wrapped in double-quotes.
    assert_eq!(PostgresProjectionGenerator::escape_identifier("id"), "\"id\"");
    assert_eq!(PostgresProjectionGenerator::escape_identifier("user_id"), "\"user_id\"");
    // Special chars (hyphens, dots) are safe inside quotes.
    assert_eq!(PostgresProjectionGenerator::escape_identifier("field-name"), "\"field-name\"");
    assert_eq!(PostgresProjectionGenerator::escape_identifier("field.name"), "\"field.name\"");
    // Double-quote chars inside the name are doubled.
    assert_eq!(
        PostgresProjectionGenerator::escape_identifier("col\"inject"),
        "\"col\"\"inject\""
    );
}

// MySQL Projection Generator Tests
#[test]
fn test_mysql_projection_single_field() {
    let generator = MySqlProjectionGenerator::new();
    let fields = vec!["id".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert_eq!(sql, "JSON_OBJECT('id', JSON_EXTRACT(`data`, '$.id'))");
}

#[test]
fn test_mysql_projection_multiple_fields() {
    let generator = MySqlProjectionGenerator::new();
    let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert!(sql.contains("JSON_OBJECT("));
    assert!(sql.contains("'id', JSON_EXTRACT(`data`, '$.id')"));
    assert!(sql.contains("'name', JSON_EXTRACT(`data`, '$.name')"));
    assert!(sql.contains("'email', JSON_EXTRACT(`data`, '$.email')"));
}

#[test]
fn test_mysql_projection_empty_fields() {
    let generator = MySqlProjectionGenerator::new();
    let fields: Vec<String> = vec![];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert_eq!(sql, "`data`");
}

#[test]
fn test_mysql_projection_custom_column() {
    let generator = MySqlProjectionGenerator::with_column("metadata");
    let fields = vec!["id".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert_eq!(sql, "JSON_OBJECT('id', JSON_EXTRACT(`metadata`, '$.id'))");
}

// SQLite Projection Generator Tests
#[test]
fn test_sqlite_projection_single_field() {
    let generator = SqliteProjectionGenerator::new();
    let fields = vec!["id".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert_eq!(sql, "json_object('id', json_extract(\"data\", '$.id'))");
}

#[test]
fn test_sqlite_projection_multiple_fields() {
    let generator = SqliteProjectionGenerator::new();
    let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert!(sql.contains("json_object("));
    assert!(sql.contains("'id', json_extract(\"data\", '$.id')"));
    assert!(sql.contains("'name', json_extract(\"data\", '$.name')"));
    assert!(sql.contains("'email', json_extract(\"data\", '$.email')"));
}

#[test]
fn test_sqlite_projection_empty_fields() {
    let generator = SqliteProjectionGenerator::new();
    let fields: Vec<String> = vec![];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert_eq!(sql, "\"data\"");
}

#[test]
fn test_sqlite_projection_custom_column() {
    let generator = SqliteProjectionGenerator::with_column("metadata");
    let fields = vec!["id".to_string()];

    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert_eq!(sql, "json_object('id', json_extract(\"metadata\", '$.id'))");
}

// ========================================================================
// Issue #269: JSONB field extraction with snake_case/camelCase mapping
// ========================================================================

#[test]
fn test_to_snake_case_conversion() {
    // Test camelCase to snake_case conversion
    assert_eq!(super::to_snake_case("id"), "id");
    assert_eq!(super::to_snake_case("firstName"), "first_name");
    assert_eq!(super::to_snake_case("createdAt"), "created_at");
    assert_eq!(super::to_snake_case("userId"), "user_id");
    assert_eq!(super::to_snake_case("updatedAtTimestamp"), "updated_at_timestamp");
}

#[test]
fn test_postgres_projection_with_field_mapping_snake_case() {
    // Problem: GraphQL converts field names to camelCase (first_name → firstName)
    // But JSONB stores them in snake_case (first_name).
    // When generating JSONB extraction SQL, we must use the original snake_case key,
    // not the camelCase GraphQL name.

    let generator = PostgresProjectionGenerator::new();

    // Simulate what happens when fields come from GraphQL query
    // These are camelCase field names (what GraphQL expects in response)
    let graphql_fields = vec![
        "id".to_string(),
        "firstName".to_string(),
        "createdAt".to_string(),
    ];

    let sql = generator.generate_projection_sql(&graphql_fields).unwrap();

    eprintln!("Generated SQL: {}", sql);

    // Current broken behavior generates:
    // jsonb_build_object('id', data->>'id', 'firstName', data->>'firstName', 'createdAt',
    // data->>'createdAt')
    //
    // This fails because JSONB has snake_case keys: first_name, created_at
    // Result: data->>'firstName' returns NULL (key not found)

    // Regression guard: SQL must use snake_case keys for JSONB access.
    // camelCase field names in the schema (firstName, createdAt) must be
    // mapped to snake_case in generated SQL (first_name, created_at) because
    // PostgreSQL stores JSONB keys verbatim and FraiseQL always writes snake_case.
    assert!(
        !sql.contains("->>'firstName'") && !sql.contains("->>'createdAt'"),
        "Regression: SQL is using camelCase keys for JSONB access. \
         JSONB has snake_case keys ('first_name', 'created_at'). SQL: {}",
        sql
    );
}

// =========================================================================
// Additional projection_generator.rs tests
// =========================================================================

#[test]
fn test_postgres_projection_sql_injection_in_field_name() {
    // A field name containing a single quote is rejected by the validator — it is
    // not a valid GraphQL / FraiseQL field identifier and must never reach SQL.
    let generator = PostgresProjectionGenerator::new();
    let fields = vec!["user'name".to_string()];
    let result = generator.generate_projection_sql(&fields);
    assert!(result.is_err(), "Field name with single quote must be rejected");
}

#[test]
fn test_postgres_projection_rejects_field_with_semicolon() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec!["id; DROP TABLE users--".to_string()];
    let result = generator.generate_projection_sql(&fields);
    assert!(result.is_err(), "Field name with SQL injection characters must be rejected");
}

#[test]
fn test_mysql_projection_rejects_unsafe_field_name() {
    let generator = MySqlProjectionGenerator::new();
    let fields = vec!["field`hack".to_string()];
    let result = generator.generate_projection_sql(&fields);
    assert!(result.is_err(), "Field name with backtick must be rejected");
}

#[test]
fn test_sqlite_projection_rejects_unsafe_field_name() {
    let generator = SqliteProjectionGenerator::new();
    let fields = vec!["field\"inject".to_string()];
    let result = generator.generate_projection_sql(&fields);
    assert!(result.is_err(), "Field name with double-quote must be rejected");
}

#[test]
fn test_validate_field_name_accepts_valid_names() {
    assert!(super::validate_field_name("id").is_ok());
    assert!(super::validate_field_name("user_id").is_ok());
    assert!(super::validate_field_name("firstName").is_ok());
    assert!(super::validate_field_name("createdAt").is_ok());
    assert!(super::validate_field_name("field123").is_ok());
    assert!(super::validate_field_name("_private").is_ok());
}

#[test]
fn test_validate_field_name_rejects_unsafe_chars() {
    assert!(super::validate_field_name("user'name").is_err());
    assert!(super::validate_field_name("field-name").is_err());
    assert!(super::validate_field_name("field.name").is_err());
    assert!(super::validate_field_name("field;inject").is_err());
    assert!(super::validate_field_name("field\"inject").is_err());
    assert!(super::validate_field_name("field`hack").is_err());
}

#[test]
fn test_mysql_projection_sql_contains_json_object() {
    let generator = MySqlProjectionGenerator::new();
    let fields = vec!["email".to_string(), "name".to_string()];
    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert!(sql.starts_with("JSON_OBJECT("), "MySQL projection must start with JSON_OBJECT");
}

#[test]
fn test_sqlite_projection_custom_column_appears_in_sql() {
    let generator = SqliteProjectionGenerator::with_column("payload");
    let fields = vec!["id".to_string()];
    let sql = generator.generate_projection_sql(&fields).unwrap();
    assert!(sql.contains("\"payload\""), "Custom column name must appear in SQLite SQL");
}

#[test]
fn test_postgres_projection_camel_to_snake_in_jsonb_key() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec!["updatedAt".to_string()];
    let sql = generator.generate_projection_sql(&fields).unwrap();
    // The JSONB extraction key should be snake_case
    assert!(
        sql.contains("'updated_at'"),
        "updatedAt must be mapped to updated_at for JSONB key"
    );
    // The response key in jsonb_build_object should be the original camelCase
    assert!(sql.contains("'updatedAt'"), "Response key must remain camelCase");
}

#[test]
fn test_postgres_select_clause_contains_from() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec!["id".to_string()];
    let sql = generator.generate_select_clause("orders", &fields).unwrap();
    assert!(
        sql.contains("FROM \"orders\""),
        "SELECT clause must include FROM clause with table name"
    );
    assert!(sql.contains("SELECT"), "SELECT clause must start with SELECT");
}

// ── generate_typed_projection_sql tests (C12) ─────────────────────────

#[test]
fn test_typed_projection_empty_fields_returns_data_column() {
    let generator = PostgresProjectionGenerator::new();
    let result = generator.generate_typed_projection_sql(&[]).unwrap();
    assert_eq!(result, "\"data\"");
}

#[test]
fn test_typed_projection_text_field_uses_text_extraction() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![ProjectionField::scalar("name")];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    // Text fields use ->> (text extraction)
    assert!(sql.contains("->>'name'"), "text field must use ->> operator, got: {sql}");
    assert!(!sql.contains("->'name'"), "text field must NOT use -> operator, got: {sql}");
}

#[test]
fn test_typed_projection_composite_field_uses_jsonb_extraction() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![ProjectionField::composite("address")];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    // Composite fields with no sub_fields use -> (full JSONB blob)
    assert!(sql.contains("->'address'"), "composite field must use -> operator, got: {sql}");
}

#[test]
fn test_typed_projection_mixed_text_native_and_composite() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![
        ProjectionField::scalar("id"),
        ProjectionField::native("age"),
        ProjectionField::composite("address"),
        ProjectionField::composite("tags"),
        ProjectionField::scalar("email"),
    ];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();

    // Text scalars use ->>
    assert!(sql.contains("->>'id'"), "id (text) must use ->>, got: {sql}");
    assert!(sql.contains("->>'email'"), "email (text) must use ->>, got: {sql}");

    // Native scalars use ->
    assert!(sql.contains("->'age'"), "age (native) must use ->, got: {sql}");

    // Composites use ->
    assert!(sql.contains("->'address'"), "address (composite) must use ->, got: {sql}");
    assert!(sql.contains("->'tags'"), "tags (composite) must use ->, got: {sql}");

    // Must be wrapped in jsonb_build_object
    assert!(
        sql.starts_with("jsonb_build_object("),
        "must wrap in jsonb_build_object, got: {sql}"
    );
}

#[test]
fn test_typed_projection_camel_case_maps_to_snake_case_jsonb_key() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![ProjectionField::scalar("firstName")];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    // Response key is camelCase, JSONB key is snake_case
    assert!(
        sql.contains("'firstName'"),
        "response key must be camelCase 'firstName', got: {sql}"
    );
    assert!(
        sql.contains("->>'first_name'"),
        "JSONB key must be snake_case 'first_name', got: {sql}"
    );
}

#[test]
fn test_typed_projection_single_quote_in_field_name_escaped() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![ProjectionField::scalar("it's")];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    // Single quotes must be doubled for SQL safety
    assert!(
        sql.contains("'it''s'"),
        "single quote in field name must be escaped, got: {sql}"
    );
}

// ── Native field extraction tests (issue #197 / #202) ──────────────────────

#[test]
fn test_native_field_uses_jsonb_extraction() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![
        ProjectionField::native("isActive"),
        ProjectionField::scalar("name"),
    ];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    // Native: -> (not ->>) to preserve native JSON type (boolean, int, etc.)
    assert!(sql.contains("->'is_active'"), "native field must use -> operator, got: {sql}");
    // Text scalar still uses ->>
    assert!(sql.contains("->>'name'"), "text scalar field must use ->> operator, got: {sql}");
}

#[test]
fn test_native_field_mixed_with_composite() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![
        ProjectionField::native("isActive"),
        ProjectionField::composite("address"),
        ProjectionField::scalar("email"),
    ];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    assert!(sql.contains("->'is_active'"), "native uses ->, got: {sql}");
    assert!(sql.contains("->'address'"), "composite uses ->, got: {sql}");
    assert!(sql.contains("->>'email'"), "text scalar uses ->>, got: {sql}");
}

#[test]
fn test_native_int_and_float_use_jsonb_extraction() {
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![
        ProjectionField::native("age"),
        ProjectionField::native("price"),
        ProjectionField::scalar("name"),
        ProjectionField::scalar("id"),
    ];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    // Native scalars (Int, Float) use ->
    assert!(sql.contains("->'age'"), "int (native) must use ->, got: {sql}");
    assert!(sql.contains("->'price'"), "float (native) must use ->, got: {sql}");
    // Text scalars (String, ID) use ->>
    assert!(sql.contains("->>'name'"), "string (text) must use ->>, got: {sql}");
    assert!(sql.contains("->>'id'"), "id (text) must use ->>, got: {sql}");
}

// ── Deep nested projection tests (issue #189) ─────────────────────────────

#[test]
fn test_typed_projection_nested_sub_fields_generate_nested_jsonb_build_object() {
    let generator = PostgresProjectionGenerator::new();
    // Simulate: comments { id content author { id username fullName } }
    let fields = vec![
        ProjectionField::scalar("id"),
        ProjectionField::scalar("content"),
        ProjectionField::composite_with_sub_fields(
            "author",
            vec![
                ProjectionField::scalar("id"),
                ProjectionField::scalar("username"),
                ProjectionField::scalar("fullName"),
            ],
        ),
    ];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    // author must use a nested jsonb_build_object instead of the full blob
    assert!(
        sql.contains("'author', jsonb_build_object("),
        "author must produce nested jsonb_build_object, got: {sql}"
    );
    // Nested scalars must use path operator with full path prefix
    assert!(
        sql.contains("'author'->>'id'"),
        "nested 'id' must use path \"data\"->'author'->>'id', got: {sql}"
    );
    assert!(
        sql.contains("'author'->>'username'"),
        "nested 'username' must use correct path, got: {sql}"
    );
    // camelCase sub-field must map to snake_case key
    assert!(
        sql.contains("->>'full_name'"),
        "fullName sub-field must map to snake_case 'full_name', got: {sql}"
    );
    // Root scalars still use top-level path
    assert!(
        sql.contains("\"data\"->>'id'"),
        "root id must use top-level data path, got: {sql}"
    );
}

#[test]
fn test_typed_projection_composite_without_sub_fields_returns_full_blob() {
    // When sub_fields is None, fall back to data->'field' (no regression)
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![ProjectionField::composite("author")];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    assert!(
        sql.contains("\"data\"->'author'"),
        "composite without sub_fields must return full blob, got: {sql}"
    );
    // The outer jsonb_build_object wraps all fields — that's expected.
    // What must NOT appear is a *nested* jsonb_build_object as the value for 'author'.
    assert!(
        !sql.contains("'author', jsonb_build_object("),
        "must NOT produce nested jsonb_build_object for author when sub_fields is None, got: {sql}"
    );
}

#[test]
fn test_typed_projection_depth_2_recursion() {
    // Three levels: post → author → profile
    let generator = PostgresProjectionGenerator::new();
    let fields = vec![
        ProjectionField::scalar("id"),
        ProjectionField::composite_with_sub_fields(
            "author",
            vec![
                ProjectionField::scalar("id"),
                ProjectionField::composite_with_sub_fields(
                    "profile",
                    vec![ProjectionField::scalar("bio")],
                ),
            ],
        ),
    ];
    let sql = generator.generate_typed_projection_sql(&fields).unwrap();
    assert!(
        sql.contains("'author', jsonb_build_object("),
        "author must be nested, got: {sql}"
    );
    assert!(
        sql.contains("'profile', jsonb_build_object("),
        "profile must be nested inside author, got: {sql}"
    );
    assert!(sql.contains("'profile'->>'bio'"), "bio must use depth-2 path, got: {sql}");
}
