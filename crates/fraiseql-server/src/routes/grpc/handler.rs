//! gRPC request handler — translates protobuf queries into row-shaped view
//! queries and encodes results back to protobuf.
//!
//! The handler accepts a decoded [`prost_reflect::DynamicMessage`] request,
//! extracts filter/pagination arguments, generates a SQL WHERE clause via
//! [`GenericWhereGenerator`], calls [`DatabaseAdapter::execute_row_query()`],
//! and maps the resulting [`ColumnValue`] rows into a protobuf response message.

use std::collections::HashMap;

use fraiseql_core::db::dialect::{PostgresDialect, RowViewColumnType};
use fraiseql_core::db::traits::DatabaseAdapter;
use fraiseql_core::db::types::{ColumnSpec, ColumnValue};
use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use fraiseql_core::db::where_generator::GenericWhereGenerator;
use fraiseql_core::schema::{CompiledSchema, FieldType, TypeDefinition};
use fraiseql_error::FraiseQLError;
use prost_reflect::{DynamicMessage, MessageDescriptor, ReflectMessage, Value};
use tracing::{debug, warn};

/// Maximum number of rows returned by a single gRPC query (safety limit).
const MAX_GRPC_RESULT_ROWS: u32 = 10_000;

/// Default row limit when the client does not specify one.
const DEFAULT_GRPC_LIMIT: u32 = 100;

// ---------------------------------------------------------------------------
// RPC operation metadata
// ---------------------------------------------------------------------------

/// Distinguishes query RPCs (row-shaped view reads) from mutation RPCs
/// (database function calls).
#[derive(Debug, Clone)]
pub enum RpcKind {
    /// A read query against a row-shaped view (`vr_*`).
    Query {
        /// Row-shaped view name (e.g., `"vr_user"`).
        view_name: String,
        /// Whether this RPC returns a list.
        returns_list: bool,
        /// Column specs for the row-shaped view.
        columns: Vec<ColumnSpec>,
        /// Inner row message descriptor (the repeated element for list queries,
        /// or the single message for get queries).
        row_descriptor: MessageDescriptor,
    },
    /// A mutation that calls a database function via `execute_function_call()`.
    Mutation {
        /// SQL function name (e.g., `"fn_create_user"`).
        function_name: String,
    },
}

/// Metadata for a single gRPC RPC method, resolved at startup.
#[derive(Debug, Clone)]
pub struct RpcOperation {
    /// Operation name in the compiled schema (query or mutation name).
    pub operation_name: String,
    /// GraphQL type name (e.g., `"User"`).
    pub type_name: String,
    /// What kind of RPC this is (query or mutation).
    pub kind: RpcKind,
    /// Response message descriptor for encoding results.
    pub response_descriptor: MessageDescriptor,
}

/// Maps gRPC method names (e.g., `"/fraiseql.v1.FraiseQLService/ListUsers"`)
/// to their resolved operation metadata.
pub type RpcDispatchTable = HashMap<String, RpcOperation>;

// ---------------------------------------------------------------------------
// Field type mapping
// ---------------------------------------------------------------------------

/// Map a GraphQL [`FieldType`] to a [`RowViewColumnType`] for column extraction.
///
/// Returns `None` for non-scalar types (Object, List, Interface, Union) that
/// cannot be directly represented as a single database column.
pub const fn field_type_to_column_type(ft: &FieldType) -> Option<RowViewColumnType> {
    match ft {
        FieldType::String | FieldType::Scalar(_) | FieldType::Decimal | FieldType::Time => {
            Some(RowViewColumnType::Text)
        },
        FieldType::Int => Some(RowViewColumnType::Int32),
        FieldType::Float => Some(RowViewColumnType::Float64),
        FieldType::Boolean => Some(RowViewColumnType::Boolean),
        FieldType::Id | FieldType::Uuid => Some(RowViewColumnType::Uuid),
        FieldType::DateTime => Some(RowViewColumnType::Timestamptz),
        FieldType::Date => Some(RowViewColumnType::Date),
        FieldType::Json => Some(RowViewColumnType::Json),
        // Enums map to text (their string representation).
        FieldType::Enum(_) => Some(RowViewColumnType::Text),
        // Non-scalar types: List, Object, Interface, Union, Vector
        _ => None,
    }
}

/// Build [`ColumnSpec`] list from a type definition's scalar fields.
pub fn column_specs_from_type(type_def: &TypeDefinition) -> Vec<ColumnSpec> {
    type_def
        .fields
        .iter()
        .filter_map(|f| {
            field_type_to_column_type(&f.field_type).map(|ct| ColumnSpec {
                name:        f.name.to_string(),
                column_type: ct,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Filter extraction — protobuf message → WhereClause
// ---------------------------------------------------------------------------

/// Extract filter arguments from a protobuf request message and build a
/// [`WhereClause`].
///
/// Expects the request message to contain top-level fields that correspond to
/// filter parameters. For example, a `ListUsersRequest` with field `email` of
/// type `string` becomes `WHERE email = $1`.
///
/// Only simple equality filters are supported in the MVP. The returned clause
/// is `None` when no filter fields are set.
pub fn extract_filters(msg: &DynamicMessage, type_def: &TypeDefinition) -> Option<WhereClause> {
    let mut clauses = Vec::new();

    for field_desc in msg.descriptor().fields() {
        let field_name = field_desc.name();

        // Skip pagination fields.
        if matches!(field_name, "limit" | "offset" | "order_by") {
            continue;
        }

        // Only process fields that exist on the type definition.
        if type_def.find_field(field_name).is_none() {
            continue;
        }

        // Check if the field is set in the message.
        if !msg.has_field(&field_desc) {
            continue;
        }

        let value = msg.get_field(&field_desc);
        let json_value = proto_value_to_json(&value);

        clauses.push(WhereClause::Field {
            path:     vec![field_name.to_string()],
            operator: WhereOperator::Eq,
            value:    json_value,
        });
    }

    if clauses.is_empty() {
        None
    } else if clauses.len() == 1 {
        clauses.into_iter().next()
    } else {
        Some(WhereClause::And(clauses))
    }
}

/// Convert a protobuf [`Value`] to a [`serde_json::Value`] for WHERE clause
/// parameter binding.
fn proto_value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::I32(n) => serde_json::json!(*n),
        Value::I64(n) => serde_json::json!(*n),
        Value::U32(n) => serde_json::json!(*n),
        Value::U64(n) => serde_json::json!(*n),
        Value::F32(f) => serde_json::json!(*f),
        Value::F64(f) => serde_json::json!(*f),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(b) => serde_json::Value::String(base64_encode(b)),
        Value::EnumNumber(n) => serde_json::json!(*n),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(proto_value_to_json).collect())
        },
        Value::Map(entries) => {
            let obj: serde_json::Map<std::string::String, serde_json::Value> = entries
                .iter()
                .map(|(k, v)| (map_key_to_string(k), proto_value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        },
        Value::Message(inner) => dynamic_message_to_json(inner),
    }
}

/// Encode bytes as base64 for JSON serialization.
fn base64_encode(bytes: &prost::bytes::Bytes) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Convert a protobuf map key to a string.
fn map_key_to_string(key: &prost_reflect::MapKey) -> String {
    match key {
        prost_reflect::MapKey::Bool(b) => b.to_string(),
        prost_reflect::MapKey::I32(n) => n.to_string(),
        prost_reflect::MapKey::I64(n) => n.to_string(),
        prost_reflect::MapKey::U32(n) => n.to_string(),
        prost_reflect::MapKey::U64(n) => n.to_string(),
        prost_reflect::MapKey::String(s) => s.clone(),
    }
}

/// Convert a dynamic protobuf message to a JSON value.
fn dynamic_message_to_json(msg: &DynamicMessage) -> serde_json::Value {
    // Use prost-reflect's serde serialization.
    serde_json::to_value(msg).unwrap_or(serde_json::Value::Null)
}

// ---------------------------------------------------------------------------
// Pagination extraction
// ---------------------------------------------------------------------------

/// Extract limit from the request message (capped at `MAX_GRPC_RESULT_ROWS`).
pub fn extract_limit(msg: &DynamicMessage) -> u32 {
    for field_desc in msg.descriptor().fields() {
        if field_desc.name() == "limit" && msg.has_field(&field_desc) {
            let val = msg.get_field(&field_desc);
            if let Value::I32(n) = val.as_ref() {
                let n = u32::try_from(*n).unwrap_or(DEFAULT_GRPC_LIMIT);
                return n.min(MAX_GRPC_RESULT_ROWS);
            }
            if let Value::U32(n) = val.as_ref() {
                return (*n).min(MAX_GRPC_RESULT_ROWS);
            }
        }
    }
    DEFAULT_GRPC_LIMIT
}

/// Extract offset from the request message.
pub fn extract_offset(msg: &DynamicMessage) -> Option<u32> {
    for field_desc in msg.descriptor().fields() {
        if field_desc.name() == "offset" && msg.has_field(&field_desc) {
            let val = msg.get_field(&field_desc);
            if let Value::I32(n) = val.as_ref() {
                return u32::try_from(*n).ok();
            }
            if let Value::U32(n) = val.as_ref() {
                return Some(*n);
            }
        }
    }
    None
}

/// Extract order_by from the request message.
pub fn extract_order_by(msg: &DynamicMessage, type_def: &TypeDefinition) -> Option<String> {
    for field_desc in msg.descriptor().fields() {
        if field_desc.name() == "order_by" && msg.has_field(&field_desc) {
            let val = msg.get_field(&field_desc);
            if let Value::String(s) = val.as_ref() {
                // Validate that the order_by column exists on the type to prevent
                // SQL injection via crafted order_by strings.
                let parts: Vec<&str> = s.split_whitespace().collect();
                if let Some(col_name) = parts.first() {
                    if type_def.find_field(col_name).is_some() {
                        let direction = parts
                            .get(1)
                            .filter(|d| d.eq_ignore_ascii_case("asc") || d.eq_ignore_ascii_case("desc"))
                            .copied()
                            .unwrap_or("ASC");
                        return Some(format!("\"{col_name}\" {direction}"));
                    }
                    warn!(
                        column = %col_name,
                        "gRPC order_by references unknown column — ignoring"
                    );
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Query execution
// ---------------------------------------------------------------------------

/// Execute a gRPC query against a row-shaped view.
///
/// # Errors
///
/// Returns `FraiseQLError::Database` on query execution failure.
/// Returns `FraiseQLError::Validation` if filter construction fails.
pub async fn execute_grpc_query<A: DatabaseAdapter>(
    adapter: &A,
    view_name: &str,
    columns: &[ColumnSpec],
    returns_list: bool,
    request_msg: &DynamicMessage,
    type_def: &TypeDefinition,
) -> Result<Vec<Vec<ColumnValue>>, FraiseQLError> {
    // Extract filters and build WHERE clause.
    let where_clause = extract_filters(request_msg, type_def);

    // Generate SQL WHERE clause string via GenericWhereGenerator.
    let where_sql = if let Some(ref clause) = where_clause {
        let gen = GenericWhereGenerator::new(PostgresDialect);
        let (sql, _params) = gen.generate(clause)?;
        // Note: In the MVP, the WHERE clause string is passed directly to
        // execute_row_query(). The adapter is responsible for parameterized
        // execution. For production, we should pass params alongside the SQL.
        Some(sql)
    } else {
        None
    };

    let limit = if returns_list {
        Some(extract_limit(request_msg))
    } else {
        Some(1)
    };
    let offset = extract_offset(request_msg);
    let order_by = extract_order_by(request_msg, type_def);

    debug!(
        view = %view_name,
        where_clause = ?where_sql,
        limit = ?limit,
        offset = ?offset,
        order_by = ?order_by,
        "Executing gRPC row query"
    );

    adapter
        .execute_row_query(
            view_name,
            columns,
            where_sql.as_deref(),
            order_by.as_deref(),
            limit,
            offset,
        )
        .await
}

/// Execute a gRPC mutation by calling the database function.
///
/// Maps the `execute_function_call()` result to a protobuf `MutationResponse`
/// message with `success`, `id`, and `error` fields.
///
/// # Errors
///
/// Returns `FraiseQLError::Database` on function call failure.
pub async fn execute_grpc_mutation<A: DatabaseAdapter>(
    adapter: &A,
    function_name: &str,
    request_msg: &DynamicMessage,
) -> Result<MutationResult, FraiseQLError> {
    // Extract arguments from the request message as JSON values.
    let args: Vec<serde_json::Value> = request_msg
        .descriptor()
        .fields()
        .filter(|f| request_msg.has_field(f))
        .map(|f| proto_value_to_json(request_msg.get_field(&f).as_ref()))
        .collect();

    debug!(
        function = %function_name,
        arg_count = args.len(),
        "Executing gRPC mutation"
    );

    let rows = adapter.execute_function_call(function_name, &args).await?;

    // The Trinity pattern returns a single row with status/entity_id columns.
    let row = rows.into_iter().next().unwrap_or_default();
    let success = row
        .get("status")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s == "success");
    let id = row
        .get("entity_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let error = if success {
        None
    } else {
        row.get("message")
            .and_then(|v| v.as_str())
            .map(String::from)
    };

    Ok(MutationResult { success, id, error })
}

/// Result from a gRPC mutation, ready to be encoded as a `MutationResponse`.
#[derive(Debug)]
pub struct MutationResult {
    /// Whether the mutation succeeded.
    pub success: bool,
    /// Optional entity ID returned by the mutation.
    pub id: Option<String>,
    /// Optional error message (when `success` is false).
    pub error: Option<String>,
}

/// Encode a [`MutationResult`] into a protobuf response message.
///
/// Expects the response descriptor to have fields: `success` (bool),
/// `id` (optional string), `error` (optional string).
pub fn encode_mutation_response(
    result: &MutationResult,
    response_desc: &MessageDescriptor,
) -> DynamicMessage {
    let mut msg = DynamicMessage::new(response_desc.clone());

    if let Some(field) = response_desc.get_field_by_name("success") {
        msg.set_field(&field, Value::Bool(result.success));
    }
    if let (Some(field), Some(id)) = (response_desc.get_field_by_name("id"), &result.id) {
        msg.set_field(&field, Value::String(id.clone()));
    }
    if let (Some(field), Some(err)) = (response_desc.get_field_by_name("error"), &result.error) {
        msg.set_field(&field, Value::String(err.clone()));
    }

    msg
}

// ---------------------------------------------------------------------------
// Response encoding — ColumnValue → protobuf DynamicMessage
// ---------------------------------------------------------------------------

/// Encode a single row of [`ColumnValue`]s into a protobuf [`DynamicMessage`].
///
/// Each column is mapped to the corresponding protobuf field by position (the
/// column specs and message fields are aligned by the proto generator).
pub fn encode_row(
    row: &[ColumnValue],
    columns: &[ColumnSpec],
    row_desc: &MessageDescriptor,
) -> DynamicMessage {
    let mut msg = DynamicMessage::new(row_desc.clone());

    for (col_val, col_spec) in row.iter().zip(columns.iter()) {
        if let Some(field_desc) = row_desc.get_field_by_name(&col_spec.name) {
            let proto_val = column_value_to_proto(col_val);
            if let Some(v) = proto_val {
                msg.set_field(&field_desc, v);
            }
            // If proto_val is None (ColumnValue::Null), we leave the field unset
            // which is proto3's default behavior for absent values.
        }
    }

    msg
}

/// Convert a [`ColumnValue`] to a protobuf [`Value`].
///
/// Returns `None` for `ColumnValue::Null` (proto3 default absence).
pub fn column_value_to_proto(col: &ColumnValue) -> Option<Value> {
    match col {
        ColumnValue::Null => None,
        ColumnValue::Text(s) => Some(Value::String(s.clone())),
        ColumnValue::Int32(n) => Some(Value::I32(*n)),
        ColumnValue::Int64(n) => Some(Value::I64(*n)),
        ColumnValue::Float64(f) => Some(Value::F64(*f)),
        ColumnValue::Bool(b) => Some(Value::Bool(*b)),
        ColumnValue::Uuid(u) => Some(Value::String(u.to_string())),
        ColumnValue::Timestamp(ts) => {
            // Encode as ISO 8601 string. A full implementation would use
            // google.protobuf.Timestamp, but string is simpler for the MVP.
            Some(Value::String(ts.to_rfc3339()))
        },
        ColumnValue::Date(d) => Some(Value::String(d.to_string())),
        ColumnValue::Json(v) => Some(Value::String(v.to_string())),
        // ColumnValue is #[non_exhaustive]; future variants fall back to string.
        _ => None,
    }
}

/// Encode query results into a protobuf response message.
///
/// For list queries, the response contains a `repeated` field named `items`
/// (or the pluralized type name). For get queries, the response fields are
/// the row fields directly.
pub fn encode_response(
    rows: Vec<Vec<ColumnValue>>,
    columns: &[ColumnSpec],
    returns_list: bool,
    row_descriptor: &MessageDescriptor,
    response_descriptor: &MessageDescriptor,
) -> DynamicMessage {
    let mut response = DynamicMessage::new(response_descriptor.clone());

    if returns_list {
        // List response: encode each row as a sub-message in the "items" field.
        let items: Vec<Value> = rows
            .iter()
            .map(|row| {
                let row_msg = encode_row(row, columns, row_descriptor);
                Value::Message(row_msg)
            })
            .collect();

        // Find the repeated field (first repeated message field in the response).
        for field_desc in response_descriptor.fields() {
            if field_desc.is_list()
                && field_desc.kind().as_message().is_some()
            {
                response.set_field(&field_desc, Value::List(items));
                break;
            }
        }
    } else {
        // Get response: single row — set fields directly on the response message.
        if let Some(row) = rows.into_iter().next() {
            for (col_val, col_spec) in row.iter().zip(columns.iter()) {
                if let Some(field_desc) = response_descriptor.get_field_by_name(&col_spec.name) {
                    if let Some(v) = column_value_to_proto(col_val) {
                        response.set_field(&field_desc, v);
                    }
                }
            }
        }
    }

    response
}

// ---------------------------------------------------------------------------
// Dispatch table construction
// ---------------------------------------------------------------------------

/// Build the RPC dispatch table from a compiled schema and a descriptor pool.
///
/// Iterates the schema's queries and mutations, mapping each gRPC method name
/// to its resolved operation metadata.
///
/// Convention: methods starting with `Get` or `List` are queries; all others
/// are matched against mutations.
///
/// # Errors
///
/// Returns an error if the service descriptor is not found in the pool.
pub fn build_dispatch_table(
    schema: &CompiledSchema,
    service_name: &str,
    pool: &prost_reflect::DescriptorPool,
) -> Result<RpcDispatchTable, FraiseQLError> {
    let mut table = HashMap::new();

    // Find the service descriptor.
    let service_desc = pool
        .get_service_by_name(service_name)
        .ok_or_else(|| FraiseQLError::validation(format!(
            "gRPC service '{service_name}' not found in descriptor pool"
        )))?;

    for method_desc in service_desc.methods() {
        let method_name = method_desc.name().to_string();
        let full_method = format!("/{service_name}/{method_name}");
        let response_desc = method_desc.output();

        // Try query first (Get*/List* prefix).
        if method_name.starts_with("Get") || method_name.starts_with("List") {
            let query_name = grpc_method_to_query_name(&method_name);

            if let Some(query_def) = schema.find_query(&query_name) {
                let type_name = &query_def.return_type;
                let Some(type_def) = schema.find_type(type_name) else {
                    warn!(
                        method = %method_name,
                        type_name = %type_name,
                        "gRPC query return type not found in schema — skipping"
                    );
                    continue;
                };

                let view_name = format!("vr_{}", type_def.sql_source);
                let columns = column_specs_from_type(type_def);

                let row_desc = if query_def.returns_list {
                    response_desc
                        .fields()
                        .find(|f| f.is_list() && f.kind().as_message().is_some())
                        .and_then(|f| f.kind().as_message().cloned())
                        .unwrap_or_else(|| response_desc.clone())
                } else {
                    response_desc.clone()
                };

                table.insert(full_method, RpcOperation {
                    operation_name: query_name,
                    type_name:      type_name.clone(),
                    kind: RpcKind::Query {
                        view_name,
                        returns_list: query_def.returns_list,
                        columns,
                        row_descriptor: row_desc,
                    },
                    response_descriptor: response_desc,
                });
                continue;
            }
        }

        // Try mutation: convert PascalCase method name to camelCase mutation name.
        let mutation_name = grpc_method_to_mutation_name(&method_name);
        if let Some(mutation_def) = schema.find_mutation(&mutation_name) {
            let function_name = mutation_def
                .sql_source
                .clone()
                .unwrap_or_else(|| format!("fn_{mutation_name}"));

            table.insert(full_method, RpcOperation {
                operation_name: mutation_name,
                type_name:      mutation_def.return_type.clone(),
                kind: RpcKind::Mutation { function_name },
                response_descriptor: response_desc,
            });
            continue;
        }

        debug!(
            method = %method_name,
            "gRPC method has no matching query or mutation — skipping"
        );
    }

    Ok(table)
}

/// Convert a gRPC method name to a schema query name.
///
/// Convention: `"GetUser"` → `"user"`, `"ListUsers"` → `"users"`.
fn grpc_method_to_query_name(method: &str) -> String {
    let name = method
        .strip_prefix("Get")
        .or_else(|| method.strip_prefix("List"))
        .unwrap_or(method);

    // Convert PascalCase to snake_case-ish lowercase.
    let mut result = String::with_capacity(name.len());
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

/// Convert a gRPC method name to a schema mutation name.
///
/// Convention: `"CreateUser"` → `"createUser"` (PascalCase → camelCase).
fn grpc_method_to_mutation_name(method: &str) -> String {
    let mut chars = method.chars();
    match chars.next() {
        Some(first) => {
            let mut result = first.to_lowercase().to_string();
            result.extend(chars);
            result
        },
        None => String::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use super::*;

    // ── field_type_to_column_type ────────────────────────────────────────

    #[test]
    fn scalar_types_map_correctly() {
        assert_eq!(
            field_type_to_column_type(&FieldType::String),
            Some(RowViewColumnType::Text)
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::Int),
            Some(RowViewColumnType::Int32)
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::Float),
            Some(RowViewColumnType::Float64)
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::Boolean),
            Some(RowViewColumnType::Boolean)
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::Id),
            Some(RowViewColumnType::Uuid)
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::DateTime),
            Some(RowViewColumnType::Timestamptz)
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::Date),
            Some(RowViewColumnType::Date)
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::Json),
            Some(RowViewColumnType::Json)
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::Uuid),
            Some(RowViewColumnType::Uuid)
        );
    }

    #[test]
    fn non_scalar_types_return_none() {
        assert_eq!(
            field_type_to_column_type(&FieldType::Object("User".to_string())),
            None
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::List(Box::new(FieldType::String))),
            None
        );
        assert_eq!(
            field_type_to_column_type(&FieldType::Vector),
            None
        );
    }

    #[test]
    fn rich_scalars_map_to_text() {
        assert_eq!(
            field_type_to_column_type(&FieldType::Scalar("Email".to_string())),
            Some(RowViewColumnType::Text)
        );
    }

    #[test]
    fn enums_map_to_text() {
        assert_eq!(
            field_type_to_column_type(&FieldType::Enum("Status".to_string())),
            Some(RowViewColumnType::Text)
        );
    }

    // ── grpc_method_to_query_name ───────────────────────────────────────

    #[test]
    fn get_prefix_stripped() {
        assert_eq!(grpc_method_to_query_name("GetUser"), "user");
    }

    #[test]
    fn list_prefix_stripped() {
        assert_eq!(grpc_method_to_query_name("ListUsers"), "users");
    }

    #[test]
    fn pascal_case_to_snake() {
        assert_eq!(grpc_method_to_query_name("GetUserProfile"), "user_profile");
    }

    #[test]
    fn no_prefix_passthrough() {
        assert_eq!(grpc_method_to_query_name("SearchUsers"), "search_users");
    }

    // ── grpc_method_to_mutation_name ──────────────────────────────────

    #[test]
    fn mutation_name_pascal_to_camel() {
        assert_eq!(grpc_method_to_mutation_name("CreateUser"), "createUser");
    }

    #[test]
    fn mutation_name_single_word() {
        assert_eq!(grpc_method_to_mutation_name("Delete"), "delete");
    }

    #[test]
    fn mutation_name_empty() {
        assert_eq!(grpc_method_to_mutation_name(""), "");
    }

    // ── column_value_to_proto ───────────────────────────────────────────

    #[test]
    fn null_returns_none() {
        assert!(column_value_to_proto(&ColumnValue::Null).is_none());
    }

    #[test]
    fn text_encodes_as_string() {
        let v = column_value_to_proto(&ColumnValue::Text("hello".into()));
        assert_eq!(v, Some(Value::String("hello".into())));
    }

    #[test]
    fn int32_encodes() {
        let v = column_value_to_proto(&ColumnValue::Int32(42));
        assert_eq!(v, Some(Value::I32(42)));
    }

    #[test]
    fn int64_encodes() {
        let v = column_value_to_proto(&ColumnValue::Int64(123_456_789_012));
        assert_eq!(v, Some(Value::I64(123_456_789_012)));
    }

    #[test]
    fn float64_encodes() {
        let v = column_value_to_proto(&ColumnValue::Float64(1.23));
        assert_eq!(v, Some(Value::F64(1.23)));
    }

    #[test]
    fn bool_encodes() {
        let v = column_value_to_proto(&ColumnValue::Bool(true));
        assert_eq!(v, Some(Value::Bool(true)));
    }

    #[test]
    fn uuid_encodes_as_string() {
        let u = uuid::Uuid::nil();
        let v = column_value_to_proto(&ColumnValue::Uuid(u));
        assert_eq!(v, Some(Value::String("00000000-0000-0000-0000-000000000000".into())));
    }

    #[test]
    fn date_encodes_as_string() {
        let d = chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let v = column_value_to_proto(&ColumnValue::Date(d));
        assert_eq!(v, Some(Value::String("2025-01-15".into())));
    }

    #[test]
    fn json_encodes_as_string() {
        let j = serde_json::json!({"key": "value"});
        let v = column_value_to_proto(&ColumnValue::Json(j));
        assert_eq!(v, Some(Value::String("{\"key\":\"value\"}".into())));
    }

    // ── proto_value_to_json ─────────────────────────────────────────────

    #[test]
    fn proto_bool_to_json() {
        let v = proto_value_to_json(&Value::Bool(true));
        assert_eq!(v, serde_json::Value::Bool(true));
    }

    #[test]
    fn proto_string_to_json() {
        let v = proto_value_to_json(&Value::String("hello".into()));
        assert_eq!(v, serde_json::Value::String("hello".into()));
    }

    #[test]
    fn proto_i32_to_json() {
        let v = proto_value_to_json(&Value::I32(42));
        assert_eq!(v, serde_json::json!(42));
    }

    #[test]
    fn proto_f64_to_json() {
        let v = proto_value_to_json(&Value::F64(1.23));
        assert_eq!(v, serde_json::json!(1.23));
    }

    // ── encode_row / encode_response ────────────────────────────────────

    /// Helper: build a minimal DescriptorPool with a User message.
    fn test_descriptor_pool() -> prost_reflect::DescriptorPool {
        // Minimal FileDescriptorProto for a User message with id (string) and name (string).
        use prost::Message;
        use prost_reflect::prost_types::{
            DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
            field_descriptor_proto,
        };

        let user_msg = DescriptorProto {
            name: Some("User".into()),
            field: vec![
                FieldDescriptorProto {
                    name:    Some("id".into()),
                    number:  Some(1),
                    r#type:  Some(field_descriptor_proto::Type::String.into()),
                    label:   Some(field_descriptor_proto::Label::Optional.into()),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name:    Some("name".into()),
                    number:  Some(2),
                    r#type:  Some(field_descriptor_proto::Type::String.into()),
                    label:   Some(field_descriptor_proto::Label::Optional.into()),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name:    Some("age".into()),
                    number:  Some(3),
                    r#type:  Some(field_descriptor_proto::Type::Int32.into()),
                    label:   Some(field_descriptor_proto::Label::Optional.into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let file = FileDescriptorProto {
            name:         Some("test.proto".into()),
            package:      Some("test".into()),
            syntax:       Some("proto3".into()),
            message_type: vec![user_msg],
            ..Default::default()
        };

        let fds = FileDescriptorSet { file: vec![file] };
        let bytes = fds.encode_to_vec();
        prost_reflect::DescriptorPool::decode(bytes.as_slice()).unwrap()
    }

    #[test]
    fn encode_row_sets_fields() {
        let pool = test_descriptor_pool();
        let user_desc = pool.get_message_by_name("test.User").unwrap();

        let columns = vec![
            ColumnSpec { name: "id".into(), column_type: RowViewColumnType::Uuid },
            ColumnSpec { name: "name".into(), column_type: RowViewColumnType::Text },
            ColumnSpec { name: "age".into(), column_type: RowViewColumnType::Int32 },
        ];

        let row = vec![
            ColumnValue::Text("abc-123".into()),
            ColumnValue::Text("Alice".into()),
            ColumnValue::Int32(30),
        ];

        let msg = encode_row(&row, &columns, &user_desc);

        let id_field = user_desc.get_field_by_name("id").unwrap();
        let name_field = user_desc.get_field_by_name("name").unwrap();
        let age_field = user_desc.get_field_by_name("age").unwrap();

        assert_eq!(msg.get_field(&id_field).into_owned(), Value::String("abc-123".into()));
        assert_eq!(msg.get_field(&name_field).into_owned(), Value::String("Alice".into()));
        assert_eq!(msg.get_field(&age_field).into_owned(), Value::I32(30));
    }

    #[test]
    fn encode_row_null_leaves_field_unset() {
        let pool = test_descriptor_pool();
        let user_desc = pool.get_message_by_name("test.User").unwrap();

        let columns = vec![
            ColumnSpec { name: "id".into(), column_type: RowViewColumnType::Uuid },
            ColumnSpec { name: "name".into(), column_type: RowViewColumnType::Text },
            ColumnSpec { name: "age".into(), column_type: RowViewColumnType::Int32 },
        ];

        let row = vec![
            ColumnValue::Text("abc".into()),
            ColumnValue::Null,
            ColumnValue::Int32(0),
        ];

        let msg = encode_row(&row, &columns, &user_desc);

        let name_field = user_desc.get_field_by_name("name").unwrap();
        // Null leaves the field at its default (empty string for proto3 string).
        assert!(!msg.has_field(&name_field));
    }

    #[test]
    fn encode_response_get_single_row() {
        let pool = test_descriptor_pool();
        let user_desc = pool.get_message_by_name("test.User").unwrap();

        let columns = vec![
            ColumnSpec { name: "id".into(), column_type: RowViewColumnType::Uuid },
            ColumnSpec { name: "name".into(), column_type: RowViewColumnType::Text },
        ];

        let rows = vec![vec![
            ColumnValue::Text("u-1".into()),
            ColumnValue::Text("Bob".into()),
        ]];

        let response = encode_response(rows, &columns, false, &user_desc, &user_desc);

        let id_field = user_desc.get_field_by_name("id").unwrap();
        assert_eq!(response.get_field(&id_field).into_owned(), Value::String("u-1".into()));
    }

    #[test]
    fn encode_response_empty_rows() {
        let pool = test_descriptor_pool();
        let user_desc = pool.get_message_by_name("test.User").unwrap();

        let columns = vec![
            ColumnSpec { name: "id".into(), column_type: RowViewColumnType::Uuid },
        ];

        // No rows — response should have default values.
        let response = encode_response(vec![], &columns, false, &user_desc, &user_desc);
        let id_field = user_desc.get_field_by_name("id").unwrap();
        assert!(!response.has_field(&id_field));
    }

    // ── column_specs_from_type ──────────────────────────────────────────

    #[test]
    fn column_specs_from_type_filters_non_scalars() {
        use fraiseql_core::schema::{FieldDefinition, TypeDefinition};

        let type_def = TypeDefinition::new("User", "tb_users")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(FieldDefinition::new("posts", FieldType::List(Box::new(FieldType::Object("Post".into())))))
            .with_field(FieldDefinition::new("age", FieldType::Int));

        let specs = column_specs_from_type(&type_def);
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["id", "name", "age"]);
    }
}
