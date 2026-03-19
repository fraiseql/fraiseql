//! REST parameter extraction and type coercion.
//!
//! Parses path parameters, query string parameters (`?select=`, `?sort=`,
//! `?limit=`, `?offset=`, `?first=`, `?after=`, `?filter=`, bracket operators),
//! validates against [`QueryDefinition`] / [`TypeDefinition`] metadata, and
//! returns a typed [`ExtractedParams`].

use fraiseql_core::schema::{FieldType, QueryDefinition, RestConfig, TypeDefinition};
use fraiseql_core::utils::operators::OPERATOR_REGISTRY;
use fraiseql_error::FraiseQLError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of total extracted variables (path + query + body).
const MAX_VARIABLES_COUNT: usize = 1_000;

/// Maximum nesting depth for `?filter=` JSON values.
const MAX_FILTER_DEPTH: usize = 64;

/// Bracket operators allowed in `?field[op]=value` syntax.
///
/// This is a curated subset of the full [`OPERATOR_REGISTRY`] — the 16 most
/// common operators that make sense in a URL query string.
const BRACKET_OPERATORS: &[&str] = &[
    "eq",
    "ne",
    "gt",
    "gte",
    "lt",
    "lte",
    "in",
    "nin",
    "like",
    "ilike",
    "icontains",
    "startswith",
    "istartswith",
    "endswith",
    "iendswith",
    "is_null",
];

/// Known query-string parameter names that are *not* filter keys.
const RESERVED_PARAMS: &[&str] = &[
    "select", "sort", "limit", "offset", "first", "after", "last", "before", "filter",
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Extracted and validated parameters from a REST request.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ExtractedParams {
    /// Path parameters (e.g., `[("id", 123)]`).
    pub path_params: Vec<(String, serde_json::Value)>,
    /// WHERE clause for the query (merged from simple/bracket/filter params).
    pub where_clause: Option<serde_json::Value>,
    /// ORDER BY clause (from `?sort=`).
    pub order_by: Option<serde_json::Value>,
    /// Pagination parameters.
    pub pagination: PaginationParams,
    /// Field selection (from `?select=`).
    pub field_selection: RestFieldSpec,
}

/// Pagination mode and parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaginationParams {
    /// Offset-based pagination.
    Offset {
        /// Maximum number of rows to return.
        limit:  u64,
        /// Number of rows to skip.
        offset: u64,
    },
    /// Cursor-based (Relay) pagination.
    Cursor {
        /// Forward page size.
        first:  Option<u64>,
        /// Cursor to start after.
        after:  Option<String>,
        /// Backward page size.
        last:   Option<u64>,
        /// Cursor to start before.
        before: Option<String>,
    },
    /// No pagination (single-resource fetch).
    None,
}

/// Parsed field selection from `?select=` query parameter.
///
/// Named `RestFieldSpec` (not `FieldSelection`) to avoid collision with the
/// existing `graphql::types::FieldSelection` used by `QueryMatch.selections`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestFieldSpec {
    /// All fields requested (no `?select=` param).
    All,
    /// Specific flat fields: `["id", "name", "address"]`.
    Fields(Vec<String>),
}

// ---------------------------------------------------------------------------
// Extractor
// ---------------------------------------------------------------------------

/// Extracts and validates REST request parameters against schema metadata.
pub struct RestParamExtractor<'a> {
    config:    &'a RestConfig,
    query_def: &'a QueryDefinition,
    type_def:  Option<&'a TypeDefinition>,
}

impl<'a> RestParamExtractor<'a> {
    /// Create a new extractor for the given query definition.
    #[must_use]
    pub const fn new(
        config: &'a RestConfig,
        query_def: &'a QueryDefinition,
        type_def: Option<&'a TypeDefinition>,
    ) -> Self {
        Self {
            config,
            query_def,
            type_def,
        }
    }

    /// Extract parameters from path segments and query string pairs.
    ///
    /// `path_pairs` are `(name, raw_value)` from the URL path (e.g., `("id", "123")`).
    /// `query_pairs` are `(key, value)` from the URL query string.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` with an actionable message when:
    /// - A query parameter name is unknown
    /// - A bracket operator is not in the allowlist
    /// - A filter field name does not exist on the type
    /// - A filter operator is not in the registry
    /// - Pagination parameters are mixed (relay + offset)
    /// - A `?select=` field contains a dot
    /// - The filter JSON exceeds `max_filter_bytes`
    /// - The filter JSON exceeds `MAX_FILTER_DEPTH`
    /// - Total parameter count exceeds `MAX_VARIABLES_COUNT`
    pub fn extract(
        &self,
        path_pairs: &[(&str, &str)],
        query_pairs: &[(&str, &str)],
    ) -> Result<ExtractedParams, FraiseQLError> {
        let is_relay = self.query_def.relay;
        let is_list = self.query_def.returns_list;

        // 1. Path params.
        let path_params = self.extract_path_params(path_pairs)?;

        // 2. Classify query params.
        let mut simple_filters: Vec<(String, serde_json::Value)> = Vec::new();
        let mut bracket_filters: Vec<(String, String, String)> = Vec::new(); // (field, op, value)
        let mut filter_json: Option<&str> = None;
        let mut select_raw: Option<&str> = None;
        let mut sort_raw: Option<&str> = None;
        let mut limit_raw: Option<&str> = None;
        let mut offset_raw: Option<&str> = None;
        let mut first_raw: Option<&str> = None;
        let mut after_raw: Option<&str> = None;
        let mut last_raw: Option<&str> = None;
        let mut before_raw: Option<&str> = None;

        for &(key, value) in query_pairs {
            if let Some((field, op)) = parse_bracket_key(key) {
                // ?field[op]=value
                self.validate_bracket_operator(&op)?;
                self.validate_field_name(&field)?;
                bracket_filters.push((field, op, value.to_string()));
            } else {
                match key {
                    "select" => select_raw = Some(value),
                    "sort" => sort_raw = Some(value),
                    "limit" => limit_raw = Some(value),
                    "offset" => offset_raw = Some(value),
                    "first" => first_raw = Some(value),
                    "after" => after_raw = Some(value),
                    "last" => last_raw = Some(value),
                    "before" => before_raw = Some(value),
                    "filter" => filter_json = Some(value),
                    _ => {
                        // Could be a simple equality filter (e.g., ?name=Alice)
                        // or an unknown param.
                        if RESERVED_PARAMS.contains(&key) {
                            // Already handled above — shouldn't reach here.
                            continue;
                        }
                        // Check if it's a valid field name for simple eq filter.
                        if is_list && self.is_valid_field(key) {
                            let coerced = self.coerce_field_value(key, value)?;
                            simple_filters.push((key.to_string(), coerced));
                        } else if !is_list {
                            return Err(validation_error(format!(
                                "Unknown query parameter '{key}' for single-resource endpoint. \
                                 Available parameters: select"
                            )));
                        } else {
                            return Err(self.unknown_param_error(key));
                        }
                    }
                }
            }
        }

        // 3. Cross-pagination guards.
        if is_list {
            let has_offset_params = limit_raw.is_some() || offset_raw.is_some();
            let has_cursor_params =
                first_raw.is_some() || after_raw.is_some() || last_raw.is_some() || before_raw.is_some();

            if is_relay && has_offset_params {
                return Err(validation_error(
                    "This endpoint uses cursor-based pagination. \
                     Use `first`/`after`/`last`/`before` instead of `limit`/`offset`."
                        .to_string(),
                ));
            }
            if !is_relay && has_cursor_params {
                return Err(validation_error(
                    "This endpoint uses offset-based pagination. \
                     Use `limit`/`offset` instead of `first`/`after`/`last`/`before`."
                        .to_string(),
                ));
            }
        }

        // 4. Parse select.
        let field_selection = self.parse_select(select_raw)?;

        // 5. Parse sort.
        let order_by = self.parse_sort(sort_raw)?;

        // 6. Build where clause.
        let filter_where = self.parse_filter(filter_json)?;
        let where_clause = self.merge_where(simple_filters, bracket_filters, filter_where)?;

        // 7. Parse pagination.
        let pagination = if !is_list {
            PaginationParams::None
        } else if is_relay {
            self.parse_cursor_pagination(first_raw, after_raw, last_raw, before_raw)?
        } else {
            self.parse_offset_pagination(limit_raw, offset_raw)?
        };

        // 8. Count total params and enforce limit.
        let total_count = path_params.len()
            + where_clause.as_ref().map_or(0, count_where_fields)
            + order_by.as_ref().map_or(0, |_| 1)
            + match &pagination {
                PaginationParams::Offset { .. } => 2,
                PaginationParams::Cursor { first, after, last, before } => {
                    first.is_some() as usize
                        + after.is_some() as usize
                        + last.is_some() as usize
                        + before.is_some() as usize
                }
                PaginationParams::None => 0,
            }
            + match &field_selection {
                RestFieldSpec::All => 0,
                RestFieldSpec::Fields(f) => f.len(),
            };

        if total_count > MAX_VARIABLES_COUNT {
            return Err(validation_error(format!(
                "Too many parameters ({total_count}). Maximum allowed: {MAX_VARIABLES_COUNT}."
            )));
        }

        Ok(ExtractedParams {
            path_params,
            where_clause,
            order_by,
            pagination,
            field_selection,
        })
    }

    // -----------------------------------------------------------------------
    // Path params
    // -----------------------------------------------------------------------

    fn extract_path_params(
        &self,
        pairs: &[(&str, &str)],
    ) -> Result<Vec<(String, serde_json::Value)>, FraiseQLError> {
        let mut out = Vec::with_capacity(pairs.len());
        for &(name, raw) in pairs {
            let arg = self.query_def.arguments.iter().find(|a| a.name == name);
            let value = match arg {
                Some(a) => coerce_to_type(raw, &a.arg_type)?,
                None => serde_json::Value::String(raw.to_string()),
            };
            out.push((name.to_string(), value));
        }
        Ok(out)
    }

    // -----------------------------------------------------------------------
    // Select
    // -----------------------------------------------------------------------

    fn parse_select(
        &self,
        raw: Option<&str>,
    ) -> Result<RestFieldSpec, FraiseQLError> {
        let Some(raw) = raw else {
            return Ok(RestFieldSpec::All);
        };
        if raw.is_empty() {
            return Ok(RestFieldSpec::All);
        }

        let fields: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).collect();

        for f in &fields {
            if f.contains('.') {
                return Err(validation_error(format!(
                    "Dot notation not supported in `select`. \
                     Use `?select={field}` to include the full nested object.",
                    field = f.split('.').next().unwrap_or(f)
                )));
            }
            self.validate_field_name(f)?;
        }

        Ok(RestFieldSpec::Fields(fields))
    }

    // -----------------------------------------------------------------------
    // Sort
    // -----------------------------------------------------------------------

    fn parse_sort(
        &self,
        raw: Option<&str>,
    ) -> Result<Option<serde_json::Value>, FraiseQLError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        if raw.is_empty() {
            return Ok(None);
        }

        let mut order_parts = Vec::new();
        for part in raw.split(',') {
            let part = part.trim();
            let (field, direction) = if let Some(stripped) = part.strip_prefix('-') {
                (stripped, "DESC")
            } else {
                (part, "ASC")
            };
            self.validate_field_name(field)?;
            order_parts.push(serde_json::json!({
                "field": field,
                "direction": direction,
            }));
        }

        Ok(Some(serde_json::Value::Array(order_parts)))
    }

    // -----------------------------------------------------------------------
    // Filter (JSON DSL)
    // -----------------------------------------------------------------------

    fn parse_filter(
        &self,
        raw: Option<&str>,
    ) -> Result<Option<serde_json::Value>, FraiseQLError> {
        let Some(raw) = raw else {
            return Ok(None);
        };

        // Size check.
        if raw.len() > self.config.max_filter_bytes {
            return Err(validation_error(format!(
                "Filter parameter exceeds maximum size ({} bytes). \
                 Maximum allowed: {} bytes.",
                raw.len(),
                self.config.max_filter_bytes
            )));
        }

        let parsed: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
            validation_error(format!("Invalid filter JSON: {e}"))
        })?;

        // Depth check.
        if json_depth(&parsed) > MAX_FILTER_DEPTH {
            return Err(validation_error(format!(
                "Filter JSON exceeds maximum nesting depth ({MAX_FILTER_DEPTH})."
            )));
        }

        // Validate field names and operators.
        self.validate_filter_value(&parsed)?;

        Ok(Some(parsed))
    }

    fn validate_filter_value(
        &self,
        value: &serde_json::Value,
    ) -> Result<(), FraiseQLError> {
        let Some(obj) = value.as_object() else {
            return Ok(());
        };

        for (key, inner) in obj {
            // Top-level keys are field names.
            self.validate_field_name(key)?;

            // Inner object keys should be operators.
            if let Some(ops) = inner.as_object() {
                for op_name in ops.keys() {
                    if !OPERATOR_REGISTRY.contains_key(op_name.as_str()) {
                        let available: Vec<&str> = {
                            let mut ops: Vec<&str> =
                                OPERATOR_REGISTRY.keys().copied().collect();
                            ops.sort_unstable();
                            ops
                        };
                        return Err(validation_error(format!(
                            "Unknown filter operator '{op_name}'. \
                             Available operators: {available}",
                            available = available.join(", ")
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Merge WHERE clause
    // -----------------------------------------------------------------------

    fn merge_where(
        &self,
        simple: Vec<(String, serde_json::Value)>,
        bracket: Vec<(String, String, String)>,
        filter: Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, FraiseQLError> {
        if simple.is_empty() && bracket.is_empty() && filter.is_none() {
            return Ok(None);
        }

        let mut merged = serde_json::Map::new();

        // Simple equality: ?name=Alice -> { "name": { "eq": "Alice" } }
        for (field, value) in simple {
            let entry = merged
                .entry(field)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("eq".to_string(), value);
            }
        }

        // Bracket: ?name[icontains]=Ali -> { "name": { "icontains": "Ali" } }
        for (field, op, value) in bracket {
            let coerced = self.coerce_field_value(&field, &value)?;
            let entry = merged
                .entry(field)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(obj) = entry.as_object_mut() {
                obj.insert(op, coerced);
            }
        }

        // JSON filter: merge fields into the same map.
        if let Some(filter_val) = filter {
            if let Some(filter_obj) = filter_val.as_object() {
                for (key, val) in filter_obj {
                    let entry = merged
                        .entry(key.clone())
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let (Some(existing), Some(new_ops)) =
                        (entry.as_object_mut(), val.as_object())
                    {
                        for (op, v) in new_ops {
                            existing.insert(op.clone(), v.clone());
                        }
                    } else {
                        // Non-object value — replace entirely.
                        merged.insert(key.clone(), val.clone());
                    }
                }
            }
        }

        if merged.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Object(merged)))
        }
    }

    // -----------------------------------------------------------------------
    // Pagination
    // -----------------------------------------------------------------------

    fn parse_offset_pagination(
        &self,
        limit_raw: Option<&str>,
        offset_raw: Option<&str>,
    ) -> Result<PaginationParams, FraiseQLError> {
        let limit = match limit_raw {
            Some(s) => {
                let v: u64 = s.parse().map_err(|_| {
                    validation_error(format!("Invalid `limit` value: '{s}'. Expected a positive integer."))
                })?;
                v.min(self.config.max_page_size)
            }
            None => self.config.default_page_size,
        };
        let offset = match offset_raw {
            Some(s) => s.parse().map_err(|_| {
                validation_error(format!("Invalid `offset` value: '{s}'. Expected a non-negative integer."))
            })?,
            None => 0,
        };
        Ok(PaginationParams::Offset { limit, offset })
    }

    fn parse_cursor_pagination(
        &self,
        first_raw: Option<&str>,
        after_raw: Option<&str>,
        last_raw: Option<&str>,
        before_raw: Option<&str>,
    ) -> Result<PaginationParams, FraiseQLError> {
        let first = match first_raw {
            Some(s) => {
                let v: u64 = s.parse().map_err(|_| {
                    validation_error(format!("Invalid `first` value: '{s}'. Expected a positive integer."))
                })?;
                Some(v.min(self.config.max_page_size))
            }
            None if after_raw.is_none() && last_raw.is_none() && before_raw.is_none() => {
                // No cursor params at all — default page size.
                Some(self.config.default_page_size)
            }
            None => None,
        };
        let after = after_raw.map(String::from);
        let last = match last_raw {
            Some(s) => Some(s.parse().map_err(|_| {
                validation_error(format!("Invalid `last` value: '{s}'. Expected a positive integer."))
            })?),
            None => None,
        };
        let before = before_raw.map(String::from);
        Ok(PaginationParams::Cursor {
            first,
            after,
            last,
            before,
        })
    }

    // -----------------------------------------------------------------------
    // Validation helpers
    // -----------------------------------------------------------------------

    fn validate_field_name(&self, name: &str) -> Result<(), FraiseQLError> {
        if let Some(td) = self.type_def {
            if td.find_field_by_output_name(name).is_none() {
                let available = field_names(td);
                return Err(validation_error(format!(
                    "Unknown field '{name}'. Available fields: {available}",
                    available = available.join(", ")
                )));
            }
        }
        Ok(())
    }

    fn validate_bracket_operator(&self, op: &str) -> Result<(), FraiseQLError> {
        if !BRACKET_OPERATORS.contains(&op) {
            return Err(validation_error(format!(
                "Unknown bracket operator '{op}'. \
                 Available bracket operators: {available}",
                available = BRACKET_OPERATORS.join(", ")
            )));
        }
        Ok(())
    }

    fn is_valid_field(&self, name: &str) -> bool {
        match self.type_def {
            Some(td) => td.find_field_by_output_name(name).is_some(),
            None => true,
        }
    }

    fn coerce_field_value(
        &self,
        field_name: &str,
        raw: &str,
    ) -> Result<serde_json::Value, FraiseQLError> {
        if let Some(td) = self.type_def {
            if let Some(fd) = td.find_field_by_output_name(field_name) {
                return coerce_to_type(raw, &fd.field_type);
            }
        }
        // No type info — return as string.
        Ok(serde_json::Value::String(raw.to_string()))
    }

    fn unknown_param_error(&self, key: &str) -> FraiseQLError {
        let mut available: Vec<&str> = RESERVED_PARAMS.to_vec();
        if let Some(td) = self.type_def {
            for f in &td.fields {
                available.push(f.output_name());
            }
        }
        available.sort_unstable();
        available.dedup();
        validation_error(format!(
            "Unknown query parameter '{key}'. Available parameters: {available}",
            available = available.join(", ")
        ))
    }
}

// ---------------------------------------------------------------------------
// Type coercion
// ---------------------------------------------------------------------------

/// Coerce a raw string value to a JSON value based on a `FieldType`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the value cannot be parsed as the
/// expected type.
fn coerce_to_type(
    raw: &str,
    field_type: &FieldType,
) -> Result<serde_json::Value, FraiseQLError> {
    match field_type {
        FieldType::Int => {
            let v: i64 = raw.parse().map_err(|_| {
                validation_error(format!("Expected integer value, got '{raw}'."))
            })?;
            Ok(serde_json::Value::Number(v.into()))
        }
        FieldType::Float | FieldType::Decimal => {
            let v: f64 = raw.parse().map_err(|_| {
                validation_error(format!("Expected numeric value, got '{raw}'."))
            })?;
            Ok(serde_json::Number::from_f64(v)
                .map(serde_json::Value::Number)
                .unwrap_or_else(|| serde_json::Value::String(raw.to_string())))
        }
        FieldType::Boolean => {
            let v = match raw {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    return Err(validation_error(format!(
                        "Expected boolean value (true/false/1/0), got '{raw}'."
                    )));
                }
            };
            Ok(serde_json::Value::Bool(v))
        }
        FieldType::Id | FieldType::Uuid | FieldType::String | FieldType::DateTime
        | FieldType::Date | FieldType::Time => {
            Ok(serde_json::Value::String(raw.to_string()))
        }
        FieldType::Json => {
            serde_json::from_str(raw).map_err(|e| {
                validation_error(format!("Expected JSON value, got '{raw}': {e}"))
            })
        }
        FieldType::List(_) => {
            // Try JSON array first, then comma-separated.
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
                if v.is_array() {
                    return Ok(v);
                }
            }
            // Comma-separated string values.
            let items: Vec<serde_json::Value> =
                raw.split(',').map(|s| serde_json::Value::String(s.trim().to_string())).collect();
            Ok(serde_json::Value::Array(items))
        }
        // Scalar, Enum, Object, etc. — pass through as string.
        _ => Ok(serde_json::Value::String(raw.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Parse a bracket key like `name[icontains]` into `("name", "icontains")`.
fn parse_bracket_key(key: &str) -> Option<(String, String)> {
    let open = key.find('[')?;
    let close = key.find(']')?;
    if close <= open + 1 || close != key.len() - 1 {
        return None;
    }
    let field = &key[..open];
    let op = &key[open + 1..close];
    if field.is_empty() || op.is_empty() {
        return None;
    }
    Some((field.to_string(), op.to_string()))
}

/// Compute the nesting depth of a JSON value.
fn json_depth(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            1 + map.values().map(json_depth).max().unwrap_or(0)
        }
        serde_json::Value::Array(arr) => {
            1 + arr.iter().map(json_depth).max().unwrap_or(0)
        }
        _ => 1,
    }
}

/// Count the number of field-level entries in a WHERE clause value.
fn count_where_fields(value: &serde_json::Value) -> usize {
    match value.as_object() {
        Some(map) => map.len(),
        None => 1,
    }
}

/// Get sorted field output names from a `TypeDefinition`.
fn field_names(td: &TypeDefinition) -> Vec<&str> {
    let mut names: Vec<&str> = td.fields.iter().map(|f| f.output_name()).collect();
    names.sort_unstable();
    names
}

/// Convenience constructor for `FraiseQLError::Validation`.
const fn validation_error(message: String) -> FraiseQLError {
    FraiseQLError::Validation {
        message,
        path: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test assertions use unwrap/unwrap_err intentionally
mod tests {
    use fraiseql_core::schema::{
        ArgumentDefinition, AutoParams, FieldDefinition, FieldType, QueryDefinition, RestConfig,
        TypeDefinition,
    };

    use super::*;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn test_config() -> RestConfig {
        RestConfig {
            max_page_size:     100,
            default_page_size: 20,
            max_filter_bytes:  4096,
            ..RestConfig::default()
        }
    }

    fn user_type_def() -> TypeDefinition {
        TypeDefinition::new("User", "v_user")
            .with_field(FieldDefinition::new("id", FieldType::Uuid))
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(FieldDefinition::new("email", FieldType::String))
            .with_field(FieldDefinition::new("age", FieldType::Int))
            .with_field(FieldDefinition::new("active", FieldType::Boolean))
    }

    fn list_query_def() -> QueryDefinition {
        QueryDefinition {
            name: "users".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            auto_params: AutoParams::all(),
            arguments: vec![
                ArgumentDefinition::optional("where", FieldType::Json),
                ArgumentDefinition::optional("orderBy", FieldType::Json),
                ArgumentDefinition::optional("limit", FieldType::Int),
                ArgumentDefinition::optional("offset", FieldType::Int),
            ],
            ..default_query_def()
        }
    }

    fn single_query_def() -> QueryDefinition {
        QueryDefinition {
            name: "user".to_string(),
            return_type: "User".to_string(),
            returns_list: false,
            arguments: vec![ArgumentDefinition::new("id", FieldType::Uuid)],
            ..default_query_def()
        }
    }

    fn relay_query_def() -> QueryDefinition {
        QueryDefinition {
            name: "users".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            relay: true,
            relay_cursor_column: Some("pk_user".to_string()),
            auto_params: AutoParams::all(),
            arguments: vec![
                ArgumentDefinition::optional("first", FieldType::Int),
                ArgumentDefinition::optional("after", FieldType::String),
                ArgumentDefinition::optional("last", FieldType::Int),
                ArgumentDefinition::optional("before", FieldType::String),
            ],
            ..default_query_def()
        }
    }

    fn default_query_def() -> QueryDefinition {
        QueryDefinition::new("test", "Test")
    }

    fn extractor_list<'a>(
        config: &'a RestConfig,
        query_def: &'a QueryDefinition,
        type_def: &'a TypeDefinition,
    ) -> RestParamExtractor<'a> {
        RestParamExtractor::new(config, query_def, Some(type_def))
    }

    // -----------------------------------------------------------------------
    // Path param extraction
    // -----------------------------------------------------------------------

    #[test]
    fn path_param_int_coercion() {
        let config = test_config();
        let qd = QueryDefinition {
            arguments: vec![ArgumentDefinition::new("id", FieldType::Int)],
            ..single_query_def()
        };
        let td = user_type_def();
        let ext = RestParamExtractor::new(&config, &qd, Some(&td));

        let result = ext.extract(&[("id", "123")], &[]).unwrap();
        assert_eq!(result.path_params, vec![("id".to_string(), serde_json::json!(123))]);
    }

    #[test]
    fn path_param_uuid_passthrough() {
        let config = test_config();
        let qd = single_query_def();
        let td = user_type_def();
        let ext = RestParamExtractor::new(&config, &qd, Some(&td));

        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let result = ext.extract(&[("id", uuid)], &[]).unwrap();
        assert_eq!(
            result.path_params,
            vec![("id".to_string(), serde_json::json!(uuid))]
        );
    }

    // -----------------------------------------------------------------------
    // Offset pagination
    // -----------------------------------------------------------------------

    #[test]
    fn offset_pagination_explicit() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("limit", "10"), ("offset", "5")]).unwrap();
        assert_eq!(result.pagination, PaginationParams::Offset { limit: 10, offset: 5 });
    }

    #[test]
    fn offset_pagination_defaults() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[]).unwrap();
        assert_eq!(
            result.pagination,
            PaginationParams::Offset {
                limit: 20, // default_page_size
                offset: 0,
            }
        );
    }

    #[test]
    fn limit_clamped_to_max_page_size() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("limit", "500")]).unwrap();
        assert_eq!(result.pagination, PaginationParams::Offset { limit: 100, offset: 0 });
    }

    // -----------------------------------------------------------------------
    // Cursor (Relay) pagination
    // -----------------------------------------------------------------------

    #[test]
    fn cursor_pagination_explicit() {
        let config = test_config();
        let qd = relay_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext
            .extract(&[], &[("first", "10"), ("after", "abc")])
            .unwrap();
        assert_eq!(
            result.pagination,
            PaginationParams::Cursor {
                first:  Some(10),
                after:  Some("abc".to_string()),
                last:   None,
                before: None,
            }
        );
    }

    #[test]
    fn cursor_pagination_defaults() {
        let config = test_config();
        let qd = relay_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[]).unwrap();
        assert_eq!(
            result.pagination,
            PaginationParams::Cursor {
                first:  Some(20), // default_page_size
                after:  None,
                last:   None,
                before: None,
            }
        );
    }

    #[test]
    fn first_clamped_to_max_page_size() {
        let config = test_config();
        let qd = relay_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("first", "500")]).unwrap();
        match result.pagination {
            PaginationParams::Cursor { first, .. } => assert_eq!(first, Some(100)),
            other => panic!("expected Cursor, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Cross-pagination guards
    // -----------------------------------------------------------------------

    #[test]
    fn relay_rejects_limit_offset() {
        let config = test_config();
        let qd = relay_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("limit", "10")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("cursor-based pagination"), "got: {msg}");
        assert!(msg.contains("first"), "got: {msg}");
    }

    #[test]
    fn offset_rejects_first_after() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("first", "10")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("offset-based pagination"), "got: {msg}");
        assert!(msg.contains("limit"), "got: {msg}");
    }

    // -----------------------------------------------------------------------
    // Simple equality filters
    // -----------------------------------------------------------------------

    #[test]
    fn simple_equality_filter() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("name", "Alice")]).unwrap();
        assert_eq!(
            result.where_clause,
            Some(serde_json::json!({ "name": { "eq": "Alice" } }))
        );
    }

    // -----------------------------------------------------------------------
    // Bracket operator filters
    // -----------------------------------------------------------------------

    #[test]
    fn bracket_operator_filter() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("name[icontains]", "Ali")]).unwrap();
        assert_eq!(
            result.where_clause,
            Some(serde_json::json!({ "name": { "icontains": "Ali" } }))
        );
    }

    #[test]
    fn bracket_operator_invalid() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("name[beginsWith]", "A")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unknown bracket operator"), "got: {msg}");
        assert!(msg.contains("Available bracket operators"), "got: {msg}");
    }

    // -----------------------------------------------------------------------
    // JSON filter
    // -----------------------------------------------------------------------

    #[test]
    fn json_filter_passthrough() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let filter = r#"{"name":{"startswith":"A"}}"#;
        let result = ext.extract(&[], &[("filter", filter)]).unwrap();
        assert_eq!(
            result.where_clause,
            Some(serde_json::json!({ "name": { "startswith": "A" } }))
        );
    }

    #[test]
    fn filter_exceeding_max_bytes() {
        let config = RestConfig {
            max_filter_bytes: 10,
            ..test_config()
        };
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let filter = r#"{"name":{"eq":"very long value here"}}"#;
        let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("exceeds maximum size"), "got: {msg}");
    }

    #[test]
    fn filter_unknown_field() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let filter = r#"{"bogus":{"eq":"x"}}"#;
        let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unknown field 'bogus'"), "got: {msg}");
        assert!(msg.contains("Available fields"), "got: {msg}");
    }

    #[test]
    fn filter_unknown_operator() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let filter = r#"{"name":{"bogusOp":"x"}}"#;
        let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unknown filter operator"), "got: {msg}");
        assert!(msg.contains("Available operators"), "got: {msg}");
    }

    #[test]
    fn filter_nesting_depth_exceeded() {
        let config = test_config();
        let qd = list_query_def();
        // No type_def — skip field validation so deeply nested JSON passes field check.
        let ext = RestParamExtractor::new(&config, &qd, None);

        // Build JSON with depth > 64.
        let mut json = r#""leaf""#.to_string();
        for i in 0..65 {
            json = format!(r#"{{"k{i}":{json}}}"#);
        }
        let filter = &json;
        let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nesting depth"), "got: {msg}");
    }

    // -----------------------------------------------------------------------
    // Sort
    // -----------------------------------------------------------------------

    #[test]
    fn sort_ascending_descending() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("sort", "name,-age")]).unwrap();
        assert_eq!(
            result.order_by,
            Some(serde_json::json!([
                { "field": "name", "direction": "ASC" },
                { "field": "age", "direction": "DESC" },
            ]))
        );
    }

    #[test]
    fn sort_invalid_field() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("sort", "bogus")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unknown field 'bogus'"), "got: {msg}");
    }

    // -----------------------------------------------------------------------
    // Select
    // -----------------------------------------------------------------------

    #[test]
    fn select_fields() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("select", "id,name")]).unwrap();
        assert_eq!(
            result.field_selection,
            RestFieldSpec::Fields(vec!["id".to_string(), "name".to_string()])
        );
    }

    #[test]
    fn select_dot_notation_rejected() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("select", "address.city")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Dot notation not supported"), "got: {msg}");
    }

    // -----------------------------------------------------------------------
    // Unknown param
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_param_rejected() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("unknown", "x")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unknown query parameter"), "got: {msg}");
        assert!(msg.contains("Available parameters"), "got: {msg}");
    }

    // -----------------------------------------------------------------------
    // Type coercion
    // -----------------------------------------------------------------------

    #[test]
    fn coerce_int() {
        let result = coerce_to_type("42", &FieldType::Int).unwrap();
        assert_eq!(result, serde_json::json!(42));
    }

    #[test]
    fn coerce_float() {
        let result = coerce_to_type("2.78", &FieldType::Float).unwrap();
        assert_eq!(result, serde_json::json!(2.78));
    }

    #[test]
    fn coerce_boolean_true() {
        assert_eq!(coerce_to_type("true", &FieldType::Boolean).unwrap(), serde_json::json!(true));
        assert_eq!(coerce_to_type("1", &FieldType::Boolean).unwrap(), serde_json::json!(true));
        assert_eq!(coerce_to_type("yes", &FieldType::Boolean).unwrap(), serde_json::json!(true));
    }

    #[test]
    fn coerce_boolean_false() {
        assert_eq!(coerce_to_type("false", &FieldType::Boolean).unwrap(), serde_json::json!(false));
        assert_eq!(coerce_to_type("0", &FieldType::Boolean).unwrap(), serde_json::json!(false));
    }

    #[test]
    fn coerce_boolean_invalid() {
        let err = coerce_to_type("maybe", &FieldType::Boolean).unwrap_err();
        assert!(err.to_string().contains("Expected boolean"), "{err}");
    }

    #[test]
    fn coerce_string_passthrough() {
        let result = coerce_to_type("hello", &FieldType::String).unwrap();
        assert_eq!(result, serde_json::json!("hello"));
    }

    #[test]
    fn coerce_json_value() {
        let result = coerce_to_type(r#"{"key":"val"}"#, &FieldType::Json).unwrap();
        assert_eq!(result, serde_json::json!({"key": "val"}));
    }

    #[test]
    fn coerce_list_csv() {
        let result = coerce_to_type("a,b,c", &FieldType::List(Box::new(FieldType::String))).unwrap();
        assert_eq!(result, serde_json::json!(["a", "b", "c"]));
    }

    #[test]
    fn coerce_list_json_array() {
        let result =
            coerce_to_type(r#"["a","b"]"#, &FieldType::List(Box::new(FieldType::String))).unwrap();
        assert_eq!(result, serde_json::json!(["a", "b"]));
    }

    // -----------------------------------------------------------------------
    // Single-resource endpoint
    // -----------------------------------------------------------------------

    #[test]
    fn single_resource_no_pagination() {
        let config = test_config();
        let qd = single_query_def();
        let td = user_type_def();
        let ext = RestParamExtractor::new(&config, &qd, Some(&td));

        let result = ext.extract(&[("id", "550e8400-e29b-41d4-a716-446655440000")], &[]).unwrap();
        assert_eq!(result.pagination, PaginationParams::None);
    }

    // -----------------------------------------------------------------------
    // Variables count limit
    // -----------------------------------------------------------------------

    #[test]
    fn total_params_exceeding_max() {
        let config = test_config();
        let qd = list_query_def();
        // No type_def to skip field validation.
        let ext = RestParamExtractor::new(&config, &qd, None);

        // Build > 1000 simple filters.
        let pairs: Vec<(String, String)> = (0..1001)
            .map(|i| (format!("f{i}"), format!("v{i}")))
            .collect();
        let query_pairs: Vec<(&str, &str)> =
            pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        let err = ext.extract(&[], &query_pairs).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Too many parameters"), "got: {msg}");
    }

    // -----------------------------------------------------------------------
    // parse_bracket_key helper
    // -----------------------------------------------------------------------

    #[test]
    fn parse_bracket_key_valid() {
        assert_eq!(
            parse_bracket_key("name[icontains]"),
            Some(("name".to_string(), "icontains".to_string()))
        );
    }

    #[test]
    fn parse_bracket_key_no_brackets() {
        assert_eq!(parse_bracket_key("name"), None);
    }

    #[test]
    fn parse_bracket_key_empty_op() {
        assert_eq!(parse_bracket_key("name[]"), None);
    }

    #[test]
    fn parse_bracket_key_empty_field() {
        assert_eq!(parse_bracket_key("[op]"), None);
    }

    // -----------------------------------------------------------------------
    // json_depth helper
    // -----------------------------------------------------------------------

    #[test]
    fn json_depth_flat() {
        assert_eq!(json_depth(&serde_json::json!("hello")), 1);
    }

    #[test]
    fn json_depth_nested_object() {
        assert_eq!(json_depth(&serde_json::json!({"a": {"b": "c"}})), 3);
    }

    #[test]
    fn json_depth_nested_array() {
        assert_eq!(json_depth(&serde_json::json!([[[1]]])), 4);
    }
}
