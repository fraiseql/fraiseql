use super::*;

#[test]
fn test_postgres_single_quote() {
    assert_eq!(escape_postgres_jsonb_segment("user'admin"), "user''admin");
}

#[test]
fn test_postgres_multiple_quotes() {
    assert_eq!(escape_postgres_jsonb_segment("it's"), "it''s");
}

#[test]
fn test_postgres_no_quote() {
    assert_eq!(escape_postgres_jsonb_segment("username"), "username");
}

#[test]
fn test_postgres_path_vector() {
    let path = vec!["user'name".to_string(), "id".to_string()];
    let result = escape_postgres_jsonb_path(&path);
    assert_eq!(result[0], "user''name");
    assert_eq!(result[1], "id");
}

#[test]
fn test_postgres_injection_drop_table() {
    let payload = "'; DROP TABLE users; --";
    let escaped = escape_postgres_jsonb_segment(payload);
    // Single quotes must be doubled so they cannot break out of a SQL string literal
    // Payload starts with ' → becomes '' in the output
    assert!(escaped.starts_with("''"), "Opening single quote must be doubled for PostgreSQL");
    assert!(!escaped.starts_with("'\""), "Must not produce an unescaped sequence");
}

#[test]
fn test_postgres_injection_or_1_eq_1() {
    let payload = "' OR '1'='1";
    let escaped = escape_postgres_jsonb_segment(payload);
    // All single quotes must be doubled — count of '' should match original ' count
    let original_quote_count = payload.chars().filter(|&c| c == '\'').count();
    let doubled_count = escaped.matches("''").count();
    assert_eq!(doubled_count, original_quote_count, "Every single quote must be doubled");
}

#[test]
fn test_postgres_injection_double_quote_or() {
    let payload = r#"" OR "1"="1"#;
    let escaped = escape_postgres_jsonb_segment(payload);
    // No single quotes in payload — output must be identical (double quotes are not special in
    // PG segment)
    assert_eq!(escaped, payload);
}

#[test]
fn test_postgres_injection_backslash() {
    let payload = r"\";
    let escaped = escape_postgres_jsonb_segment(payload);
    // PostgreSQL does not treat backslash specially in dollar-quoted / JSONB operators; output
    // is unchanged
    assert_eq!(escaped, payload);
}

#[test]
fn test_postgres_injection_like_percent() {
    let payload = "%";
    let escaped = escape_postgres_jsonb_segment(payload);
    // No single quotes — output unchanged
    assert_eq!(escaped, payload);
}

#[test]
fn test_postgres_injection_like_underscore() {
    let payload = "_";
    let escaped = escape_postgres_jsonb_segment(payload);
    assert_eq!(escaped, payload);
}

#[test]
fn test_postgres_injection_xss_script_tag() {
    let payload = "<script>alert(1)</script>";
    let escaped = escape_postgres_jsonb_segment(payload);
    // No single quotes in XSS payload — output identical
    assert_eq!(escaped, payload);
}

#[test]
fn test_postgres_injection_null_literal() {
    let payload = "NULL";
    let escaped = escape_postgres_jsonb_segment(payload);
    assert_eq!(escaped, "NULL");
}

#[test]
fn test_postgres_injection_empty_string() {
    let payload = "";
    let escaped = escape_postgres_jsonb_segment(payload);
    assert_eq!(escaped, "");
}

#[test]
fn test_postgres_injection_unicode_accents() {
    let payload = "François";
    let escaped = escape_postgres_jsonb_segment(payload);
    // No single quotes — output unchanged
    assert_eq!(escaped, "François");
}

#[test]
fn test_postgres_segment_double_single_quote_roundtrip() {
    // A single quote in input → doubled in output
    let input = "it's";
    let escaped = escape_postgres_jsonb_segment(input);
    assert_eq!(escaped, "it''s");
}

#[test]
fn test_postgres_path_multi_segment_escaping() {
    let path = vec!["user'name".to_string(), "field's".to_string()];
    let result = escape_postgres_jsonb_path(&path);
    assert_eq!(result[0], "user''name");
    assert_eq!(result[1], "field''s");
}
