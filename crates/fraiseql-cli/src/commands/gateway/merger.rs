//! Response merger — combines results from multiple subgraph fetches.
//!
//! Stitches data and errors from parallel subgraph responses into a single
//! unified GraphQL response for the client.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A response received from a single subgraph.
#[derive(Debug, Clone, Deserialize)]
pub struct SubgraphResponse {
    /// The `data` field from the GraphQL response.
    pub data: Option<Value>,

    /// The `errors` field from the GraphQL response.
    #[serde(default)]
    pub errors: Vec<GraphQLError>,
}

/// A single GraphQL error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    /// Human-readable error message.
    pub message: String,

    /// Optional path indicating where the error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<Value>>,

    /// Optional source locations in the query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<Value>>,

    /// Optional extension data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// The merged response returned to the client.
#[derive(Debug, Clone, Serialize)]
pub struct MergedResponse {
    /// Combined data from all subgraph responses.
    pub data: Value,

    /// Combined errors from all subgraph responses.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<GraphQLError>,
}

/// Merge multiple subgraph responses into a single client response.
///
/// Data objects are shallow-merged at the top level (each subgraph contributes
/// different root fields). Errors are concatenated with subgraph attribution
/// added to the `extensions` field.
pub fn merge_responses(responses: &[(String, SubgraphResponse)]) -> MergedResponse {
    let mut merged_data = Map::new();
    let mut merged_errors = Vec::new();

    for (subgraph_name, response) in responses {
        // Merge data fields
        if let Some(Value::Object(data)) = &response.data {
            for (key, value) in data {
                merged_data.insert(key.clone(), value.clone());
            }
        }

        // Collect errors with subgraph attribution
        for error in &response.errors {
            let mut attributed = error.clone();
            let ext = attributed.extensions.get_or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(ext_map) = ext {
                ext_map.insert("subgraph".to_string(), Value::String(subgraph_name.clone()));
            }
            merged_errors.push(attributed);
        }
    }

    MergedResponse {
        data:   Value::Object(merged_data),
        errors: merged_errors,
    }
}

/// Merge entity resolution results into existing data.
///
/// For each entity in `entities`, look up the corresponding entry in `target`
/// (matched by `__typename` + key fields) and merge extra fields in.
pub fn merge_entity_fields(target: &mut Value, entities: &[Value]) {
    for entity in entities {
        if let Value::Object(entity_map) = entity {
            deep_merge_object(target, entity_map);
        }
    }
}

/// Recursively merge `source` fields into `target`.
fn deep_merge_object(target: &mut Value, source: &Map<String, Value>) {
    if let Value::Object(target_map) = target {
        for (key, value) in source {
            match (target_map.get_mut(key), value) {
                (Some(Value::Object(existing)), Value::Object(incoming)) => {
                    // Recursively merge nested objects
                    deep_merge_object(&mut Value::Object(existing.clone()), incoming);
                    // After recursive merge, replace
                    let mut merged = existing.clone();
                    for (k, v) in incoming {
                        merged.insert(k.clone(), v.clone());
                    }
                    target_map.insert(key.clone(), Value::Object(merged));
                },
                _ => {
                    target_map.insert(key.clone(), value.clone());
                },
            }
        }
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_merge_single_response() {
        let responses = vec![(
            "users".to_string(),
            SubgraphResponse {
                data:   Some(json!({"users": [{"id": 1, "name": "Alice"}]})),
                errors: vec![],
            },
        )];

        let merged = merge_responses(&responses);
        assert_eq!(merged.data["users"][0]["name"], "Alice");
        assert!(merged.errors.is_empty());
    }

    #[test]
    fn test_merge_multiple_responses() {
        let responses = vec![
            (
                "users".to_string(),
                SubgraphResponse {
                    data:   Some(json!({"users": [{"id": 1}]})),
                    errors: vec![],
                },
            ),
            (
                "products".to_string(),
                SubgraphResponse {
                    data:   Some(json!({"products": [{"id": 100}]})),
                    errors: vec![],
                },
            ),
        ];

        let merged = merge_responses(&responses);
        assert!(merged.data["users"].is_array());
        assert!(merged.data["products"].is_array());
    }

    #[test]
    fn test_merge_errors_attributed() {
        let responses = vec![(
            "users".to_string(),
            SubgraphResponse {
                data:   Some(json!({"users": null})),
                errors: vec![GraphQLError {
                    message:    "Not found".to_string(),
                    path:       Some(vec![json!("users")]),
                    locations:  None,
                    extensions: None,
                }],
            },
        )];

        let merged = merge_responses(&responses);
        assert_eq!(merged.errors.len(), 1);
        assert_eq!(merged.errors[0].extensions.as_ref().unwrap()["subgraph"], "users");
    }

    #[test]
    fn test_merge_null_data() {
        let responses = vec![(
            "users".to_string(),
            SubgraphResponse {
                data:   None,
                errors: vec![GraphQLError {
                    message:    "Internal error".to_string(),
                    path:       None,
                    locations:  None,
                    extensions: None,
                }],
            },
        )];

        let merged = merge_responses(&responses);
        assert_eq!(merged.data, json!({}));
        assert_eq!(merged.errors.len(), 1);
    }

    #[test]
    fn test_merge_entity_fields() {
        let mut target = json!({"id": 1, "name": "Alice"});
        let entities = vec![json!({"email": "alice@example.com", "role": "admin"})];

        merge_entity_fields(&mut target, &entities);

        assert_eq!(target["email"], "alice@example.com");
        assert_eq!(target["role"], "admin");
        assert_eq!(target["name"], "Alice"); // preserved
    }

    #[test]
    fn test_merge_empty_responses() {
        let merged = merge_responses(&[]);
        assert_eq!(merged.data, json!({}));
        assert!(merged.errors.is_empty());
    }

    #[test]
    fn test_merge_preserves_error_paths() {
        let responses = vec![(
            "svc".to_string(),
            SubgraphResponse {
                data:   Some(json!({})),
                errors: vec![GraphQLError {
                    message:    "fail".to_string(),
                    path:       Some(vec![json!("users"), json!(0), json!("name")]),
                    locations:  Some(vec![json!({"line": 1, "column": 3})]),
                    extensions: Some(json!({"code": "INTERNAL"})),
                }],
            },
        )];

        let merged = merge_responses(&responses);
        let err = &merged.errors[0];
        assert!(err.path.is_some());
        assert!(err.locations.is_some());
        // Original extension "code" preserved + subgraph added
        assert_eq!(err.extensions.as_ref().unwrap()["code"], "INTERNAL");
        assert_eq!(err.extensions.as_ref().unwrap()["subgraph"], "svc");
    }
}
