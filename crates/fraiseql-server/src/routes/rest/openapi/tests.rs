//! Tests for the `openapi` module.

#![allow(clippy::unwrap_used)] // Reason: test code

// ---------------------------------------------------------------------------
// mod.rs generator integration tests
// ---------------------------------------------------------------------------

use fraiseql_core::schema::{
    DeleteResponse, DeprecationInfo, FieldType, MutationDefinition, MutationOperation, RestConfig,
};
use fraiseql_test_utils::schema_builder::{TestFieldBuilder, TestSchemaBuilder, TestTypeBuilder};

use super::*;
use crate::routes::rest::resource::{HttpMethod, RestRoute, RestRouteTable, RouteSource};

// -- Helpers -----------------------------------------------------------------

fn mutation(name: &str, op: MutationOperation) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, "User");
    m.operation = op;
    m.sql_source = Some(format!("fn_{name}"));
    if name != "create_user" {
        m.arguments
            .push(fraiseql_core::schema::ArgumentDefinition::new("id", FieldType::Int));
    }
    if name.starts_with("update") {
        m.arguments
            .push(fraiseql_core::schema::ArgumentDefinition::new("name", FieldType::String));
        m.arguments
            .push(fraiseql_core::schema::ArgumentDefinition::new("email", FieldType::String));
    }
    m
}

fn rest_schema() -> CompiledSchema {
    let table = "users".to_string();
    let mut users_query = fraiseql_core::schema::QueryDefinition::new("users", "User");
    users_query.returns_list = true;
    users_query.auto_params = fraiseql_core::schema::AutoParams::all();
    users_query.sql_source = Some("v_user".to_string());

    let mut schema = TestSchemaBuilder::new()
        .with_query(users_query)
        .with_simple_query("user", "User", false)
        .with_mutation(mutation(
            "create_user",
            MutationOperation::Insert {
                table: table.clone(),
            },
        ))
        .with_mutation(mutation(
            "update_user",
            MutationOperation::Update {
                table: table.clone(),
            },
        ))
        .with_mutation(mutation("delete_user", MutationOperation::Delete { table }))
        .with_type(
            TestTypeBuilder::new("User", "v_user")
                .with_field(TestFieldBuilder::new("pk_user_id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("email", FieldType::String).build())
                .build(),
        )
        .build();

    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: true,
        ..RestConfig::default()
    });

    schema
}

fn generate(schema: &CompiledSchema) -> serde_json::Value {
    let route_table = RestRouteTable::from_compiled_schema(schema).unwrap();
    generate_openapi(schema, &route_table).unwrap()
}

// -- Structural tests --------------------------------------------------------

#[test]
fn spec_is_valid_openapi_303() {
    let spec = generate(&rest_schema());
    assert_eq!(spec["openapi"], "3.0.3");
}

#[test]
fn spec_has_info_title_and_version() {
    let spec = generate(&rest_schema());
    assert!(spec["info"]["title"].is_string());
    assert!(spec["info"]["version"].is_string());
}

#[test]
fn spec_has_paths_and_components() {
    let spec = generate(&rest_schema());
    assert!(spec["paths"].is_object());
    assert!(spec["components"].is_object());
    assert!(spec["components"]["schemas"].is_object());
}

#[test]
fn spec_has_server_entry() {
    let spec = generate(&rest_schema());
    assert!(spec["servers"].is_array());
    assert_eq!(spec["servers"][0]["url"], "/rest/v1");
}

// -- Type schemas ------------------------------------------------------------

#[test]
fn type_definition_produces_component_schema() {
    let spec = generate(&rest_schema());
    let user_schema = &spec["components"]["schemas"]["User"];
    assert_eq!(user_schema["type"], "object");
    assert!(user_schema["properties"]["name"].is_object());
    assert!(user_schema["properties"]["email"].is_object());
}

#[test]
fn scalar_fields_map_to_json_schema_types() {
    let spec = generate(&rest_schema());
    let props = &spec["components"]["schemas"]["User"]["properties"];
    assert_eq!(props["name"]["type"], "string");
    assert_eq!(props["pk_user_id"]["type"], "integer");
}

#[test]
fn nested_object_produces_ref() {
    let mut schema = rest_schema();
    schema.types.push(
        TestTypeBuilder::new("Address", "v_address")
            .with_field(TestFieldBuilder::new("city", FieldType::String).build())
            .build(),
    );
    for td in &mut schema.types {
        if td.name == "User" {
            td.fields.push(
                TestFieldBuilder::new("address", FieldType::Object("Address".to_string())).build(),
            );
        }
    }

    let spec = generate(&schema);
    let addr_prop = &spec["components"]["schemas"]["User"]["properties"]["address"];
    assert_eq!(addr_prop["$ref"], "#/components/schemas/Address");
    assert!(spec["components"]["schemas"]["Address"].is_object());
}

#[test]
fn enum_field_produces_ref() {
    let mut schema = rest_schema();
    schema.enums.push(fraiseql_core::schema::EnumDefinition {
        name:        "Status".to_string(),
        values:      vec![
            fraiseql_core::schema::EnumValueDefinition {
                name:        "ACTIVE".to_string(),
                description: None,
                deprecation: None,
            },
            fraiseql_core::schema::EnumValueDefinition {
                name:        "INACTIVE".to_string(),
                description: None,
                deprecation: None,
            },
        ],
        description: None,
    });
    for td in &mut schema.types {
        if td.name == "User" {
            td.fields.push(
                TestFieldBuilder::new("status", FieldType::Enum("Status".to_string())).build(),
            );
        }
    }

    let spec = generate(&schema);
    let status_prop = &spec["components"]["schemas"]["User"]["properties"]["status"];
    assert_eq!(status_prop["$ref"], "#/components/schemas/Status");
    let enum_schema = &spec["components"]["schemas"]["Status"];
    assert_eq!(enum_schema["type"], "string");
    let enum_vals = enum_schema["enum"].as_array().unwrap();
    assert_eq!(enum_vals.len(), 2);
}

// -- Query paths -------------------------------------------------------------

#[test]
fn list_query_produces_get_collection_path() {
    let spec = generate(&rest_schema());
    let paths = spec["paths"].as_object().unwrap();
    let users_path = paths.keys().find(|k| *k == "/users");
    assert!(users_path.is_some(), "Expected /users path");
    assert!(paths["/users"]["get"].is_object());
}

#[test]
fn single_query_produces_get_by_id_path() {
    let spec = generate(&rest_schema());
    let paths = spec["paths"].as_object().unwrap();
    let user_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users"));
    assert!(
        user_path.is_some(),
        "Expected /users/{{pk_user_id}} path, found: {:?}",
        paths.keys().collect::<Vec<_>>()
    );
}

#[test]
fn collection_get_has_pagination_params() {
    let spec = generate(&rest_schema());
    let params = &spec["paths"]["/users"]["get"]["parameters"];
    let param_names: Vec<&str> =
        params.as_array().unwrap().iter().filter_map(|p| p["name"].as_str()).collect();
    assert!(param_names.contains(&"limit"));
    assert!(param_names.contains(&"offset"));
    assert!(param_names.contains(&"select"));
    assert!(param_names.contains(&"sort"));
}

#[test]
fn relay_query_has_cursor_params() {
    let mut schema = rest_schema();
    for q in &mut schema.queries {
        if q.name == "users" {
            q.relay = true;
        }
    }

    let spec = generate(&schema);
    let params = &spec["paths"]["/users"]["get"]["parameters"];
    let param_names: Vec<&str> =
        params.as_array().unwrap().iter().filter_map(|p| p["name"].as_str()).collect();
    assert!(param_names.contains(&"first"));
    assert!(param_names.contains(&"after"));
    assert!(param_names.contains(&"last"));
    assert!(param_names.contains(&"before"));
    assert!(!param_names.contains(&"limit"));
}

// -- Mutation paths ----------------------------------------------------------

#[test]
fn insert_mutation_produces_post_path() {
    let spec = generate(&rest_schema());
    assert!(spec["paths"]["/users"]["post"].is_object());
}

#[test]
fn update_mutation_produces_put_and_patch() {
    let spec = generate(&rest_schema());
    let paths = spec["paths"].as_object().unwrap();
    let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
    assert!(paths[id_path]["put"].is_object() || paths[id_path]["patch"].is_object());
}

#[test]
fn delete_mutation_produces_delete_path() {
    let spec = generate(&rest_schema());
    let paths = spec["paths"].as_object().unwrap();
    let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
    assert!(paths[id_path]["delete"].is_object());
}

#[test]
fn post_has_request_body() {
    let spec = generate(&rest_schema());
    let post_op = &spec["paths"]["/users"]["post"];
    assert!(post_op["requestBody"].is_object());
    assert!(post_op["requestBody"]["content"]["application/json"].is_object());
}

#[test]
fn put_has_422_response() {
    let spec = generate(&rest_schema());
    let paths = spec["paths"].as_object().unwrap();
    let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
    if let Some(put_op) = paths[id_path].get("put") {
        assert!(put_op["responses"]["422"].is_object());
    }
}

#[test]
fn custom_mutation_produces_post_action() {
    let mut schema = rest_schema();
    schema.mutations.push({
        let mut m = MutationDefinition::new("archiveUser", "User");
        m.operation = MutationOperation::Custom;
        m.sql_source = Some("fn_archive_user".to_string());
        m
    });

    let spec = generate(&schema);
    let paths = spec["paths"].as_object().unwrap();
    let action_path = paths
        .keys()
        .find(|k| k.contains("archive"))
        .expect("Expected an archive action path");
    assert!(paths[action_path]["post"].is_object());
}

// -- Deprecated operations ---------------------------------------------------

#[test]
fn deprecated_operation_has_deprecated_flag() {
    let mut schema = rest_schema();
    for q in &mut schema.queries {
        if q.name == "user" {
            q.deprecation = Some(DeprecationInfo {
                reason: Some("Use v2".to_string()),
            });
        }
    }

    let spec = generate(&schema);
    let paths = spec["paths"].as_object().unwrap();
    let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
    let get_op = &paths[id_path]["get"];
    assert_eq!(get_op["deprecated"], true);
}

// -- Auth --------------------------------------------------------------------

#[test]
fn auth_required_produces_security_schemes() {
    let schema = rest_schema();
    let spec = generate(&schema);
    assert!(spec["components"]["securitySchemes"]["BearerAuth"].is_object());
}

#[test]
fn auth_required_adds_401_403_to_responses() {
    let schema = rest_schema();
    let spec = generate(&schema);
    let get_op = &spec["paths"]["/users"]["get"];
    assert!(get_op["responses"]["401"].is_object());
    assert!(get_op["responses"]["403"].is_object());
}

#[test]
fn no_auth_omits_security_schemes() {
    let mut schema = rest_schema();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: false,
        ..RestConfig::default()
    });

    let spec = generate(&schema);
    assert!(spec["components"]["securitySchemes"].is_null());
}

// -- Prefer header -----------------------------------------------------------

#[test]
fn collection_get_has_prefer_header() {
    let spec = generate(&rest_schema());
    let params = &spec["paths"]["/users"]["get"]["parameters"];
    let has_prefer = params.as_array().unwrap().iter().any(|p| p["name"] == "Prefer");
    assert!(has_prefer);
}

#[test]
fn delete_has_prefer_header() {
    let spec = generate(&rest_schema());
    let paths = spec["paths"].as_object().unwrap();
    let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
    if let Some(delete_op) = paths[id_path].get("delete") {
        let has_prefer = delete_op["parameters"]
            .as_array()
            .is_some_and(|arr| arr.iter().any(|p| p["name"] == "Prefer"));
        assert!(has_prefer);
    }
}

// -- Bracket operators -------------------------------------------------------

#[test]
fn filter_params_document_bracket_operators() {
    let spec = generate(&rest_schema());
    let params = spec["paths"]["/users"]["get"]["parameters"].as_array().unwrap();
    let filter_param = params
        .iter()
        .find(|p| p["name"].as_str().is_some_and(|n| n.contains("[operator]")));
    assert!(filter_param.is_some(), "Expected bracket operator param");
    let desc = filter_param.unwrap()["description"].as_str().unwrap();
    assert!(desc.contains("eq"));
    assert!(desc.contains("like"));
}

// -- OpenAPI self-reference endpoint -----------------------------------------

#[test]
fn openapi_json_endpoint_present() {
    let spec = generate(&rest_schema());
    assert!(spec["paths"]["/openapi.json"]["get"].is_object());
}

// -- Delete response modes ---------------------------------------------------

#[test]
fn delete_no_content_mode() {
    let spec = generate(&rest_schema());
    let paths = spec["paths"].as_object().unwrap();
    let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
    if let Some(delete_op) = paths[id_path].get("delete") {
        assert!(delete_op["responses"]["204"].is_object());
    }
}

#[test]
fn delete_entity_mode() {
    let mut schema = rest_schema();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        delete_response: DeleteResponse::Entity,
        ..RestConfig::default()
    });

    let spec = generate(&schema);
    let paths = spec["paths"].as_object().unwrap();
    let id_path = paths.keys().find(|k| k.contains('{') && k.starts_with("/users")).unwrap();
    if let Some(delete_op) = paths[id_path].get("delete") {
        assert!(delete_op["responses"]["200"].is_object());
    }
}

// -- Error schema ------------------------------------------------------------

#[test]
fn error_schema_present() {
    let spec = generate(&rest_schema());
    let error_schema = &spec["components"]["schemas"]["Error"];
    assert_eq!(error_schema["type"], "object");
    assert!(error_schema["properties"]["error"].is_object());
}

// -- Edge cases --------------------------------------------------------------

#[test]
fn missing_rest_config_returns_error() {
    let schema = TestSchemaBuilder::new().build();
    let route_table = RestRouteTable {
        base_path:   "/rest/v1".to_string(),
        resources:   vec![],
        diagnostics: vec![],
    };
    let result = generate_openapi(&schema, &route_table);
    assert!(result.is_err());
}

#[test]
fn empty_route_table_produces_minimal_spec() {
    let mut schema = TestSchemaBuilder::new().build();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    let route_table = RestRouteTable {
        base_path:   "/rest/v1".to_string(),
        resources:   vec![],
        diagnostics: vec![],
    };
    let spec = generate_openapi(&schema, &route_table).unwrap();
    assert_eq!(spec["openapi"], "3.0.3");
    let paths = spec["paths"].as_object().unwrap();
    assert_eq!(paths.len(), 1);
}

// -- Bulk operations ---------------------------------------------------------

#[test]
fn bulk_update_produces_collection_patch() {
    let spec = generate(&rest_schema());
    let patch_op = &spec["paths"]["/users"]["patch"];
    assert!(patch_op.is_object(), "Expected PATCH on /users");
    assert_eq!(patch_op["operationId"], "bulk_update_users");
    assert!(patch_op["responses"]["200"].is_object());
    assert!(patch_op["responses"]["400"].is_object());
}

#[test]
fn bulk_delete_produces_collection_delete() {
    let spec = generate(&rest_schema());
    let delete_op = &spec["paths"]["/users"]["delete"];
    assert!(delete_op.is_object(), "Expected DELETE on /users");
    assert_eq!(delete_op["operationId"], "bulk_delete_users");
    assert!(delete_op["responses"]["200"].is_object());
    assert!(delete_op["responses"]["400"].is_object());
}

#[test]
fn post_body_supports_array_for_bulk_insert() {
    let spec = generate(&rest_schema());
    let post_body =
        &spec["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"]["schema"];
    assert!(post_body["oneOf"].is_array(), "Expected oneOf schema for bulk insert support");
    let variants = post_body["oneOf"].as_array().unwrap();
    assert_eq!(variants.len(), 2);
    assert_eq!(variants[1]["type"], "array");
}

#[test]
fn post_has_prefer_header_for_upsert() {
    let spec = generate(&rest_schema());
    let params = &spec["paths"]["/users"]["post"]["parameters"];
    let has_prefer = params.as_array().unwrap().iter().any(|p| p["name"] == "Prefer");
    assert!(has_prefer, "POST should have Prefer header for upsert/bulk preferences");
}

#[test]
fn info_has_default_title() {
    let spec = generate(&rest_schema());
    assert_eq!(spec["info"]["title"], "FraiseQL REST API");
}

#[test]
fn info_has_default_version() {
    let spec = generate(&rest_schema());
    assert_eq!(spec["info"]["version"], "1.0.0");
}

// -- Logical operators -------------------------------------------------------

#[test]
fn collection_get_has_logical_operator_params() {
    let spec = generate(&rest_schema());
    let params = spec["paths"]["/users"]["get"]["parameters"].as_array().unwrap();
    let param_names: Vec<&str> = params.iter().filter_map(|p| p["name"].as_str()).collect();
    assert!(param_names.contains(&"or"), "Expected `or` logical param");
    assert!(param_names.contains(&"and"), "Expected `and` logical param");
    assert!(param_names.contains(&"not"), "Expected `not` logical param");
}

// -- Full-text search --------------------------------------------------------

#[test]
fn fts_enabled_resource_has_search_param() {
    let schema = rest_schema();
    let spec = generate(&schema);
    let params = spec["paths"]["/users"]["get"]["parameters"].as_array().unwrap();
    let search_param = params.iter().find(|p| p["name"] == "search");
    assert!(search_param.is_some(), "Expected `search` param on FTS-enabled resource");
    let desc = search_param.unwrap()["description"].as_str().unwrap();
    assert!(desc.contains("name"), "Expected field name in search description: {desc}");
}

#[test]
fn non_fts_resource_has_no_search_param() {
    let mut users_query = fraiseql_core::schema::QueryDefinition::new("counters", "Counter");
    users_query.returns_list = true;
    users_query.auto_params = fraiseql_core::schema::AutoParams::all();
    users_query.sql_source = Some("v_counter".to_string());

    let mut schema = TestSchemaBuilder::new()
        .with_query(users_query)
        .with_type(
            TestTypeBuilder::new("Counter", "v_counter")
                .with_field(TestFieldBuilder::new("pk_id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("value", FieldType::Int).build())
                .build(),
        )
        .build();
    schema.rest_config = Some(RestConfig::default());

    let spec = generate(&schema);
    let params = spec["paths"]["/counters"]["get"]["parameters"].as_array().unwrap();
    let search_param = params.iter().find(|p| p["name"] == "search");
    assert!(search_param.is_none(), "Non-FTS resource should not have search param");
}

// ---------------------------------------------------------------------------
// format.rs tests
// ---------------------------------------------------------------------------

use super::format::{
    capitalize, extract_action, method_to_string, should_have_prefer_header, to_snake,
};

#[test]
fn capitalize_test() {
    assert_eq!(capitalize("users"), "Users");
    assert_eq!(capitalize(""), "");
    assert_eq!(capitalize("a"), "A");
}

#[test]
fn to_snake_test() {
    assert_eq!(to_snake("Users"), "users");
    assert_eq!(to_snake("UserProfile"), "user_profile");
    assert_eq!(to_snake("API"), "a_p_i");
}

#[test]
fn extract_action_suffix() {
    assert_eq!(extract_action("archiveUser", "User"), "archive");
    assert_eq!(extract_action("deleteUser", "User"), "delete");
}

#[test]
fn extract_action_prefix() {
    assert_eq!(extract_action("userArchive", "User"), "archive");
    assert_eq!(extract_action("userDelete", "User"), "delete");
}

#[test]
fn extract_action_fallback() {
    assert_eq!(extract_action("complexAction", "Other"), "complex-action");
}

#[test]
fn method_to_string_all() {
    assert_eq!(method_to_string(HttpMethod::Get), "get");
    assert_eq!(method_to_string(HttpMethod::Post), "post");
    assert_eq!(method_to_string(HttpMethod::Put), "put");
    assert_eq!(method_to_string(HttpMethod::Patch), "patch");
    assert_eq!(method_to_string(HttpMethod::Delete), "delete");
}

#[test]
fn should_have_prefer_header_get_collection() {
    let mut route = RestRoute {
        method:          HttpMethod::Get,
        path:            "/users".to_string(),
        source:          RouteSource::Query {
            name: "users".to_string(),
        },
        update_coverage: None,
        success_status:  200,
    };
    assert!(should_have_prefer_header(&route));

    route.path = "/users/{id}".to_string();
    assert!(!should_have_prefer_header(&route));
}

#[test]
fn should_have_prefer_header_post() {
    let route = RestRoute {
        method:          HttpMethod::Post,
        path:            "/users".to_string(),
        source:          RouteSource::Mutation {
            name: "createUser".to_string(),
        },
        update_coverage: None,
        success_status:  201,
    };
    assert!(should_have_prefer_header(&route));
}

#[test]
fn should_have_prefer_header_put() {
    let route = RestRoute {
        method:          HttpMethod::Put,
        path:            "/users/{id}".to_string(),
        source:          RouteSource::Mutation {
            name: "updateUser".to_string(),
        },
        update_coverage: None,
        success_status:  200,
    };
    assert!(!should_have_prefer_header(&route));
}
