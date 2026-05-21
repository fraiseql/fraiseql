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
pub(crate) fn should_include_type(
    name: &str,
    include_types: &[String],
    exclude_types: &[String],
) -> bool {
    if !include_types.is_empty() && !include_types.iter().any(|t| t == name) {
        return false;
    }
    !exclude_types.iter().any(|t| t == name)
}

/// Convert a snake_case or camelCase name to PascalCase.
pub(crate) fn to_pascal_case(name: &str) -> String {
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
pub(crate) fn to_screaming_snake(name: &str) -> String {
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
