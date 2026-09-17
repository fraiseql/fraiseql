//! gRPC request handler — translates protobuf queries into row-shaped view
//! queries and encodes results back to protobuf.
//!
//! The handler accepts a decoded [`prost_reflect::DynamicMessage`] request,
//! extracts filter/pagination arguments, generates a SQL WHERE clause via
//! [`GenericWhereGenerator`], calls [`DatabaseAdapter::execute_row_query()`],
//! and maps the resulting [`ColumnValue`] rows into a protobuf response message.

use std::collections::HashMap;

use fraiseql_core::{
    db::{
        dialect::{PostgresDialect, RowViewColumnType},
        traits::DatabaseAdapter,
        types::{ColumnSpec, ColumnValue},
        where_clause::{WhereClause, WhereOperator},
        where_generator::GenericWhereGenerator,
    },
    schema::{CompiledSchema, FieldType, TypeDefinition},
    security::SecurityContext,
};
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
#[non_exhaustive]
pub enum RpcKind {
    /// A read query against a row-shaped view (`vr_*`).
    Query {
        /// Row-shaped view name (e.g., `"vr_user"`).
        view_name:      String,
        /// Whether this RPC returns a list.
        returns_list:   bool,
        /// Column specs for the row-shaped view.
        columns:        Vec<ColumnSpec>,
        /// Inner row message descriptor (the repeated element for list queries,
        /// or the single message for get queries).
        row_descriptor: MessageDescriptor,
    },
    /// A server-streaming query against a row-shaped view (`vr_*`).
    ///
    /// Used for list queries when the descriptor marks the RPC as
    /// `server_streaming`. Rows are fetched in batches and streamed
    /// individually as gRPC frames.
    ServerStream {
        /// Row-shaped view name (e.g., `"vr_user"`).
        view_name:      String,
        /// Column specs for the row-shaped view.
        columns:        Vec<ColumnSpec>,
        /// Row message descriptor (the entity type, e.g., `User`).
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
    pub operation_name:      String,
    /// GraphQL type name (e.g., `"User"`).
    pub type_name:           String,
    /// What kind of RPC this is (query or mutation).
    pub kind:                RpcKind,
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
#[must_use]
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
        // Types with no columnar representation: List, Object, Interface,
        // Union, Vector, BitVector — the vector kinds are searched, not
        // projected into the columnar view.
        _ => None,
    }
}

/// Build [`ColumnSpec`] list from a type definition's scalar fields.
#[must_use]
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
#[must_use]
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
pub(crate) fn proto_value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::I32(n) | Value::EnumNumber(n) => serde_json::json!(*n),
        Value::I64(n) => serde_json::json!(*n),
        Value::U32(n) => serde_json::json!(*n),
        Value::U64(n) => serde_json::json!(*n),
        Value::F32(f) => serde_json::json!(*f),
        Value::F64(f) => serde_json::json!(*f),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(b) => serde_json::Value::String(base64_encode(b)),
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

/// Recursively recase every object key of a gRPC mutation arg to canonical
/// `snake_case` with the engine's acronym-aware
/// [`to_snake_case`](fraiseql_core::utils::to_snake_case), recursing into nested
/// objects and arrays; scalar values are returned untouched.
///
/// Applied to each arg in [`execute_grpc_mutation`] only under
/// [`NamingConvention::CamelCase`](fraiseql_core::schema::NamingConvention), so a
/// nested `input` message reaches the SQL function as `snake_case` (#456). Shares
/// `to_snake_case` with the read path, so writes round-trip exactly as reads
/// (`s3Key` → `s3_key`, `dns1Id` → `dns_1_id`).
pub(crate) fn recase_keys_to_snake(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(k, v)| (fraiseql_core::utils::to_snake_case(&k), recase_keys_to_snake(v)))
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(recase_keys_to_snake).collect())
        },
        other => other,
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
#[must_use]
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
#[must_use]
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

/// Extract `order_by` from the request message.
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
                            .filter(|d| {
                                d.eq_ignore_ascii_case("asc") || d.eq_ignore_ascii_case("desc")
                            })
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
/// When a [`SecurityContext`] is provided, RLS (Row-Level Security) WHERE
/// clauses are generated from the user's identity and AND-ed with any
/// client-supplied filters.  RLS always wins — client filters can only
/// *narrow*, never *widen*, the result set.
///
/// # Errors
///
/// Returns `FraiseQLError::Database` on query execution failure.
/// Returns `FraiseQLError::Validation` if filter construction fails.
// Reason: mirrors build_streaming_body's signature; grouping into a struct adds
// indirection without reducing call-site complexity
#[allow(clippy::too_many_arguments)]
pub async fn execute_grpc_query<A: DatabaseAdapter>(
    adapter: &A,
    view_name: &str,
    columns: &[ColumnSpec],
    returns_list: bool,
    request_msg: &DynamicMessage,
    type_def: &TypeDefinition,
    security_context: Option<&SecurityContext>,
    rls_policy: Option<&dyn fraiseql_core::security::RLSPolicy>,
) -> Result<Vec<Vec<ColumnValue>>, FraiseQLError> {
    // Extract filters and build WHERE clause.
    let user_where = extract_filters(request_msg, type_def);

    // #1348: the policy the deployment **configured**, never one built here. This arm
    // used to construct `DefaultRLSPolicy::new()` itself, which was wrong in both
    // directions: a deployment with a custom policy never had it consulted, and one
    // with none — the default — got `DefaultRLSPolicy` on gRPC and no RLS on
    // GraphQL/REST, so the same query answered differently depending on which transport
    // asked. `None` means no row filter, exactly as the engine's read path treats an
    // unconfigured `RuntimeConfig.rls_policy`.
    let rls_where = match (security_context, rls_policy) {
        (Some(ctx), Some(policy)) => {
            policy.evaluate(ctx, type_def.name.as_str())?.map(|rls| rls.into_where_clause())
        },
        _ => None,
    };

    // Combine: RLS first, then user filters — RLS always wins.
    let combined = match (rls_where, user_where) {
        (Some(rls), Some(user)) => Some(WhereClause::And(vec![rls, user])),
        (Some(rls), None) => Some(rls),
        (None, user) => user,
    };

    // Generate SQL WHERE clause string via GenericWhereGenerator.
    let where_sql = if let Some(ref clause) = combined {
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
        user_id = ?security_context.map(|c| &c.user_id),
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
pub async fn execute_grpc_mutation<A>(
    executor: &fraiseql_core::runtime::Executor<A>,
    mutation_name: &str,
    request_msg: &DynamicMessage,
    recase_input_keys: bool,
    security_context: Option<&fraiseql_core::security::SecurityContext>,
) -> Result<MutationResult, FraiseQLError>
where
    A: DatabaseAdapter + fraiseql_core::db::SupportsMutations,
{
    // #1330: this used to call `adapter.execute_function_call` directly, which
    // reached the database without passing `execute_mutation_impl` — the single
    // point every other write converges on. `requires_role`, `requires_actor`, the
    // `Authorizer`, selection-set and argument-name validation, the required-argument
    // check, `before:mutation` and the change-log write were all skipped, and
    // `requires_actor`'s own claim that "this chokepoint is why 'every transport' is
    // a fact rather than a claim" was false for exactly as long as that line existed.
    //
    // It also means arguments are bound **by name** now. The old path collected the
    // set fields into a positional `Vec` in protobuf field order and handed it
    // straight to the SQL function, so the binding was correct only while the
    // descriptor's field order matched the schema's argument order — a coincidence
    // nothing enforced, and one a renumbered proto field would break silently.
    let variables =
        grpc_mutation_variables(executor.schema(), mutation_name, request_msg, recase_input_keys);

    debug!(
        mutation = %mutation_name,
        arg_count = variables.as_object().map_or(0, serde_json::Map::len),
        "Executing gRPC mutation through the chokepoint"
    );

    // The chokepoint validates the selection set (§ 5.3.1), and gRPC has none:
    // its response shape is the flat protobuf `MutationResponse`, not a GraphQL
    // document. Synthesise one from the mutation's **declared return type**, the
    // same choice REST makes — except built structurally rather than by formatting
    // field names into a string and reparsing them (#1331).
    let selections = return_type_selections(executor.schema(), mutation_name);

    let execution = executor
        .execute_mutation_as(mutation_name, Some(&variables), security_context, &selections)
        .await?;

    Ok(mutation_result_from_outcome(&execution.outcome))
}

/// Bind the request message's set fields to the mutation's **declared argument
/// names** (#1330).
///
/// Matched by name rather than by position: a protobuf field carries the
/// `snake_case` spelling of the argument, while a camelCase GraphQL surface declares
/// it in camelCase, so both spellings are accepted for each declared argument. A
/// field the mutation does not declare is dropped here rather than being passed
/// into whatever positional slot it happened to line up with.
fn grpc_mutation_variables(
    schema: &fraiseql_core::schema::CompiledSchema,
    mutation_name: &str,
    request_msg: &DynamicMessage,
    recase_input_keys: bool,
) -> serde_json::Value {
    let declared: Vec<String> = schema
        .find_mutation(mutation_name)
        .map(|m| m.arguments.iter().map(|a| a.name.clone()).collect())
        .unwrap_or_default();

    let mut out = serde_json::Map::new();
    for field in request_msg.descriptor().fields() {
        if !request_msg.has_field(&field) {
            continue;
        }
        // Object-valued args keep #456's key recasing: a nested `input` message
        // serializes with protobuf JSON names, and the SQL function reads
        // `payload->>'snake_field'`.
        let value = proto_value_to_json(request_msg.get_field(&field).as_ref());
        let value = if recase_input_keys {
            recase_keys_to_snake(value)
        } else {
            value
        };

        let proto_name = field.name();
        let matched = declared.iter().find(|name| {
            name.as_str() == proto_name || fraiseql_core::utils::to_snake_case(name) == proto_name
        });
        match matched {
            Some(name) => {
                out.insert(name.clone(), value);
            },
            // No declared argument answers to this field. Keeping the protobuf
            // spelling lets the chokepoint's own argument validation report it
            // rather than this function silently deciding.
            None => {
                out.insert(proto_name.to_string(), value);
            },
        }
    }
    serde_json::Value::Object(out)
}

/// The mutation's declared return-type fields, as a flat selection set.
///
/// Scalar fields only: a nested composite would need its own selection set, and
/// the protobuf `MutationResponse` has nowhere to put one.
fn return_type_selections(
    schema: &fraiseql_core::schema::CompiledSchema,
    mutation_name: &str,
) -> Vec<fraiseql_core::graphql::FieldSelection> {
    schema
        .find_mutation(mutation_name)
        .and_then(|m| schema.find_type(&m.return_type))
        .map(|t| {
            t.fields
                .iter()
                .filter(|f| f.field_type.is_scalar())
                .map(|f| fraiseql_core::graphql::FieldSelection {
                    name:          f.output_name().to_string(),
                    alias:         None,
                    arguments:     vec![],
                    nested_fields: vec![],
                    directives:    vec![],
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Flatten the chokepoint's `mutation_response` envelope into the gRPC wire result.
///
/// gRPC's `MutationResponse{success, id, error}` **is** that envelope, which is why
/// this reads the envelope rather than the projection: on success the projection
/// carries the *entity*, whose `id` is the row's rather than the envelope's, and on
/// failure it carries only the error class, not the message a client shows.
///
/// A declined write (`succeeded = false`) is a completed call carrying a refusal,
/// not a transport error — a gRPC error status would tell the client the call
/// failed, which is a different thing and retried differently.
fn mutation_result_from_outcome(
    outcome: &fraiseql_core::runtime::mutation_result::MutationOutcome,
) -> MutationResult {
    use fraiseql_core::runtime::mutation_result::MutationOutcome;

    match outcome {
        MutationOutcome::Success { entity_id, .. } => MutationResult {
            success: true,
            id:      entity_id.clone(),
            error:   None,
        },
        MutationOutcome::Error { message, .. } => MutationResult {
            success: false,
            id:      None,
            error:   Some(message.clone()),
        },
        // `MutationOutcome` is `#[non_exhaustive]`, so this arm is required rather
        // than chosen. It reports **failure**: a variant added upstream is one this
        // build cannot interpret, and answering `success = true` to a write whose
        // outcome is unknown is the one answer that cannot be walked back.
        _ => MutationResult {
            success: false,
            id:      None,
            error:   Some("mutation outcome not recognised by this build".to_string()),
        },
    }
}

/// Result from a gRPC mutation, ready to be encoded as a `MutationResponse`.
#[derive(Debug)]
pub struct MutationResult {
    /// Whether the mutation succeeded.
    pub success: bool,
    /// Optional entity ID returned by the mutation.
    pub id:      Option<String>,
    /// Optional error message (when `success` is false).
    pub error:   Option<String>,
}

/// Encode a [`MutationResult`] into a protobuf response message.
///
/// Expects the response descriptor to have fields: `success` (bool),
/// `id` (optional string), `error` (optional string).
#[must_use]
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
#[must_use]
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
#[must_use]
pub fn column_value_to_proto(col: &ColumnValue) -> Option<Value> {
    match col {
        ColumnValue::Null => None,
        ColumnValue::Text(s) => Some(Value::String(s.clone())),
        ColumnValue::Int32(n) => Some(Value::I32(*n)),
        ColumnValue::Int64(n) => Some(Value::I64(*n)),
        ColumnValue::Float64(f) => Some(Value::F64(*f)),
        ColumnValue::Boolean(b) => Some(Value::Bool(*b)),
        ColumnValue::Uuid(u) => Some(Value::String(u.clone())),
        ColumnValue::Timestamptz(ts) => {
            // Encode as ISO 8601 string. A full implementation would use
            // google.protobuf.Timestamp, but string is simpler for the MVP.
            Some(Value::String(ts.clone()))
        },
        ColumnValue::Date(d) => Some(Value::String(d.clone())),
        ColumnValue::Json(v) => Some(Value::String(v.clone())),
    }
}

/// Encode query results into a protobuf response message.
///
/// For list queries, the response contains a `repeated` field named `items`
/// (or the pluralized type name). For get queries, the response fields are
/// the row fields directly.
#[must_use]
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
            if field_desc.is_list() && field_desc.kind().as_message().is_some() {
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
    let service_desc = pool.get_service_by_name(service_name).ok_or_else(|| {
        FraiseQLError::validation(format!(
            "gRPC service '{service_name}' not found in descriptor pool"
        ))
    })?;

    for method_desc in service_desc.methods() {
        let method_name = method_desc.name().to_string();
        let full_method = format!("/{service_name}/{method_name}");
        let response_desc = method_desc.output();

        // Try query first (Get*/List* prefix).
        if method_name.starts_with("Get") || method_name.starts_with("List") {
            let query_name = grpc_method_to_query_name(&method_name);

            if let Some(query_def) = schema.find_query(&query_name) {
                // #1329: a function-backed query is answered by a function, and this
                // table answers a method by reading `vr_<type.sql_source>` directly —
                // the resolver is never consulted. Registering one would serve the
                // type's rows in place of the function's computed answer: a wrong
                // result that looks like a right one, which is worse than the absent
                // method skipping it produces. gRPC carries the SQL-backed surface, as
                // REST does.
                if query_def.function.is_some() {
                    warn!(
                        method = %method_name,
                        query = %query_name,
                        "gRPC does not carry function-backed queries (#1329) — the method \
                         would read the type's view instead of invoking the function; \
                         skipping"
                    );
                    continue;
                }
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

                // Server-streaming list queries: the descriptor marks
                // the method with `server_streaming = true` and the
                // response type is the entity message directly.
                let is_server_streaming = method_desc.is_server_streaming();

                let kind = if is_server_streaming && query_def.returns_list {
                    RpcKind::ServerStream {
                        view_name,
                        columns,
                        row_descriptor: response_desc.clone(),
                    }
                } else {
                    let row_desc = if query_def.returns_list {
                        response_desc
                            .fields()
                            .find(|f| f.is_list() && f.kind().as_message().is_some())
                            .and_then(|f| f.kind().as_message().cloned())
                            .unwrap_or_else(|| response_desc.clone())
                    } else {
                        response_desc.clone()
                    };
                    RpcKind::Query {
                        view_name,
                        returns_list: query_def.returns_list,
                        columns,
                        row_descriptor: row_desc,
                    }
                };

                table.insert(
                    full_method,
                    RpcOperation {
                        operation_name: query_name,
                        type_name: type_name.clone(),
                        kind,
                        response_descriptor: response_desc,
                    },
                );
                continue;
            }
        }

        // Try mutation: convert PascalCase method name to camelCase mutation name.
        let mutation_name = grpc_method_to_mutation_name(&method_name);
        if let Some(mutation_def) = schema.find_mutation(&mutation_name) {
            let function_name =
                mutation_def.sql_source.clone().unwrap_or_else(|| format!("fn_{mutation_name}"));

            table.insert(
                full_method,
                RpcOperation {
                    operation_name:      mutation_name,
                    type_name:           mutation_def.return_type.clone(),
                    kind:                RpcKind::Mutation { function_name },
                    response_descriptor: response_desc,
                },
            );
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
pub(crate) fn grpc_method_to_query_name(method: &str) -> String {
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
/// Convention: `"CreateUser"` → `"createUser"` (`PascalCase` → camelCase).
pub(crate) fn grpc_method_to_mutation_name(method: &str) -> String {
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
