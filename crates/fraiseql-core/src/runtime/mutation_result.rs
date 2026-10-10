//! Mutation response parser for `app.mutation_response` composite rows.
//!
//! Parses a typed, column-per-concern row into [`MutationOutcome`], which the
//! executor uses to build the GraphQL response. The row shape maps 1:1 to the
//! `app.mutation_response` PostgreSQL composite type — see
//! `docs/architecture/mutation-response.md` for the DDL and semantics table.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::cascade::MutationErrorClass;
use crate::error::{FraiseQLError, Result};

/// Minimum legal HTTP status code (informational range start).
const HTTP_STATUS_MIN: i16 = 100;
/// Maximum legal HTTP status code (end of 5xx range).
const HTTP_STATUS_MAX: i16 = 599;

/// Whether the server checks a failed mutation's `error_detail.errors[]` (#1425).
///
/// Clients translate a failure by `errors[].identifier`, so a failure that carries no
/// `errors` array, or an identifier that is not a translation key (`^[a-z][a-z0-9_]*$`),
/// reaches a person untranslated. With [`Warn`](Self::Warn) each such response is logged
/// at `warn` and counted in [`mutation_error_shape_violations`]; the response itself is
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum MutationErrorShapeCheck {
    /// No check (the default).
    #[default]
    Off,
    /// Log and count a malformed failure response.
    Warn,
}

/// What a mutation's typed error says about the constraint its function violated (#1531).
///
/// A class-23 SQLSTATE served as the mutation's error member (#1424) carries one
/// `errors[]` entry, the same shape a function's own `mutation_err_entries` gives:
/// `identifier` is the constraint's name (a unique index's for a partial unique index),
/// `code` its HTTP status, and `details.sqlstate` the SQLSTATE. Never the database's
/// `DETAIL`, never a row value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ConstraintMetadata {
    /// The constraint's name and the SQLSTATE (the default): a client needs the name to
    /// act on the error.
    #[default]
    Identifier,
    /// Also `details.table` and `details.columns`, the constraint's columns resolved from
    /// the catalog (none for an expression index: never guessed).
    Full,
    /// No entry: the typed error carries its status and generic message only.
    None,
}

/// Failed mutation responses whose `errors[]` was malformed, since startup.
static ERROR_SHAPE_VIOLATIONS: AtomicU64 = AtomicU64::new(0);

/// How many failed mutation responses [`MutationErrorShapeCheck::Warn`] found malformed
/// since the process started (#1425). Exported by the server as
/// `fraiseql_mutation_error_shape_violations_total`.
#[must_use]
pub fn mutation_error_shape_violations() -> u64 {
    ERROR_SHAPE_VIOLATIONS.load(Ordering::Relaxed)
}

/// Check a failed outcome's `errors[]` when `check` asks for it: log at `warn` and count
/// a malformed one. A success, or `check` off, does nothing.
pub(crate) fn check_error_shape(
    check: MutationErrorShapeCheck,
    outcome: &MutationOutcome,
    mutation: &str,
    function: &str,
) {
    if check == MutationErrorShapeCheck::Off {
        return;
    }
    let MutationOutcome::Error { metadata, .. } = outcome else {
        return;
    };
    let problems = error_shape_problems(metadata);
    if problems.is_empty() {
        return;
    }
    ERROR_SHAPE_VIOLATIONS.fetch_add(1, Ordering::Relaxed);
    tracing::warn!(
        mutation = %mutation,
        function = %function,
        problems = %problems.join("; "),
        "mutation failure without a translatable errors[] entry"
    );
}

/// What is wrong with a failure's `error_detail` as the carrier of `errors[]` entries.
///
/// Empty when it carries a non-empty `errors` array whose every `identifier` is a
/// translation key (`^[a-z][a-z0-9_]*$`).
#[must_use]
pub fn error_shape_problems(error_detail: &JsonValue) -> Vec<String> {
    let Some(entries) = error_detail.get("errors").and_then(JsonValue::as_array) else {
        return vec!["no `errors` array in error_detail".to_string()];
    };
    if entries.is_empty() {
        return vec!["an empty `errors` array in error_detail".to_string()];
    }
    entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| match entry.get("identifier").and_then(JsonValue::as_str) {
            None => Some(format!("errors[{i}] has no string `identifier`")),
            Some(id) if !is_translation_key(id) => Some(format!(
                "errors[{i}].identifier '{id}' is not a translation key (^[a-z][a-z0-9_]*$)"
            )),
            Some(_) => None,
        })
        .collect()
}

/// `^[a-z][a-z0-9_]*$`: the shape `fraiseql.error_entry` normalises an identifier into.
fn is_translation_key(identifier: &str) -> bool {
    let mut chars = identifier.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Outcome of parsing a single `mutation_response` row.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum MutationOutcome {
    /// The mutation succeeded; the result entity is available.
    Success {
        /// The entity JSONB returned by the function.
        entity:         JsonValue,
        /// GraphQL type name for the entity (from the `entity_type` column).
        entity_type:    Option<String>,
        /// UUID string of the mutated entity (from the `entity_id` column).
        ///
        /// Present for UPDATE and DELETE mutations. Used for entity-aware cache
        /// invalidation: only cache entries containing this UUID are evicted,
        /// leaving unrelated entries warm.
        entity_id:      Option<String>,
        /// Cascade operations associated with this mutation.
        cascade:        Option<JsonValue>,
        /// GraphQL field names changed by this mutation (from the `updated_fields`
        /// column; empty on noop). Surfaced selection-gated as `updatedFields` on
        /// the success arm, symmetric with `cascade` (#433).
        updated_fields: Vec<String>,
        /// The declared success fields the function returned, by name (#1397): the
        /// `result` column. `None` when the row has no such column, `Some(Value::Null)` when
        /// it is SQL `NULL`.
        result:         Option<JsonValue>,
    },
    /// The mutation failed; error metadata is available.
    Error {
        /// Typed classification of the failure (mirrors `app.mutation_error_class`).
        error_class: MutationErrorClass,
        /// Human-readable error message.
        message:     String,
        /// Suggested HTTP status code, when the composite supplied one.
        http_status: Option<i16>,
        /// Concrete GraphQL type name for the failure (from the `entity_type`
        /// column). On the error path a function stamps the declared error type it
        /// produced (e.g. `"DuplicateEmailError"`), letting the executor route the
        /// result onto the right `is_error` union member — symmetric with the
        /// success arm's use of `entity_type` (#465).
        entity_type: Option<String>,
        /// Structured metadata JSONB containing error-type field values.
        metadata:    JsonValue,
    },
}

/// Typed `app.mutation_response` row.
///
/// Field types map 1:1 to the PostgreSQL composite columns. See
/// `docs/architecture/mutation-response.md`.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct MutationResponse {
    /// Terminal outcome. `true` means the operation completed (including noops).
    pub succeeded:      bool,
    /// Did the database actually change? Independent of `succeeded`.
    pub state_changed:  bool,
    /// `NULL` iff `succeeded`. Drives the cascade error code 1:1.
    #[serde(default)]
    pub error_class:    Option<MutationErrorClass>,
    /// Human-readable subtype (e.g. `"duplicate_email"`); not parsed.
    #[serde(default)]
    pub status_detail:  Option<String>,
    /// HTTP status, first-class. Validated to 100..=599 on ingest.
    #[serde(default)]
    pub http_status:    Option<i16>,
    /// Human-readable summary safe to show to end users.
    #[serde(default)]
    pub message:        Option<String>,
    /// Primary key of the affected entity. Present for updates/deletes.
    #[serde(default)]
    pub entity_id:      Option<Uuid>,
    /// GraphQL type name (e.g. `"User"`). Used for cache invalidation.
    #[serde(default)]
    pub entity_type:    Option<String>,
    /// Full entity payload. Populated even for noops.
    #[serde(default)]
    pub entity:         JsonValue,
    /// GraphQL field names that changed. Empty on noop. A SQL-`NULL` column
    /// (rendered by `row_to_map` as JSON `null`) is read as the empty list — see
    /// `null_as_empty_string_vec`.
    #[serde(default, deserialize_with = "null_as_empty_string_vec")]
    pub updated_fields: Vec<String>,
    /// Cascade operations (see the graphql-cascade specification).
    #[serde(default)]
    pub cascade:        JsonValue,
    /// Structured error payload only (field / constraint / severity).
    #[serde(default)]
    pub error_detail:   JsonValue,
    /// Observability only (trace IDs, timings, audit extras).
    #[serde(default)]
    pub metadata:       JsonValue,
    /// The mutation's declared success fields, by stored name (#1397). Optional: only a
    /// mutation that declares success fields reads it. `None` when the row has no `result`
    /// column, `Some(Value::Null)` when it is SQL `NULL`, so the two stay apart.
    #[serde(default, deserialize_with = "present_column")]
    pub result:         Option<JsonValue>,
}

/// Deserialize a column that is present, `null` included, as `Some`: with `#[serde(default)]`
/// only an absent column is `None`.
fn present_column<'de, D>(deserializer: D) -> std::result::Result<Option<JsonValue>, D::Error>
where
    D: Deserializer<'de>,
{
    JsonValue::deserialize(deserializer).map(Some)
}

/// Deserialize a possibly-`null` `TEXT[]` column as an empty `Vec`.
///
/// A failed mutation's function commonly leaves `updated_fields` unset (SQL NULL),
/// which `row_to_map` renders as JSON `null`. Serde's `#[serde(default)]` only fills
/// an *absent* key, so an explicit null still reaches `Vec<String>`'s deserializer
/// and fails with `invalid type: null, expected a sequence` — turning every such
/// failure into an opaque parse error before the typed error arm is reached (#473).
/// Treating null as the empty list matches the absent-key behaviour.
fn null_as_empty_string_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
}

/// Parse a `mutation_response` row into a [`MutationOutcome`].
///
/// Deserializes typed columns directly — no string parsing. Rejects the illegal
/// combination `succeeded=false AND state_changed=true` (the builder refuses to
/// construct such a row; defense in depth here so a hand-written SQL path
/// cannot slip a partial-failure row past the parser).
///
/// `error_detail` (not `metadata`) feeds the executor's error-field projection
/// so downstream consumers remain untouched: `metadata` carries observability
/// only and must not be used as an error-data carrier.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if:
/// - the row fails to deserialize into [`MutationResponse`];
/// - `http_status` is outside `100..=599`;
/// - `succeeded=false` with `state_changed=true` (illegal per the semantics table);
/// - `succeeded=false` with `error_class` missing.
pub fn parse_mutation_row<S: ::std::hash::BuildHasher>(
    row: &HashMap<String, JsonValue, S>,
) -> Result<MutationOutcome> {
    let obj: serde_json::Map<String, JsonValue> =
        row.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let parsed: MutationResponse =
        serde_json::from_value(JsonValue::Object(obj)).map_err(|e| FraiseQLError::Validation {
            message: format!("mutation_response row failed to deserialize: {e}"),
            path:    None,
        })?;
    to_outcome(parsed)
}

/// Lower a deserialized [`MutationResponse`] to the shared outcome seam.
fn to_outcome(row: MutationResponse) -> Result<MutationOutcome> {
    if let Some(status) = row.http_status {
        if !(HTTP_STATUS_MIN..=HTTP_STATUS_MAX).contains(&status) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "mutation_response 'http_status' out of range: {status} \
                     (expected {HTTP_STATUS_MIN}..={HTTP_STATUS_MAX})"
                ),
                path:    None,
            });
        }
    }

    if row.succeeded {
        if row.error_class.is_some() {
            return Err(FraiseQLError::Validation {
                message: "mutation_response: succeeded=true but error_class is set".to_string(),
                path:    None,
            });
        }
        Ok(MutationOutcome::Success {
            entity:         row.entity,
            entity_type:    row.entity_type,
            entity_id:      row.entity_id.map(|u| u.to_string()),
            cascade:        filter_null(row.cascade),
            updated_fields: row.updated_fields,
            result:         row.result,
        })
    } else {
        if row.state_changed {
            return Err(FraiseQLError::Validation {
                message: "mutation_response: succeeded=false with state_changed=true is illegal \
                          (partial-failure rows are builder-rejected)"
                    .to_string(),
                path:    None,
            });
        }
        let Some(class) = row.error_class else {
            return Err(FraiseQLError::Validation {
                message: "mutation_response: succeeded=false requires error_class".to_string(),
                path:    None,
            });
        };
        Ok(MutationOutcome::Error {
            error_class: class,
            message:     row.message.unwrap_or_default(),
            http_status: row.http_status,
            entity_type: row.entity_type,
            metadata:    row.error_detail,
        })
    }
}

fn filter_null(v: JsonValue) -> Option<JsonValue> {
    if v.is_null() { None } else { Some(v) }
}
