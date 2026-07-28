//! Tests for `fraiseql analyze`.
//!
//! The load-bearing one is `two_schemas_with_different_security_postures_report_differently`:
//! the previous implementation produced byte-identical output for a schema with every
//! security control enabled and one with all of them disabled, scoring both 100/100 (#818).

#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

use std::io::Write as _;

use super::*;

/// The category enum published by `--show-output-schema analyze`.
const CATEGORIES: &[&str] = &[
    "performance",
    "security",
    "federation",
    "complexity",
    "caching",
    "indexing",
];

/// Write `json` to a scratch file and analyse it.
fn analyse(name: &str, json: &str) -> serde_json::Value {
    let path = std::env::temp_dir().join(format!("fraiseql-analyze-{name}.json"));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(json.as_bytes()).unwrap();
    drop(f);

    let result = run(path.to_str().unwrap()).expect("analyze a valid compiled schema");
    let value = serde_json::to_value(&result).unwrap();
    std::fs::remove_file(&path).ok();
    value["data"].clone()
}

/// Build the fixture from the real types and serialize it, rather than
/// hand-writing JSON. A hand-written fixture encodes today's serde shape and stops
/// matching silently when a field is renamed; this one is a compile error instead.
fn compiled_schema(controls_enabled: Option<bool>) -> String {
    use fraiseql_core::schema::{SecurityConfig, TypeDefinition};

    let mut schema = CompiledSchema {
        types: vec![TypeDefinition::new("User", "v_user")],
        ..CompiledSchema::default()
    };

    if let Some(enabled) = controls_enabled {
        let mut security = SecurityConfig::default();
        for section in ["rate_limiting", "audit_logging", "error_sanitization"] {
            security
                .additional
                .insert(section.to_string(), serde_json::json!({ "enabled": enabled }));
        }
        schema.security = Some(security);
    }

    serde_json::to_string(&schema).unwrap()
}

#[test]
fn two_schemas_with_different_security_postures_report_differently() {
    // The defect in one assertion: the old implementation produced byte-identical
    // output for these two, and scored both 100/100 (#818).
    let secure = analyse("secure", &compiled_schema(Some(true)));
    let insecure = analyse("insecure", &compiled_schema(Some(false)));

    assert_ne!(
        secure["recommendations"], insecure["recommendations"],
        "a schema with security controls off must not report the same as one with them on"
    );
    assert!(
        secure["summary"]["health_score"].as_u64() > insecure["summary"]["health_score"].as_u64(),
        "the health score must fall when controls are disabled; it used to be pinned at 100 \
         for every possible input"
    );
}

#[test]
fn a_schema_with_controls_disabled_never_claims_they_are_active() {
    let data = analyse("claims", &compiled_schema(Some(false)));
    let text = serde_json::to_string(&data["recommendations"]).unwrap();

    for claim in [
        "Rate limiting configured and active",
        "Audit logging enabled for compliance",
    ] {
        assert!(
            !text.contains(claim),
            "the report asserts {claim:?} about a schema that disables it"
        );
    }
    assert!(text.contains("disabled"), "the report must say the controls are off: {text}");
}

#[test]
fn every_disabled_control_is_a_warning() {
    let data = analyse("warnings", &compiled_schema(Some(false)));
    assert!(
        data["summary"]["warnings"].as_u64().unwrap() >= 3,
        "three disabled controls must produce at least three warnings"
    );
}

#[test]
fn an_enabled_schema_scores_higher_than_one_that_configures_nothing() {
    let bare = analyse("bare", &compiled_schema(None));
    let secure = analyse("secure2", &compiled_schema(Some(true)));
    assert!(secure["summary"]["health_score"].as_u64() > bare["summary"]["health_score"].as_u64());
}

#[test]
fn the_report_matches_the_published_output_contract() {
    // `--show-output-schema analyze` publishes a `recommendations` array of
    // {category, severity, message, suggestion}. The command used to emit
    // `categories` instead, so the machine contract described output that did not
    // exist (#818).
    let data = analyse("contract", &compiled_schema(Some(true)));
    let recs = data["recommendations"].as_array().expect("recommendations array");
    assert!(!recs.is_empty());
    for rec in recs {
        for key in ["category", "severity", "message", "suggestion"] {
            assert!(rec.get(key).is_some(), "recommendation missing {key}: {rec}");
        }
        let category = rec["category"].as_str().unwrap();
        assert!(CATEGORIES.contains(&category), "{category:?} is outside the published enum");
    }
}

#[test]
fn an_empty_schema_does_not_score_full_marks() {
    // `{}` deserializes into an empty `CompiledSchema` — every field carries a
    // serde default — so it is analysable rather than refusable. What must not
    // survive is the verdict: it used to score 100/100 and affirm that rate
    // limiting, audit logging and error sanitization were all active (#818).
    let data = analyse("empty", "{}");
    assert!(
        data["summary"]["health_score"].as_u64().unwrap() < 100,
        "a schema declaring nothing at all cannot be in perfect health"
    );
    assert!(data["summary"]["warnings"].as_u64().unwrap() > 0);
}

#[test]
fn malformed_json_is_refused() {
    let path = std::env::temp_dir().join("fraiseql-analyze-malformed.json");
    std::fs::write(&path, b"{not json").unwrap();
    let result = run(path.to_str().unwrap());
    std::fs::remove_file(&path).ok();
    assert!(result.is_err());
}

/// Emit the checked-in integration fixture from the real types.
///
/// Ignored by default: it writes a file. Run with
/// `cargo test -p fraiseql-cli --lib emit_integration_fixture -- --ignored` after a
/// schema-shape change, so `tests/fixtures/analyze_compiled_schema.json` stays a
/// *real* compiled schema rather than a hand-written approximation of one (the
/// previous fixture had `type` where the type declares `field_type`, so it never
/// deserialized).
#[test]
#[ignore = "writes tests/fixtures/analyze_compiled_schema.json; run explicitly"]
fn emit_integration_fixture() {
    let json = compiled_schema(Some(true));
    let pretty: serde_json::Value = serde_json::from_str(&json).unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/analyze_compiled_schema.json");
    std::fs::write(path, serde_json::to_string_pretty(&pretty).unwrap() + "\n").unwrap();
}
