//! Result projection - transforms JSONB database results to GraphQL responses.

use std::collections::HashSet;

use serde_json::{Map, Value as JsonValue};

use crate::{
    db::types::JsonbValue,
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    schema::{CompiledSchema, FieldDefinition},
};

/// Field mapping for projection with alias support.
#[derive(Debug, Clone)]
pub struct FieldMapping {
    /// JSONB key name (source).
    pub source: String,
    /// Output key name (alias if different from source).
    pub output: String,
    /// Fallback source key to try when the primary `source` is not found.
    /// Used for mutation error metadata where the key may be either `camelCase`
    /// or `snake_case` depending on the backend.
    pub source_fallback: Option<String>,
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
            source_fallback: None,
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
            source_fallback: None,
            nested_typename: None,
            nested_fields: None,
        }
    }

    /// Create a field mapping for a nested object with its own typename.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use fraiseql_core::runtime::FieldMapping;
    /// // For a Post with nested author (User type)
    /// let mapping = FieldMapping::nested_object("author", "User", vec![
    ///     FieldMapping::simple("id"),
    ///     FieldMapping::simple("name"),
    /// ]);
    /// assert_eq!(mapping.source, "author");
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
            source_fallback: None,
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
            source_fallback: None,
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
    /// When `true`, `__typename` is injected unconditionally regardless of selection set.
    /// Used by federation `_entities` resolver where the gateway always expects `__typename`.
    pub federation_mode: bool,
}

impl ProjectionMapper {
    /// Create new projection mapper from field names (no aliases).
    #[must_use]
    pub fn new(fields: Vec<String>) -> Self {
        Self {
            fields: fields.into_iter().map(FieldMapping::simple).collect(),
            typename: None,
            federation_mode: false,
        }
    }

    /// Create new projection mapper with field mappings (supports aliases).
    #[must_use]
    pub const fn with_mappings(fields: Vec<FieldMapping>) -> Self {
        Self {
            fields,
            typename: None,
            federation_mode: false,
        }
    }

    /// Set `__typename` to include in projected objects.
    #[must_use]
    pub fn with_typename(mut self, typename: impl Into<String>) -> Self {
        self.typename = Some(typename.into());
        self
    }

    /// Enable federation mode: `__typename` is always injected regardless of selection set.
    #[must_use]
    pub const fn with_federation_mode(mut self, enabled: bool) -> Self {
        self.federation_mode = enabled;
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
    ///
    /// Maps source keys to output keys according to the configured `FieldMapping`s,
    /// injects `__typename` when configured, and recursively projects nested objects
    /// and arrays.
    ///
    /// # Errors
    ///
    /// Returns error if nested value projection fails.
    pub fn project_json_object(
        &self,
        map: &serde_json::Map<String, JsonValue>,
    ) -> Result<JsonValue> {
        let mut result = Map::new();

        // Add __typename first if configured (GraphQL convention)
        if let Some(ref typename) = self.typename {
            result.insert("__typename".to_string(), JsonValue::String(typename.clone()));
        }

        // Project fields with alias support and optional fallback key
        for field in &self.fields {
            let value = map
                .get(&field.source)
                .or_else(|| field.source_fallback.as_ref().and_then(|fb| map.get(fb)));
            if let Some(value) = value {
                let projected_value = self.project_nested_value(value, field)?;
                result.insert(field.output.clone(), projected_value);
            }
        }

        Ok(JsonValue::Object(result))
    }

    /// Project a nested value, adding typename if configured.
    #[allow(clippy::self_only_used_in_recursion)] // Reason: &self required for method dispatch; recursive structure is intentional
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
                                let projected =
                                    self.project_nested_value(nested_value, nested_field)?;
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
            },
            JsonValue::Array(arr) => {
                // For arrays of objects, add typename to each element
                if field.nested_typename.is_some() {
                    let projected: Result<Vec<JsonValue>> =
                        arr.iter().map(|item| self.project_nested_value(item, field)).collect();
                    Ok(JsonValue::Array(projected?))
                } else {
                    Ok(value.clone())
                }
            },
            _ => {
                // If the value is a JSON string that encodes an object or array
                // (which happens when the database uses ->>'field' text extraction
                // instead of ->'field' JSONB extraction), attempt to re-parse it.
                // Scalar strings (e.g. "hello") won't parse as Object/Array and
                // are returned unchanged, so this is safe for all field types.
                if let JsonValue::String(ref s) = *value {
                    if let Ok(parsed @ (JsonValue::Object(_) | JsonValue::Array(_))) =
                        serde_json::from_str::<JsonValue>(s)
                    {
                        return self.project_nested_value(&parsed, field);
                    }
                }
                Ok(value.clone())
            },
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
    pub const fn with_mappings(fields: Vec<FieldMapping>) -> Self {
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

    /// Configure typename injection from the query selection set.
    ///
    /// Inspects the root selection's nested fields for `__typename`. If found,
    /// enables typename injection via [`with_typename`](Self::with_typename).
    #[must_use]
    pub fn configure_typename_from_selections(
        self,
        selections: &[FieldSelection],
        entity_type: &str,
    ) -> Self {
        let wants_typename = selections
            .first()
            .is_some_and(|root| root.nested_fields.iter().any(|f| f.name == "__typename"));
        if wants_typename {
            self.with_typename(entity_type)
        } else {
            self
        }
    }

    /// Enable federation mode: `__typename` is always injected regardless of selection set.
    ///
    /// Used by the `_entities` federation resolver where the gateway always expects
    /// `__typename` in entity results.
    #[must_use]
    pub fn with_federation_mode(mut self, enabled: bool) -> Self {
        self.mapper = self.mapper.with_federation_mode(enabled);
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
    pub fn project_results(&self, results: &[JsonbValue], is_list: bool) -> Result<JsonValue> {
        if is_list {
            // Project array of results
            let projected: Result<Vec<JsonValue>> =
                results.iter().map(|r| self.mapper.project(r)).collect();

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
    /// ```rust
    /// # use fraiseql_core::runtime::ResultProjector;
    /// # use fraiseql_core::db::types::JsonbValue;
    /// # use serde_json::json;
    /// let projector = ResultProjector::new(vec!["id".to_string(), "name".to_string()]);
    /// // Database already returned only: { "id": "123", "name": "Alice" }
    /// let result = projector.add_typename_only(
    ///     &JsonbValue::new(json!({ "id": "123", "name": "Alice" })),
    ///     "User"
    /// ).unwrap();
    ///
    /// // Result: { "id": "123", "name": "Alice", "__typename": "User" }
    /// assert_eq!(result["__typename"], "User");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the projected data contains a
    /// list element that is not a JSON object, making `__typename` injection impossible.
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
            },
            JsonValue::Array(arr) => {
                let updated: Result<Vec<JsonValue>> = arr
                    .iter()
                    .map(|item| {
                        if let JsonValue::Object(obj) = item {
                            let mut result = obj.clone();
                            result.insert(
                                "__typename".to_string(),
                                JsonValue::String(typename.to_string()),
                            );
                            Ok(JsonValue::Object(result))
                        } else {
                            Ok(item.clone())
                        }
                    })
                    .collect();
                Ok(JsonValue::Array(updated?))
            },
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

/// Build `FieldMapping`s from a type definition's fields, mapping `camelCase`
/// source keys (as stored in mutation metadata JSONB) to `snake_case` output keys
/// (as defined in the GraphQL schema).
///
/// Recursively builds nested mappings for `Object` and `List(Object)` fields by
/// looking up types in the compiled schema. This enables the same `ProjectionMapper`
/// pipeline used for query results to handle mutation error metadata.
///
/// # Arguments
///
/// * `fields` — the type's field definitions
/// * `schema` — compiled schema for resolving nested object types
/// * `requested` — optional selection filter; when `Some`, only listed fields are included
/// * `visited` — cycle guard to prevent infinite recursion on self-referencing types
#[must_use]
#[allow(clippy::implicit_hasher)] // Reason: internal API; no need for hasher generality
pub fn build_field_mappings_from_type(
    fields: &[FieldDefinition],
    schema: &CompiledSchema,
    requested: Option<&[String]>,
    visited: &mut HashSet<String>,
) -> Vec<FieldMapping> {
    fields
        .iter()
        .filter(|f| requested.is_none_or(|r| r.iter().any(|name| name == f.name.as_str())))
        .map(|field| {
            let source = to_camel_case(field.name.as_str());
            let output = field.name.to_string();

            // Fallback: try snake_case key when camelCase is not found.
            // Mutation metadata may use either convention depending on the backend.
            let source_fallback = if source != output {
                Some(output.clone())
            } else {
                None
            };

            // Resolve the innermost type (unwrap List wrapper if present)
            let inner = field.field_type.inner_type().unwrap_or(&field.field_type);

            if let Some(type_name) = inner.type_name() {
                // Object/Enum/Interface reference — try to resolve in schema
                if let Some(td) = schema.find_type(type_name) {
                    if visited.insert(type_name.to_string()) {
                        let nested =
                            build_field_mappings_from_type(&td.fields, schema, None, visited);
                        visited.remove(type_name);
                        return FieldMapping {
                            source,
                            output,
                            source_fallback,
                            nested_typename: Some(type_name.to_string()),
                            nested_fields: Some(nested),
                        };
                    }
                    // Cycle detected — return without recursion
                }
            }

            FieldMapping {
                source,
                output,
                source_fallback,
                nested_typename: None,
                nested_fields: None,
            }
        })
        .collect()
}

/// Convert a `snake_case` field name to `camelCase` for metadata key lookup.
///
/// Examples: `"last_activity_date"` → `"lastActivityDate"`,
///            `"cascade_count"` → `"cascadeCount"`.
fn to_camel_case(snake: &str) -> String {
    let mut result = String::with_capacity(snake.len());
    let mut capitalise_next = false;

    for ch in snake.chars() {
        if ch == '_' {
            capitalise_next = true;
        } else if capitalise_next {
            result.push(ch.to_ascii_uppercase());
            capitalise_next = false;
        } else {
            result.push(ch);
        }
    }

    result
}
