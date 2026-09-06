//! REST parameter extraction and type coercion.
//!
//! Parses path parameters, query string parameters (`?select=`, `?sort=`,
//! `?limit=`, `?offset=`, `?first=`, `?after=`, `?filter=`, bracket operators),
//! validates against [`QueryDefinition`] / [`TypeDefinition`] metadata, and
//! returns a typed [`ExtractedParams`].

pub mod bracket;
pub mod coerce;
pub mod helpers;
pub mod logical;
pub mod select;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

pub use bracket::parse_bracket_key;
pub use coerce::coerce_to_type;
use fraiseql_core::{
    schema::{QueryDefinition, RestConfig, TypeDefinition},
    utils::operators::OPERATOR_REGISTRY,
};
use fraiseql_error::FraiseQLError;
pub use helpers::{count_where_fields, field_names, json_depth, validation_error};
pub use logical::{
    parse_logical_group, parse_logical_value, parse_nested_logical, split_logical_parts,
};
pub use select::{parse_select_entries, validate_embedding_depth};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of total extracted variables (path + query + body).
const MAX_VARIABLES_COUNT: usize = 1_000;

/// Maximum total byte length of all query-string values combined.
///
/// Prevents memory exhaustion from requests with many large parameter values
/// (e.g., 1 000 field names each 1 MB long).  The HTTP body is capped
/// separately; this limit covers URL-encoded query strings.
const MAX_QUERY_STRING_BYTES: usize = 1_048_576; // 1 MiB

/// Maximum nesting depth for `?filter=` JSON values.
const MAX_FILTER_DEPTH: usize = 64;

/// Bracket operators allowed in `?field[op]=value` syntax.
///
/// A curated subset of the full [`OPERATOR_REGISTRY`] — the operators that make
/// sense in a URL query string. Every entry is asserted to parse via
/// `WhereOperator::from_str`, so this list cannot advertise something the
/// executor refuses (#828).
pub(super) const BRACKET_OPERATORS: &[&str] = &[
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
    "contains",
    "icontains",
    "startswith",
    "istartswith",
    "endswith",
    "iendswith",
    "is_null",
    "is_not_null",
];

/// Known query-string parameter names that are *not* filter keys.
const RESERVED_PARAMS: &[&str] = &[
    "select", "sort", "limit", "offset", "first", "after", "last", "before", "filter", "search",
    "or", "and", "not",
];

/// Parse one numeric pagination parameter, naming it in the refusal.
///
/// The four numeric parameters had four hand-written copies of this, differing only in the
/// name and in whether the expected value was described as positive or non-negative. The
/// messages are unchanged.
///
/// # Errors
///
/// Returns a validation error when the value is present and not a `u64`.
fn parse_page_number(
    raw: Option<&str>,
    parameter: &str,
    expected: &str,
) -> Result<Option<u64>, FraiseQLError> {
    raw.map(|s| {
        s.parse::<u64>().map_err(|_| {
            validation_error(format!("Invalid `{parameter}` value: '{s}'. Expected {expected}."))
        })
    })
    .transpose()
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Extracted and validated parameters from a REST request.
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ExtractedParams {
    /// Path parameters (e.g., `[("id", 123)]`).
    pub path_params:          Vec<(String, serde_json::Value)>,
    /// WHERE clause for the query (merged from simple/bracket/filter/logical params).
    pub where_clause:         Option<serde_json::Value>,
    /// ORDER BY clause (from `?sort=`).
    pub order_by:             Option<serde_json::Value>,
    /// Pagination as the server will **apply** it — the client's values with
    /// `default_page_size` filled in and `max_page_size` enforced.
    pub pagination:           PaginationParams,
    /// Pagination as the client **asked** for it, before any default or clamp.
    ///
    /// See [`RequestedPagination`] for why both are recorded.
    pub requested_pagination: RequestedPagination,
    /// Field selection (from `?select=`).
    pub field_selection:      RestFieldSpec,
    /// Full-text search query (from `?search=`).
    pub search_query:         Option<String>,
    /// Embedded resource specifications (from parenthetical select syntax).
    pub embeddings:           Vec<EmbeddedSpec>,
    /// Embedded resource filters (from `?rel.field[op]=value` syntax).
    pub embedding_filters:    HashMap<String, serde_json::Value>,
    /// Count-only embeddings (from `?select=id,posts.count`).
    pub embedding_counts:     Vec<String>,
}

/// The pagination parameters the client actually sent, before any server default or clamp.
///
/// [`PaginationParams`] is the *plan*, and only the plan. Resolving a request fills an
/// absent `?limit=` with `default_page_size` and an absent cursor with
/// `first: Some(default_page_size)`, so by the time it reaches a consumer "the client asked
/// to be on a page" and "the server supplied one" are the same value. That is exactly what
/// the JSON representation wants — it needs the plan and nothing else — and wrong for every
/// consumer whose question is what the *client* asked:
///
/// * an export refuses a request that put itself on a page, and must not refuse one that did not.
///   Reading the plan meant a `relay = true` route refused every export it was ever offered, naming
///   a parameter no request had carried (#1273).
/// * an export's `?limit=` bounds its **total** rather than a page, and an absent one means "the
///   whole table" (#811) — a distinction `streaming::helpers::requested_total_limit` used to
///   recover by re-parsing the raw query pairs behind the extractor's back, and resolving a repeat
///   in the opposite direction while it was there.
///
/// Recording both, once, is what makes those consumers stop reconstructing the request.
/// Values here are exactly as sent: parsed, so an unusable one is still refused with the
/// message it always had, but never defaulted and never clamped to `max_page_size` — an
/// export total is not a page (`streaming::helpers::export_rows`).
///
/// A single-resource route leaves this empty. It applies no pagination and never parsed
/// these parameters, so there is nothing it could faithfully record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestedPagination {
    /// `?limit=` — a page size on a JSON read, an export **total** on a streamed one.
    pub limit:  Option<u64>,
    /// `?offset=`.
    pub offset: Option<u64>,
    /// `?first=`.
    pub first:  Option<u64>,
    /// `?after=`.
    pub after:  Option<String>,
    /// `?last=`.
    pub last:   Option<u64>,
    /// `?before=`.
    pub before: Option<String>,
}

impl RequestedPagination {
    /// The bound this request puts on an **export's total**, or `None` for the whole relation.
    ///
    /// One helper rather than a read at each of the three export representations: NDJSON, CSV
    /// and XLSX each open their rows through `streaming::helpers::export_rows`, and three
    /// hand-copied bodies of one rule is exactly how a fourth rule came to be missing from all
    /// three at once (#1268), in this same subsystem.
    ///
    /// # Why `limit` *or* `first`
    ///
    /// Each pagination family has exactly one **count**, and an export honours the count while
    /// refusing every position and direction (`refuse_unstreamable_request`). The offset
    /// family's count is `?limit=` (#811); the cursor family's is `?first=` (#1278).
    ///
    /// The `or` is total rather than a precedence choice: at most one of the two can be set on
    /// any request that reaches here, because the cross-pagination guard in
    /// [`RestParamExtractor::extract`] refuses `?first=` on an offset route and `?limit=` on a
    /// relay one before an export is ever resolved. Should that guard ever be relaxed, this
    /// would need a stated rule instead of an `or`.
    ///
    /// Values are as the client sent them — never defaulted, never clamped to `max_page_size`,
    /// because an export total is not a page and `export_rows` applies it to the stream.
    #[must_use]
    pub fn export_total(&self) -> Option<u64> {
        self.limit.or(self.first)
    }
}

/// Pagination mode and parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum RestFieldSpec {
    /// All fields requested (no `?select=` param).
    All,
    /// Specific flat fields: `["id", "name", "address"]`.
    Fields(Vec<String>),
}

/// A single entry in a parsed `?select=` list, either a flat field or embedded resource.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Skip unknown query parameters instead of rejecting them.
    ///
    /// Set from `Prefer: handling=lenient` (RFC 7240 §4.4). The preference was parsed,
    /// merged across repeated headers, and advertised in the served `OpenAPI` document —
    /// with the example summary "Ignore unknown parameters" — while having no reader
    /// anywhere outside `prefer.rs` (#873.1). A client whose proxy appends `utm_source`
    /// followed the published guidance and still got a 400.
    lenient:   bool,
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
            lenient: false,
        }
    }

    /// Honour `Prefer: handling=lenient` — ignore unknown query parameters rather than
    /// rejecting the request.
    ///
    /// Applies **only** to parameters this extractor does not recognise. A malformed
    /// value, an unknown bracket operator, or a filter over a field the type does not
    /// declare all still fail: "lenient" means the server tolerates parameters it was
    /// not asked about, not that it silently misreads the ones it was.
    #[must_use]
    pub const fn with_lenient_handling(mut self, lenient: bool) -> Self {
        self.lenient = lenient;
        self
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
    /// - Total query-string byte length exceeds `MAX_QUERY_STRING_BYTES`
    pub fn extract(
        &self,
        path_pairs: &[(&str, &str)],
        query_pairs: &[(&str, &str)],
    ) -> Result<ExtractedParams, FraiseQLError> {
        // 0. Enforce aggregate query-string size limit.
        let total_query_bytes: usize = query_pairs.iter().map(|(k, v)| k.len() + v.len()).sum();
        if total_query_bytes > MAX_QUERY_STRING_BYTES {
            return Err(validation_error(format!(
                "Total query string size ({total_query_bytes} bytes) exceeds maximum \
                 allowed ({MAX_QUERY_STRING_BYTES} bytes)."
            )));
        }

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
                        } else if self.lenient {
                            // `Prefer: handling=lenient`: an unrecognised parameter is
                            // ignored rather than fatal. Logged, not silent — a dropped
                            // parameter the caller believed was a filter is exactly the
                            // kind of quiet widening this program exists to remove, so it
                            // must be visible to an operator reading the logs.
                            tracing::debug!(
                                parameter = %key,
                                "ignoring unknown REST query parameter (Prefer: handling=lenient)"
                            );
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

        // 7. Parse pagination — twice over, deliberately.
        //
        // `requested_pagination` is what arrived in the query string; `pagination` is what
        // the server will apply to it. Collapsing the two is what made a bare export
        // request on a relay route indistinguishable from one asking for a page (#1273).
        // See [`RequestedPagination`].
        //
        // A single-resource route parses neither: `?limit=abc` on `/users/1` has always
        // been ignored rather than refused, and this is not the change that revisits it.
        let (requested_pagination, pagination) = if is_list {
            let requested = self.parse_requested_pagination(
                limit_raw, offset_raw, first_raw, after_raw, last_raw, before_raw,
            )?;
            let resolved = self.resolve_pagination(&requested, is_relay);
            (requested, resolved)
        } else {
            (RequestedPagination::default(), PaginationParams::None)
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
            requested_pagination,
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
    ///
    /// **No validation happens here**, and #1279 is what that costs on the JSON path: the
    /// classification loop above skips every key containing a `.`, so a dotted parameter
    /// never reaches the unknown-parameter refusal, and this stores it whatever it names.
    /// `execute_embeddings` then reads the map once per *selected* embed, so an entry naming
    /// a relationship no `?select=` asked for — or no relationship at all — is dropped in
    /// silence. `?nonsense=x` is a `400`; `?nonsense.field=x` is a `200`.
    ///
    /// The export representations refuse the whole parameter (#1275), because there no entry
    /// could ever be honoured. The JSON path honours some of them, which is why the same
    /// answer does not fit and the question is still open.
    /// Is `rel_name` a relationship this type declares — and if not, is the parameter
    /// refused or ignored?
    ///
    /// `Ok(true)` to store the filter, `Ok(false)` to drop it under
    /// `Prefer: handling=lenient`, `Err` to refuse the request.
    ///
    /// # Why this exists (#1279)
    ///
    /// A dotted key reaches this function *because* the classification loop skipped it: it
    /// `continue`s on any key containing a `.` before every check that could refuse one. Until
    /// now nothing downstream checked either — no relationship name, no field name — so
    /// `?nonsense.field=x` became a `params.embedding_filters` entry as quietly as
    /// `?author.name=x`, and `execute_embeddings`, which reads the map by relationship, never
    /// looked at it.
    ///
    /// The comparison that makes that a defect rather than a design choice is the undotted
    /// spelling. `?nonsense=x` is refused by name, and `Prefer: handling=lenient` exists to
    /// grant exactly that exemption deliberately — refusing by default, ignoring on request,
    /// and logging when it ignores, because "a dropped parameter the caller believed was a
    /// filter is exactly the kind of quiet widening this program exists to remove". A dot bought
    /// the lenient behaviour without the opt-in and without the log line.
    ///
    /// So this is not a new rule about dotted keys. It is the existing unknown-parameter rule,
    /// reaching the one spelling that had been skipping it.
    ///
    /// # What it does not decide
    ///
    /// A relationship the type *does* declare, filtered without being selected, is still
    /// accepted here and still dropped by the consumer — that is #1285. The refusal it needs
    /// depends on the representation (an export cannot be told to add the embed to `?select=`;
    /// #1268 refuses that too), and this function does not know which one is being served.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Validation` naming the relationship and the ones the type has.
    fn embedding_relationship_is_known(
        &self,
        key: &str,
        rel_name: &str,
    ) -> Result<bool, FraiseQLError> {
        // Without a type definition there is nothing to check against, and `is_valid_field`
        // takes the same permissive view for the undotted spelling.
        if self.type_def.is_none() {
            return Ok(true);
        }

        match self.validate_embedding_relationship_name(rel_name) {
            Ok(()) => Ok(true),
            Err(e) if self.lenient => {
                tracing::debug!(
                    parameter = %key,
                    relationship = %rel_name,
                    "ignoring REST query parameter naming an unknown relationship \
                     (Prefer: handling=lenient)"
                );
                let _ = e;
                Ok(false)
            },
            Err(e) => Err(e),
        }
    }

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
                    if !self.embedding_relationship_is_known(key, rel_name)? {
                        continue;
                    }
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
                    if !self.embedding_relationship_is_known(key, rel_name)? {
                        continue;
                    }
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
            let coerced = self.coerce_bracket_operand(&field, &op, &value)?;
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

    /// Record the pagination parameters the client sent, parsing each exactly once.
    ///
    /// The one place each of these six query parameters is read. `requested_total_limit`
    /// used to be a second reader of `?limit=`, searching the raw pairs — first-wins —
    /// while the classification above assigns — last-wins. `?limit=5&limit=9` therefore
    /// capped an export at 5 rows while every other consumer of the same request read 9:
    /// #1274's shape (one parameter, two parsers, opposite resolutions) reached through a
    /// different parameter.
    ///
    /// Nothing here is defaulted or clamped; that is [`Self::resolve_pagination`]'s job.
    ///
    /// # Errors
    ///
    /// Returns a validation error naming the parameter whose value is not a number. Only
    /// one pagination family can be present — the cross-pagination guard in
    /// [`Self::extract`] runs first — so the order in which they are parsed is the order
    /// within a family, unchanged from when each family had its own function.
    fn parse_requested_pagination(
        &self,
        limit_raw: Option<&str>,
        offset_raw: Option<&str>,
        first_raw: Option<&str>,
        after_raw: Option<&str>,
        last_raw: Option<&str>,
        before_raw: Option<&str>,
    ) -> Result<RequestedPagination, FraiseQLError> {
        Ok(RequestedPagination {
            limit:  parse_page_number(limit_raw, "limit", "a positive integer")?,
            offset: parse_page_number(offset_raw, "offset", "a non-negative integer")?,
            first:  parse_page_number(first_raw, "first", "a positive integer")?,
            after:  after_raw.map(String::from),
            last:   parse_page_number(last_raw, "last", "a positive integer")?,
            before: before_raw.map(String::from),
        })
    }

    /// Resolve what the client asked for into the pagination the server will apply.
    ///
    /// The only place `default_page_size` and `max_page_size` enter a request. A value
    /// that has passed through here carries a server policy and can no longer answer
    /// "did the client ask for this?" — which is why [`ExtractedParams`] keeps the input
    /// alongside the output rather than only this.
    ///
    /// The route's shape picks the family: a relay route with no cursor parameter still
    /// resolves to `Cursor`, holding the default page it will serve.
    fn resolve_pagination(
        &self,
        requested: &RequestedPagination,
        is_relay: bool,
    ) -> PaginationParams {
        if is_relay {
            let names_a_cursor =
                requested.after.is_some() || requested.last.is_some() || requested.before.is_some();
            PaginationParams::Cursor {
                first:  match requested.first {
                    Some(v) => Some(v.min(self.config.max_page_size)),
                    // No cursor parameter at all — the route's default page.
                    None if !names_a_cursor => Some(self.config.default_page_size),
                    None => None,
                },
                after:  requested.after.clone(),
                last:   requested.last,
                before: requested.before.clone(),
            }
        } else {
            PaginationParams::Offset {
                limit:  requested
                    .limit
                    .map_or(self.config.default_page_size, |v| v.min(self.config.max_page_size)),
                offset: requested.offset.unwrap_or(0),
            }
        }
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

    /// Coerce a bracket operand to the type the *operator* takes.
    ///
    /// A null check asks a question about the field, not a value of it:
    /// `?deletedAt[is_null]=true` on a `DateTime` field must produce the boolean
    /// `true`, not the string `"true"` coerced to a `DateTime`. Coercing to the
    /// field's declared type is how #828's null checks ended up inverted —
    /// a non-boolean operand read as "assume IS NULL".
    fn coerce_bracket_operand(
        &self,
        field_name: &str,
        operator: &str,
        raw: &str,
    ) -> Result<serde_json::Value, FraiseQLError> {
        if matches!(operator, "is_null" | "isnull" | "is_not_null" | "isnotnull") {
            return match raw {
                "true" | "1" => Ok(serde_json::Value::Bool(true)),
                "false" | "0" => Ok(serde_json::Value::Bool(false)),
                other => Err(validation_error(format!(
                    "Operator '{operator}' on '{field_name}' takes a boolean \
                     (true/false), got '{other}'"
                ))),
            };
        }
        self.coerce_field_value(field_name, raw)
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
