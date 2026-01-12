//! Result projection - transforms JSONB database results to GraphQL responses.

use crate::db::types::JsonbValue;
use crate::error::{FraiseQLError, Result};
use serde_json::{Map, Value as JsonValue};

/// Projection mapper - maps JSONB fields to GraphQL selection set.
#[derive(Debug, Clone)]
pub struct ProjectionMapper {
    /// Fields to project.
    pub fields: Vec<String>,
}

impl ProjectionMapper {
    /// Create new projection mapper.
    #[must_use]
    pub fn new(fields: Vec<String>) -> Self {
        Self { fields }
    }

    /// Project fields from JSONB value.
    ///
    /// # Arguments
    ///
    /// * `jsonb` - JSONB value from database
    ///
    /// # Returns
    ///
    /// Projected JSON value with only requested fields
    ///
    /// # Errors
    ///
    /// Returns error if projection fails.
    pub fn project(&self, jsonb: &JsonbValue) -> Result<JsonValue> {
        // Extract the inner serde_json::Value
        let value = jsonb.as_value();

        match value {
            JsonValue::Object(map) => self.project_json_object(map),
            JsonValue::Array(arr) => self.project_json_array(arr),
            v => Ok(v.clone()),
        }
    }

    /// Project object fields from JSON object.
    fn project_json_object(&self, map: &serde_json::Map<String, JsonValue>) -> Result<JsonValue> {
        let mut result = Map::new();

        for field in &self.fields {
            if let Some(value) = map.get(field) {
                result.insert(field.clone(), value.clone());
            }
        }

        Ok(JsonValue::Object(result))
    }

    /// Project array elements from JSON array.
    fn project_json_array(&self, arr: &[JsonValue]) -> Result<JsonValue> {
        let projected: Vec<JsonValue> = arr
            .iter()
            .filter_map(|item| {
                if let JsonValue::Object(obj) = item {
                    self.project_json_object(obj).ok()
                } else {
                    Some(item.clone())
                }
            })
            .collect();

        Ok(JsonValue::Array(projected))
    }
}

/// Result projector - high-level result transformation.
pub struct ResultProjector {
    mapper: ProjectionMapper,
}

impl ResultProjector {
    /// Create new result projector.
    #[must_use]
    pub fn new(fields: Vec<String>) -> Self {
        Self {
            mapper: ProjectionMapper::new(fields),
        }
    }

    /// Project database results to GraphQL response.
    ///
    /// # Arguments
    ///
    /// * `results` - Database results as JSONB values
    /// * `is_list` - Whether the query returns a list
    ///
    /// # Returns
    ///
    /// GraphQL-compatible JSON response
    ///
    /// # Errors
    ///
    /// Returns error if projection fails.
    pub fn project_results(
        &self,
        results: &[JsonbValue],
        is_list: bool,
    ) -> Result<JsonValue> {
        if is_list {
            // Project array of results
            let projected: Result<Vec<JsonValue>> = results
                .iter()
                .map(|r| self.mapper.project(r))
                .collect();

            Ok(JsonValue::Array(projected?))
        } else {
            // Project single result
            if let Some(first) = results.first() {
                self.mapper.project(first)
            } else {
                Ok(JsonValue::Null)
            }
        }
    }

    /// Wrap result in GraphQL data envelope.
    ///
    /// # Arguments
    ///
    /// * `result` - Projected result
    /// * `query_name` - Query operation name
    ///
    /// # Returns
    ///
    /// GraphQL response with `{ "data": { "queryName": result } }` structure
    #[must_use]
    pub fn wrap_in_data_envelope(result: JsonValue, query_name: &str) -> JsonValue {
        let mut data = Map::new();
        data.insert(query_name.to_string(), result);

        let mut response = Map::new();
        response.insert("data".to_string(), JsonValue::Object(data));

        JsonValue::Object(response)
    }

    /// Wrap error in GraphQL error envelope.
    ///
    /// # Arguments
    ///
    /// * `error` - Error to wrap
    ///
    /// # Returns
    ///
    /// GraphQL error response with `{ "errors": [...] }` structure
    #[must_use]
    pub fn wrap_error(error: &FraiseQLError) -> JsonValue {
        let mut error_obj = Map::new();
        error_obj.insert("message".to_string(), JsonValue::String(error.to_string()));

        let mut response = Map::new();
        response.insert("errors".to_string(), JsonValue::Array(vec![JsonValue::Object(error_obj)]));

        JsonValue::Object(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_projection_mapper_new() {
        let mapper = ProjectionMapper::new(vec!["id".to_string(), "name".to_string()]);
        assert_eq!(mapper.fields.len(), 2);
    }

    #[test]
    fn test_project_object() {
        let mapper = ProjectionMapper::new(vec!["id".to_string(), "name".to_string()]);

        let data = json!({
            "id": "123",
            "name": "Alice",
            "email": "alice@example.com"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(result, json!({ "id": "123", "name": "Alice" }));
    }

    #[test]
    fn test_project_array() {
        let mapper = ProjectionMapper::new(vec!["id".to_string()]);

        let data = json!([
            { "id": "1", "name": "Alice" },
            { "id": "2", "name": "Bob" }
        ]);

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(result, json!([{ "id": "1" }, { "id": "2" }]));
    }

    #[test]
    fn test_result_projector_list() {
        let projector = ResultProjector::new(vec!["id".to_string()]);

        let data = json!({ "id": "1", "name": "Alice" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, true).unwrap();

        assert_eq!(result, json!([{ "id": "1" }]));
    }

    #[test]
    fn test_result_projector_single() {
        let projector = ResultProjector::new(vec!["id".to_string()]);

        let data = json!({ "id": "1", "name": "Alice" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        assert_eq!(result, json!({ "id": "1" }));
    }

    #[test]
    fn test_wrap_in_data_envelope() {
        let result = json!([{ "id": "1" }]);
        let wrapped = ResultProjector::wrap_in_data_envelope(result, "users");

        assert_eq!(wrapped, json!({ "data": { "users": [{ "id": "1" }] } }));
    }

    #[test]
    fn test_wrap_error() {
        let error = FraiseQLError::Validation {
            message: "Invalid query".to_string(),
            path: None,
        };

        let wrapped = ResultProjector::wrap_error(&error);

        assert!(wrapped.get("errors").is_some());
        assert_eq!(wrapped.get("data"), None);
    }
}
