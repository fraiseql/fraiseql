//! The text a `SecurityContext`'s and an `AuditEntry`'s instants are serialized as, and
//! the instant text each reads back, case by case. A context crosses process boundaries as
//! JSON and an audit entry is exported as JSON, so these strings are what a reader sees.

#![allow(clippy::unwrap_used)] // Reason: test code, a failed (de)serialization is a test failure

use serde_json::{Value, json};

use super::SecurityContext;
use crate::security::audit::AuditEntry;

fn context(authenticated_at: &str) -> Value {
    json!({
        "user_id": "user-1",
        "roles": [],
        "tenant_id": null,
        "scopes": [],
        "attributes": {},
        "request_id": "request-1",
        "ip_address": null,
        "authenticated_at": authenticated_at,
        "expires_at": "2024-01-15T11:30:00Z",
        "issuer": null,
        "audience": null,
        "email": null,
        "display_name": null,
    })
}

/// The `authenticated_at` text a context read from `text` is written back as, or `None`
/// when the text is refused.
fn context_round_trip(text: &str) -> Option<String> {
    let parsed: SecurityContext = serde_json::from_value(context(text)).ok()?;
    let written = serde_json::to_value(&parsed).unwrap();
    Some(written["authenticated_at"].as_str().unwrap().to_owned())
}

fn audit_entry(timestamp: &str) -> Value {
    json!({
        "id": null,
        "timestamp": timestamp,
        "level": "INFO",
        "user_id": 1,
        "tenant_id": 1,
        "operation": "query",
        "query": "{ me { id } }",
        "variables": {},
        "ip_address": "127.0.0.1",
        "user_agent": "test",
        "error": null,
        "duration_ms": null,
        "previous_hash": null,
        "integrity_hash": null,
    })
}

#[test]
fn an_instant_is_written_in_utc_with_z_and_the_fraction_in_3_6_or_9_digits() {
    let cases = [
        ("2024-01-15T10:30:00Z", "2024-01-15T10:30:00Z"),
        ("2024-01-15T10:30:00.5Z", "2024-01-15T10:30:00.500Z"),
        ("2024-01-15T10:30:00.1234Z", "2024-01-15T10:30:00.123400Z"),
        ("2024-01-15T10:30:00.123456Z", "2024-01-15T10:30:00.123456Z"),
        ("2024-01-15T10:30:00.123456789Z", "2024-01-15T10:30:00.123456789Z"),
        ("1969-12-31T23:59:59.999999Z", "1969-12-31T23:59:59.999999Z"),
    ];
    for (read, written) in cases {
        assert_eq!(context_round_trip(read).as_deref(), Some(written), "{read:?}");
    }
}

#[test]
fn an_instant_with_an_offset_is_read_as_its_utc_instant() {
    let cases = [
        ("2024-01-15T12:30:00+02:00", "2024-01-15T10:30:00Z"),
        ("2024-01-15T05:30:00-05:00", "2024-01-15T10:30:00Z"),
        ("2024-01-15T10:30:00+00:00", "2024-01-15T10:30:00Z"),
        ("2024-01-15t10:30:00z", "2024-01-15T10:30:00Z"),
        ("2024-01-15 10:30:00Z", "2024-01-15T10:30:00Z"),
    ];
    for (read, written) in cases {
        assert_eq!(context_round_trip(read).as_deref(), Some(written), "{read:?}");
    }
}

#[test]
fn an_instant_without_an_offset_or_a_time_is_refused() {
    for read in [
        "2024-01-15T10:30:00",
        "2024-01-15",
        "2024-01-15T25:00:00Z",
        "",
        "now",
    ] {
        assert_eq!(context_round_trip(read), None, "{read:?}");
    }
}

#[test]
fn an_audit_entry_writes_its_timestamp_as_a_context_writes_its_instants() {
    let entry: AuditEntry =
        serde_json::from_value(audit_entry("2024-01-15T12:30:00.5+02:00")).unwrap();
    let written = serde_json::to_value(&entry).unwrap();
    assert_eq!(written["timestamp"], json!("2024-01-15T10:30:00.500Z"));
}

/// An audit entry's hash covers its timestamp's text, so every chain already written
/// verifies only while that text is byte-identical: RFC 3339 with `+00:00`, here
/// `2024-01-15T10:30:00.500+00:00`.
#[test]
fn an_audit_entrys_hash_covers_its_timestamp_as_rfc_3339_with_a_utc_offset() {
    let entry: AuditEntry = serde_json::from_value(audit_entry("2024-01-15T10:30:00.5Z")).unwrap();
    assert_eq!(
        entry.calculate_hash(),
        "de90f75d484b64f978a8d8827f18bf509ba825b32b8fe44fd144eb6377021f5a",
    );
}
