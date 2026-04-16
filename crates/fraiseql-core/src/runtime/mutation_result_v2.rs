//! v2 `app.mutation_response` parser.
//!
//! The v2 composite replaces v1's stringly-typed `status` prefix with typed,
//! orthogonal columns. See `docs/architecture/mutation-response.md` for the
//! DDL, the (`succeeded` × `state_changed` × `error_class`) semantics table,
//! and the 1:1 `MutationErrorClass` → `CascadeErrorCode` mapping.
//!
//! This module deserializes a v2 row into [`MutationResponseV2`] and projects
//! it onto the shared [`MutationOutcome`] seam used by the executor. The v1
//! parser in `mutation_result.rs` remains in place; [`super::mutation_result::parse_mutation_row`]
//! dispatches on `schema_version`.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::cascade::MutationErrorClass;
use super::mutation_result::MutationOutcome;
use crate::error::{FraiseQLError, Result};

/// Minimum legal HTTP status code (informational range start).
const HTTP_STATUS_MIN: i16 = 100;
/// Maximum legal HTTP status code (end of 5xx range).
const HTTP_STATUS_MAX: i16 = 599;

/// Typed, versioned `app.mutation_response` v2 row.
///
/// Field types map 1:1 to the PostgreSQL composite columns. See
/// `docs/architecture/mutation-response.md`.
#[derive(Debug, Clone, Deserialize)]
#[non_exhaustive]
pub struct MutationResponseV2 {
    /// Always `2` for this shape.
    pub schema_version: u16,
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

/// Parse a v2 `app.mutation_response` row into a [`MutationOutcome`].
///
/// Consumes typed columns directly — no string parsing. Rejects the illegal
/// combination `succeeded=false AND state_changed=true` (the builder refuses to
/// construct such a row; defense in depth here so a hand-written SQL path
/// cannot slip a partial-failure row past the parser).
///
/// `error_detail` (not `metadata`) feeds the executor's error-field projection
/// so downstream consumers remain untouched: `metadata` carries observability
/// only in v2 and must not be used as an error-data carrier.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if:
/// - the row fails to deserialize into [`MutationResponseV2`];
/// - `http_status` is outside `100..=599`;
/// - `succeeded=false` with `state_changed=true` (illegal per the semantics
///   table);
/// - `succeeded=false` with `error_class` missing.
pub fn parse_mutation_row_v2<S: ::std::hash::BuildHasher>(
    row: &HashMap<String, JsonValue, S>,
) -> Result<MutationOutcome> {
    let obj: serde_json::Map<String, JsonValue> =
        row.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let parsed: MutationResponseV2 = serde_json::from_value(JsonValue::Object(obj)).map_err(
        |e| FraiseQLError::Validation {
            message: format!("mutation_response v2 row failed to deserialize: {e}"),
            path:    None,
        },
    )?;
    outcome_from_v2(parsed)
}

/// Lower a deserialized [`MutationResponseV2`] to the shared outcome seam.
fn outcome_from_v2(row: MutationResponseV2) -> Result<MutationOutcome> {
    if let Some(status) = row.http_status {
        if !(HTTP_STATUS_MIN..=HTTP_STATUS_MAX).contains(&status) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "mutation_response v2 'http_status' out of range: {status} (expected {HTTP_STATUS_MIN}..={HTTP_STATUS_MAX})"
                ),
                path:    None,
            });
        }
    }

    if row.succeeded {
        // error_class must be absent on success; tolerate None/Some(_) asymmetry by rejecting Some.
        if row.error_class.is_some() {
            return Err(FraiseQLError::Validation {
                message: "mutation_response v2: succeeded=true but error_class is set".to_string(),
                path:    None,
            });
        }
        Ok(MutationOutcome::Success {
            entity:      row.entity,
            entity_type: row.entity_type,
            entity_id:   row.entity_id.map(|u| u.to_string()),
            cascade:     filter_null(row.cascade),
        })
    } else {
        if row.state_changed {
            return Err(FraiseQLError::Validation {
                message: "mutation_response v2: succeeded=false with state_changed=true is illegal \
                          (partial-failure rows are builder-rejected)"
                    .to_string(),
                path:    None,
            });
        }
        let Some(class) = row.error_class else {
            return Err(FraiseQLError::Validation {
                message: "mutation_response v2: succeeded=false requires error_class".to_string(),
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
        fn v2(succeeded: bool, state_changed: bool) -> Self {
            let mut r = Self::default();
            r.0.insert("schema_version".into(), json!(2));
            r.0.insert("succeeded".into(), json!(succeeded));
            r.0.insert("state_changed".into(), json!(state_changed));
            r
        }

        fn with(mut self, key: &str, value: JsonValue) -> Self {
            self.0.insert(key.into(), value);
            self
        }

        fn parse(&self) -> Result<MutationOutcome> {
            parse_mutation_row_v2(&self.0)
        }
    }

    #[test]
    fn deserializes_all_v2_columns() {
        let eid = "550e8400-e29b-41d4-a716-446655440000";
        let mut row = HashMap::new();
        row.insert("schema_version".to_string(), json!(2));
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

        let obj: serde_json::Map<String, JsonValue> =
            row.into_iter().collect();
        let parsed: MutationResponseV2 =
            serde_json::from_value(JsonValue::Object(obj)).unwrap();

        assert_eq!(parsed.schema_version, 2);
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
        let parsed: MutationResponseV2 = serde_json::from_value(json!({
            "schema_version": 2,
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

    // ---- Semantics table ----------------------------------------------------

    #[test]
    fn semantics_success_state_changed_true() {
        // (true, true, NULL): create / update / delete applied.
        let entity = json!({"id": "x"});
        let outcome = Row::v2(true, true)
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
        // (true, false, NULL): noop — entity still populated.
        let entity = json!({"id": "x", "name": "current"});
        let outcome = Row::v2(true, false).with("entity", entity.clone()).parse().unwrap();
        match outcome {
            MutationOutcome::Success { entity: e, .. } => assert_eq!(e, entity),
            MutationOutcome::Error { .. } => panic!("expected Success (noop)"),
        }
    }

    #[test]
    fn semantics_error_routes_to_error_outcome() {
        // (false, false, Conflict): error — conflict class maps to v1-compat prefix.
        let outcome = Row::v2(false, false)
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
        // (false, true, non-null): illegal.
        let err = Row::v2(false, true)
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
        let err = Row::v2(false, false)
            .parse()
            .expect_err("error row without error_class must be rejected");
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[test]
    fn success_rejects_error_class() {
        let err = Row::v2(true, true)
            .with("error_class", json!("validation"))
            .parse()
            .expect_err("succeeded=true with error_class must be rejected");
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[test]
    fn http_status_range_enforced() {
        let err = Row::v2(true, false)
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
            Row::v2(true, false)
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
        let outcome = Row::v2(true, true)
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
}
