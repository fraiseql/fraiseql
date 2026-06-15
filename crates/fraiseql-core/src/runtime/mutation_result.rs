//! Mutation response parser for `app.mutation_response` composite rows.
//!
//! Parses a typed, column-per-concern row into [`MutationOutcome`], which the
//! executor uses to build the GraphQL response. The row shape maps 1:1 to the
//! `app.mutation_response` PostgreSQL composite type — see
//! `docs/architecture/mutation-response.md` for the DDL and semantics table.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::cascade::MutationErrorClass;
use crate::error::{FraiseQLError, Result};

/// Minimum legal HTTP status code (informational range start).
const HTTP_STATUS_MIN: i16 = 100;
/// Maximum legal HTTP status code (end of 5xx range).
const HTTP_STATUS_MAX: i16 = 599;

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
    },
    /// The mutation failed; error metadata is available.
    Error {
        /// Typed classification of the failure (mirrors `app.mutation_error_class`).
        error_class: MutationErrorClass,
        /// Human-readable error message.
        message:     String,
        /// Suggested HTTP status code, when the composite supplied one.
        http_status: Option<i16>,
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
    /// GraphQL field names that changed. Empty on noop.
    #[serde(default)]
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
            metadata:    row.error_detail,
        })
    }
}

fn filter_null(v: JsonValue) -> Option<JsonValue> {
    if v.is_null() { None } else { Some(v) }
}
