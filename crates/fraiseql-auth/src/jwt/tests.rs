use super::*;

// -----------------------------------------------------------------------
// extract_claim_string
// -----------------------------------------------------------------------

#[test]
fn extract_plain_string() {
    let v = serde_json::json!("user@example.com");
    assert_eq!(extract_claim_string(&v), Some("user@example.com".to_owned()));
}

#[test]
fn extract_nested_value() {
    let v = serde_json::json!({"value": "user@corp.com", "verified": true});
    assert_eq!(extract_claim_string(&v), Some("user@corp.com".to_owned()));
}

#[test]
fn extract_nested_formatted() {
    let v = serde_json::json!({"formatted": "John Doe", "given": "John", "family": "Doe"});
    assert_eq!(extract_claim_string(&v), Some("John Doe".to_owned()));
}

#[test]
fn extract_nested_email_key() {
    let v = serde_json::json!({"email": "az@example.com", "type": "work"});
    assert_eq!(extract_claim_string(&v), Some("az@example.com".to_owned()));
}

#[test]
fn extract_fallback_first_string() {
    let v = serde_json::json!({"custom_key": "fallback@example.com"});
    assert_eq!(extract_claim_string(&v), Some("fallback@example.com".to_owned()));
}

#[test]
fn extract_array_first_element() {
    let v = serde_json::json!(["a@b.com", "c@d.com"]);
    assert_eq!(extract_claim_string(&v), Some("a@b.com".to_owned()));
}

#[test]
fn extract_null_returns_none() {
    assert_eq!(extract_claim_string(&serde_json::Value::Null), None);
}

#[test]
fn extract_number_returns_none() {
    let v = serde_json::json!(42);
    assert_eq!(extract_claim_string(&v), None);
}

#[test]
fn extract_bool_returns_none() {
    let v = serde_json::json!(true);
    assert_eq!(extract_claim_string(&v), None);
}

#[test]
fn extract_nested_value_null() {
    let v = serde_json::json!({"value": null});
    assert_eq!(extract_claim_string(&v), None);
}

#[test]
fn extract_nested_value_empty_string() {
    let v = serde_json::json!({"value": ""});
    assert_eq!(extract_claim_string(&v), None);
}

#[test]
fn extract_whitespace_only_string() {
    let v = serde_json::json!("  ");
    assert_eq!(extract_claim_string(&v), None);
}

#[test]
fn extract_string_with_surrounding_whitespace() {
    let v = serde_json::json!("  user@example.com  ");
    assert_eq!(extract_claim_string(&v), Some("user@example.com".to_owned()));
}

#[test]
fn extract_empty_object() {
    let v = serde_json::json!({});
    assert_eq!(extract_claim_string(&v), None);
}

#[test]
fn extract_empty_array() {
    let v = serde_json::json!([]);
    assert_eq!(extract_claim_string(&v), None);
}

#[test]
fn extract_array_skips_non_strings() {
    let v = serde_json::json!([42, null, "real@example.com"]);
    assert_eq!(extract_claim_string(&v), Some("real@example.com".to_owned()));
}

// -----------------------------------------------------------------------
// extract_name_string
// -----------------------------------------------------------------------

#[test]
fn name_plain_string() {
    let v = serde_json::json!("John Doe");
    assert_eq!(extract_name_string(&v), Some("John Doe".to_owned()));
}

#[test]
fn name_with_formatted() {
    let v = serde_json::json!({"given": "John", "family": "Doe", "formatted": "John Doe"});
    assert_eq!(extract_name_string(&v), Some("John Doe".to_owned()));
}

#[test]
fn name_given_family_concatenation() {
    let v = serde_json::json!({"given": "John", "family": "Doe"});
    assert_eq!(extract_name_string(&v), Some("John Doe".to_owned()));
}

#[test]
fn name_given_only() {
    let v = serde_json::json!({"given": "John", "family": "  "});
    assert_eq!(extract_name_string(&v), Some("John".to_owned()));
}

#[test]
fn name_family_only() {
    let v = serde_json::json!({"given": "", "family": "Doe"});
    assert_eq!(extract_name_string(&v), Some("Doe".to_owned()));
}

#[test]
fn name_both_empty() {
    let v = serde_json::json!({"given": "", "family": ""});
    assert_eq!(extract_name_string(&v), None);
}

#[test]
fn name_both_whitespace() {
    let v = serde_json::json!({"given": "  ", "family": "  "});
    assert_eq!(extract_name_string(&v), None);
}

// -----------------------------------------------------------------------
// Claims::email() and Claims::name() accessors
// -----------------------------------------------------------------------

fn make_claims(extra: serde_json::Value) -> Claims {
    let mut extra_map = HashMap::new();
    if let serde_json::Value::Object(map) = extra {
        for (k, v) in map {
            extra_map.insert(k, v);
        }
    }
    Claims {
        sub:   "user-1".to_owned(),
        iat:   1_000_000,
        exp:   2_000_000,
        nbf:   None,
        iss:   "test-issuer".to_owned(),
        aud:   vec!["test-aud".to_owned()],
        extra: extra_map,
    }
}

#[test]
fn claims_email_flat_string() {
    let claims = make_claims(serde_json::json!({"email": "user@example.com"}));
    assert_eq!(claims.email(), Some("user@example.com".to_owned()));
}

#[test]
fn claims_email_nested() {
    let claims = make_claims(
        serde_json::json!({"email": {"value": "nested@example.com", "verified": true}}),
    );
    assert_eq!(claims.email(), Some("nested@example.com".to_owned()));
}

#[test]
fn claims_email_missing() {
    let claims = make_claims(serde_json::json!({"other": "value"}));
    assert_eq!(claims.email(), None);
}

#[test]
fn claims_name_flat_string() {
    let claims = make_claims(serde_json::json!({"name": "Jane Doe"}));
    assert_eq!(claims.name(), Some("Jane Doe".to_owned()));
}

#[test]
fn claims_name_nested_given_family() {
    let claims = make_claims(serde_json::json!({"name": {"given": "Jane", "family": "Doe"}}));
    assert_eq!(claims.name(), Some("Jane Doe".to_owned()));
}

#[test]
fn claims_name_missing() {
    let claims = make_claims(serde_json::json!({"other": "value"}));
    assert_eq!(claims.name(), None);
}

#[test]
fn claims_mixed_nesting() {
    let claims = make_claims(serde_json::json!({
        "email": {"value": "user@corp.com"},
        "name": "Flat Name"
    }));
    assert_eq!(claims.email(), Some("user@corp.com".to_owned()));
    assert_eq!(claims.name(), Some("Flat Name".to_owned()));
}

// -----------------------------------------------------------------------
// Format-specific integration fixtures (#246)
// -----------------------------------------------------------------------

#[test]
fn format_nested_value_key() {
    let claims = make_claims(serde_json::json!({
        "email": {"value": "user@corp.com", "verified": true}
    }));
    assert_eq!(claims.email(), Some("user@corp.com".to_owned()));
}

#[test]
fn format_nested_name_object() {
    let claims = make_claims(serde_json::json!({
        "name": {"given": "John", "family": "Doe"}
    }));
    assert_eq!(claims.name(), Some("John Doe".to_owned()));
}

#[test]
fn format_flat_strings() {
    let claims = make_claims(serde_json::json!({
        "email": "user@example.com",
        "name": "John Doe"
    }));
    assert_eq!(claims.email(), Some("user@example.com".to_owned()));
    assert_eq!(claims.name(), Some("John Doe".to_owned()));
}

#[test]
fn format_array_form() {
    let claims = make_claims(serde_json::json!({
        "email": ["primary@x.com", "secondary@x.com"]
    }));
    assert_eq!(claims.email(), Some("primary@x.com".to_owned()));
}

#[test]
fn format_mixed_nesting() {
    let claims = make_claims(serde_json::json!({
        "email": {"value": "user@corp.com"},
        "name": "Flat Name"
    }));
    assert_eq!(claims.email(), Some("user@corp.com".to_owned()));
    assert_eq!(claims.name(), Some("Flat Name".to_owned()));
}
