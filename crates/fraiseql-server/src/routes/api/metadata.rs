//! Schema field security metadata flattening and HTTP handler.
//!
//! Walks the compiled schema and collects non-default security annotations
//! (field encryption, scope requirements, type-level role guards) into a flat
//! `BTreeMap` keyed by `"TypeName"` (type-level) or `"TypeName.fieldName"`
//! (field-level).
//!
//! The `metadata_handler` exposes this map at `GET /api/v1/schema/metadata`.

use std::collections::BTreeMap;

use axum::{Json, extract::State};
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    schema::{CompiledSchema, FieldDenyPolicy},
};
use serde::Serialize;

use crate::routes::{api::types::ApiResponse, graphql::AppState};

/// Security metadata for a single field or type in the compiled schema.
///
/// Only non-default annotations are populated; all fields are `Option` and
/// serialise with `skip_serializing_if = "Option::is_none"` so that the JSON
/// output is minimal.
///
/// The struct is `#[non_exhaustive]` to allow adding new annotation kinds in
/// future releases without breaking downstream pattern matches.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldSecurityMetadata {
    /// `true` when this field is encrypted at rest.
    ///
    /// Only present (and always `true`) for fields that carry an
    /// `FieldEncryptionConfig`.  Absent when the field is not encrypted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted: Option<bool>,

    /// OAuth 2.0 scope required to read this field.
    ///
    /// When `None`, the field is visible to any authenticated user (subject to
    /// type-level `requires_role`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_scope: Option<String>,

    /// Policy applied when the required scope is absent.
    ///
    /// Possible values: `"reject"`, `"mask"`.
    /// Only serialised when the policy is non-default (i.e., not `"reject"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_deny: Option<String>,

    /// Role required to access this type (type-level guard).
    ///
    /// Only present on type-keyed entries (keys without a `.` separator).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_role: Option<String>,
}

impl FieldSecurityMetadata {
    /// Returns `true` if all annotations are absent (entry should be omitted).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.encrypted.is_none()
            && self.requires_scope.is_none()
            && self.on_deny.is_none()
            && self.requires_role.is_none()
    }
}

/// Response body for `GET /api/v1/schema/metadata`.
#[non_exhaustive]
#[derive(Debug, Serialize)]
pub struct MetadataResponse {
    /// Flat map of security annotations, keyed by `"TypeName"` or `"TypeName.fieldName"`.
    pub metadata: BTreeMap<String, FieldSecurityMetadata>,
}

/// Return field-level security metadata for the compiled schema.
///
/// Walks all types and fields, collecting non-default security annotations
/// (encryption, scope requirements, deny policies, role guards) into a flat
/// map and returns it as a JSON object.
///
/// The handler is infallible — the schema is always present in `AppState`.
pub async fn metadata_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Json<ApiResponse<MetadataResponse>> {
    let metadata = flatten_field_metadata(state.executor().schema());
    Json(ApiResponse {
        status: "success".to_string(),
        data: MetadataResponse { metadata },
    })
}

/// Flatten all non-default security annotations from a compiled schema into a map.
///
/// Keys use the following format:
/// - `"TypeName"` — type-level annotations (`requires_role`)
/// - `"TypeName.fieldName"` — field-level annotations (`encrypted`, `requires_scope`, `on_deny`)
///
/// Types and fields with all-default annotations are omitted from the result.
/// The map is ordered (`BTreeMap`) so that the output is deterministic.
#[must_use]
pub fn flatten_field_metadata(schema: &CompiledSchema) -> BTreeMap<String, FieldSecurityMetadata> {
    let mut map = BTreeMap::new();

    for type_def in &schema.types {
        let type_name = type_def.name.as_str();

        // ── Type-level: requires_role ─────────────────────────────────────────
        if let Some(role) = &type_def.requires_role {
            map.insert(
                type_name.to_string(),
                FieldSecurityMetadata {
                    encrypted: None,
                    requires_scope: None,
                    on_deny: None,
                    requires_role: Some(role.clone()),
                },
            );
        }

        // ── Field-level annotations ───────────────────────────────────────────
        for field in &type_def.fields {
            let encrypted = field.encryption.as_ref().map(|_| true);
            let requires_scope = field.requires_scope.clone();
            let on_deny = match field.on_deny {
                FieldDenyPolicy::Mask => Some("mask".to_string()),
                // FieldDenyPolicy is #[non_exhaustive]; all other variants (including
                // the default Reject) produce no output.
                _ => None,
            };

            let meta = FieldSecurityMetadata {
                encrypted,
                requires_scope,
                on_deny,
                requires_role: None,
            };

            if !meta.is_empty() {
                map.insert(format!("{type_name}.{}", field.name), meta);
            }
        }
    }

    map
}
