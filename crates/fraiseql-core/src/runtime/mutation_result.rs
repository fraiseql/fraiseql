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

/// Deserializes an optional JSON array: both `null` and a missing key become the
/// type's `Default`. Used on `Vec<String>` fields that map to nullable SQL arrays.
fn null_as_empty_vec<'de, D>(d: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Vec<String>>::deserialize(d).map(|opt| opt.unwrap_or_default())
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
        /// GraphQL field names that changed. Empty on noop.
        updated_fields: Vec<String>,
    },
    /// The mutation failed; error metadata is available.
    Error {
        /// Typed classification of the failure (mirrors `app.mutation_error_class`).
        error_class: MutationErrorClass,
        /// Human-readable error message.
        message:     String,
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
    #[serde(default, deserialize_with = "null_as_empty_vec")]
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
            metadata:    row.error_detail,
        })
    }
}

fn filter_null(v: JsonValue) -> Option<JsonValue> {
    if v.is_null() { None } else { Some(v) }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    /// Terse builder for constructing row fixtures in tests.
    #[derive(Default)]
    struct Row(HashMap<String, JsonValue>);

    impl Row {
        fn new(succeeded: bool, state_changed: bool) -> Self {
            let mut r = Self::default();
            r.0.insert("succeeded".into(), json!(succeeded));
            r.0.insert("state_changed".into(), json!(state_changed));
            r
        }

        fn with(mut self, key: &str, value: JsonValue) -> Self {
            self.0.insert(key.into(), value);
            self
        }

        fn parse(&self) -> Result<MutationOutcome> {
            parse_mutation_row(&self.0)
        }
    }

    // ── Deserialization ────────────────────────────────────────────────────

    #[test]
    fn deserializes_all_columns() {
        let eid = "550e8400-e29b-41d4-a716-446655440000";
        let mut row = HashMap::new();
        row.insert("succeeded".to_string(), json!(false));
        row.insert("state_changed".to_string(), json!(false));
        row.insert("error_class".to_string(), json!("validation"));
        row.insert("status_detail".to_string(), json!("duplicate_email"));
        row.insert("http_status".to_string(), json!(422));
        row.insert("message".to_string(), json!("email already in use"));
        row.insert("entity_id".to_string(), json!(eid));
        row.insert("entity_type".to_string(), json!("User"));
        row.insert("entity".to_string(), json!({"id": eid}));
        row.insert("updated_fields".to_string(), json!(["email"]));
        row.insert("cascade".to_string(), json!({}));
        row.insert("error_detail".to_string(), json!({"field": "email"}));
        row.insert("metadata".to_string(), json!({"trace_id": "abc"}));

        let obj: serde_json::Map<String, JsonValue> = row.into_iter().collect();
        let parsed: MutationResponse = serde_json::from_value(JsonValue::Object(obj)).unwrap();

        assert!(!parsed.succeeded);
        assert!(!parsed.state_changed);
        assert_eq!(parsed.error_class, Some(MutationErrorClass::Validation));
        assert_eq!(parsed.status_detail.as_deref(), Some("duplicate_email"));
        assert_eq!(parsed.http_status, Some(422));
        assert_eq!(parsed.message.as_deref(), Some("email already in use"));
        assert_eq!(parsed.entity_id.map(|u| u.to_string()).as_deref(), Some(eid));
        assert_eq!(parsed.entity_type.as_deref(), Some("User"));
        assert_eq!(parsed.updated_fields, vec!["email".to_string()]);
        assert_eq!(parsed.error_detail["field"], "email");
        assert_eq!(parsed.metadata["trace_id"], "abc");
    }

    #[test]
    fn defaults_missing_jsonb_columns_to_null() {
        let parsed: MutationResponse = serde_json::from_value(json!({
            "succeeded": true,
            "state_changed": false,
        }))
        .unwrap();
        assert!(parsed.entity.is_null());
        assert!(parsed.cascade.is_null());
        assert!(parsed.error_detail.is_null());
        assert!(parsed.metadata.is_null());
        assert!(parsed.updated_fields.is_empty());
        assert!(parsed.entity_id.is_none());
    }

    // ── Semantics table ────────────────────────────────────────────────────

    #[test]
    fn semantics_success_state_changed_true() {
        let entity = json!({"id": "x"});
        let outcome = Row::new(true, true)
            .with("entity", entity.clone())
            .with("entity_type", json!("Machine"))
            .parse()
            .unwrap();
        match outcome {
            MutationOutcome::Success {
                entity: e,
                entity_type,
                entity_id,
                cascade,
                ..
            } => {
                assert_eq!(e, entity);
                assert_eq!(entity_type.as_deref(), Some("Machine"));
                assert!(entity_id.is_none());
                assert!(cascade.is_none());
            },
            MutationOutcome::Error { .. } => panic!("expected Success"),
        }
    }

    #[test]
    fn semantics_success_noop() {
        let entity = json!({"id": "x", "name": "current"});
        let outcome = Row::new(true, false).with("entity", entity.clone()).parse().unwrap();
        match outcome {
            MutationOutcome::Success { entity: e, .. } => assert_eq!(e, entity),
            MutationOutcome::Error { .. } => panic!("expected Success (noop)"),
        }
    }

    #[test]
    fn semantics_error_routes_to_error_outcome() {
        let outcome = Row::new(false, false)
            .with("error_class", json!("conflict"))
            .with("message", json!("duplicate"))
            .with("error_detail", json!({"field": "email"}))
            .with("metadata", json!({"trace_id": "zzz"}))
            .parse()
            .unwrap();
        match outcome {
            MutationOutcome::Error {
                error_class,
                message,
                metadata,
            } => {
                assert_eq!(error_class, MutationErrorClass::Conflict);
                assert_eq!(message, "duplicate");
                // error_detail (not metadata) feeds the error-field projection.
                assert_eq!(metadata, json!({"field": "email"}));
            },
            MutationOutcome::Success { .. } => panic!("expected Error"),
        }
    }

    #[test]
    fn semantics_illegal_partial_failure_rejected() {
        let err = Row::new(false, true)
            .with("error_class", json!("internal"))
            .parse()
            .expect_err("partial failure must be rejected");
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("state_changed=true is illegal"), "got: {message}");
            },
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn error_requires_error_class() {
        let err = Row::new(false, false)
            .parse()
            .expect_err("error row without error_class must be rejected");
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[test]
    fn success_rejects_error_class() {
        let err = Row::new(true, true)
            .with("error_class", json!("validation"))
            .parse()
            .expect_err("succeeded=true with error_class must be rejected");
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[test]
    fn http_status_range_enforced() {
        let err = Row::new(true, false)
            .with("http_status", json!(42))
            .parse()
            .expect_err("http_status out of range must be rejected");
        match err {
            FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("http_status"), "got: {message}");
            },
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn http_status_boundaries_accepted() {
        for code in [100_i16, 200, 422, 599] {
            Row::new(true, false)
                .with("http_status", json!(code))
                .parse()
                .unwrap_or_else(|e| panic!("code {code} should be accepted: {e:?}"));
        }
    }

    #[test]
    fn as_str_round_trips_all_error_classes() {
        let cases = [
            (MutationErrorClass::Validation, "validation"),
            (MutationErrorClass::Conflict, "conflict"),
            (MutationErrorClass::NotFound, "not_found"),
            (MutationErrorClass::Unauthorized, "unauthorized"),
            (MutationErrorClass::Forbidden, "forbidden"),
            (MutationErrorClass::Internal, "internal"),
            (MutationErrorClass::TransactionFailed, "transaction_failed"),
            (MutationErrorClass::Timeout, "timeout"),
            (MutationErrorClass::RateLimited, "rate_limited"),
            (MutationErrorClass::ServiceUnavailable, "service_unavailable"),
        ];
        for (class, expected) in cases {
            assert_eq!(class.as_str(), expected, "class = {class:?}");
        }
    }

    #[test]
    fn entity_id_uuid_serialized_back_to_canonical_string() {
        let eid = "550e8400-e29b-41d4-a716-446655440000";
        let outcome = Row::new(true, true)
            .with("entity_id", json!(eid))
            .with("entity", json!({"id": eid}))
            .parse()
            .unwrap();
        match outcome {
            MutationOutcome::Success { entity_id, .. } => {
                assert_eq!(entity_id.as_deref(), Some(eid));
            },
            MutationOutcome::Error { .. } => panic!("expected Success"),
        }
    }

    #[test]
    fn extra_columns_ignored() {
        // Rows may contain columns the parser doesn't know about (e.g. schema_version
        // from older DB functions). These must be silently ignored.
        let outcome = Row::new(true, true)
            .with("entity", json!({"id": "1"}))
            .with("schema_version", json!(2))
            .with("some_future_column", json!("whatever"))
            .parse()
            .unwrap();
        assert!(matches!(outcome, MutationOutcome::Success { .. }));
    }

    #[test]
    fn updated_fields_explicit_null_deserializes_as_empty() {
        // Reproduces the production error: row_to_map emits null for a SQL-NULL
        // TEXT[] column; serde's #[serde(default)] does NOT cover explicit null.
        let result: std::result::Result<MutationResponse, _> = serde_json::from_value(json!({
            "succeeded": true,
            "state_changed": false,
            "updated_fields": null,
        }));
        assert!(result.is_ok(), "expected Ok but got: {result:?}");
        assert!(result.unwrap().updated_fields.is_empty());
    }

    #[test]
    fn updated_fields_empty_array_deserializes_correctly() {
        let parsed: MutationResponse = serde_json::from_value(json!({
            "succeeded": true,
            "state_changed": false,
            "updated_fields": [],
        }))
        .unwrap();
        assert!(parsed.updated_fields.is_empty());
    }
}
