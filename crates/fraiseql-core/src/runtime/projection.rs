//! Result projection - transforms JSONB database results to GraphQL responses.

use crate::db::types::JsonbValue;
use crate::error::{FraiseQLError, Result};
use serde_json::{Map, Value as JsonValue};

/// Field mapping for projection with alias support.
#[derive(Debug, Clone)]
pub struct FieldMapping {
    /// JSONB key name (source).
    pub source: String,
    /// Output key name (alias if different from source).
    pub output: String,
    /// For nested object fields, the typename to add.
    /// This enables `__typename` to be added recursively to nested objects.
    pub nested_typename: Option<String>,
    /// Nested field mappings (for related objects).
    pub nested_fields: Option<Vec<FieldMapping>>,
}

impl FieldMapping {
    /// Create a simple field mapping (no alias).
    #[must_use]
    pub fn simple(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            source: name.clone(),
            output: name,
            nested_typename: None,
            nested_fields: None,
        }
    }

    /// Create a field mapping with an alias.
    #[must_use]
    pub fn aliased(source: impl Into<String>, alias: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            output: alias.into(),
            nested_typename: None,
            nested_fields: None,
        }
    }

    /// Create a field mapping for a nested object with its own typename.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // For a Post with nested author (User type)
    /// FieldMapping::nested_object("author", "User", vec![
    ///     FieldMapping::simple("id"),
    ///     FieldMapping::simple("name"),
    /// ])
    /// ```
    #[must_use]
    pub fn nested_object(
        name: impl Into<String>,
        typename: impl Into<String>,
        fields: Vec<FieldMapping>,
    ) -> Self {
        let name = name.into();
        Self {
            source: name.clone(),
            output: name,
            nested_typename: Some(typename.into()),
            nested_fields: Some(fields),
        }
    }

    /// Create an aliased nested object field.
    #[must_use]
    pub fn nested_object_aliased(
        source: impl Into<String>,
        alias: impl Into<String>,
        typename: impl Into<String>,
        fields: Vec<FieldMapping>,
    ) -> Self {
        Self {
            source: source.into(),
            output: alias.into(),
            nested_typename: Some(typename.into()),
            nested_fields: Some(fields),
        }
    }

    /// Set the typename for a nested object field.
    #[must_use]
    pub fn with_nested_typename(mut self, typename: impl Into<String>) -> Self {
        self.nested_typename = Some(typename.into());
        self
    }

    /// Set nested field mappings.
    #[must_use]
    pub fn with_nested_fields(mut self, fields: Vec<FieldMapping>) -> Self {
        self.nested_fields = Some(fields);
        self
    }
}

/// Projection mapper - maps JSONB fields to GraphQL selection set.
#[derive(Debug, Clone)]
pub struct ProjectionMapper {
    /// Fields to project (with optional aliases).
    pub fields: Vec<FieldMapping>,
    /// Optional `__typename` value to add to each object.
    pub typename: Option<String>,
}

impl ProjectionMapper {
    /// Create new projection mapper from field names (no aliases).
    #[must_use]
    pub fn new(fields: Vec<String>) -> Self {
        Self {
            fields: fields.into_iter().map(FieldMapping::simple).collect(),
            typename: None,
        }
    }

    /// Create new projection mapper with field mappings (supports aliases).
    #[must_use]
    pub fn with_mappings(fields: Vec<FieldMapping>) -> Self {
        Self {
            fields,
            typename: None,
        }
    }

    /// Set `__typename` to include in projected objects.
    #[must_use]
    pub fn with_typename(mut self, typename: impl Into<String>) -> Self {
        self.typename = Some(typename.into());
        self
    }

    /// Project fields from JSONB value.
    ///
    /// # Arguments
    ///
    /// * `jsonb` - JSONB value from database
    ///
    /// # Returns
    ///
    /// Projected JSON value with only requested fields (and aliases applied)
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

        // Add __typename first if configured (GraphQL convention)
        if let Some(ref typename) = self.typename {
            result.insert("__typename".to_string(), JsonValue::String(typename.clone()));
        }

        // Project fields with alias support
        for field in &self.fields {
            if let Some(value) = map.get(&field.source) {
                // Handle nested objects with their own typename
                let projected_value = self.project_nested_value(value, field)?;
                result.insert(field.output.clone(), projected_value);
            }
        }

        Ok(JsonValue::Object(result))
    }

    /// Project a nested value, adding typename if configured.
    #[allow(clippy::only_used_in_recursion)]
    fn project_nested_value(&self, value: &JsonValue, field: &FieldMapping) -> Result<JsonValue> {
        match value {
            JsonValue::Object(obj) => {
                // If this field has nested typename, add it
                if let Some(ref typename) = field.nested_typename {
                    let mut result = Map::new();
                    result.insert("__typename".to_string(), JsonValue::String(typename.clone()));

                    // If we have nested field mappings, use them; otherwise copy all fields
                    if let Some(ref nested_fields) = field.nested_fields {
                        for nested_field in nested_fields {
                            if let Some(nested_value) = obj.get(&nested_field.source) {
                                let projected = self.project_nested_value(nested_value, nested_field)?;
                                result.insert(nested_field.output.clone(), projected);
                            }
                        }
                    } else {
                        // No specific field mappings - copy all fields from source
                        for (k, v) in obj {
                            result.insert(k.clone(), v.clone());
                        }
                    }
                    Ok(JsonValue::Object(result))
                } else {
                    // No typename for this nested object - return as-is
                    Ok(value.clone())
                }
            }
            JsonValue::Array(arr) => {
                // For arrays of objects, add typename to each element
                if field.nested_typename.is_some() {
                    let projected: Result<Vec<JsonValue>> = arr
                        .iter()
                        .map(|item| self.project_nested_value(item, field))
                        .collect();
                    Ok(JsonValue::Array(projected?))
                } else {
                    Ok(value.clone())
                }
            }
            _ => Ok(value.clone()),
        }
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
    /// Create new result projector from field names (no aliases).
    #[must_use]
    pub fn new(fields: Vec<String>) -> Self {
        Self {
            mapper: ProjectionMapper::new(fields),
        }
    }

    /// Create new result projector with field mappings (supports aliases).
    #[must_use]
    pub fn with_mappings(fields: Vec<FieldMapping>) -> Self {
        Self {
            mapper: ProjectionMapper::with_mappings(fields),
        }
    }

    /// Set `__typename` to include in all projected objects.
    ///
    /// Per GraphQL spec §2.7, `__typename` returns the name of the object type.
    /// This should be called when the client requests `__typename` in the selection set.
    #[must_use]
    pub fn with_typename(mut self, typename: impl Into<String>) -> Self {
        self.mapper = self.mapper.with_typename(typename);
        self
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

    /// Add __typename field to SQL-projected data.
    ///
    /// For data that has already been projected at the SQL level, we only need to add
    /// the `__typename` field in Rust. This is much faster than projecting all fields
    /// since the SQL already filtered to only requested fields.
    ///
    /// # Arguments
    ///
    /// * `projected_data` - JSONB data already projected by SQL
    /// * `typename` - GraphQL type name to add
    ///
    /// # Returns
    ///
    /// New JSONB value with `__typename` field added
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Database already returned only: { "id": "123", "name": "Alice" }
    /// let result = projector.add_typename_only(
    ///     &JsonbValue::new(json!({ "id": "123", "name": "Alice" })),
    ///     "User"
    /// ).unwrap();
    ///
    /// // Result: { "id": "123", "name": "Alice", "__typename": "User" }
    /// ```
    pub fn add_typename_only(
        &self,
        projected_data: &JsonbValue,
        typename: &str,
    ) -> Result<JsonValue> {
        let value = projected_data.as_value();

        match value {
            JsonValue::Object(map) => {
                let mut result = map.clone();
                result.insert("__typename".to_string(), JsonValue::String(typename.to_string()));
                Ok(JsonValue::Object(result))
            }
            JsonValue::Array(arr) => {
                let updated: Result<Vec<JsonValue>> = arr
                    .iter()
                    .map(|item| {
                        if let JsonValue::Object(obj) = item {
                            let mut result = obj.clone();
                            result.insert("__typename".to_string(), JsonValue::String(typename.to_string()));
                            Ok(JsonValue::Object(result))
                        } else {
                            Ok(item.clone())
                        }
                    })
                    .collect();
                Ok(JsonValue::Array(updated?))
            }
            v => Ok(v.clone()),
        }
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

    #[test]
    fn test_add_typename_only_object() {
        let projector = ResultProjector::new(vec!["id".to_string()]);

        let data = json!({ "id": "123", "name": "Alice" });
        let jsonb = JsonbValue::new(data);
        let result = projector.add_typename_only(&jsonb, "User").unwrap();

        assert_eq!(
            result,
            json!({ "id": "123", "name": "Alice", "__typename": "User" })
        );
    }

    #[test]
    fn test_add_typename_only_array() {
        let projector = ResultProjector::new(vec!["id".to_string()]);

        let data = json!([
            { "id": "1", "name": "Alice" },
            { "id": "2", "name": "Bob" }
        ]);
        let jsonb = JsonbValue::new(data);
        let result = projector.add_typename_only(&jsonb, "User").unwrap();

        assert_eq!(
            result,
            json!([
                { "id": "1", "name": "Alice", "__typename": "User" },
                { "id": "2", "name": "Bob", "__typename": "User" }
            ])
        );
    }

    #[test]
    fn test_add_typename_only_primitive() {
        let projector = ResultProjector::new(vec![]);

        let jsonb = JsonbValue::new(json!("string_value"));
        let result = projector.add_typename_only(&jsonb, "String").unwrap();

        // Primitive values are returned unchanged (cannot add __typename to string)
        assert_eq!(result, json!("string_value"));
    }

    // ========================================================================
    // Alias tests
    // ========================================================================

    #[test]
    fn test_field_mapping_simple() {
        let mapping = FieldMapping::simple("name");
        assert_eq!(mapping.source, "name");
        assert_eq!(mapping.output, "name");
    }

    #[test]
    fn test_field_mapping_aliased() {
        let mapping = FieldMapping::aliased("author", "writer");
        assert_eq!(mapping.source, "author");
        assert_eq!(mapping.output, "writer");
    }

    #[test]
    fn test_project_with_alias() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::aliased("author", "writer"),
        ]);

        let data = json!({
            "id": "123",
            "author": { "name": "Alice" },
            "title": "Hello World"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        // "author" should be output as "writer"
        assert_eq!(
            result,
            json!({
                "id": "123",
                "writer": { "name": "Alice" }
            })
        );
    }

    #[test]
    fn test_project_with_typename() {
        let mapper = ProjectionMapper::new(vec!["id".to_string(), "name".to_string()])
            .with_typename("User");

        let data = json!({
            "id": "123",
            "name": "Alice",
            "email": "alice@example.com"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "User",
                "id": "123",
                "name": "Alice"
            })
        );
    }

    #[test]
    fn test_project_with_alias_and_typename() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::aliased("author", "writer"),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "author": { "name": "Alice" },
            "title": "Hello"
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "writer": { "name": "Alice" }
            })
        );
    }

    #[test]
    fn test_result_projector_with_typename() {
        let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()])
            .with_typename("User");

        let data = json!({ "id": "1", "name": "Alice", "email": "alice@example.com" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "User",
                "id": "1",
                "name": "Alice"
            })
        );
    }

    #[test]
    fn test_result_projector_list_with_typename() {
        let projector = ResultProjector::new(vec!["id".to_string()])
            .with_typename("User");

        let results = vec![
            JsonbValue::new(json!({ "id": "1", "name": "Alice" })),
            JsonbValue::new(json!({ "id": "2", "name": "Bob" })),
        ];
        let result = projector.project_results(&results, true).unwrap();

        assert_eq!(
            result,
            json!([
                { "__typename": "User", "id": "1" },
                { "__typename": "User", "id": "2" }
            ])
        );
    }

    #[test]
    fn test_result_projector_with_mappings() {
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::aliased("full_name", "name"),
        ]);

        let data = json!({ "id": "1", "full_name": "Alice Smith", "email": "alice@example.com" });
        let results = vec![JsonbValue::new(data)];
        let result = projector.project_results(&results, false).unwrap();

        // "full_name" should be output as "name"
        assert_eq!(
            result,
            json!({
                "id": "1",
                "name": "Alice Smith"
            })
        );
    }

    // ========================================================================
    // Nested typename tests
    // ========================================================================

    #[test]
    fn test_nested_object_typename() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("title"),
            FieldMapping::nested_object("author", "User", vec![
                FieldMapping::simple("id"),
                FieldMapping::simple("name"),
            ]),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "title": "Hello World",
            "author": {
                "id": "user-1",
                "name": "Alice",
                "email": "alice@example.com"
            }
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "title": "Hello World",
                "author": {
                    "__typename": "User",
                    "id": "user-1",
                    "name": "Alice"
                }
            })
        );
    }

    #[test]
    fn test_nested_array_typename() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("name"),
            FieldMapping::nested_object("posts", "Post", vec![
                FieldMapping::simple("id"),
                FieldMapping::simple("title"),
            ]),
        ])
        .with_typename("User");

        let data = json!({
            "id": "user-1",
            "name": "Alice",
            "posts": [
                { "id": "post-1", "title": "First Post", "views": 100 },
                { "id": "post-2", "title": "Second Post", "views": 200 }
            ]
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "User",
                "id": "user-1",
                "name": "Alice",
                "posts": [
                    { "__typename": "Post", "id": "post-1", "title": "First Post" },
                    { "__typename": "Post", "id": "post-2", "title": "Second Post" }
                ]
            })
        );
    }

    #[test]
    fn test_deeply_nested_typename() {
        // Post -> author (User) -> company (Company)
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::nested_object("author", "User", vec![
                FieldMapping::simple("name"),
                FieldMapping::nested_object("company", "Company", vec![
                    FieldMapping::simple("name"),
                ]),
            ]),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "author": {
                "name": "Alice",
                "company": {
                    "name": "Acme Corp",
                    "revenue": 1000000
                }
            }
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "author": {
                    "__typename": "User",
                    "name": "Alice",
                    "company": {
                        "__typename": "Company",
                        "name": "Acme Corp"
                    }
                }
            })
        );
    }

    #[test]
    fn test_nested_object_with_alias_and_typename() {
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::nested_object_aliased("author", "writer", "User", vec![
                FieldMapping::simple("id"),
                FieldMapping::simple("name"),
            ]),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "author": {
                "id": "user-1",
                "name": "Alice"
            }
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        // "author" should be output as "writer" with typename
        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "writer": {
                    "__typename": "User",
                    "id": "user-1",
                    "name": "Alice"
                }
            })
        );
    }

    #[test]
    fn test_nested_without_specific_fields() {
        // When nested_fields is None, all source fields are copied
        let mapper = ProjectionMapper::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("author").with_nested_typename("User"),
        ])
        .with_typename("Post");

        let data = json!({
            "id": "post-1",
            "author": {
                "id": "user-1",
                "name": "Alice",
                "email": "alice@example.com"
            }
        });

        let jsonb = JsonbValue::new(data);
        let result = mapper.project(&jsonb).unwrap();

        // All author fields should be copied, plus __typename
        assert_eq!(
            result,
            json!({
                "__typename": "Post",
                "id": "post-1",
                "author": {
                    "__typename": "User",
                    "id": "user-1",
                    "name": "Alice",
                    "email": "alice@example.com"
                }
            })
        );
    }
}
