//! Schema field security metadata flattening.
//!
//! Walks the compiled schema and collects non-default security annotations
//! (field encryption, scope requirements, type-level role guards) into a flat
//! `BTreeMap` keyed by `"TypeName"` (type-level) or `"TypeName.fieldName"`
//! (field-level).

use std::collections::BTreeMap;

use fraiseql_core::schema::{CompiledSchema, FieldDenyPolicy};
use serde::Serialize;

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
                    encrypted:      None,
                    requires_scope: None,
                    on_deny:        None,
                    requires_role:  Some(role.clone()),
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions

    use fraiseql_core::schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldEncryptionConfig, FieldType,
        TypeDefinition,
    };

    use super::*;

    // ── Test fixture ─────────────────────────────────────────────────────────

    fn make_schema() -> CompiledSchema {
        // User type:
        //   email — encrypted
        //   ssn   — requires_scope "read:pii", on_deny Mask
        //   id    — no annotations (default, must be omitted)
        let email_field = FieldDefinition::new("email", FieldType::String).with_encryption(
            FieldEncryptionConfig {
                key_reference: "keys/email".to_string(),
                algorithm:     "AES-256-GCM".to_string(),
            },
        );

        let ssn_field = FieldDefinition::new("ssn", FieldType::String)
            .with_requires_scope("read:pii")
            .with_on_deny(FieldDenyPolicy::Mask);

        let id_field = FieldDefinition::new("id", FieldType::String);

        let mut user_type = TypeDefinition::new("User", "v_user");
        user_type.fields = vec![email_field, ssn_field, id_field];

        // AdminDashboard type — type-level requires_role "admin", no fields
        let mut admin_type = TypeDefinition::new("AdminDashboard", "v_admin_dashboard");
        admin_type.requires_role = Some("admin".to_string());

        // PlainType — no annotations at any level (must be omitted entirely)
        let mut plain_type = TypeDefinition::new("PlainType", "v_plain");
        plain_type.fields = vec![FieldDefinition::new("name", FieldType::String)];

        CompiledSchema {
            types: vec![user_type, admin_type, plain_type],
            ..CompiledSchema::default()
        }
    }

    // ── RED: individual assertion tests ──────────────────────────────────────

    #[test]
    fn encrypted_field_is_present() {
        let map = flatten_field_metadata(&make_schema());
        let entry = map.get("User.email").unwrap();
        assert_eq!(entry.encrypted, Some(true));
        assert!(entry.requires_scope.is_none());
        assert!(entry.on_deny.is_none());
        assert!(entry.requires_role.is_none());
    }

    #[test]
    fn scoped_field_with_mask_is_present() {
        let map = flatten_field_metadata(&make_schema());
        let entry = map.get("User.ssn").unwrap();
        assert_eq!(entry.requires_scope.as_deref(), Some("read:pii"));
        assert_eq!(entry.on_deny.as_deref(), Some("mask"));
        assert!(entry.encrypted.is_none());
        assert!(entry.requires_role.is_none());
    }

    #[test]
    fn type_level_requires_role_is_present() {
        let map = flatten_field_metadata(&make_schema());
        let entry = map.get("AdminDashboard").unwrap();
        assert_eq!(entry.requires_role.as_deref(), Some("admin"));
        assert!(entry.encrypted.is_none());
        assert!(entry.requires_scope.is_none());
        assert!(entry.on_deny.is_none());
    }

    #[test]
    fn default_annotated_field_is_omitted() {
        let map = flatten_field_metadata(&make_schema());
        assert!(!map.contains_key("User.id"), "User.id has no annotations");
    }

    #[test]
    fn plain_type_fields_are_omitted() {
        let map = flatten_field_metadata(&make_schema());
        assert!(!map.contains_key("PlainType.name"));
        assert!(!map.contains_key("PlainType"));
    }

    #[test]
    fn exact_entry_count_is_three() {
        let map = flatten_field_metadata(&make_schema());
        // Expected: User.email, User.ssn, AdminDashboard
        assert_eq!(map.len(), 3, "unexpected keys: {map:?}");
    }

    #[test]
    fn empty_schema_produces_empty_map() {
        let map = flatten_field_metadata(&CompiledSchema::default());
        assert!(map.is_empty());
    }

    // ── Serde output matches exact JSON spec ──────────────────────────────────

    #[test]
    fn email_serialises_to_encrypted_only() {
        let map = flatten_field_metadata(&make_schema());
        let json = serde_json::to_value(map.get("User.email").unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({"encrypted": true}));
    }

    #[test]
    fn ssn_serialises_to_scope_and_deny() {
        let map = flatten_field_metadata(&make_schema());
        let json = serde_json::to_value(map.get("User.ssn").unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"requires_scope": "read:pii", "on_deny": "mask"})
        );
    }

    #[test]
    fn admin_dashboard_serialises_to_role_only() {
        let map = flatten_field_metadata(&make_schema());
        let json = serde_json::to_value(map.get("AdminDashboard").unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({"requires_role": "admin"}));
    }

    // ── Default reject policy is NOT serialised ───────────────────────────────

    #[test]
    fn reject_on_deny_does_not_appear_in_output() {
        let mut field =
            FieldDefinition::new("salary", FieldType::String).with_requires_scope("read:payroll");
        field.on_deny = FieldDenyPolicy::Reject; // explicit default

        let mut type_def = TypeDefinition::new("Employee", "v_employee");
        type_def.fields = vec![field];

        let schema = CompiledSchema {
            types: vec![type_def],
            ..CompiledSchema::default()
        };

        let map = flatten_field_metadata(&schema);
        let entry = map.get("Employee.salary").unwrap();
        assert!(entry.on_deny.is_none(), "Reject is the default — must not appear");
        let json = serde_json::to_value(entry).unwrap();
        assert!(!json.as_object().unwrap().contains_key("on_deny"));
    }

    // ── FieldSecurityMetadata::is_empty ──────────────────────────────────────

    #[test]
    fn is_empty_true_when_all_none() {
        let meta = FieldSecurityMetadata {
            encrypted:      None,
            requires_scope: None,
            on_deny:        None,
            requires_role:  None,
        };
        assert!(meta.is_empty());
    }

    #[test]
    fn is_empty_false_when_any_some() {
        let meta = FieldSecurityMetadata {
            encrypted:      Some(true),
            requires_scope: None,
            on_deny:        None,
            requires_role:  None,
        };
        assert!(!meta.is_empty());
    }
}
