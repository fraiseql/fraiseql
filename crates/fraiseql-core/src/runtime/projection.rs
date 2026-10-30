//! Result projection - transforms JSONB database results to GraphQL responses.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use serde_json::{Map, Value as JsonValue};

use crate::{
    backend::types::JsonbValue,
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
    /// A localized field's chain for the request locale (#1513): a stored locale map is read
    /// through it. Set by [`ProjectionMapper::with_declared_scalars`]; `None` otherwise, and
    /// for a value SQL already resolved (a string), which passes through.
    pub localized:       Option<Vec<String>>,
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
pub fn stored_key_candidates(field_name: &str) -> (String, Option<String>) {
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
            localized: None,
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
            localized: None,
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
            localized: None,
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
            localized: None,
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

    /// [`lookup_in`](Self::lookup_in), taking the entry, key included, out of `map`.
    fn take_from(&self, map: &mut Map<String, JsonValue>) -> Option<(String, JsonValue)> {
        map.remove_entry(&self.source)
            .or_else(|| self.source_fallback.as_ref().and_then(|fb| map.remove_entry(fb)))
    }

    /// The stored keys this field may read.
    fn stored_keys(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.source.as_str()).chain(self.source_fallback.as_deref())
    }
}

/// For each of `fields`, whether a later one reads one of its stored keys. An owned row's value
/// for such a field is cloned rather than taken, so the later field still finds it: `fooBar`
/// and `foo_bar` both read `foo_bar`, and so do an alias and the field it renames.
fn later_readers(fields: &[FieldMapping]) -> Vec<bool> {
    let mut last_reader: HashMap<&str, usize> = HashMap::with_capacity(fields.len() * 2);
    for (index, field) in fields.iter().enumerate() {
        for key in field.stored_keys() {
            last_reader.insert(key, index);
        }
    }
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| field.stored_keys().any(|key| last_reader[key] > index))
        .collect()
}

/// A value by its JSON kind, borrowed or owned as it came.
enum Shape<'a> {
    Object(Cow<'a, Map<String, JsonValue>>),
    Array(Cow<'a, [JsonValue]>),
    Other(Cow<'a, JsonValue>),
}

impl<'a> Shape<'a> {
    fn of(value: Cow<'a, JsonValue>) -> Self {
        match value {
            Cow::Borrowed(JsonValue::Object(map)) => Self::Object(Cow::Borrowed(map)),
            Cow::Owned(JsonValue::Object(map)) => Self::Object(Cow::Owned(map)),
            Cow::Borrowed(JsonValue::Array(items)) => Self::Array(Cow::Borrowed(items.as_slice())),
            Cow::Owned(JsonValue::Array(items)) => Self::Array(Cow::Owned(items)),
            other => Self::Other(other),
        }
    }
}

/// The items of an array, each borrowed or owned as the array came.
fn items_of(items: Cow<'_, [JsonValue]>) -> Box<dyn Iterator<Item = Cow<'_, JsonValue>> + '_> {
    match items {
        Cow::Borrowed(items) => Box::new(items.iter().map(Cow::Borrowed)),
        Cow::Owned(items) => Box::new(items.into_iter().map(Cow::Owned)),
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
                if fd.localized {
                    mapping.localized = crate::runtime::localization_chain(schema);
                }
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
        self.project_row(Cow::Borrowed(jsonb.as_value()), &[])
    }

    /// Project one row, borrowed or owned (#1556): an owned row gives up its values instead of
    /// having them cloned. `later_readers` is [`later_readers`] of `self.fields`, read only for
    /// an owned row; a field it does not cover is cloned.
    fn project_row(&self, row: Cow<'_, JsonValue>, later_readers: &[bool]) -> Result<JsonValue> {
        match Shape::of(row) {
            Shape::Object(map) => {
                self.project_object(map, self.typename.as_deref(), &self.fields, later_readers)
            },
            // A row that is an array is projected element-wise. A row that fails projection
            // propagates as an error rather than being silently dropped from the result set
            // (#736).
            Shape::Array(items) => items_of(items)
                .map(|item| match Shape::of(item) {
                    Shape::Object(obj) => self.project_object(
                        obj,
                        self.typename.as_deref(),
                        &self.fields,
                        later_readers,
                    ),
                    Shape::Array(items) => Ok(JsonValue::Array(items.into_owned())),
                    Shape::Other(other) => Ok(other.into_owned()),
                })
                .collect::<Result<Vec<_>>>()
                .map(JsonValue::Array),
            Shape::Other(other) => Ok(other.into_owned()),
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
        self.project_object(Cow::Borrowed(map), self.typename.as_deref(), &self.fields, &[])
    }

    /// Project `fields` out of an object, `__typename` first when there is one (GraphQL
    /// convention), each stored key resolved by the rule every consumer of a declared field name
    /// shares (#1271). An owned object's values are taken, unless `later_readers` says a later
    /// field reads the same key.
    fn project_object(
        &self,
        source: Cow<'_, Map<String, JsonValue>>,
        typename: Option<&str>,
        fields: &[FieldMapping],
        later_readers: &[bool],
    ) -> Result<JsonValue> {
        let mut result = Map::new();
        if let Some(typename) = typename {
            result.insert("__typename".to_string(), JsonValue::String(typename.to_string()));
        }
        let mut source = source;
        for (index, field) in fields.iter().enumerate() {
            let found = match &mut source {
                Cow::Borrowed(map) => {
                    let map: &Map<String, JsonValue> = map;
                    field.lookup_in(map).map(|value| (None, Cow::Borrowed(value)))
                },
                Cow::Owned(map) => {
                    let taken = if later_readers.get(index).copied().unwrap_or(true) {
                        field.lookup_in(map).map(|value| (None, value.clone()))
                    } else {
                        field.take_from(map).map(|(key, value)| (Some(key), value))
                    };
                    taken.map(|(key, value)| (key, Cow::Owned(value)))
                },
            };
            // A requested field the row has no value for completes to `null`, in its place
            // (GraphQL § 6.3; #1618), as `project_entity` and a SQL-projected read answer it.
            let Some((key, value)) = found else {
                result.insert(field.output.clone(), JsonValue::Null);
                continue;
            };
            let output =
                key.filter(|key| *key == field.output).unwrap_or_else(|| field.output.clone());
            result.insert(output, self.project_nested_value(value, field)?);
        }
        Ok(JsonValue::Object(result))
    }

    /// Project a nested value, adding typename if configured.
    #[allow(clippy::self_only_used_in_recursion)] // Reason: &self required for method dispatch; recursive structure is intentional
    fn project_nested_value(
        &self,
        value: Cow<'_, JsonValue>,
        field: &FieldMapping,
    ) -> Result<JsonValue> {
        if let Some(chain) = &field.localized {
            return Ok(crate::runtime::localize(&value, chain));
        }
        match Shape::of(value) {
            Shape::Object(obj) => {
                let Some(typename) = field.nested_typename.as_deref() else {
                    // No typename for this nested object - return as-is
                    return Ok(JsonValue::Object(obj.into_owned()));
                };
                // #1271: the nested branch reads its own sub-map, so it needs the rule
                // explicitly rather than inheriting it from above.
                if let Some(nested_fields) = &field.nested_fields {
                    let later = match obj {
                        Cow::Owned(_) => later_readers(nested_fields),
                        Cow::Borrowed(_) => Vec::new(),
                    };
                    return self.project_object(obj, Some(typename), nested_fields, &later);
                }
                // No specific field mappings - copy all fields from source
                let mut result = Map::new();
                result.insert("__typename".to_string(), JsonValue::String(typename.to_string()));
                result.extend(obj.into_owned());
                Ok(JsonValue::Object(result))
            },
            Shape::Array(items) => {
                // For arrays of objects, add typename to each element
                if field.nested_typename.is_some() {
                    items_of(items)
                        .map(|item| self.project_nested_value(item, field))
                        .collect::<Result<Vec<_>>>()
                        .map(JsonValue::Array)
                } else {
                    Ok(JsonValue::Array(items.into_owned()))
                }
            },
            Shape::Other(value) => {
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
                    return Ok(value.into_owned());
                }
                if let JsonValue::String(ref s) = *value {
                    if let Ok(parsed @ (JsonValue::Object(_) | JsonValue::Array(_))) =
                        serde_json::from_str::<JsonValue>(s)
                    {
                        return self.project_nested_value(Cow::Owned(parsed), field);
                    }
                }
                Ok(value.into_owned())
            },
        }
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

    /// [`project_results`](Self::project_results), taking the rows: a row no one else holds
    /// gives up its values instead of having them cloned (#1556).
    ///
    /// # Errors
    ///
    /// As [`project_results`](Self::project_results).
    pub fn project_results_owned(
        &self,
        results: std::sync::Arc<Vec<JsonbValue>>,
        is_list: bool,
    ) -> Result<JsonValue> {
        // Rows the row cache (or anyone else) still holds are projected as borrowed.
        let rows = match std::sync::Arc::try_unwrap(results) {
            Ok(rows) => rows,
            Err(shared) => return self.project_results(&shared, is_list),
        };
        let later_readers = later_readers(&self.mapper.fields);
        let mut rows = rows
            .into_iter()
            .map(|row| self.mapper.project_row(Cow::Owned(row.into_value()), &later_readers));
        if is_list {
            rows.collect::<Result<Vec<_>>>().map(JsonValue::Array)
        } else {
            rows.next().unwrap_or(Ok(JsonValue::Null))
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
/// - **object fields**, single or listed, are projected through their sub-selection at **any
///   depth**. The recursion follows the selection, which `max_query_depth` bounds, so there is no
///   depth at which a stored sub-object passes through: that fall-through, at a cap of 4, served
///   every stored key of an object five levels down, gated or not. An object field with no
///   sub-selection selects none of its fields and projects to `{}`,
/// - **scalar fields** pass through their stored value,
/// - **`__typename`** is emitted only where the client selected it.
///
/// `type_name` is the concrete GraphQL object type of `entity` (resolved by the
/// caller). `selections` is the result selection set for this level, with inline
/// `... on T` fragments preserved.
///
/// # An empty selection set projects **nothing**
///
/// It used to return the stored entity unchanged, and that one line is what made
/// #1352 and #1357 invisible: an empty slice is the *permissive* shape, not a
/// neutral one, so a transport that arrived without a selection set was not
/// refused — it was granted every field of the row, policy-gated ones included,
/// while `selection_set_selects_gated_field` reported that nothing gated was
/// selected and the #423 field authorizer took zero calls.
///
/// Nothing legitimate reaches here empty any more: GraphQL § 5.3.3 refuses a
/// composite field named without a selection set (#1357), every mutation's return
/// type is composite (#1358), and the write entries take a non-empty
/// [`WriteSelections`](crate::runtime::WriteSelections). An empty slice arriving
/// here is therefore a defect upstream, and the safe answer to a defect is the
/// empty object — which is also what the read path already answers for a selection
/// set that `@skip` resolves to nothing, so the two agree.
///
/// There is deliberately **no `project_entity_unfiltered`** to reach for. Every
/// caller in the tree either has a real selection set or guards emptiness before
/// calling (federation returns early on `root.nested_fields.is_empty()`), so a
/// named permissive helper would be a public function with no caller — and an
/// unused facility is how the permissive branch stayed reachable in the first
/// place. A caller that genuinely wants the whole entity can write `entity.clone()`
/// and own that decision where it is made.
#[must_use]
pub fn project_entity(
    entity: &JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
) -> JsonValue {
    if selections.is_empty() {
        return JsonValue::Object(Map::new());
    }
    project_entity_at(entity, type_name, selections, schema, None)
}

/// The fields a caller may not read and that mask, by `(type, field)`.
type Masked<'a> = Option<&'a HashSet<(String, String)>>;

/// Whether `masked` masks `field` of `type_name`.
fn is_masked(masked: Masked<'_>, type_name: &str, field: &str) -> bool {
    masked.is_some_and(|m| !m.is_empty() && m.contains(&(type_name.to_string(), field.to_string())))
}

/// What a masked field reads as: `null`, or `[]` for a translations sibling, whose
/// `[LocalizedString!]!` has no `null` (#1523).
pub fn masked_value(type_def: Option<&crate::schema::TypeDefinition>, field: &str) -> JsonValue {
    if type_def.and_then(|t| t.translations_of(field)).is_some() {
        JsonValue::Array(Vec::new())
    } else {
        JsonValue::Null
    }
}

fn project_entity_at(
    entity: &JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    masked: Masked<'_>,
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
        if is_masked(masked, type_name, &sel.name) {
            out.insert(sel.response_key().to_string(), masked_value(type_def, &sel.name));
            continue;
        }
        // A localized field's translations sibling lists its stored map (#1513); an absent
        // map lists nothing, as in SQL.
        if let Some(base) = type_def.and_then(|td| td.translations_of(&sel.name)) {
            let listed = crate::runtime::translations_read(schema, sel).map_or_else(
                || JsonValue::Array(Vec::new()),
                |read| {
                    let stored = lookup_source(obj, base.name.as_str()).unwrap_or(&JsonValue::Null);
                    crate::runtime::translations(stored, &read)
                },
            );
            out.insert(sel.response_key().to_string(), listed);
            continue;
        }
        let field_def =
            type_def.and_then(|td| td.fields.iter().find(|f| f.name.as_str() == sel.name));
        let Some(value) = lookup_source(obj, &sel.name) else {
            // A selected field the document has no value for completes to `null`, in its
            // place (GraphQL § 6.3; #1618), as a SQL-projected read answers the same key.
            // Whether `null` may stand there is non-null completion's to decide.
            out.insert(sel.response_key().to_string(), JsonValue::Null);
            continue;
        };
        let projected = project_field_value(value, field_def, sel, schema, masked);
        out.insert(sel.response_key().to_string(), projected);
    }

    JsonValue::Object(out)
}

/// Project a single field value, recursing into nested object selections.
fn project_field_value(
    value: &JsonValue,
    field_def: Option<&FieldDefinition>,
    selection: &FieldSelection,
    schema: &CompiledSchema,
    masked: Masked<'_>,
) -> JsonValue {
    let nested = selection.nested_fields.as_slice();
    // An object field is projected through its sub-selection at every depth, and an empty
    // sub-selection projects nothing of it. Only a scalar returns its stored value; a
    // localized one, its label for the selection's locale (#1513).
    if let Some(fd) = field_def {
        if fd.localized {
            return crate::runtime::selection_chain(schema, selection)
                .map_or_else(|| value.clone(), |chain| crate::runtime::localize(value, &chain));
        }
        if !fd.field_type.is_scalar() && !fd.field_type.is_list() {
            if let Some(child_type) = fd.field_type.type_name() {
                match value {
                    JsonValue::Object(_) => {
                        return project_entity_at(value, child_type, nested, schema, masked);
                    },
                    // The DB sometimes returns a nested object as a JSON string
                    // (when extracted via `->>`). Re-parse and project it.
                    JsonValue::String(s) => {
                        if let Ok(parsed @ JsonValue::Object(_)) =
                            serde_json::from_str::<JsonValue>(s)
                        {
                            return project_entity_at(&parsed, child_type, nested, schema, masked);
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
            if let Some(child_type) = fd.field_type.inner_type().and_then(FieldType::type_name) {
                if let JsonValue::Array(arr) = value {
                    return JsonValue::Array(
                        arr.iter()
                            .map(|el| project_list_element(el, child_type, nested, schema, masked))
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
                                    project_list_element(el, child_type, nested, schema, masked)
                                })
                                .collect(),
                        );
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
    masked: Masked<'_>,
) -> JsonValue {
    match element {
        JsonValue::Object(_) => project_entity_at(element, child_type, nested, schema, masked),
        // An element itself text-encoded as a JSON object.
        JsonValue::String(s) => match serde_json::from_str::<JsonValue>(s) {
            Ok(parsed @ JsonValue::Object(_)) => {
                project_entity_at(&parsed, child_type, nested, schema, masked)
            },
            _ => element.clone(),
        },
        _ => element.clone(),
    }
}

/// Shape a SQL-projected result by its selection, at every depth, in one walk.
///
/// `value` is what the projector built from the rows the SQL projection returned: an
/// object, or a list of them, of `type_name`. Each object at every depth is rebuilt in
/// the order `selections` gives, holding only the keys they select:
///
/// - a selected key is read under its response key, which is how the SQL projection writes a level
///   it built (`jsonb_build_object`); a key nothing selects is dropped, whatever the adapter
///   returned, and a selected key it did not return is `null`;
/// - `__typename` is written where it was selected, as `type_name`;
/// - a field in `masked` reads as `null` (`[]` for a translations sibling);
/// - a single object is shaped the same way, one level down;
/// - a list of objects, which the SQL projection leaves as the stored sub-documents, is projected
///   element by element from their stored keys ([`project_entity`]).
///
/// No level below the root is taken as the adapter shaped it: an adapter that ignored
/// the projection serves no key the query did not select.
pub fn shape_projected(
    value: &mut JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    masked: Option<&HashSet<(String, String)>>,
) {
    match value {
        // A list-returning query: each element is an entity of `type_name`.
        JsonValue::Array(items) => {
            for item in items {
                shape_projected(item, type_name, selections, schema, masked);
            }
        },
        JsonValue::Object(object) => shape_root(object, type_name, selections, schema, masked),
        _ => {},
    }
}

/// [`shape_projected`] over a row no projector built: its root is rebuilt from the
/// selection as every level below it is.
pub fn shape_unprojected_root(
    value: &mut JsonValue,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    masked: Option<&HashSet<(String, String)>>,
) {
    if let JsonValue::Object(object) = value {
        *object = shape_level(std::mem::take(object), type_name, selections, schema, masked);
    }
}

/// The root level of [`shape_projected`], shaped in place: the projector built it from
/// the selection already, in its order and holding only its keys, so only its masked
/// fields and the objects below it are left to do.
fn shape_root(
    object: &mut Map<String, JsonValue>,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    masked: Masked<'_>,
) {
    let level = effective_selections(selections, type_name, schema);
    // `__typename` is answered where it was selected, under its alias, as at every level
    // below (§ 6.3): the level is rebuilt in its selection's order.
    if level.iter().any(|sel| sel.name == "__typename") {
        *object = shape_level(std::mem::take(object), type_name, selections, schema, masked);
        return;
    }
    let type_def = schema.find_type(type_name);
    for sel in level {
        // A scalar the projector placed is left where it is: only a masked field or an
        // object is looked up.
        if is_masked(masked, type_name, &sel.name) {
            if let Some(slot) = object.get_mut(sel.response_key()) {
                *slot = masked_value(type_def, &sel.name);
            }
            continue;
        }
        let Some(field_def) = type_def.and_then(|td| td.fields.iter().find(|f| f.name == sel.name))
        else {
            continue;
        };
        if field_def.field_type.is_scalar() {
            continue;
        }
        if let Some(slot) = object.get_mut(sel.response_key()) {
            *slot = shape_field(std::mem::take(slot), field_def, sel, schema, masked);
        }
    }
}

/// One level of [`shape_projected`]: `object` rebuilt from its selected response keys.
fn shape_level(
    mut object: Map<String, JsonValue>,
    type_name: &str,
    selections: &[FieldSelection],
    schema: &CompiledSchema,
    masked: Masked<'_>,
) -> Map<String, JsonValue> {
    let type_def = schema.find_type(type_name);
    let level = effective_selections(selections, type_name, schema);
    let mut out = Map::with_capacity(level.len());
    for sel in level {
        let key = sel.response_key();
        // A key selected twice is one response key, filled once.
        if out.contains_key(key) {
            continue;
        }
        if sel.name == "__typename" {
            out.insert(key.to_string(), JsonValue::String(type_name.to_string()));
            continue;
        }
        // The key the level held is moved, not copied. A level that arrived with its stored
        // keys (a leaf selection of an object field, read whole) is read under those, copied:
        // an alias and the field it names read one stored key.
        let (key, value) = object.remove_entry(key).unwrap_or_else(|| {
            let (stored, fallback) = stored_key_candidates(&sel.name);
            let value = object
                .get(&stored)
                .or_else(|| fallback.as_ref().and_then(|f| object.get(f)))
                .cloned()
                .unwrap_or(JsonValue::Null);
            (key.to_string(), value)
        });
        if is_masked(masked, type_name, &sel.name) {
            out.insert(key, masked_value(type_def, &sel.name));
            continue;
        }
        let field_def = type_def.and_then(|td| td.fields.iter().find(|f| f.name == sel.name));
        let value = match field_def {
            Some(fd) if !fd.field_type.is_scalar() => shape_field(value, fd, sel, schema, masked),
            _ => value,
        };
        out.insert(key, value);
    }
    out
}

/// A selected object field's value in a SQL-projected level.
fn shape_field(
    value: JsonValue,
    field_def: &FieldDefinition,
    sel: &FieldSelection,
    schema: &CompiledSchema,
    masked: Masked<'_>,
) -> JsonValue {
    let nested = sel.nested_fields.as_slice();
    if field_def.field_type.is_list() {
        let Some(child) = field_def.field_type.inner_type().and_then(FieldType::type_name) else {
            return value;
        };
        return match value {
            // Each element replaced where it is: the one it replaces is freed before the
            // next is built, so the allocator reuses it.
            JsonValue::Array(mut items) => {
                for item in &mut items {
                    *item = project_list_element(item, child, nested, schema, masked);
                }
                JsonValue::Array(items)
            },
            // A whole array as JSON text, as `->>` extracts it.
            text @ JsonValue::String(_) => {
                project_field_value(&text, Some(field_def), sel, schema, masked)
            },
            other => other,
        };
    }
    let Some(child) = field_def.field_type.type_name() else {
        return value;
    };
    match value {
        JsonValue::Object(object) => {
            JsonValue::Object(shape_level(object, child, nested, schema, masked))
        },
        // An object as JSON text holds its stored keys: projected as a stored document.
        text @ JsonValue::String(_) => {
            project_field_value(&text, Some(field_def), sel, schema, masked)
        },
        other => other,
    }
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

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
