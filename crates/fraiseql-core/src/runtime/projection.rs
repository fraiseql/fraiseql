//! Result projection - transforms JSONB database results to GraphQL responses.

use serde_json::{Map, Value as JsonValue};

use crate::{
    db::types::JsonbValue,
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    schema::{CompiledSchema, FieldDefinition, FieldType},
    utils::casing::{to_camel_case, to_snake_case},
};

/// Field mapping for projection with alias support.
#[derive(Debug, Clone)]
pub struct FieldMapping {
    /// JSONB key name (source).
    pub source:          String,
    /// Output key name (alias if different from source).
    pub output:          String,
    /// Fallback stored key, tried when `source` is not present in the row.
    ///
    /// Populated by every constructor from `stored_key_candidates`. It is not
    /// support for `camelCase` *storage* — a view exposes `snake_case` keys and
    /// columns, and the SQL projection generator has no fallback at all. It is
    /// there because this projector runs over **two different maps**: the raw
    /// stored document, whose keys are `snake_case`, and the output of
    /// `jsonb_build_object`, whose keys are the camelCase *response* names the
    /// SQL projection already emitted. One mapping has to read both.
    ///
    /// Until #1271 this field had **no producer** anywhere in the workspace —
    /// its documented job was described and never performed, and the primary
    /// `source` was the declared name verbatim, so the raw-document case was
    /// simply unreadable.
    pub source_fallback: Option<String>,
    /// For nested object fields, the typename to add.
    /// This enables `__typename` to be added recursively to nested objects.
    pub nested_typename: Option<String>,
    /// Nested field mappings (for related objects).
    pub nested_fields:   Option<Vec<FieldMapping>>,
    /// The schema positively declares this field a **scalar** (#1192).
    ///
    /// Set by [`ProjectionMapper::with_declared_scalars`], which is the only
    /// thing that knows. It gates the JSON-text re-parse below: a value the
    /// database handed back as a string is re-read as an object or an array when
    /// it parses as one, to recover a nested object the SQL side extracted with
    /// `->>` — and the comment justifying that said scalar strings "won't parse
    /// as Object/Array, so this is safe for all field types". That premise is
    /// false for exactly the rows a text column full of serialized JSON exists
    /// to carry: an audit payload, a webhook body, an imported document. A field
    /// declared `String` was returned as the value its text encodes, so the
    /// response violated the schema the server publishes, per row, depending on
    /// whether that row's text happened to parse.
    ///
    /// Defaults to `false` — *not known to be scalar* — so a mapping built
    /// without schema knowledge keeps the recovery behaviour it had.
    pub declared_scalar: bool,
}

/// The stored JSONB key a declared field name resolves to, plus the legacy
/// spelling to fall back on.
///
/// **The single definition of the rule**, shared by the two Rust projectors:
/// [`FieldMapping`]'s constructors resolve it once per query, and
/// [`lookup_source`] resolves it per lookup for the selection-driven
/// [`project_entity`]. Before #1271 they were two implementations and only one
/// of them was right — the mapper read the declared name verbatim, so a
/// `camelCase` field over a `snake_case` view was silently absent from a 200.
///
/// `snake_case` first, because that is the key the SQL projection generator
/// (`projection_generator::render_field`) and the `where` parser both derive;
/// the `camelCase` spelling second, for stored rows built with the surface
/// casing. A single-word name yields the same string twice, so it has no
/// fallback.
fn stored_key_candidates(field_name: &str) -> (String, Option<String>) {
    let snake = to_snake_case(field_name);
    let camel = to_camel_case(field_name);
    let fallback = (camel != snake).then_some(camel);
    (snake, fallback)
}

impl FieldMapping {
    /// Create a simple field mapping (no alias).
    ///
    /// `name` is the **declared field name**; the stored key it reads is
    /// derived by `stored_key_candidates`, not taken verbatim.
    ///
    /// ```rust
    /// # use fraiseql_core::runtime::FieldMapping;
    /// let mapping = FieldMapping::simple("fkUser");
    /// assert_eq!(mapping.source, "fk_user");
    /// assert_eq!(mapping.output, "fkUser");
    /// ```
    #[must_use]
    pub fn simple(name: impl Into<String>) -> Self {
        let name = name.into();
        let (source, source_fallback) = stored_key_candidates(&name);
        Self {
            source,
            output: name,
            source_fallback,
            nested_typename: None,
            nested_fields: None,
            declared_scalar: false,
        }
    }

    /// Create a field mapping with an alias.
    ///
    /// `source` is the **declared field name** and `alias` the response key —
    /// the same split the SQL projection generator makes, where an aliased
    /// field `myName: fullName` reads `data->>'full_name'` and is emitted as
    /// `myName` (#418). The stored key is derived from `source` by
    /// `stored_key_candidates`.
    #[must_use]
    pub fn aliased(source: impl Into<String>, alias: impl Into<String>) -> Self {
        let (source, source_fallback) = stored_key_candidates(&source.into());
        Self {
            source,
            output: alias.into(),
            source_fallback,
            nested_typename: None,
            nested_fields: None,
            declared_scalar: false,
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
        let (source, source_fallback) = stored_key_candidates(&name);
        Self {
            source,
            output: name,
            source_fallback,
            nested_typename: Some(typename.into()),
            nested_fields: Some(fields),
            declared_scalar: false,
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
        let (source, source_fallback) = stored_key_candidates(&source.into());
        Self {
            source,
            output: alias.into(),
            source_fallback,
            nested_typename: Some(typename.into()),
            nested_fields: Some(fields),
            declared_scalar: false,
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

    /// Read this field's value out of a stored JSONB object.
    ///
    /// The primary key first, then [`source_fallback`](Self::source_fallback) —
    /// both resolved once at construction, so this stays allocation-free on the
    /// per-row path.
    #[must_use]
    pub fn lookup_in<'a>(
        &self,
        map: &'a serde_json::Map<String, JsonValue>,
    ) -> Option<&'a JsonValue> {
        map.get(&self.source)
            .or_else(|| self.source_fallback.as_ref().and_then(|fb| map.get(fb)))
    }
}

/// Projection mapper - maps JSONB fields to GraphQL selection set.
#[derive(Debug, Clone)]
pub struct ProjectionMapper {
    /// Fields to project (with optional aliases).
    pub fields:          Vec<FieldMapping>,
    /// Optional `__typename` value to add to each object.
    pub typename:        Option<String>,
    /// When `true`, `__typename` is injected unconditionally regardless of selection set.
    /// Used by federation `_entities` resolver where the gateway always expects `__typename`.
    pub federation_mode: bool,
}

/// Does this declared type's value space **exclude** JSON objects and arrays?
///
/// The question [`FieldMapping::declared_scalar`] needs, and deliberately not
/// [`FieldType::is_scalar`], which answers a different one. `is_scalar` is true
/// of `Json` — whose value space is *all* JSON — and of the vector types, which
/// serialize as arrays. Marking either would stop the text-recovery re-parse
/// from firing where it is doing its job.
///
/// A **custom** scalar is also left unmarked. A project defines its own
/// serialization, so whether one of its values can be a JSON object is a
/// question this module cannot adjudicate — the same rule the argument-value
/// validator follows: narrow to what is positively known, never widen. #1192
/// raised the custom-scalar case explicitly and this is the answer to it.
const fn excludes_json_composites(field_type: &FieldType) -> bool {
    matches!(
        field_type,
        FieldType::String
            | FieldType::Int
            | FieldType::Float
            | FieldType::Boolean
            | FieldType::Id
            | FieldType::DateTime
            | FieldType::Date
            | FieldType::Time
            | FieldType::Uuid
            | FieldType::Decimal
    )
}

impl ProjectionMapper {
    /// Create new projection mapper from field names (no aliases).
    #[must_use]
    pub fn new(fields: Vec<String>) -> Self {
        Self {
            fields:          fields.into_iter().map(FieldMapping::simple).collect(),
            typename:        None,
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

    /// Mark every mapping whose field `entity_type` declares as a **scalar**
    /// (#1192), so the JSON-text recovery in [`FieldMapping::declared_scalar`]
    /// does not fire on a `String` whose characters happen to be JSON.
    ///
    /// Lookup is by the mapping's `output` name — the GraphQL field name, which
    /// is what a type definition carries — falling back to `source` for a
    /// mapping built from a stored key. A field the schema does not describe
    /// stays unmarked: this narrows the recovery to what is positively known,
    /// and never widens it.
    #[must_use]
    pub fn with_declared_scalars(mut self, schema: &CompiledSchema, entity_type: &str) -> Self {
        let Some(type_def) = schema.find_type(entity_type) else {
            return self;
        };
        for mapping in &mut self.fields {
            let declared = type_def
                .fields
                .iter()
                .find(|f| f.name == mapping.output || f.name == mapping.source);
            if let Some(fd) = declared {
                mapping.declared_scalar = excludes_json_composites(&fd.field_type);
            }
        }
        self
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

        // Project fields with alias support, resolving each stored key by the
        // rule every consumer of a declared field name shares (#1271).
        for field in &self.fields {
            if let Some(value) = field.lookup_in(map) {
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
                            // #1271: the nested branch reads its own sub-map and
                            // had its own verbatim lookup, so it needs the rule
                            // explicitly rather than inheriting it from above.
                            if let Some(nested_value) = nested_field.lookup_in(obj) {
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
                //
                // #1192: unless the schema declares the field a scalar. This used
                // to run for every field type, justified by "scalar strings won't
                // parse as Object/Array" — true of `"hello"`, false of the
                // serialized JSON that text columns routinely hold, and the
                // response then carried an object where the published schema
                // promised a string.
                if field.declared_scalar {
                    return Ok(value.clone());
                }
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
    ///
    /// A row that fails projection propagates as an error rather than being
    /// silently dropped from the result set (#736): `project_json_object` is
    /// currently infallible, but if it ever gains an error path, an array with
    /// a bad element must not shrink without a trace.
    fn project_json_array(&self, arr: &[JsonValue]) -> Result<JsonValue> {
        let projected: Result<Vec<JsonValue>> = arr
            .iter()
            .map(|item| {
                if let JsonValue::Object(obj) = item {
                    self.project_json_object(obj)
                } else {
                    Ok(item.clone())
                }
            })
            .collect();

        Ok(JsonValue::Array(projected?))
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

    /// Mark the mappings the schema declares scalar (#1192).
    ///
    /// See [`ProjectionMapper::with_declared_scalars`].
    #[must_use]
    pub fn with_declared_scalars(mut self, schema: &CompiledSchema, entity_type: &str) -> Self {
        self.mapper = self.mapper.with_declared_scalars(schema, entity_type);
        self
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
    /// * `results` - Database results as JSONB values (borrowed; not mutated).
    /// * `is_list` - Whether the query returns a list.
    ///
    /// # Returns
    ///
    /// A freshly-allocated GraphQL-compatible JSON response. The projector
    /// **never aliases the input slice**: each field of every `JsonbValue` is
    /// cloned out into a new `serde_json::Value` tree (see F029 — ownership
    /// contract is on `JsonbValue` itself).
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

/// Maximum nesting depth for entity projection.
///
/// Mirrors the SQL projection generator's depth cap so the Rust (mutation) and
/// SQL (query) paths stop recursing at the same level and fall back to returning
/// the stored sub-blob identically.
const MAX_ENTITY_PROJECTION_DEPTH: usize = 4;

/// Project an entity-shaped JSONB value into a GraphQL response object, mirroring
/// the query path's SQL projection exactly.
///
/// This is the **single, canonical** entity projector, shared by the mutation
/// success arm (projecting the returned entity) and the error arm (projecting the
/// error metadata). It is behaviourally equivalent to the SQL query projection
/// (`projection_generator::render_field`):
///
/// - **output key** = the selection's response key (the camelCase GraphQL surface name, honouring
///   aliases),
/// - **source key** = [`to_snake_case`](crate::utils::casing::to_snake_case) of the field name (the
///   stored JSONB key), with a `camelCase` fallback for legacy metadata that used the surface
///   casing,
/// - **single object fields** with a sub-selection are recursed (depth-capped at
///   `MAX_ENTITY_PROJECTION_DEPTH`),
/// - **list fields, scalar fields, sub-selection-less object fields and over-depth fields** pass
///   through their stored value (matching the SQL side's full-sub-blob fallback),
/// - **`__typename`** is emitted only where the client selected it.
///
/// `type_name` is the concrete GraphQL object type of `entity` (resolved by the
/// caller). `selections` is the result selection set for this level, with inline
/// `... on T` fragments preserved; an **empty** slice means "no field filtering"
/// and returns the stored entity unchanged.
#[must_use]
pub fn project_entity(
    entity: &JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
) -> JsonValue {
    if selections.is_empty() {
        // No selection set (e.g. the REST/typed path) — no field filtering.
        return entity.clone();
    }
    project_entity_at(entity, type_name, selections, schema, 0)
}

fn project_entity_at(
    entity: &JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    depth: usize,
) -> JsonValue {
    let JsonValue::Object(obj) = entity else {
        // Not an object (e.g. null) — nothing to project.
        return entity.clone();
    };
    let type_def = schema.find_type(type_name);
    let mut out = Map::new();

    for sel in effective_selections(selections, type_name, schema) {
        if sel.name == "__typename" {
            out.insert(sel.response_key().to_string(), JsonValue::String(type_name.to_string()));
            continue;
        }
        let field_def =
            type_def.and_then(|td| td.fields.iter().find(|f| f.name.as_str() == sel.name));
        let Some(value) = lookup_source(obj, &sel.name) else {
            // Absent stored key → omit (matches query/SQL behaviour: absent stays absent).
            continue;
        };
        let projected = project_field_value(value, field_def, &sel.nested_fields, schema, depth);
        out.insert(sel.response_key().to_string(), projected);
    }

    JsonValue::Object(out)
}

/// Project a single field value, recursing into nested object selections.
fn project_field_value(
    value: &JsonValue,
    field_def: Option<&FieldDefinition>,
    nested: &[FieldSelection],
    schema: &CompiledSchema,
    depth: usize,
) -> JsonValue {
    // Recurse only into single (non-list) object fields that carry a sub-selection,
    // within the depth cap — exactly as the SQL projector decides. Everything else
    // (scalars, lists, sub-selection-less objects, over-depth) returns the stored
    // value verbatim, matching the SQL full-sub-blob fallback.
    if depth < MAX_ENTITY_PROJECTION_DEPTH && !nested.is_empty() {
        if let Some(fd) = field_def {
            if !fd.field_type.is_scalar() && !fd.field_type.is_list() {
                if let Some(child_type) = fd.field_type.type_name() {
                    match value {
                        JsonValue::Object(_) => {
                            return project_entity_at(value, child_type, nested, schema, depth + 1);
                        },
                        // The DB sometimes returns a nested object as a JSON string
                        // (when extracted via `->>`). Re-parse and project it.
                        JsonValue::String(s) => {
                            if let Ok(parsed @ JsonValue::Object(_)) =
                                serde_json::from_str::<JsonValue>(s)
                            {
                                return project_entity_at(
                                    &parsed,
                                    child_type,
                                    nested,
                                    schema,
                                    depth + 1,
                                );
                            }
                        },
                        _ => {},
                    }
                }
            } else if fd.field_type.is_list() {
                // #489: a nested LIST-of-object field. The SQL/stored side returns the
                // raw aggregated sub-blob (snake_case keys, unselected keys included);
                // project every element at the element type — the same recasing +
                // selection-set projection applied to single nested objects — so nested
                // list output matches top-level and nested-object output.
                if let Some(child_type) = fd.field_type.inner_type().and_then(FieldType::type_name)
                {
                    if let JsonValue::Array(arr) = value {
                        return JsonValue::Array(
                            arr.iter()
                                .map(|el| {
                                    project_list_element(el, child_type, nested, schema, depth)
                                })
                                .collect(),
                        );
                    }
                    // The DB sometimes returns the whole array as a JSON string
                    // (`->>` text extraction). Re-parse and project each element.
                    if let JsonValue::String(s) = value {
                        if let Ok(JsonValue::Array(arr)) = serde_json::from_str::<JsonValue>(s) {
                            return JsonValue::Array(
                                arr.iter()
                                    .map(|el| {
                                        project_list_element(el, child_type, nested, schema, depth)
                                    })
                                    .collect(),
                            );
                        }
                    }
                }
            }
        }
    }
    value.clone()
}

/// Project one element of a nested list-of-object field (#489): recase + project
/// object elements at `child_type`, passing scalars and non-object strings through.
fn project_list_element(
    element: &JsonValue,
    child_type: &str,
    nested: &[FieldSelection],
    schema: &CompiledSchema,
    depth: usize,
) -> JsonValue {
    match element {
        JsonValue::Object(_) => project_entity_at(element, child_type, nested, schema, depth + 1),
        // An element itself text-encoded as a JSON object.
        JsonValue::String(s) => match serde_json::from_str::<JsonValue>(s) {
            Ok(parsed @ JsonValue::Object(_)) => {
                project_entity_at(&parsed, child_type, nested, schema, depth + 1)
            },
            _ => element.clone(),
        },
        _ => element.clone(),
    }
}

/// #489 — recase + project nested LIST-of-object fields left raw by the SQL projection.
///
/// The query path projects top-level fields and nested single objects at the SQL level
/// (`jsonb_build_object`), but list fields fall back to the raw stored sub-blob
/// (`snake_case` keys, unselected keys included). This walks the already-projected
/// `value` guided by the selection set and, for each list-of-object field (up to the
/// projection depth cap), replaces its elements with the fully projected form via
/// [`project_entity`] at the element type. Non-list fields are left untouched — the SQL
/// side already projected them; single-object fields are only recursed into to reach
/// any lists nested inside them.
///
/// `type_name` is the entity type of `value` (or of each element when `value` is a
/// list-returning query's top-level array); `selections` is the entity-level selection
/// set (the query field's `nested_fields`).
pub fn project_nested_lists(
    value: &mut JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
) {
    project_nested_lists_at(value, type_name, selections, schema, 0);
}

fn project_nested_lists_at(
    value: &mut JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    depth: usize,
) {
    if depth >= MAX_ENTITY_PROJECTION_DEPTH {
        return;
    }
    match value {
        // A list-returning query: each element is an entity of `type_name` (the list
        // itself is not a nesting level).
        JsonValue::Array(arr) => {
            for el in arr.iter_mut() {
                project_nested_lists_at(el, type_name, selections, schema, depth);
            }
        },
        JsonValue::Object(obj) => {
            let type_def = schema.find_type(type_name);
            for sel in effective_selections(selections, type_name, schema) {
                if sel.name == "__typename" {
                    continue;
                }
                let Some(fd) =
                    type_def.and_then(|td| td.fields.iter().find(|f| f.name.as_str() == sel.name))
                else {
                    continue;
                };
                if fd.field_type.is_scalar() {
                    continue;
                }
                let key = sel.response_key();
                if fd.field_type.is_list() {
                    // The element type of a list-of-object field. `project_entity` fully
                    // projects each raw element, including any lists nested inside it.
                    if let Some(child_type) =
                        fd.field_type.inner_type().and_then(FieldType::type_name)
                    {
                        if let Some(JsonValue::Array(arr)) = obj.get_mut(key) {
                            for el in arr.iter_mut() {
                                *el = project_entity(el, child_type, &sel.nested_fields, schema);
                            }
                        }
                    }
                } else if let Some(child_type) = fd.field_type.type_name() {
                    // Single object already projected by the SQL side — recurse to reach
                    // any lists nested inside it.
                    if let Some(child) = obj.get_mut(key) {
                        project_nested_lists_at(
                            child,
                            child_type,
                            &sel.nested_fields,
                            schema,
                            depth + 1,
                        );
                    }
                }
            }
        },
        _ => {},
    }
}

/// Stamp `__typename` on the nested single objects whose selection asked for it.
///
/// `__typename` is `String!` (GraphQL spec § Type Name Introspection): it can
/// never be null, and a requested field can never be absent. It is a meta-field,
/// not a JSONB key, so it is stripped from the SQL projection at every depth —
/// projecting it would emit `data->>'__typename'`, a literal NULL (the symptom
/// #912 reports). Something on the Rust side has to put it back.
///
/// Two of the three levels already had an owner: the root object is stamped by
/// [`ResultProjector::configure_typename_from_selections`], and list elements by
/// [`project_entity`]. A *single* nested object had none, so a requested nested
/// `__typename` was dropped from the response with no error at all.
///
/// The key is inserted at the position the client's selection set puts it, not
/// appended: a response's fields follow the query's order (spec § Response
/// Format).
pub fn stamp_nested_typenames(
    value: &mut JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
) {
    stamp_nested_typenames_at(value, type_name, selections, schema, 0);
}

fn stamp_nested_typenames_at(
    value: &mut JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    depth: usize,
) {
    if depth >= MAX_ENTITY_PROJECTION_DEPTH {
        return;
    }
    match value {
        // A list-returning query: each element is an entity of `type_name` (the
        // list itself is not a nesting level).
        JsonValue::Array(arr) => {
            for el in arr.iter_mut() {
                stamp_nested_typenames_at(el, type_name, selections, schema, depth);
            }
        },
        JsonValue::Object(obj) => {
            let type_def = schema.find_type(type_name);
            for sel in effective_selections(selections, type_name, schema) {
                // This level's own `__typename` belongs to whoever built this
                // object; only children are this pass's business.
                if sel.name == "__typename" {
                    continue;
                }
                let Some(fd) =
                    type_def.and_then(|td| td.fields.iter().find(|f| f.name.as_str() == sel.name))
                else {
                    continue;
                };
                // Lists are projected element-by-element through `project_entity`,
                // which stamps them already.
                if fd.field_type.is_scalar() || fd.field_type.is_list() {
                    continue;
                }
                let Some(child_type) = fd.field_type.type_name() else {
                    continue;
                };
                let key = sel.response_key().to_string();
                let child_selections = &sel.nested_fields;
                let Some(child) = obj.get_mut(&key) else {
                    continue;
                };
                if let JsonValue::Object(child_obj) = child {
                    reinsert_typename_in_order(child_obj, child_selections, child_type, schema);
                }
                stamp_nested_typenames_at(child, child_type, child_selections, schema, depth + 1);
            }
        },
        _ => {},
    }
}

/// Rebuild `obj` with its selected `__typename` keys in their requested
/// positions, leaving every other key in its existing order.
///
/// `serde_json::Map` preserves insertion order (`preserve_order`), so a plain
/// insert would append — putting `__typename` last however the client wrote it.
fn reinsert_typename_in_order(
    obj: &mut Map<String, JsonValue>,
    selections: &[FieldSelection],
    type_name: &str,
    schema: &CompiledSchema,
) {
    // Nothing to insert → leave the object, and its existing order, untouched.
    if !effective_selections(selections, type_name, schema)
        .iter()
        .any(|sel| sel.name == "__typename")
    {
        return;
    }

    let mut rebuilt = Map::new();
    for sel in effective_selections(selections, type_name, schema) {
        let key = sel.response_key();
        if sel.name == "__typename" {
            rebuilt.insert(key.to_string(), JsonValue::String(type_name.to_string()));
        } else if let Some(v) = obj.get(key) {
            rebuilt.insert(key.to_string(), v.clone());
        }
    }
    // Anything the selection walker did not account for keeps its place at the
    // end rather than being dropped — this pass adds a key, it never removes one.
    for (k, v) in obj.iter() {
        if !rebuilt.contains_key(k) {
            rebuilt.insert(k.clone(), v.clone());
        }
    }
    *obj = rebuilt;
}

/// Look up a field's stored value: canonical `snake_case` key first, then a
/// `camelCase` fallback for legacy metadata that used the GraphQL surface casing.
fn lookup_source<'a>(obj: &'a Map<String, JsonValue>, field_name: &str) -> Option<&'a JsonValue> {
    let (snake, camel) = stored_key_candidates(field_name);
    obj.get(&snake).or_else(|| camel.and_then(|c| obj.get(&c)))
}

/// Flatten a selection set for a concrete object type: direct fields, plus the
/// contents of any inline fragment `... on T` (or on an interface `T` implements).
/// Resolve inline `... on T` fragments in `selections` against `type_name`,
/// returning the flat list of field selections that apply to it.
///
/// Shared with the mutation runner's cascade projection, which navigates the
/// payload/envelope selection sets (`... on <Name>Payload`, `... on <EntityType>`)
/// the same way the entity projector does.
pub fn effective_selections<'a>(
    selections: &'a [FieldSelection],
    type_name: &str,
    schema: &CompiledSchema,
) -> Vec<&'a FieldSelection> {
    let mut out = Vec::new();
    for sel in selections {
        if let Some(frag_type) = sel.name.strip_prefix("...on ") {
            let frag_type = frag_type.trim();
            let applies = frag_type == type_name
                || schema
                    .find_type(type_name)
                    .is_some_and(|td| td.implements.iter().any(|i| i == frag_type));
            if applies {
                out.extend(effective_selections(&sel.nested_fields, type_name, schema));
            }
        } else {
            out.push(sel);
        }
    }
    out
}
