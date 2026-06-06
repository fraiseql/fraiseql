//! Tests for tenant-key validation (#333).
//!
//! The `X-Tenant-ID` header validator must agree with the schema-mode DDL
//! helpers on both the accepted alphabet (`[a-zA-Z0-9_]`) and the length cap, so
//! a key accepted at dispatch time is also usable for schema-mode provisioning.

use super::{MAX_TENANT_KEY_LEN, validate_tenant_key};
use crate::tenancy::schema_isolation::tenant_schema_name;

#[test]
fn accepts_alphanumeric_underscore() {
    assert!(validate_tenant_key("acme_corp").is_ok());
    assert!(validate_tenant_key("Tenant123").is_ok());
}

#[test]
fn rejects_hyphen() {
    // Hyphens were accepted before #333 but break schema-mode (PG identifiers
    // cannot contain '-'), so the validators silently disagreed.
    assert!(validate_tenant_key("acme-corp").is_err());
}

#[test]
fn rejects_other_punctuation_and_spaces() {
    assert!(validate_tenant_key("acme.corp").is_err());
    assert!(validate_tenant_key("acme corp").is_err());
    assert!(validate_tenant_key("acme/corp").is_err());
}

#[test]
fn length_cap_is_56() {
    assert_eq!(MAX_TENANT_KEY_LEN, 56, "63 minus len(\"tenant_\")");
    assert!(validate_tenant_key(&"a".repeat(56)).is_ok());
    assert!(validate_tenant_key(&"a".repeat(57)).is_err());
}

#[test]
fn header_validator_agrees_with_schema_mode() {
    // Every key the header validator accepts, schema-mode provisioning must also
    // accept — and vice versa — across the alphabet and length boundary (#333).
    let long_ok = "x".repeat(MAX_TENANT_KEY_LEN);
    let long_bad = "x".repeat(MAX_TENANT_KEY_LEN + 1);

    let accepted = ["acme_corp", "a", "Tenant_42", long_ok.as_str()];
    for key in accepted {
        assert!(validate_tenant_key(key).is_ok(), "header should accept {key:?}");
        assert!(tenant_schema_name(key).is_ok(), "schema-mode should accept {key:?}");
    }

    let rejected = ["acme-corp", "acme.corp", long_bad.as_str()];
    for key in rejected {
        assert!(validate_tenant_key(key).is_err(), "header should reject {key:?}");
        assert!(tenant_schema_name(key).is_err(), "schema-mode should reject {key:?}");
    }
}
