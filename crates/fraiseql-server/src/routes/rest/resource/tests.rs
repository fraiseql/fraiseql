//! Tests for the `resource` module.

#![allow(clippy::unwrap_used)] // Reason: test code

use fraiseql_core::schema::{
    ArgumentDefinition, DeleteResponse, FieldDefinition, FieldType, MutationDefinition,
    MutationOperation, RestConfig, TypeDefinition,
};

use super::{
    naming::{
        camel_to_kebab, derive_action_name, simple_pluralize, strip_cqrs_prefix, type_name_to_snake,
    },
    validation::is_filtered_out,
    *,
};

// -----------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------

fn user_type_def() -> TypeDefinition {
    TypeDefinition::new("User", "v_user")
        .with_field(FieldDefinition::new("id", FieldType::Uuid))
        .with_field(FieldDefinition::new("pk_user", FieldType::Int))
        .with_field(FieldDefinition::new("email", FieldType::String))
        .with_field(FieldDefinition::new("name", FieldType::String))
}

fn list_query(name: &str, return_type: &str) -> QueryDefinition {
    QueryDefinition::new(name, return_type).returning_list()
}

fn single_query(name: &str, return_type: &str) -> QueryDefinition {
    let mut q = QueryDefinition::new(name, return_type);
    q.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
    q
}

fn insert_mutation(name: &str, return_type: &str, table: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, return_type);
    m.operation = MutationOperation::Insert {
        table: table.to_string(),
    };
    m.arguments.push(ArgumentDefinition::new("email", FieldType::String));
    m.arguments.push(ArgumentDefinition::new("name", FieldType::String));
    m
}

fn full_update_mutation(name: &str, return_type: &str, table: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, return_type);
    m.operation = MutationOperation::Update {
        table: table.to_string(),
    };
    m.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
    // All writable fields of user_type_def: email, name.
    m.arguments.push(ArgumentDefinition::new("email", FieldType::String));
    m.arguments.push(ArgumentDefinition::new("name", FieldType::String));
    m
}

fn partial_update_mutation(name: &str, return_type: &str, table: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, return_type);
    m.operation = MutationOperation::Update {
        table: table.to_string(),
    };
    m.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
    // Only email — partial coverage.
    m.arguments.push(ArgumentDefinition::new("email", FieldType::String));
    m
}

fn delete_mutation(name: &str, return_type: &str, table: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, return_type);
    m.operation = MutationOperation::Delete {
        table: table.to_string(),
    };
    m.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
    m
}

fn custom_mutation(name: &str, return_type: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, return_type);
    m.operation = MutationOperation::Custom;
    m.arguments.push(ArgumentDefinition::new("id", FieldType::Uuid));
    m
}

fn schema_with_rest_config(config: Option<RestConfig>) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.rest_config = config;
    schema
}

// -----------------------------------------------------------------------
// Full resource derivation
// -----------------------------------------------------------------------

#[test]
fn test_full_crud_resource() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User").with_sql_source("v_user"));
    schema.queries.push(single_query("user", "User"));
    schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));
    schema.mutations.push(full_update_mutation("updateUser", "User", "tb_user"));
    schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert_eq!(table.resources.len(), 1);
    let r = &table.resources[0];
    assert_eq!(r.name, "users");
    assert_eq!(r.type_name, "User");
    assert_eq!(r.id_arg.as_deref(), Some("id"));

    let methods: Vec<_> = r.routes.iter().map(|rt| (rt.method, rt.path.as_str())).collect();
    assert!(methods.contains(&(HttpMethod::Get, "/users")));
    assert!(methods.contains(&(HttpMethod::Get, "/users/{id}")));
    assert!(methods.contains(&(HttpMethod::Post, "/users")));
    assert!(methods.contains(&(HttpMethod::Put, "/users/{id}")));
    assert!(methods.contains(&(HttpMethod::Patch, "/users/{id}")));
    assert!(methods.contains(&(HttpMethod::Delete, "/users/{id}")));
}

#[test]
fn test_full_coverage_update_generates_put_and_patch() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.mutations.push(full_update_mutation("updateUser", "User", "tb_user"));
    schema.queries.push(single_query("user", "User"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    let update_routes: Vec<_> = r
        .routes
        .iter()
        .filter(|rt| rt.update_coverage == Some(UpdateCoverage::Full))
        .collect();
    assert_eq!(update_routes.len(), 2);
    assert!(update_routes.iter().any(|rt| rt.method == HttpMethod::Put));
    assert!(update_routes.iter().any(|rt| rt.method == HttpMethod::Patch));
}

#[test]
fn test_partial_coverage_update_generates_patch_action() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema
        .mutations
        .push(partial_update_mutation("updateUserEmail", "User", "tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    let patch_route = r.routes.iter().find(|rt| rt.method == HttpMethod::Patch).unwrap();
    assert_eq!(patch_route.path, "/users/{id}/update-email");
    assert_eq!(patch_route.update_coverage, Some(UpdateCoverage::Partial));
}

#[test]
fn test_custom_mutation_post_action() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema.mutations.push(custom_mutation("archiveUser", "User"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    let custom = r.routes.iter().find(|rt| rt.method == HttpMethod::Post).unwrap();
    assert_eq!(custom.path, "/users/{id}/archive");
    assert_eq!(custom.success_status, 200);
}

#[test]
fn test_no_list_query_derives_name_from_type() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    // Only a single query, no list query.
    schema.queries.push(single_query("user", "User"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    assert_eq!(r.name, "users");
    assert!(table.diagnostics.iter().any(|d| d.message.contains("No list query")));
}

#[test]
fn test_rest_path_override_on_query() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    let mut q = list_query("users", "User");
    q.rest_path = Some("/custom/users".to_string());
    schema.queries.push(q);

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    let route = r.routes.iter().find(|rt| rt.path == "/custom/users").unwrap();
    assert_eq!(route.method, HttpMethod::Get);
}

#[test]
fn test_rest_path_override_on_mutation() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    let mut m = insert_mutation("createUser", "User", "tb_user");
    m.rest_path = Some("/custom/create".to_string());
    m.rest_method = Some("PUT".to_string());
    schema.mutations.push(m);

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    let route = r.routes.iter().find(|rt| rt.path == "/custom/create").unwrap();
    assert_eq!(route.method, HttpMethod::Put);
}

// -----------------------------------------------------------------------
// Route conflict detection
// -----------------------------------------------------------------------

#[test]
fn test_route_conflict_detected() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    // Two full-coverage updates → conflict on PUT /users/{id}.
    schema.mutations.push(full_update_mutation("updateUser", "User", "tb_user"));
    let mut m2 = full_update_mutation("updateUser2", "User", "tb_user");
    m2.name = "updateUser2".to_string();
    schema.mutations.push(m2);

    let result = RestRouteTable::from_compiled_schema(&schema);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Route conflict"));
}

// -----------------------------------------------------------------------
// Exclusion rules
// -----------------------------------------------------------------------

#[test]
fn test_scalar_return_type_excluded() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    // No TypeDefinition for "Int" — query returning Int is excluded.
    let q = QueryDefinition::new("totalCount", "Int").returning_list();
    schema.queries.push(q);

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(table.resources.is_empty());
}

#[test]
fn test_aggregate_query_excluded() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(TypeDefinition::new("UserAggregate", "v_user_aggregate"));
    schema.queries.push(list_query("users_aggregate", "UserAggregate"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(table.resources.is_empty());
}

#[test]
fn test_window_query_excluded() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(TypeDefinition::new("SalesWindow", "tv_sales_window"));
    schema.queries.push(list_query("sales_window", "SalesWindow"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(table.resources.is_empty());
}

// -----------------------------------------------------------------------
// Include/exclude filters
// -----------------------------------------------------------------------

#[test]
fn test_exclude_filter() {
    let config = RestConfig {
        exclude: vec!["deleteUser".to_string()],
        ..RestConfig::default()
    };
    let mut schema = schema_with_rest_config(Some(config));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));
    schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    assert!(!r.routes.iter().any(|rt| rt.method == HttpMethod::Delete));
}

#[test]
fn test_include_filter() {
    let config = RestConfig {
        include: vec!["users".to_string(), "createUser".to_string()],
        ..RestConfig::default()
    };
    let mut schema = schema_with_rest_config(Some(config));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));
    schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    assert!(r.routes.iter().any(|rt| rt.method == HttpMethod::Get));
    assert!(r.routes.iter().any(|rt| rt.method == HttpMethod::Post));
    assert!(!r.routes.iter().any(|rt| rt.method == HttpMethod::Delete));
}

// -----------------------------------------------------------------------
// CQRS validation
// -----------------------------------------------------------------------

#[test]
fn test_cqrs_query_from_view_no_warning() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User").with_sql_source("v_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(
        !table
            .diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Warning && d.message.contains("CQRS"))
    );
}

#[test]
fn test_cqrs_query_from_table_warns() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User").with_sql_source("tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(table.diagnostics.iter().any(|d| {
        d.level == DiagnosticLevel::Warning && d.message.contains("reads from write table")
    }));
}

#[test]
fn test_cqrs_query_from_table_view_no_warning() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    let td = TypeDefinition::new("Analytics", "tv_analytics");
    schema.types.push(td);
    schema
        .queries
        .push(list_query("analytics", "Analytics").with_sql_source("tv_analytics"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(
        !table
            .diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Warning && d.message.contains("CQRS"))
    );
}

#[test]
fn test_cqrs_mutation_to_view_warns() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema.mutations.push(insert_mutation("createUser", "User", "v_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(
        table.diagnostics.iter().any(|d| {
            d.level == DiagnosticLevel::Warning && d.message.contains("writes to view")
        })
    );
}

#[test]
fn test_cqrs_mutation_to_table_no_warning() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(
        !table
            .diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Warning && d.message.contains("writes to"))
    );
}

// -----------------------------------------------------------------------
// PK field type validation
// -----------------------------------------------------------------------

#[test]
fn test_pk_field_varchar_warns() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    let td = TypeDefinition::new("User", "v_user")
        .with_field(FieldDefinition::new("pk_user", FieldType::String));
    schema.types.push(td);
    schema.queries.push(list_query("users", "User"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(table.diagnostics.iter().any(|d| {
        d.level == DiagnosticLevel::Warning && d.message.contains("pk_/fk_ field 'pk_user'")
    }));
}

#[test]
fn test_id_field_bigint_warns() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    let td = TypeDefinition::new("User", "v_user")
        .with_field(FieldDefinition::new("id", FieldType::Int))
        .with_field(FieldDefinition::new("email", FieldType::String));
    schema.types.push(td);
    schema.queries.push(list_query("users", "User"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert!(table.diagnostics.iter().any(|d| {
        d.level == DiagnosticLevel::Warning && d.message.contains("id field on 'User' is Int")
    }));
}

// -----------------------------------------------------------------------
// ID parameter detection
// -----------------------------------------------------------------------

#[test]
fn test_pk_fallback_when_no_id() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    let td = TypeDefinition::new("User", "v_user")
        .with_field(FieldDefinition::new("pk_user", FieldType::Int))
        .with_field(FieldDefinition::new("email", FieldType::String));
    schema.types.push(td);
    let mut m = MutationDefinition::new("updateUser", "User");
    m.operation = MutationOperation::Update {
        table: "tb_user".to_string(),
    };
    m.arguments.push(ArgumentDefinition::new("pk_user", FieldType::Int));
    m.arguments.push(ArgumentDefinition::new("email", FieldType::String));
    schema.mutations.push(m);

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    assert_eq!(r.id_arg.as_deref(), Some("pk_user"));
    assert!(table.diagnostics.iter().any(|d| d.message.contains("using `pk_user`")));
}

// -----------------------------------------------------------------------
// Resource name derivation from CQRS
// -----------------------------------------------------------------------

#[test]
fn test_resource_name_from_view() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    let td = TypeDefinition::new("User", "v_user")
        .with_field(FieldDefinition::new("id", FieldType::Uuid));
    schema.types.push(td);
    // No list query, but single query with sql_source.
    let q = QueryDefinition::new("user", "User").with_sql_source("v_user");
    schema.queries.push(q);

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    assert_eq!(r.name, "users");
}

#[test]
fn test_resource_name_from_table_view() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    let td = TypeDefinition::new("Analytics", "tv_analytics")
        .with_field(FieldDefinition::new("id", FieldType::Uuid));
    schema.types.push(td);
    let q = QueryDefinition::new("analytics_item", "Analytics").with_sql_source("tv_analytics");
    schema.queries.push(q);

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let r = &table.resources[0];
    // Falls back since no list query; derives from sql_source.
    assert_eq!(r.name, "analytics");
}

// -----------------------------------------------------------------------
// Action naming
// -----------------------------------------------------------------------

#[test]
fn test_action_name_archive_user() {
    assert_eq!(derive_action_name("archiveUser", "User"), "archive");
}

#[test]
fn test_action_name_update_user_email() {
    assert_eq!(derive_action_name("updateUserEmail", "User"), "update-email");
}

#[test]
fn test_action_name_no_prefix_match() {
    assert_eq!(derive_action_name("doSomething", "User"), "do-something");
}

// -----------------------------------------------------------------------
// DeleteResponse config
// -----------------------------------------------------------------------

#[test]
fn test_delete_response_no_content() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let del = table.resources[0]
        .routes
        .iter()
        .find(|r| r.method == HttpMethod::Delete)
        .unwrap();
    assert_eq!(del.success_status, 204);
}

#[test]
fn test_delete_response_entity() {
    let config = RestConfig {
        delete_response: DeleteResponse::Entity,
        ..RestConfig::default()
    };
    let mut schema = schema_with_rest_config(Some(config));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema.mutations.push(delete_mutation("deleteUser", "User", "tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let del = table.resources[0]
        .routes
        .iter()
        .find(|r| r.method == HttpMethod::Delete)
        .unwrap();
    assert_eq!(del.success_status, 200);
}

// -----------------------------------------------------------------------
// Display trait
// -----------------------------------------------------------------------

#[test]
fn test_route_table_display() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let display = format!("{table}");
    assert!(display.contains("REST Route Table"));
    assert!(display.contains("/rest/v1"));
    assert!(display.contains("GET"));
}

// -----------------------------------------------------------------------
// Helper unit tests
// -----------------------------------------------------------------------

#[test]
fn test_simple_pluralize() {
    assert_eq!(simple_pluralize("user"), "users");
    assert_eq!(simple_pluralize("bus"), "buses");
    assert_eq!(simple_pluralize("box"), "boxes");
    assert_eq!(simple_pluralize("church"), "churches");
    assert_eq!(simple_pluralize("dish"), "dishes");
    assert_eq!(simple_pluralize("category"), "categories");
    assert_eq!(simple_pluralize("key"), "keys");
    assert_eq!(simple_pluralize("analytics"), "analytics");
}

#[test]
fn test_camel_to_kebab() {
    assert_eq!(camel_to_kebab("updateEmail"), "update-email");
    assert_eq!(camel_to_kebab("archive"), "archive");
    assert_eq!(camel_to_kebab("UpdateEmail"), "update-email");
    assert_eq!(camel_to_kebab(""), "");
}

#[test]
fn test_type_name_to_snake() {
    assert_eq!(type_name_to_snake("User"), "user");
    assert_eq!(type_name_to_snake("BlogPost"), "blog_post");
    assert_eq!(type_name_to_snake("HTTPResponse"), "h_t_t_p_response");
}

#[test]
fn test_strip_cqrs_prefix() {
    assert_eq!(strip_cqrs_prefix("v_user"), "user");
    assert_eq!(strip_cqrs_prefix("tv_analytics"), "analytics");
    assert_eq!(strip_cqrs_prefix("tb_user"), "user");
    assert_eq!(strip_cqrs_prefix("user"), "user");
}

#[test]
fn test_is_filtered_out() {
    let config = RestConfig {
        include: vec!["users".to_string()],
        ..RestConfig::default()
    };
    assert!(!is_filtered_out("users", &config));
    assert!(is_filtered_out("posts", &config));

    let config2 = RestConfig {
        exclude: vec!["deleteUser".to_string()],
        ..RestConfig::default()
    };
    assert!(!is_filtered_out("createUser", &config2));
    assert!(is_filtered_out("deleteUser", &config2));
}

#[test]
fn test_no_rest_config_uses_defaults() {
    let mut schema = CompiledSchema::new();
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    assert_eq!(table.base_path, "/rest/v1");
    assert_eq!(table.resources.len(), 1);
}

#[test]
fn test_insert_mutation_returns_201() {
    let mut schema = schema_with_rest_config(Some(RestConfig::default()));
    schema.types.push(user_type_def());
    schema.queries.push(list_query("users", "User"));
    schema.mutations.push(insert_mutation("createUser", "User", "tb_user"));

    let table = RestRouteTable::from_compiled_schema(&schema).unwrap();
    let create = table.resources[0].routes.iter().find(|r| r.method == HttpMethod::Post).unwrap();
    assert_eq!(create.success_status, 201);
}
