#![allow(clippy::unwrap_used)] // Reason: test assertions, panics are acceptable

use fraiseql_core::schema::FieldType;

use super::*;

#[test]
fn test_schema_builder_empty() {
    let schema = TestSchemaBuilder::new().build();
    assert!(schema.queries.is_empty());
    assert!(schema.mutations.is_empty());
    assert!(schema.types.is_empty());
}

#[test]
fn test_schema_builder_with_simple_query() {
    let schema = TestSchemaBuilder::new().with_simple_query("users", "User", true).build();

    assert_eq!(schema.queries.len(), 1);
    assert_eq!(schema.queries[0].name, "users");
    assert_eq!(schema.queries[0].return_type, "User");
    assert!(schema.queries[0].returns_list);
    // Default sql_source
    assert_eq!(schema.queries[0].sql_source.as_deref(), Some("v_users"));
}

#[test]
fn test_schema_builder_indexes_populated() {
    let schema = TestSchemaBuilder::new()
        .with_simple_query("users", "User", true)
        .with_simple_query("user", "User", false)
        .build();

    // Indexes must be populated for O(1) lookup to work
    assert!(schema.find_query("users").is_some());
    assert!(schema.find_query("user").is_some());
    assert!(schema.find_query("missing").is_none());
}

#[test]
fn test_schema_builder_with_role_guarded_query() {
    let schema = TestSchemaBuilder::new()
        .with_role_guarded_query("adminStats", "Stats", false, "admin")
        .build();

    let query = schema.find_query("adminStats").unwrap();
    assert_eq!(query.requires_role.as_deref(), Some("admin"));
}

#[test]
fn test_schema_builder_with_mutation() {
    let schema = TestSchemaBuilder::new().with_simple_mutation("createUser", "User").build();

    assert_eq!(schema.mutations.len(), 1);
    assert_eq!(schema.mutations[0].name, "createUser");
    assert_eq!(schema.mutations[0].sql_source.as_deref(), Some("fn_createUser"));
}

#[test]
fn test_type_builder_with_fields() {
    let type_def = TestTypeBuilder::new("User", "v_user")
        .with_simple_field("id", FieldType::Int)
        .with_nullable_field("bio", FieldType::String)
        .build();

    assert_eq!(type_def.fields.len(), 2);
    assert!(!type_def.fields[0].nullable);
    assert!(type_def.fields[1].nullable);
}

#[test]
fn test_type_builder_scoped_field() {
    let type_def = TestTypeBuilder::new("Employee", "v_employee")
        .with_scoped_field("salary", FieldType::Int, "read:Employee.salary")
        .build();

    assert_eq!(type_def.fields[0].requires_scope.as_deref(), Some("read:Employee.salary"));
}

#[test]
fn test_query_builder_deprecated() {
    let query = TestQueryBuilder::new("oldQuery", "Result")
        .deprecated("Use newQuery instead")
        .build();

    assert!(query.deprecation.is_some());
    assert_eq!(query.deprecation.unwrap().reason.as_deref(), Some("Use newQuery instead"));
}

#[test]
fn test_field_builder_on_deny_policy() {
    let field = TestFieldBuilder::new("secret", FieldType::String)
        .requires_scope("admin:read")
        .on_deny(FieldDenyPolicy::Reject)
        .build();

    assert_eq!(field.on_deny, FieldDenyPolicy::Reject);
}

#[test]
fn test_mutation_builder_with_sql_source() {
    let mutation = TestMutationBuilder::new("archivePost", "Post")
        .with_sql_source("fn_archive_post")
        .build();

    assert_eq!(mutation.sql_source.as_deref(), Some("fn_archive_post"));
}

#[test]
fn test_schema_with_type_and_query() {
    let schema = TestSchemaBuilder::new()
        .with_simple_query("users", "User", true)
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_simple_field("id", FieldType::Int)
                .with_simple_field("name", FieldType::String)
                .build(),
        )
        .build();

    assert_eq!(schema.queries.len(), 1);
    assert_eq!(schema.types.len(), 1);
    assert_eq!(schema.types[0].fields.len(), 2);
}

#[test]
fn test_query_builder_cache_ttl() {
    let query = TestQueryBuilder::new("hot", "Item").with_cache_ttl(300).build();

    assert_eq!(query.cache_ttl_seconds, Some(300));
}
