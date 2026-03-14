//! HTTP → GraphQL query translator for the REST transport layer.
//!
//! Given a query/mutation name, its compiled argument definitions, the path
//! parameters extracted by axum, and an optional JSON body, this module builds
//! the GraphQL query string and variables map that the executor expects.

use std::collections::HashMap;

use fraiseql_core::schema::ArgumentDefinition;
use serde_json::Value;

/// Result of translating an HTTP request into a GraphQL invocation.
pub struct TranslatedRequest {
    /// GraphQL document, e.g. `query($id: ID!) { get_user(id: $id) { ... } }`.
    pub query:     String,
    /// Variables object passed alongside the document.
    pub variables: Option<Value>,
}

/// Build a GraphQL query/mutation document for a REST request.
///
/// # Arguments
///
/// * `operation` — `"query"` or `"mutation"`
/// * `name` — operation name (e.g. `"get_user"`)
/// * `arguments` — compiled argument definitions from the schema
/// * `return_fields` — flat list of field names to select (empty → `__typename` only)
/// * `path_params` — parameters extracted from the URL path (e.g. `{id}`)
/// * `query_params` — key/value pairs from the query string
/// * `body` — optional JSON body (for POST/PUT/PATCH)
#[allow(clippy::implicit_hasher)] // Reason: callers always use std HashMap; generics add complexity without benefit
pub fn build_graphql_request(
    operation: &str,
    name: &str,
    arguments: &[ArgumentDefinition],
    return_fields: &[String],
    path_params: &HashMap<String, String>,
    query_params: &HashMap<String, String>,
    body: Option<&Value>,
) -> TranslatedRequest {
    // Merge all argument sources: path > body > query string
    let mut arg_values: HashMap<String, Value> = HashMap::new();

    // Query-string params (lowest priority)
    for (k, v) in query_params {
        arg_values.insert(k.clone(), Value::String(v.clone()));
    }

    // Body fields (medium priority)
    if let Some(Value::Object(map)) = body {
        for (k, v) in map {
            arg_values.insert(k.clone(), v.clone());
        }
    }

    // Path params (highest priority)
    for (k, v) in path_params {
        arg_values.insert(k.clone(), Value::String(v.clone()));
    }

    // Build variable declarations and argument list, restricted to declared args
    let mut var_decls: Vec<String> = Vec::new();
    let mut arg_refs: Vec<String> = Vec::new();
    let mut variables: serde_json::Map<String, Value> = serde_json::Map::new();

    for arg_def in arguments {
        let Some(val) = arg_values.get(&arg_def.name) else {
            // Skip undeclared or missing optional args
            if arg_def.nullable {
                continue;
            }
            // Required arg missing — skip gracefully; GraphQL validator will catch it
            continue;
        };

        let graphql_type =
            graphql_type_string(&arg_def.arg_type.to_graphql_string(), arg_def.nullable);
        var_decls.push(format!("${}: {}", arg_def.name, graphql_type));
        arg_refs.push(format!("{}: ${}", arg_def.name, arg_def.name));
        variables.insert(arg_def.name.clone(), val.clone());
    }

    // Build field selection
    let selection = if return_fields.is_empty() {
        "__typename".to_string()
    } else {
        return_fields.join("\n    ")
    };

    // Compose the document
    let vars_clause = if var_decls.is_empty() {
        String::new()
    } else {
        format!("({})", var_decls.join(", "))
    };

    let args_clause = if arg_refs.is_empty() {
        String::new()
    } else {
        format!("({})", arg_refs.join(", "))
    };

    let document =
        format!("{operation}{vars_clause} {{\n  {name}{args_clause} {{\n    {selection}\n  }}\n}}");

    let variables_value = if variables.is_empty() {
        None
    } else {
        Some(Value::Object(variables))
    };

    TranslatedRequest {
        query:     document,
        variables: variables_value,
    }
}

/// Format a GraphQL type string for a variable declaration.
///
/// Examples: `(base="String", nullable=true)` → `"String"`,
///           `(base="ID", nullable=false)` → `"ID!"`
fn graphql_type_string(base: &str, nullable: bool) -> String {
    if nullable {
        base.to_string()
    } else {
        format!("{base}!")
    }
}

/// Extract the `data.<operation>` slice from a GraphQL response JSON string.
///
/// Returns the raw value if the slice exists, or the full body on parse failure.
pub fn extract_data_field(response_json: &str, operation_name: &str) -> Value {
    let Ok(val) = serde_json::from_str::<Value>(response_json) else {
        return Value::String(response_json.to_string());
    };

    // Return `data.<operation>` if present
    if let Some(data) = val.get("data") {
        if let Some(field) = data.get(operation_name) {
            return field.clone();
        }
    }

    // Fall back to the full response (carries `errors` etc.)
    val
}

/// Outcome of classifying a GraphQL executor response for REST transport semantics.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum RestOutcome {
    /// Query succeeded — return HTTP 200 with the operation data as the body.
    Ok(Value),

    /// Partial success: both data and errors are present (e.g., field-level permission errors).
    ///
    /// Return HTTP 200 with `{"data": ..., "errors": [...], "_partial": true}`.
    Partial {
        /// Resolved operation data (may contain `null` fields).
        data:   Value,
        /// Array of partial errors that occurred during resolution.
        errors: Value,
    },

    /// Query-level failure: `data` is `null` and `errors` are present.
    ///
    /// The HTTP status code (401/403/404/400/429/500) is derived from the first
    /// error's `extensions.code` field.
    Failure {
        /// HTTP status code to return.
        status: u16,
        /// Errors array to include in the response body.
        body:   Value,
    },

    /// Single-item query returned `null` (resource not found).
    ///
    /// Return HTTP 404 with `{"error": "Not found", "operation": "<name>"}`.
    NotFound,
}

/// Classify a GraphQL executor response into a REST semantic outcome.
///
/// # Arguments
///
/// * `response_json` — raw response string from the executor
/// * `operation_name` — name of the GraphQL query/mutation being served
/// * `is_list` — `true` when the operation returns a list; `false` for single-item queries
///
/// # Behaviour
///
/// | Situation | Outcome |
/// |-----------|---------|
/// | data + errors both present | `Partial` → HTTP 200 with `_partial: true` |
/// | data null, errors present | `Failure` → HTTP 401/403/404/400/429/500 |
/// | data present, no errors | `Ok` → HTTP 200 |
/// | single-item, data null, no errors | `NotFound` → HTTP 404 |
pub fn classify_response(response_json: &str, operation_name: &str, is_list: bool) -> RestOutcome {
    let Ok(root) = serde_json::from_str::<Value>(response_json) else {
        return RestOutcome::Failure {
            status: 500,
            body:   serde_json::json!({"errors": [{"message": "Internal server error"}]}),
        };
    };

    let data_field = root.get("data");
    let errors_field = root.get("errors").filter(|e| !e.is_null());

    match (data_field, errors_field) {
        // Partial success: data non-null + errors both present
        (Some(data_obj), Some(errors_val)) if !data_obj.is_null() => {
            let op_data = data_obj.get(operation_name).cloned().unwrap_or(Value::Null);
            RestOutcome::Partial {
                data:   op_data,
                errors: errors_val.clone(),
            }
        },

        // Query-level failure: data null, errors present
        (Some(data_obj), Some(errors_val)) if data_obj.is_null() => RestOutcome::Failure {
            status: classify_error_status(errors_val),
            body:   errors_val.clone(),
        },

        // No data field at all, errors present
        (None, Some(errors_val)) => RestOutcome::Failure {
            status: classify_error_status(errors_val),
            body:   errors_val.clone(),
        },

        // Data present, no errors
        (Some(data_obj), None) if !data_obj.is_null() => {
            let op_data = data_obj.get(operation_name).cloned().unwrap_or(Value::Null);

            // Single-item query returned null → resource not found
            if !is_list && op_data.is_null() {
                return RestOutcome::NotFound;
            }

            RestOutcome::Ok(op_data)
        },

        // Anything else (data null, no errors; parse ok but empty) → not found
        _ => RestOutcome::NotFound,
    }
}

/// Map GraphQL error extension codes to HTTP status codes.
///
/// Uses the first error in the array that carries an `extensions.code` field.
/// Falls back to 500 when no recognised code is found.
fn classify_error_status(errors: &Value) -> u16 {
    let Some(arr) = errors.as_array() else {
        return 500;
    };

    for error in arr {
        if let Some(code) =
            error.get("extensions").and_then(|e| e.get("code")).and_then(Value::as_str)
        {
            return match code {
                "UNAUTHENTICATED" => 401,
                "FORBIDDEN" => 403,
                "NOT_FOUND" => 404,
                "VALIDATION_ERROR" | "BAD_USER_INPUT" => 400,
                "RATE_LIMITED" => 429,
                _ => 500,
            };
        }
    }

    500
}
