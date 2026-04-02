//! REST parameter extraction and type coercion.
//!
//! Parses path parameters, query string parameters (`?select=`, `?sort=`,
//! `?limit=`, `?offset=`, `?first=`, `?after=`, `?filter=`, bracket operators),
//! validates against [`QueryDefinition`] / [`TypeDefinition`] metadata, and
//! returns a typed [`ExtractedParams`].

use std::collections::HashMap;

use fraiseql_core::{
    schema::{FieldType, QueryDefinition, RestConfig, TypeDefinition},
    utils::operators::OPERATOR_REGISTRY,
};
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

/// Maximum nesting depth for logical operator groups (`or=()`, `and=()`, `not=()`).
const MAX_LOGICAL_DEPTH: usize = 64;

/// Known query-string parameter names that are *not* filter keys.
const RESERVED_PARAMS: &[&str] = &[
    "select", "sort", "limit", "offset", "first", "after", "last", "before", "filter", "search",
    "or", "and", "not",
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Extracted and validated parameters from a REST request.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ExtractedParams {
    /// Path parameters (e.g., `[("id", 123)]`).
    pub path_params:       Vec<(String, serde_json::Value)>,
    /// WHERE clause for the query (merged from simple/bracket/filter/logical params).
    pub where_clause:      Option<serde_json::Value>,
    /// ORDER BY clause (from `?sort=`).
    pub order_by:          Option<serde_json::Value>,
    /// Pagination parameters.
    pub pagination:        PaginationParams,
    /// Field selection (from `?select=`).
    pub field_selection:   RestFieldSpec,
    /// Full-text search query (from `?search=`).
    pub search_query:      Option<String>,
    /// Embedded resource specifications (from parenthetical select syntax).
    pub embeddings:        Vec<EmbeddedSpec>,
    /// Embedded resource filters (from `?rel.field[op]=value` syntax).
    pub embedding_filters: HashMap<String, serde_json::Value>,
    /// Count-only embeddings (from `?select=id,posts.count`).
    pub embedding_counts:  Vec<String>,
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

/// A single entry in a parsed `?select=` list, either a flat field or embedded resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectEntry {
    /// A flat field name (e.g., `"id"`).
    Field(String),
    /// An embedded resource with parenthetical sub-select (e.g., `posts(id,title)`).
    Embedded(EmbeddedSpec),
    /// Count-only embedding (e.g., `posts.count`).
    Count(String),
}

/// Specification for an embedded (nested) resource in a `?select=` parameter.
///
/// Represents `posts(id,title)` or `author:fk_user(id,name)` syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedSpec {
    /// Relationship name (e.g., "posts") or FK column (e.g., "`fk_user`").
    pub relationship: String,
    /// Optional rename for the embedded field (e.g., `author` in `author:fk_user(...)`).
    pub rename:       Option<String>,
    /// Sub-selected fields (may include nested `EmbeddedSpec`).
    pub fields:       Vec<SelectEntry>,
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
        let mut search_raw: Option<&str> = None;
        let mut logical_groups: Vec<(&str, &str)> = Vec::new(); // (operator, value)

        for &(key, value) in query_pairs {
            // Skip dot-prefixed params (embedding filters) — handled in step 9.
            if key.contains('.') && !RESERVED_PARAMS.contains(&key) {
                continue;
            }

            if let Some((field, op)) = parse_bracket_key(key) {
                // Skip embedding bracket filters (e.g., posts.status[eq]=value).
                if field.contains('.') {
                    continue;
                }
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
                    "search" => search_raw = Some(value),
                    "or" | "and" | "not" => logical_groups.push((key, value)),
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
                    },
                }
            }
        }

        // 3. Cross-pagination guards.
        if is_list {
            let has_offset_params = limit_raw.is_some() || offset_raw.is_some();
            let has_cursor_params = first_raw.is_some()
                || after_raw.is_some()
                || last_raw.is_some()
                || before_raw.is_some();

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

        // 4. Parse select (with embedding support).
        let (field_selection, embeddings, embedding_counts) =
            self.parse_select_with_embeddings(select_raw)?;

        // 5. Parse sort.
        let order_by = self.parse_sort(sort_raw)?;

        // 6a. Validate and parse search.
        let search_query = if let Some(raw) = search_raw {
            if !is_list {
                return Err(validation_error(
                    "Full-text search not available on single-resource endpoints.".to_string(),
                ));
            }
            if let Some(td) = self.type_def {
                if td.searchable_fields().is_empty() {
                    return Err(validation_error(format!(
                        "Full-text search not available on '{}'. \
                         No searchable fields configured.",
                        td.name.as_str()
                    )));
                }
            }
            Some(raw.to_string())
        } else {
            None
        };

        // 6b. Parse logical operators.
        let parsed_logical = self.parse_logical_groups(&logical_groups)?;

        // 6c. Build where clause (excluding embedding filters).
        let filter_where = self.parse_filter(filter_json)?;
        let where_clause = self.merge_where_with_logical(
            simple_filters,
            bracket_filters,
            filter_where,
            parsed_logical,
        )?;

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
                PaginationParams::Cursor {
                    first,
                    after,
                    last,
                    before,
                } => {
                    usize::from(first.is_some())
                        + usize::from(after.is_some())
                        + usize::from(last.is_some())
                        + usize::from(before.is_some())
                },
                PaginationParams::None => 0,
            }
            + match &field_selection {
                RestFieldSpec::All => 0,
                RestFieldSpec::Fields(f) => f.len(),
            }
            + embeddings.len()
            + embedding_counts.len();

        if total_count > MAX_VARIABLES_COUNT {
            return Err(validation_error(format!(
                "Too many parameters ({total_count}). Maximum allowed: {MAX_VARIABLES_COUNT}."
            )));
        }

        // 9. Extract embedding filters (dot-prefixed query params).
        let embedding_filters = self.extract_embedding_filters(query_pairs)?;

        Ok(ExtractedParams {
            path_params,
            where_clause,
            order_by,
            pagination,
            field_selection,
            search_query,
            embeddings,
            embedding_filters,
            embedding_counts,
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

    /// Parse `?select=` with support for parenthetical embedding syntax.
    ///
    /// Returns `(flat_field_spec, embedded_specs, count_fields)`.
    fn parse_select_with_embeddings(
        &self,
        raw: Option<&str>,
    ) -> Result<(RestFieldSpec, Vec<EmbeddedSpec>, Vec<String>), FraiseQLError> {
        let Some(raw) = raw else {
            return Ok((RestFieldSpec::All, Vec::new(), Vec::new()));
        };
        if raw.is_empty() {
            return Ok((RestFieldSpec::All, Vec::new(), Vec::new()));
        }

        let entries = parse_select_entries(raw)?;

        let max_depth = self.config.max_embedding_depth;
        let mut flat_fields = Vec::new();
        let mut embedded = Vec::new();
        let mut counts = Vec::new();

        for entry in entries {
            match entry {
                SelectEntry::Field(name) => {
                    self.validate_field_name(&name)?;
                    flat_fields.push(name);
                },
                SelectEntry::Embedded(spec) => {
                    validate_embedding_depth(&spec, 1, max_depth as usize)?;
                    self.validate_embedding_relationship(&spec)?;
                    embedded.push(spec);
                },
                SelectEntry::Count(name) => {
                    self.validate_embedding_relationship_name(&name)?;
                    counts.push(name);
                },
            }
        }

        let field_spec = if flat_fields.is_empty() && !embedded.is_empty() {
            // Only embedded fields selected — return All for the parent fields
            RestFieldSpec::All
        } else {
            RestFieldSpec::Fields(flat_fields)
        };

        Ok((field_spec, embedded, counts))
    }

    /// Validate that an embedded relationship name exists on the type.
    fn validate_embedding_relationship(&self, spec: &EmbeddedSpec) -> Result<(), FraiseQLError> {
        self.validate_embedding_relationship_name(&spec.relationship)
    }

    /// Validate a relationship name exists on the type.
    fn validate_embedding_relationship_name(&self, name: &str) -> Result<(), FraiseQLError> {
        let Some(td) = self.type_def else {
            return Err(validation_error(format!(
                "Cannot embed '{name}': type definition not available"
            )));
        };

        let has_rel = td.relationships.iter().any(|r| r.name == name);
        if !has_rel {
            let available: Vec<&str> = td.relationships.iter().map(|r| r.name.as_str()).collect();
            let avail_str = if available.is_empty() {
                "none".to_string()
            } else {
                available.join(", ")
            };
            return Err(validation_error(format!(
                "Type '{}' has no relationship '{name}'. Available: {avail_str}",
                td.name.as_str()
            )));
        }
        Ok(())
    }

    /// Extract embedding filters from dot-prefixed query params.
    ///
    /// E.g., `?posts.status=published` or `?posts.status[eq]=published`.
    fn extract_embedding_filters(
        &self,
        query_pairs: &[(&str, &str)],
    ) -> Result<HashMap<String, serde_json::Value>, FraiseQLError> {
        let mut filters: HashMap<String, serde_json::Value> = HashMap::new();

        for &(key, value) in query_pairs {
            // Check for dot-prefixed bracket: posts.status[eq]=value
            if let Some((full_field, op)) = parse_bracket_key(key) {
                if let Some(dot_pos) = full_field.find('.') {
                    let rel_name = &full_field[..dot_pos];
                    let field_name = &full_field[dot_pos + 1..];
                    let entry = filters
                        .entry(rel_name.to_string())
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert(field_name.to_string(), serde_json::json!({ op: value }));
                    }
                    continue;
                }
            }

            // Check for dot-prefixed simple: posts.status=published
            if let Some(dot_pos) = key.find('.') {
                if !RESERVED_PARAMS.contains(&key) {
                    let rel_name = &key[..dot_pos];
                    let field_name = &key[dot_pos + 1..];
                    let entry = filters
                        .entry(rel_name.to_string())
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert(field_name.to_string(), serde_json::json!({ "eq": value }));
                    }
                }
            }
        }

        Ok(filters)
    }

    // -----------------------------------------------------------------------
    // Sort
    // -----------------------------------------------------------------------

    fn parse_sort(&self, raw: Option<&str>) -> Result<Option<serde_json::Value>, FraiseQLError> {
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

    fn parse_filter(&self, raw: Option<&str>) -> Result<Option<serde_json::Value>, FraiseQLError> {
        let Some(raw) = raw else {
            return Ok(None);
        };

        // Size check.
        if raw.len() > usize::try_from(self.config.max_filter_bytes).unwrap_or(usize::MAX) {
            return Err(validation_error(format!(
                "Filter parameter exceeds maximum size ({} bytes). \
                 Maximum allowed: {} bytes.",
                raw.len(),
                self.config.max_filter_bytes
            )));
        }

        let parsed: serde_json::Value = serde_json::from_str(raw)
            .map_err(|e| validation_error(format!("Invalid filter JSON: {e}")))?;

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

    fn validate_filter_value(&self, value: &serde_json::Value) -> Result<(), FraiseQLError> {
        let Some(obj) = value.as_object() else {
            return Ok(());
        };

        for (key, inner) in obj {
            // Logical DSL keys: recurse into their array elements.
            if matches!(key.as_str(), "_or" | "_and" | "_not") {
                if let Some(arr) = inner.as_array() {
                    for item in arr {
                        self.validate_filter_value(item)?;
                    }
                }
                continue;
            }

            // Top-level keys are field names.
            self.validate_field_name(key)?;

            // Inner object keys should be operators.
            if let Some(ops) = inner.as_object() {
                for op_name in ops.keys() {
                    if !OPERATOR_REGISTRY.contains_key(op_name.as_str()) {
                        let available: Vec<&str> = {
                            let mut ops: Vec<&str> = OPERATOR_REGISTRY.keys().copied().collect();
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
    // Logical operators
    // -----------------------------------------------------------------------

    /// Parse `?or=()`, `?and=()`, `?not=()` query parameters into JSON DSL.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if:
    /// - The value is not enclosed in parentheses
    /// - Nesting depth exceeds `MAX_LOGICAL_DEPTH`
    fn parse_logical_groups(
        &self,
        groups: &[(&str, &str)],
    ) -> Result<Vec<serde_json::Value>, FraiseQLError> {
        let mut result = Vec::with_capacity(groups.len());
        for &(op, value) in groups {
            let dsl_key = format!("_{op}");
            let parsed = parse_logical_group(value, &dsl_key, 1)?;
            result.push(parsed);
        }
        Ok(result)
    }

    /// Merge WHERE clause from all sources including logical operators.
    ///
    /// When both regular filters and logical groups are present, they are
    /// combined with an implicit `_and`.
    fn merge_where_with_logical(
        &self,
        simple: Vec<(String, serde_json::Value)>,
        bracket: Vec<(String, String, String)>,
        filter: Option<serde_json::Value>,
        logical: Vec<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, FraiseQLError> {
        let regular = self.merge_where(simple, bracket, filter)?;

        if logical.is_empty() {
            return Ok(regular);
        }

        // Wrap each logical group as-is (they're already `{"_or": [...]}` etc).
        match regular {
            Some(regular_where) => {
                // Combine: { "_and": [regular_filters, ...logical_groups] }
                let mut and_parts = vec![regular_where];
                and_parts.extend(logical);
                Ok(Some(serde_json::json!({ "_and": and_parts })))
            },
            None if logical.len() == 1 => {
                // Single logical group with no other filters.
                // SAFE: match guard `logical.len() == 1` guarantees next() returns Some.
                Ok(Some(logical.into_iter().next().expect("match guard guarantees len == 1")))
            },
            None => {
                // Multiple logical groups, no regular filters — wrap in _and.
                Ok(Some(serde_json::json!({ "_and": logical })))
            },
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
                    validation_error(format!(
                        "Invalid `limit` value: '{s}'. Expected a positive integer."
                    ))
                })?;
                v.min(self.config.max_page_size)
            },
            None => self.config.default_page_size,
        };
        let offset = match offset_raw {
            Some(s) => s.parse().map_err(|_| {
                validation_error(format!(
                    "Invalid `offset` value: '{s}'. Expected a non-negative integer."
                ))
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
                    validation_error(format!(
                        "Invalid `first` value: '{s}'. Expected a positive integer."
                    ))
                })?;
                Some(v.min(self.config.max_page_size))
            },
            None if after_raw.is_none() && last_raw.is_none() && before_raw.is_none() => {
                // No cursor params at all — default page size.
                Some(self.config.default_page_size)
            },
            None => None,
        };
        let after = after_raw.map(String::from);
        let last = match last_raw {
            Some(s) => Some(s.parse().map_err(|_| {
                validation_error(format!(
                    "Invalid `last` value: '{s}'. Expected a positive integer."
                ))
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
fn coerce_to_type(raw: &str, field_type: &FieldType) -> Result<serde_json::Value, FraiseQLError> {
    match field_type {
        FieldType::Int => {
            let v: i64 = raw
                .parse()
                .map_err(|_| validation_error(format!("Expected integer value, got '{raw}'.")))?;
            Ok(serde_json::Value::Number(v.into()))
        },
        FieldType::Float | FieldType::Decimal => {
            let v: f64 = raw
                .parse()
                .map_err(|_| validation_error(format!("Expected numeric value, got '{raw}'.")))?;
            Ok(serde_json::Number::from_f64(v).map_or_else(
                || serde_json::Value::String(raw.to_string()),
                serde_json::Value::Number,
            ))
        },
        FieldType::Boolean => {
            let v = match raw {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    return Err(validation_error(format!(
                        "Expected boolean value (true/false/1/0), got '{raw}'."
                    )));
                },
            };
            Ok(serde_json::Value::Bool(v))
        },
        FieldType::Json => serde_json::from_str(raw)
            .map_err(|e| validation_error(format!("Expected JSON value, got '{raw}': {e}"))),
        FieldType::List(_) => {
            // Try JSON array first, then comma-separated.
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
                if v.is_array() {
                    return Ok(v);
                }
            }
            // Comma-separated string values.
            let items: Vec<serde_json::Value> = raw
                .split(',')
                .map(|s| serde_json::Value::String(s.trim().to_string()))
                .collect();
            Ok(serde_json::Value::Array(items))
        },
        // Id, Uuid, String, DateTime, Date, Time, Scalar, Enum, Object, etc. — pass through as
        // string.
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

/// Parse a logical operator group value like `(name[eq]=Alice,name[eq]=Bob)`.
///
/// Returns a JSON value like `{"_or": [{"name": {"eq": "Alice"}}, {"name": {"eq": "Bob"}}]}`.
///
/// Supports nesting: `(and=(age[gte]=18,active[eq]=true),name[eq]=admin)` produces
/// `{"_or": [{"_and": [...]}, {"name": {"eq": "admin"}}]}`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if:
/// - Input is not enclosed in parentheses
/// - Nesting depth exceeds `MAX_LOGICAL_DEPTH`
fn parse_logical_group(
    input: &str,
    dsl_key: &str,
    depth: usize,
) -> Result<serde_json::Value, FraiseQLError> {
    if depth > MAX_LOGICAL_DEPTH {
        return Err(validation_error(format!(
            "Logical operator nesting depth exceeds maximum ({MAX_LOGICAL_DEPTH})."
        )));
    }

    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return Err(validation_error(format!(
            "Logical operator value must be enclosed in parentheses: `{dsl_key}=(...)`. \
             Got: `{trimmed}`"
        )));
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let parts = split_logical_parts(inner);

    let mut conditions = Vec::with_capacity(parts.len());
    for part in &parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Check for nested logical operator: `and=(...)` or `or=(...)` or `not=(...)`
        if let Some((nested_op, nested_val)) = parse_nested_logical(part) {
            let nested_key = format!("_{nested_op}");
            let nested = parse_logical_group(nested_val, &nested_key, depth + 1)?;
            conditions.push(nested);
        } else if let Some((field_op, value)) = part.split_once('=') {
            let json_val = parse_logical_value(value);
            if let Some((field, op)) = parse_bracket_key(field_op) {
                // Bracket condition: `field[op]=value`
                conditions.push(serde_json::json!({ field: { op: json_val } }));
            } else {
                // Simple equality: `field=value`
                conditions.push(serde_json::json!({ field_op: { "eq": json_val } }));
            }
        }
    }

    Ok(serde_json::json!({ dsl_key: conditions }))
}

/// Split logical group contents by commas, respecting nested parentheses.
fn split_logical_parts(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in input.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            },
            ')' => {
                depth -= 1;
                current.push(ch);
            },
            ',' if depth == 0 => {
                parts.push(current.clone());
                current.clear();
            },
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Check if a part is a nested logical operator: `and=(...)`, `or=(...)`, `not=(...)`.
fn parse_nested_logical(part: &str) -> Option<(&str, &str)> {
    for op in &["and", "or", "not"] {
        let prefix = format!("{op}=");
        if let Some(rest) = part.strip_prefix(&prefix) {
            if rest.starts_with('(') && rest.ends_with(')') {
                return Some((op, rest));
            }
        }
    }
    None
}

/// Parse a value from a logical group, attempting numeric and boolean coercion.
fn parse_logical_value(raw: &str) -> serde_json::Value {
    // Try integer.
    if let Ok(v) = raw.parse::<i64>() {
        return serde_json::Value::Number(v.into());
    }
    // Try float.
    if let Ok(v) = raw.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return serde_json::Value::Number(n);
        }
    }
    // Try boolean.
    match raw {
        "true" => return serde_json::Value::Bool(true),
        "false" => return serde_json::Value::Bool(false),
        _ => {},
    }
    // Default to string.
    serde_json::Value::String(raw.to_string())
}

/// Compute the nesting depth of a JSON value.
fn json_depth(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => 1 + map.values().map(json_depth).max().unwrap_or(0),
        serde_json::Value::Array(arr) => 1 + arr.iter().map(json_depth).max().unwrap_or(0),
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
// Parenthetical select parser
// ---------------------------------------------------------------------------

/// Parse a `?select=` value into a list of [`SelectEntry`] items.
///
/// Supports:
/// - Flat fields: `id`, `name`
/// - Embedded resources: `posts(id,title)`
/// - Nested embedding: `posts(id,comments(id,body))`
/// - Renamed embedding: `author:fk_user(id,name)`
/// - Count-only: `posts.count`
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` on unbalanced parentheses or empty field names.
pub fn parse_select_entries(input: &str) -> Result<Vec<SelectEntry>, FraiseQLError> {
    let mut entries = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace and leading commas.
        while i < len && (chars[i] == ',' || chars[i] == ' ') {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Read the field/relationship name (until we hit '(', ',', '.', or end).
        let name_start = i;
        while i < len && chars[i] != '(' && chars[i] != ',' && chars[i] != '.' && chars[i] != ' ' {
            i += 1;
        }
        let name = &input[name_start..i];
        let name = name.trim();

        if name.is_empty() {
            return Err(validation_error("Empty field name in `select` parameter".to_string()));
        }

        // Skip whitespace.
        while i < len && chars[i] == ' ' {
            i += 1;
        }

        if i < len && chars[i] == '.' {
            // Count-only: posts.count
            i += 1; // skip '.'
            let suffix_start = i;
            while i < len && chars[i] != ',' && chars[i] != ' ' {
                i += 1;
            }
            let suffix = &input[suffix_start..i];
            if suffix == "count" {
                entries.push(SelectEntry::Count(name.to_string()));
            } else {
                return Err(validation_error(format!(
                    "Unsupported dot-suffix '{suffix}' in `select`. Only `.count` is supported."
                )));
            }
        } else if i < len && chars[i] == '(' {
            // Embedded resource: posts(id,title) or author:rel_name(id,name)
            let (rename, relationship) = if let Some(colon_pos) = name.find(':') {
                (Some(name[..colon_pos].to_string()), name[colon_pos + 1..].to_string())
            } else {
                (None, name.to_string())
            };

            // Find matching closing paren (handle nesting).
            i += 1; // skip '('
            let inner_start = i;
            let mut depth = 1;
            while i < len && depth > 0 {
                if chars[i] == '(' {
                    depth += 1;
                } else if chars[i] == ')' {
                    depth -= 1;
                }
                if depth > 0 {
                    i += 1;
                }
            }
            if depth != 0 {
                return Err(validation_error(format!(
                    "Unbalanced parentheses in `select` for '{relationship}'"
                )));
            }
            let inner = &input[inner_start..i];
            i += 1; // skip ')'

            // Recursively parse the inner fields.
            let sub_entries = parse_select_entries(inner)?;

            entries.push(SelectEntry::Embedded(EmbeddedSpec {
                relationship,
                rename,
                fields: sub_entries,
            }));
        } else {
            // Check for rename syntax on flat field (shouldn't happen, but handle gracefully).
            entries.push(SelectEntry::Field(name.to_string()));
        }
    }

    Ok(entries)
}

/// Validate that embedding depth does not exceed the configured maximum.
fn validate_embedding_depth(
    spec: &EmbeddedSpec,
    current_depth: usize,
    max_depth: usize,
) -> Result<(), FraiseQLError> {
    if current_depth > max_depth {
        return Err(validation_error(format!(
            "Embedding depth {current_depth} exceeds maximum of {max_depth}. \
             Reduce nesting in `select` parameter."
        )));
    }
    for field in &spec.fields {
        if let SelectEntry::Embedded(nested) = field {
            validate_embedding_depth(nested, current_depth + 1, max_depth)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test assertions use unwrap/unwrap_err intentionally
mod tests {
    use fraiseql_core::schema::{
        ArgumentDefinition, AutoParams, Cardinality, FieldDefinition, FieldType, QueryDefinition,
        Relationship, RestConfig, TypeDefinition,
    };

    use super::*;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn test_config() -> RestConfig {
        RestConfig {
            max_page_size: 100,
            default_page_size: 20,
            max_filter_bytes: 4096,
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
        assert_eq!(result.path_params, vec![("id".to_string(), serde_json::json!(uuid))]);
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
        assert_eq!(
            result.pagination,
            PaginationParams::Offset {
                limit:  10,
                offset: 5,
            }
        );
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
                limit:  20, // default_page_size
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
        assert_eq!(
            result.pagination,
            PaginationParams::Offset {
                limit:  100,
                offset: 0,
            }
        );
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

        let result = ext.extract(&[], &[("first", "10"), ("after", "abc")]).unwrap();
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
        assert_eq!(result.where_clause, Some(serde_json::json!({ "name": { "eq": "Alice" } })));
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
        assert_eq!(result.where_clause, Some(serde_json::json!({ "name": { "startswith": "A" } })));
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
    fn select_dot_notation_rejects_non_count_suffix() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("select", "address.city")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Unsupported dot-suffix"), "got: {msg}");
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
        let result =
            coerce_to_type("a,b,c", &FieldType::List(Box::new(FieldType::String))).unwrap();
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
        let pairs: Vec<(String, String)> =
            (0..1001).map(|i| (format!("f{i}"), format!("v{i}"))).collect();
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

    // -----------------------------------------------------------------------
    // Parenthetical select parser
    // -----------------------------------------------------------------------

    #[test]
    fn parse_select_entries_flat_fields() {
        let entries = parse_select_entries("id,name,email").unwrap();
        assert_eq!(
            entries,
            vec![
                SelectEntry::Field("id".to_string()),
                SelectEntry::Field("name".to_string()),
                SelectEntry::Field("email".to_string()),
            ]
        );
    }

    #[test]
    fn parse_select_entries_embedded() {
        let entries = parse_select_entries("id,name,posts(id,title)").unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], SelectEntry::Field("id".to_string()));
        assert_eq!(entries[1], SelectEntry::Field("name".to_string()));
        match &entries[2] {
            SelectEntry::Embedded(spec) => {
                assert_eq!(spec.relationship, "posts");
                assert!(spec.rename.is_none());
                assert_eq!(
                    spec.fields,
                    vec![
                        SelectEntry::Field("id".to_string()),
                        SelectEntry::Field("title".to_string()),
                    ]
                );
            },
            _ => panic!("Expected Embedded"),
        }
    }

    #[test]
    fn parse_select_entries_nested_depth_2() {
        let entries = parse_select_entries("id,posts(id,title,comments(id,body))").unwrap();
        assert_eq!(entries.len(), 2);
        match &entries[1] {
            SelectEntry::Embedded(spec) => {
                assert_eq!(spec.relationship, "posts");
                assert_eq!(spec.fields.len(), 3);
                match &spec.fields[2] {
                    SelectEntry::Embedded(inner) => {
                        assert_eq!(inner.relationship, "comments");
                        assert_eq!(
                            inner.fields,
                            vec![
                                SelectEntry::Field("id".to_string()),
                                SelectEntry::Field("body".to_string()),
                            ]
                        );
                    },
                    _ => panic!("Expected nested Embedded"),
                }
            },
            _ => panic!("Expected Embedded"),
        }
    }

    #[test]
    fn parse_select_entries_rename_syntax() {
        let entries = parse_select_entries("id,author:fk_user(id,name)").unwrap();
        assert_eq!(entries.len(), 2);
        match &entries[1] {
            SelectEntry::Embedded(spec) => {
                assert_eq!(spec.relationship, "fk_user");
                assert_eq!(spec.rename, Some("author".to_string()));
                assert_eq!(
                    spec.fields,
                    vec![
                        SelectEntry::Field("id".to_string()),
                        SelectEntry::Field("name".to_string()),
                    ]
                );
            },
            _ => panic!("Expected Embedded"),
        }
    }

    #[test]
    fn parse_select_entries_count_only() {
        let entries = parse_select_entries("id,posts.count").unwrap();
        assert_eq!(
            entries,
            vec![
                SelectEntry::Field("id".to_string()),
                SelectEntry::Count("posts".to_string()),
            ]
        );
    }

    #[test]
    fn parse_select_entries_unbalanced_parens() {
        let err = parse_select_entries("id,posts(id,title").unwrap_err();
        assert!(err.to_string().contains("Unbalanced parentheses"));
    }

    #[test]
    fn parse_select_entries_invalid_dot_suffix() {
        let err = parse_select_entries("id,posts.foo").unwrap_err();
        assert!(err.to_string().contains("Unsupported dot-suffix"));
    }

    // -----------------------------------------------------------------------
    // Embedding depth validation
    // -----------------------------------------------------------------------

    #[test]
    fn embedding_depth_within_limit() {
        let spec = EmbeddedSpec {
            relationship: "posts".to_string(),
            rename:       None,
            fields:       vec![SelectEntry::Field("id".to_string())],
        };
        assert!(validate_embedding_depth(&spec, 1, 3).is_ok());
    }

    #[test]
    fn embedding_depth_exceeds_limit() {
        let inner = EmbeddedSpec {
            relationship: "comments".to_string(),
            rename:       None,
            fields:       vec![SelectEntry::Field("id".to_string())],
        };
        let outer = EmbeddedSpec {
            relationship: "posts".to_string(),
            rename:       None,
            fields:       vec![SelectEntry::Embedded(inner)],
        };
        // depth=1, max=1 -> inner at depth=2 should fail
        let err = validate_embedding_depth(&outer, 1, 1).unwrap_err();
        assert!(err.to_string().contains("exceeds maximum"));
    }

    // -----------------------------------------------------------------------
    // Embedding relationship validation via extractor
    // -----------------------------------------------------------------------

    fn user_type_with_relationships() -> TypeDefinition {
        let mut td = user_type_def();
        td.relationships = vec![Relationship {
            name:           "posts".to_string(),
            target_type:    "Post".to_string(),
            foreign_key:    "fk_user".to_string(),
            referenced_key: "pk_user".to_string(),
            cardinality:    Cardinality::OneToMany,
        }];
        td
    }

    #[test]
    fn extract_with_valid_embedding() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_with_relationships();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("select", "id,name,posts(id,title)")]);
        let params = result.unwrap();
        assert_eq!(params.embeddings.len(), 1);
        assert_eq!(params.embeddings[0].relationship, "posts");
    }

    #[test]
    fn extract_with_invalid_relationship() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def(); // No relationships
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("select", "id,comments(id,body)")]).unwrap_err();
        assert!(err.to_string().contains("has no relationship 'comments'"));
        assert!(err.to_string().contains("Available: none"));
    }

    #[test]
    fn extract_with_embedding_filter() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_with_relationships();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(
            &[],
            &[
                ("select", "id,posts(id,title)"),
                ("posts.status", "published"),
            ],
        );
        let params = result.unwrap();
        assert_eq!(params.embedding_filters.len(), 1);
        let posts_filter = params.embedding_filters.get("posts").unwrap();
        assert_eq!(posts_filter, &serde_json::json!({"status": {"eq": "published"}}),);
    }

    #[test]
    fn extract_count_only_embedding() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_with_relationships();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("select", "id,posts.count")]);
        let params = result.unwrap();
        assert_eq!(params.embedding_counts, vec!["posts"]);
    }

    #[test]
    fn extract_embedding_depth_exceeded() {
        let mut config = test_config();
        config.max_embedding_depth = 1;
        let qd = list_query_def();
        let td = user_type_with_relationships();
        let ext = extractor_list(&config, &qd, &td);

        // Depth 2: posts -> comments (but max is 1)
        let err = ext.extract(&[], &[("select", "id,posts(id,comments(id,body))")]).unwrap_err();
        assert!(err.to_string().contains("exceeds maximum"));
    }

    // -----------------------------------------------------------------------
    // Full-text search
    // -----------------------------------------------------------------------

    fn article_type_def() -> TypeDefinition {
        TypeDefinition::new("Article", "v_article")
            .with_field(FieldDefinition::new("id", FieldType::Uuid))
            .with_field(FieldDefinition::new("title", FieldType::String))
            .with_field(FieldDefinition::new("body", FieldType::String))
            .with_field(FieldDefinition::new("status", FieldType::String))
    }

    fn article_list_query_def() -> QueryDefinition {
        QueryDefinition {
            name: "articles".to_string(),
            return_type: "Article".to_string(),
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

    #[test]
    fn search_param_parsed() {
        let config = test_config();
        let qd = article_list_query_def();
        let td = article_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("search", "rust async")]).unwrap();
        assert_eq!(result.search_query, Some("rust async".to_string()));
    }

    #[test]
    fn search_combined_with_filters() {
        let config = test_config();
        let qd = article_list_query_def();
        let td = article_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("search", "rust"), ("status[eq]", "published")]).unwrap();
        assert_eq!(result.search_query, Some("rust".to_string()));
        assert_eq!(
            result.where_clause,
            Some(serde_json::json!({ "status": { "eq": "published" } }))
        );
    }

    #[test]
    fn search_on_resource_without_searchable_fields_fails() {
        let config = test_config();
        let qd = list_query_def();
        // Use a type with no String fields so searchable_fields() returns empty.
        let td = TypeDefinition::new("Counter", "v_counter")
            .with_field(FieldDefinition::new("id", FieldType::Uuid))
            .with_field(FieldDefinition::new("value", FieldType::Int));
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("search", "hello")]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Full-text search not available"), "got: {msg}");
        assert!(msg.contains("No searchable fields"), "got: {msg}");
    }

    #[test]
    fn search_with_explicit_sort_preserves_sort() {
        let config = test_config();
        let qd = article_list_query_def();
        let td = article_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("search", "rust"), ("sort", "title")]).unwrap();
        assert_eq!(result.search_query, Some("rust".to_string()));
        assert!(result.order_by.is_some());
    }

    #[test]
    fn search_on_single_resource_fails() {
        // `?search=x` on a non-searchable single-resource endpoint fails with
        // "not available" (search is a reserved param, not treated as a filter).
        let config = test_config();
        let qd = single_query_def();
        let td = user_type_def();
        let ext = RestParamExtractor::new(&config, &qd, Some(&td));

        let err = ext
            .extract(&[("id", "550e8400-e29b-41d4-a716-446655440000")], &[("search", "x")])
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Full-text search not available"), "got: {msg}");
    }

    // -----------------------------------------------------------------------
    // Logical operators
    // -----------------------------------------------------------------------

    #[test]
    fn logical_or_two_conditions() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("or", "(name[eq]=Alice,name[eq]=Bob)")]).unwrap();
        assert_eq!(
            result.where_clause,
            Some(serde_json::json!({
                "_or": [
                    { "name": { "eq": "Alice" } },
                    { "name": { "eq": "Bob" } }
                ]
            }))
        );
    }

    #[test]
    fn logical_and_explicit() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("and", "(age[gte]=18,age[lte]=65)")]).unwrap();
        assert_eq!(
            result.where_clause,
            Some(serde_json::json!({
                "_and": [
                    { "age": { "gte": 18 } },
                    { "age": { "lte": 65 } }
                ]
            }))
        );
    }

    #[test]
    fn logical_not() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext.extract(&[], &[("not", "(active[eq]=false)")]).unwrap();
        assert_eq!(
            result.where_clause,
            Some(serde_json::json!({
                "_not": [
                    { "active": { "eq": false } }
                ]
            }))
        );
    }

    #[test]
    fn logical_nested_or_and() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext
            .extract(&[], &[("or", "(and=(age[gte]=18,active[eq]=true),name[eq]=admin)")])
            .unwrap();
        let wc = result.where_clause.unwrap();
        assert!(wc.get("_or").is_some(), "expected _or in {wc}");
        let or_arr = wc["_or"].as_array().unwrap();
        assert_eq!(or_arr.len(), 2);
        assert!(or_arr[0].get("_and").is_some(), "expected _and in {}", or_arr[0]);
    }

    #[test]
    fn logical_combined_with_regular_filters() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let result = ext
            .extract(
                &[],
                &[
                    ("active[eq]", "true"),
                    ("or", "(name[eq]=Alice,name[eq]=Bob)"),
                ],
            )
            .unwrap();

        let wc = result.where_clause.unwrap();
        // Should have _and wrapping the regular filter + the or group.
        assert!(wc.get("_and").is_some(), "expected _and wrapper in {wc}");
    }

    #[test]
    fn logical_depth_exceeded() {
        let config = test_config();
        let qd = list_query_def();
        // No type_def to skip field validation.
        let ext = RestParamExtractor::new(&config, &qd, None);

        // Build deeply nested: or=(and=(or=(and=(...))))
        let mut inner = "name[eq]=x".to_string();
        for _ in 0..65 {
            inner = format!("or=({inner})");
        }
        let input = format!("({inner})");
        let err = ext.extract(&[], &[("or", &input)]).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nesting depth") || msg.contains("depth"), "got: {msg}");
    }

    #[test]
    fn filter_json_with_logical_operators() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let filter = r#"{"_or":[{"name":{"eq":"Alice"}},{"name":{"eq":"Bob"}}]}"#;
        let result = ext.extract(&[], &[("filter", filter)]).unwrap();
        let wc = result.where_clause.unwrap();
        assert!(wc.get("_or").is_some(), "expected _or in {wc}");
    }

    #[test]
    fn filter_json_with_nested_logical_validates_fields() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let filter = r#"{"_or":[{"bogus":{"eq":"x"}}]}"#;
        let err = ext.extract(&[], &[("filter", filter)]).unwrap_err();
        assert!(err.to_string().contains("Unknown field 'bogus'"));
    }

    #[test]
    fn logical_invalid_syntax() {
        let config = test_config();
        let qd = list_query_def();
        let td = user_type_def();
        let ext = extractor_list(&config, &qd, &td);

        let err = ext.extract(&[], &[("or", "not-parenthetical")]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("must be enclosed in parentheses") || msg.contains("syntax"),
            "got: {msg}"
        );
    }
}
