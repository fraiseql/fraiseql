//! GraphQL → Protobuf type mapping and `.proto` file generation.

use std::collections::BTreeSet;

use fraiseql_core::{
    db::dialect::RowViewColumnType,
    schema::{CompiledSchema, FieldDefinition, FieldType},
};

/// Map a GraphQL type name to a Protobuf type name.
///
/// Returns the protobuf scalar or well-known type for the given GraphQL type.
/// Unknown types fall back to `"string"`.
///
/// # Examples
///
/// ```
/// use fraiseql_cli::codegen::proto_gen::graphql_to_proto_type;
///
/// assert_eq!(graphql_to_proto_type("String"), "string");
/// assert_eq!(graphql_to_proto_type("Int"), "int32");
/// assert_eq!(graphql_to_proto_type("DateTime"), "google.protobuf.Timestamp");
/// ```
#[must_use]
pub fn graphql_to_proto_type(graphql_type: &str) -> &'static str {
    match graphql_type {
        "String" => "string",
        "Int" => "int32",
        "Float" => "double",
        "Boolean" => "bool",
        "ID" => "string",
        "DateTime" => "google.protobuf.Timestamp",
        "Date" => "string",
        "BigInt" => "int64",
        "JSON" => "google.protobuf.Struct",
        _ => "string", // Custom scalars fall back to string
    }
}

/// Map a GraphQL type name to a [`RowViewColumnType`] for SQL view generation.
///
/// Used by the row-shaped view DDL generator to determine the target SQL type
/// for each field extracted from the JSON column.
///
/// # Examples
///
/// ```
/// use fraiseql_cli::codegen::proto_gen::graphql_to_row_view_type;
/// use fraiseql_core::db::dialect::RowViewColumnType;
///
/// assert_eq!(graphql_to_row_view_type("String"), RowViewColumnType::Text);
/// assert_eq!(graphql_to_row_view_type("Int"), RowViewColumnType::Int32);
/// ```
#[must_use]
pub fn graphql_to_row_view_type(graphql_type: &str) -> RowViewColumnType {
    match graphql_type {
        "String" => RowViewColumnType::Text,
        "Date" => RowViewColumnType::Date,
        "Int" => RowViewColumnType::Int32,
        "BigInt" => RowViewColumnType::Int64,
        "Float" => RowViewColumnType::Float64,
        "Boolean" => RowViewColumnType::Boolean,
        "ID" => RowViewColumnType::Uuid,
        "DateTime" => RowViewColumnType::Timestamptz,
        "JSON" => RowViewColumnType::Json,
        _ => RowViewColumnType::Text, // Custom scalars → text
    }
}

/// Returns `true` if the given protobuf type requires an import of a
/// well-known type `.proto` file.
#[must_use]
pub fn needs_well_known_import(proto_type: &str) -> bool {
    matches!(proto_type, "google.protobuf.Timestamp" | "google.protobuf.Struct")
}

/// Map a [`FieldType`] to a protobuf type string.
///
/// Handles scalars, lists (`repeated`), enums (referenced by name),
/// and object references (referenced by message name).
fn field_type_to_proto(ft: &FieldType) -> ProtoFieldType {
    match ft {
        FieldType::String => ProtoFieldType::scalar("string"),
        FieldType::Int => ProtoFieldType::scalar("int32"),
        FieldType::Float => ProtoFieldType::scalar("double"),
        FieldType::Boolean => ProtoFieldType::scalar("bool"),
        FieldType::Id | FieldType::Uuid => ProtoFieldType::scalar("string"),
        FieldType::DateTime => ProtoFieldType::scalar("google.protobuf.Timestamp"),
        FieldType::Date | FieldType::Time | FieldType::Decimal => ProtoFieldType::scalar("string"),
        FieldType::Json => ProtoFieldType::scalar("google.protobuf.Struct"),
        FieldType::Vector => ProtoFieldType::repeated("double"),
        FieldType::Scalar(_) => ProtoFieldType::scalar("string"),
        FieldType::Enum(name) => ProtoFieldType::scalar(name),
        FieldType::Object(name) | FieldType::Interface(name) | FieldType::Union(name) => {
            ProtoFieldType::scalar(name)
        },
        FieldType::Input(name) => ProtoFieldType::scalar(name),
        FieldType::List(inner) => {
            let inner_proto = field_type_to_proto(inner);
            ProtoFieldType::repeated(&inner_proto.type_name)
        },
        _ => ProtoFieldType::scalar("string"),
    }
}

/// Intermediate representation of a protobuf field type.
struct ProtoFieldType {
    type_name: String,
    repeated:  bool,
}

impl ProtoFieldType {
    fn scalar(name: &str) -> Self {
        Self {
            type_name: name.to_string(),
            repeated:  false,
        }
    }

    fn repeated(name: &str) -> Self {
        Self {
            type_name: name.to_string(),
            repeated:  true,
        }
    }
}

/// Generate a complete `.proto` file from a compiled schema.
///
/// Produces a proto3 service definition with:
/// - One message per GraphQL type (fields sorted alphabetically for stable numbering)
/// - One RPC per query (Get for single, List for list queries)
/// - One RPC per mutation (returns `MutationResponse`)
/// - Enum definitions from the schema
/// - Request/response wrapper messages
///
/// # Errors
///
/// Returns an error if the schema contains no types to expose.
pub fn generate_proto_file(
    schema: &CompiledSchema,
    package: &str,
    include_types: &[String],
    exclude_types: &[String],
) -> String {
    let mut out = String::new();
    let mut imports = BTreeSet::new();

    // Collect which types to expose
    let types: Vec<_> = schema
        .types
        .iter()
        .filter(|t| should_include_type(t.name.as_ref(), include_types, exclude_types))
        .collect();

    // Pre-scan for needed imports
    for td in &types {
        for field in &td.fields {
            let proto = field_type_to_proto(&field.field_type);
            if needs_well_known_import(&proto.type_name) {
                add_import_for_type(&proto.type_name, &mut imports);
            }
        }
    }
    // Scan query/mutation arguments too
    for q in &schema.queries {
        for arg in &q.arguments {
            let proto = field_type_to_proto(&arg.arg_type);
            if needs_well_known_import(&proto.type_name) {
                add_import_for_type(&proto.type_name, &mut imports);
            }
        }
    }

    // Header
    out.push_str("syntax = \"proto3\";\n\n");
    out.push_str(&format!("package {package};\n\n"));

    // Imports
    for imp in &imports {
        out.push_str(&format!("import \"{imp}\";\n"));
    }
    if !imports.is_empty() {
        out.push('\n');
    }

    // Enum definitions
    for enum_def in &schema.enums {
        generate_enum(&mut out, &enum_def.name, &enum_def.values);
    }

    // Type messages
    for td in &types {
        generate_message(&mut out, td.name.as_ref(), &td.fields);
    }

    // MutationResponse message (if any mutations exist)
    if !schema.mutations.is_empty() {
        out.push_str("message MutationResponse {\n");
        out.push_str("  bool success = 1;\n");
        out.push_str("  optional string id = 2;\n");
        out.push_str("  optional string error = 3;\n");
        out.push_str("}\n\n");
    }

    // Request/response messages for queries
    for q in &schema.queries {
        if !types.iter().any(|t| t.name == q.return_type) {
            continue;
        }
        generate_query_messages(&mut out, q);
    }

    // Request messages for mutations
    for m in &schema.mutations {
        generate_mutation_request_message(&mut out, m);
    }

    // Service definition
    let service_name = package_to_service(package);
    out.push_str(&format!("service {service_name} {{\n"));

    for q in &schema.queries {
        if !types.iter().any(|t| t.name == q.return_type) {
            continue;
        }
        let rpc_name = to_pascal_case(&q.name);
        let req = format!("{rpc_name}Request");
        if q.returns_list {
            // Server-streaming RPC: each response frame is a single entity message.
            out.push_str(&format!("  rpc {rpc_name}({req}) returns (stream {});\n", q.return_type));
        } else {
            out.push_str(&format!("  rpc {rpc_name}({req}) returns ({});\n", q.return_type));
        }
    }

    for m in &schema.mutations {
        let rpc_name = to_pascal_case(&m.name);
        let req = format!("{rpc_name}Request");
        out.push_str(&format!("  rpc {rpc_name}({req}) returns (MutationResponse);\n"));
    }

    out.push_str("}\n");

    out
}

/// Generate a protobuf message from a type's fields.
///
/// Fields are sorted alphabetically for deterministic field numbering.
fn generate_message(out: &mut String, name: &str, fields: &[FieldDefinition]) {
    out.push_str(&format!("message {name} {{\n"));

    let mut sorted_fields: Vec<&FieldDefinition> = fields.iter().collect();
    sorted_fields.sort_by(|a, b| a.name.as_ref().cmp(b.name.as_ref()));

    for (i, field) in sorted_fields.iter().enumerate() {
        let proto = field_type_to_proto(&field.field_type);
        let field_num = i + 1;
        let optional = if field.nullable && !proto.repeated {
            "optional "
        } else {
            ""
        };
        let repeated = if proto.repeated { "repeated " } else { "" };
        out.push_str(&format!(
            "  {optional}{repeated}{} {} = {field_num};\n",
            proto.type_name, field.name
        ));
    }

    out.push_str("}\n\n");
}

/// Generate a protobuf enum definition.
fn generate_enum(
    out: &mut String,
    name: &str,
    values: &[fraiseql_core::schema::EnumValueDefinition],
) {
    out.push_str(&format!("enum {name} {{\n"));
    out.push_str(&format!("  {}_UNSPECIFIED = 0;\n", to_screaming_snake(name)));

    for (i, val) in values.iter().enumerate() {
        out.push_str(&format!("  {} = {};\n", val.name, i + 1));
    }

    out.push_str("}\n\n");
}

/// Generate request/response messages for a query.
fn generate_query_messages(out: &mut String, q: &fraiseql_core::schema::QueryDefinition) {
    let rpc_name = to_pascal_case(&q.name);

    // Request message
    out.push_str(&format!("message {rpc_name}Request {{\n"));

    let mut sorted_args: Vec<_> = q.arguments.iter().collect();
    sorted_args.sort_by(|a, b| a.name.cmp(&b.name));

    for (i, arg) in sorted_args.iter().enumerate() {
        let proto = field_type_to_proto(&arg.arg_type);
        let optional = if arg.nullable && !proto.repeated {
            "optional "
        } else {
            ""
        };
        let repeated = if proto.repeated { "repeated " } else { "" };
        out.push_str(&format!(
            "  {optional}{repeated}{} {} = {};\n",
            proto.type_name,
            arg.name,
            i + 1,
        ));
    }

    // Add standard pagination fields for list queries
    if q.returns_list {
        let next_num = sorted_args.len() + 1;
        out.push_str(&format!("  optional int32 limit = {next_num};\n"));
        out.push_str(&format!("  optional int32 offset = {};\n", next_num + 1));
    }

    out.push_str("}\n\n");

    // Note: list queries use server-streaming RPCs and do not need a
    // response wrapper message — each streamed frame is the entity type directly.
}

/// Generate a request message for a mutation.
fn generate_mutation_request_message(
    out: &mut String,
    m: &fraiseql_core::schema::MutationDefinition,
) {
    let rpc_name = to_pascal_case(&m.name);

    out.push_str(&format!("message {rpc_name}Request {{\n"));

    let mut sorted_args: Vec<_> = m.arguments.iter().collect();
    sorted_args.sort_by(|a, b| a.name.cmp(&b.name));

    for (i, arg) in sorted_args.iter().enumerate() {
        let proto = field_type_to_proto(&arg.arg_type);
        let optional = if arg.nullable && !proto.repeated {
            "optional "
        } else {
            ""
        };
        let repeated = if proto.repeated { "repeated " } else { "" };
        out.push_str(&format!(
            "  {optional}{repeated}{} {} = {};\n",
            proto.type_name,
            arg.name,
            i + 1,
        ));
    }

    out.push_str("}\n\n");
}

/// Check if a type should be included based on include/exclude lists.
fn should_include_type(name: &str, include_types: &[String], exclude_types: &[String]) -> bool {
    if !include_types.is_empty() && !include_types.iter().any(|t| t == name) {
        return false;
    }
    !exclude_types.iter().any(|t| t == name)
}

/// Convert a snake_case or camelCase name to PascalCase.
fn to_pascal_case(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let mut s = c.to_uppercase().to_string();
                    s.push_str(&chars.collect::<String>());
                    s
                },
                None => String::new(),
            }
        })
        .collect()
}

/// Convert a PascalCase name to SCREAMING_SNAKE_CASE.
fn to_screaming_snake(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_uppercase());
    }
    result
}

/// Extract service name from package (e.g., "fraiseql.v1" → "FraiseQLService").
fn package_to_service(package: &str) -> String {
    let parts: Vec<&str> = package.split('.').collect();
    let base = parts.first().copied().unwrap_or("FraiseQL");
    let mut service = to_pascal_case(base);
    service.push_str("Service");
    service
}

/// Add the import path for a well-known protobuf type.
fn add_import_for_type(proto_type: &str, imports: &mut BTreeSet<String>) {
    match proto_type {
        "google.protobuf.Timestamp" => {
            imports.insert("google/protobuf/timestamp.proto".to_string());
        },
        "google.protobuf.Struct" => {
            imports.insert("google/protobuf/struct.proto".to_string());
        },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use fraiseql_core::schema::{
        CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDenyPolicy, FieldType,
        TypeDefinition,
    };

    use super::*;

    fn make_field(name: &str, ft: FieldType, nullable: bool) -> FieldDefinition {
        FieldDefinition {
            name: name.into(),
            field_type: ft,
            nullable,
            description: None,
            default_value: None,
            vector_config: None,
            alias: None,
            deprecation: None,
            requires_scope: None,
            on_deny: FieldDenyPolicy::default(),
            encryption: None,
        }
    }

    fn make_type(name: &str, fields: Vec<FieldDefinition>) -> TypeDefinition {
        TypeDefinition {
            name: name.into(),
            sql_source: String::new().into(),
            jsonb_column: "data".to_string(),
            fields,
            description: None,
            sql_projection_hint: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
            relationships: Vec::new(),
        }
    }

    /// Build a query via JSON deserialization to leverage `#[serde(default)]`.
    fn make_query(
        name: &str,
        return_type: &str,
        returns_list: bool,
    ) -> fraiseql_core::schema::QueryDefinition {
        let json = serde_json::json!({
            "name": name,
            "return_type": return_type,
            "returns_list": returns_list,
        });
        serde_json::from_value(json).expect("test query definition")
    }

    /// Build a mutation via JSON deserialization.
    fn make_mutation(
        name: &str,
        args: Vec<fraiseql_core::schema::ArgumentDefinition>,
    ) -> fraiseql_core::schema::MutationDefinition {
        let mut m: fraiseql_core::schema::MutationDefinition =
            serde_json::from_value(serde_json::json!({
                "name": name,
                "return_type": "MutationResponse",
            }))
            .expect("test mutation definition");
        m.arguments = args;
        m
    }

    fn make_arg(
        name: &str,
        ft: FieldType,
        nullable: bool,
    ) -> fraiseql_core::schema::ArgumentDefinition {
        fraiseql_core::schema::ArgumentDefinition {
            name: name.to_string(),
            arg_type: ft,
            nullable,
            default_value: None,
            description: None,
            deprecation: None,
        }
    }

    // ── graphql_to_proto_type ───────────────────────────────────────────

    #[test]
    fn test_proto_type_string() {
        assert_eq!(graphql_to_proto_type("String"), "string");
    }

    #[test]
    fn test_proto_type_int() {
        assert_eq!(graphql_to_proto_type("Int"), "int32");
    }

    #[test]
    fn test_proto_type_float() {
        assert_eq!(graphql_to_proto_type("Float"), "double");
    }

    #[test]
    fn test_proto_type_boolean() {
        assert_eq!(graphql_to_proto_type("Boolean"), "bool");
    }

    #[test]
    fn test_proto_type_id() {
        assert_eq!(graphql_to_proto_type("ID"), "string");
    }

    #[test]
    fn test_proto_type_datetime() {
        assert_eq!(graphql_to_proto_type("DateTime"), "google.protobuf.Timestamp");
    }

    #[test]
    fn test_proto_type_date() {
        assert_eq!(graphql_to_proto_type("Date"), "string");
    }

    #[test]
    fn test_proto_type_bigint() {
        assert_eq!(graphql_to_proto_type("BigInt"), "int64");
    }

    #[test]
    fn test_proto_type_json() {
        assert_eq!(graphql_to_proto_type("JSON"), "google.protobuf.Struct");
    }

    #[test]
    fn test_proto_type_custom_scalar_fallback() {
        assert_eq!(graphql_to_proto_type("Email"), "string");
        assert_eq!(graphql_to_proto_type("PhoneNumber"), "string");
    }

    // ── graphql_to_row_view_type ────────────────────────────────────────

    #[test]
    fn test_row_view_type_string() {
        assert_eq!(graphql_to_row_view_type("String"), RowViewColumnType::Text);
    }

    #[test]
    fn test_row_view_type_int() {
        assert_eq!(graphql_to_row_view_type("Int"), RowViewColumnType::Int32);
    }

    #[test]
    fn test_row_view_type_bigint() {
        assert_eq!(graphql_to_row_view_type("BigInt"), RowViewColumnType::Int64);
    }

    #[test]
    fn test_row_view_type_float() {
        assert_eq!(graphql_to_row_view_type("Float"), RowViewColumnType::Float64);
    }

    #[test]
    fn test_row_view_type_boolean() {
        assert_eq!(graphql_to_row_view_type("Boolean"), RowViewColumnType::Boolean);
    }

    #[test]
    fn test_row_view_type_id() {
        assert_eq!(graphql_to_row_view_type("ID"), RowViewColumnType::Uuid);
    }

    #[test]
    fn test_row_view_type_datetime() {
        assert_eq!(graphql_to_row_view_type("DateTime"), RowViewColumnType::Timestamptz);
    }

    #[test]
    fn test_row_view_type_json() {
        assert_eq!(graphql_to_row_view_type("JSON"), RowViewColumnType::Json);
    }

    #[test]
    fn test_row_view_type_date() {
        assert_eq!(graphql_to_row_view_type("Date"), RowViewColumnType::Date);
    }

    #[test]
    fn test_row_view_type_custom_scalar_fallback() {
        assert_eq!(graphql_to_row_view_type("Email"), RowViewColumnType::Text);
    }

    // ── needs_well_known_import ─────────────────────────────────────────

    #[test]
    fn test_needs_import_timestamp() {
        assert!(needs_well_known_import("google.protobuf.Timestamp"));
    }

    #[test]
    fn test_needs_import_struct() {
        assert!(needs_well_known_import("google.protobuf.Struct"));
    }

    #[test]
    fn test_no_import_for_scalars() {
        assert!(!needs_well_known_import("string"));
        assert!(!needs_well_known_import("int32"));
        assert!(!needs_well_known_import("bool"));
    }

    // ── to_pascal_case ──────────────────────────────────────────────────

    #[test]
    fn test_pascal_case_snake() {
        assert_eq!(to_pascal_case("get_user"), "GetUser");
    }

    #[test]
    fn test_pascal_case_single() {
        assert_eq!(to_pascal_case("users"), "Users");
    }

    #[test]
    fn test_pascal_case_already() {
        assert_eq!(to_pascal_case("User"), "User");
    }

    // ── to_screaming_snake ──────────────────────────────────────────────

    #[test]
    fn test_screaming_snake() {
        assert_eq!(to_screaming_snake("OrderStatus"), "ORDER_STATUS");
    }

    // ── should_include_type ─────────────────────────────────────────────

    #[test]
    fn test_include_all_when_empty() {
        assert!(should_include_type("User", &[], &[]));
    }

    #[test]
    fn test_include_whitelist() {
        assert!(should_include_type("User", &["User".to_string()], &[]));
        assert!(!should_include_type("Post", &["User".to_string()], &[]));
    }

    #[test]
    fn test_exclude_blacklist() {
        assert!(!should_include_type("Secret", &[], &["Secret".to_string()]));
        assert!(should_include_type("User", &[], &["Secret".to_string()]));
    }

    // ── generate_proto_file ─────────────────────────────────────────────

    #[test]
    fn test_generate_proto_basic_type() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("name", FieldType::String, false),
                make_field("email", FieldType::String, true),
            ],
        ));
        schema.queries.push(make_query("get_user", "User", false));
        schema.queries.push(make_query("list_users", "User", true));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("syntax = \"proto3\";"));
        assert!(proto.contains("package fraiseql.v1;"));
        assert!(proto.contains("message User {"));
        // Fields sorted alphabetically: email=1, id=2, name=3
        assert!(proto.contains("optional string email = 1;"));
        assert!(proto.contains("string id = 2;"));
        assert!(proto.contains("string name = 3;"));
        // Service
        assert!(proto.contains("service FraiseqlService {"));
        assert!(proto.contains("rpc GetUser(GetUserRequest) returns (User);"));
        assert!(proto.contains("rpc ListUsers(ListUsersRequest) returns (stream User);"));
    }

    #[test]
    fn test_generate_proto_with_datetime_import() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "Post",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("created_at", FieldType::DateTime, false),
            ],
        ));
        schema.queries.push(make_query("get_post", "Post", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("import \"google/protobuf/timestamp.proto\";"));
        assert!(proto.contains("google.protobuf.Timestamp created_at = 1;"));
    }

    #[test]
    fn test_generate_proto_with_mutations() {
        let mut schema = CompiledSchema::new();
        schema
            .types
            .push(make_type("User", vec![make_field("id", FieldType::Id, false)]));
        schema.mutations.push(make_mutation(
            "create_user",
            vec![
                make_arg("name", FieldType::String, false),
                make_arg("email", FieldType::String, false),
            ],
        ));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("message MutationResponse {"));
        assert!(proto.contains("message CreateUserRequest {"));
        // Args sorted: email=1, name=2
        assert!(proto.contains("string email = 1;"));
        assert!(proto.contains("string name = 2;"));
        assert!(proto.contains("rpc CreateUser(CreateUserRequest) returns (MutationResponse);"));
    }

    #[test]
    fn test_generate_proto_with_enum() {
        let mut schema = CompiledSchema::new();
        schema.enums.push(EnumDefinition {
            name:        "OrderStatus".to_string(),
            values:      vec![
                EnumValueDefinition {
                    name:        "PENDING".to_string(),
                    description: None,
                    deprecation: None,
                },
                EnumValueDefinition {
                    name:        "SHIPPED".to_string(),
                    description: None,
                    deprecation: None,
                },
            ],
            description: None,
        });
        schema.types.push(make_type(
            "Order",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("status", FieldType::Enum("OrderStatus".to_string()), false),
            ],
        ));
        schema.queries.push(make_query("get_order", "Order", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("enum OrderStatus {"));
        assert!(proto.contains("ORDER_STATUS_UNSPECIFIED = 0;"));
        assert!(proto.contains("PENDING = 1;"));
        assert!(proto.contains("SHIPPED = 2;"));
        assert!(proto.contains("OrderStatus status = 2;"));
    }

    #[test]
    fn test_generate_proto_exclude_types() {
        let mut schema = CompiledSchema::new();
        schema
            .types
            .push(make_type("User", vec![make_field("id", FieldType::Id, false)]));
        schema
            .types
            .push(make_type("Secret", vec![make_field("id", FieldType::Id, false)]));
        schema.queries.push(make_query("get_user", "User", false));
        schema.queries.push(make_query("get_secret", "Secret", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &["Secret".to_string()]);

        assert!(proto.contains("message User {"));
        assert!(!proto.contains("message Secret {"));
        assert!(proto.contains("rpc GetUser"));
        assert!(!proto.contains("rpc GetSecret"));
    }

    #[test]
    fn test_generate_proto_list_query_pagination() {
        let mut schema = CompiledSchema::new();
        schema
            .types
            .push(make_type("User", vec![make_field("id", FieldType::Id, false)]));
        schema.queries.push(make_query("list_users", "User", true));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        // Pagination fields added to list request
        assert!(proto.contains("optional int32 limit = 1;"));
        assert!(proto.contains("optional int32 offset = 2;"));
        // Server-streaming: no ListUsersResponse wrapper, returns stream User
        assert!(proto.contains("rpc ListUsers(ListUsersRequest) returns (stream User);"));
        assert!(!proto.contains("ListUsersResponse"), "No response wrapper for streaming RPCs");
    }

    #[test]
    fn test_generate_proto_nullable_field() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![
                make_field("name", FieldType::String, false),
                make_field("bio", FieldType::String, true),
            ],
        ));
        schema.queries.push(make_query("get_user", "User", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("optional string bio = 1;"));
        assert!(proto.contains("string name = 2;"));
    }

    #[test]
    fn test_generate_proto_list_field() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![make_field(
                "tags",
                FieldType::List(Box::new(FieldType::String)),
                false,
            )],
        ));
        schema.queries.push(make_query("get_user", "User", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("repeated string tags = 1;"));
    }
}
