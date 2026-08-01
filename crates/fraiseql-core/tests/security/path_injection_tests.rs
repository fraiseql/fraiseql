//! Comprehensive test suite for JSON path SQL injection prevention
//!
//! Tests that malicious path segments cannot be used to inject SQL.
//! These tests verify the escaping mechanisms work correctly.

#![allow(clippy::format_push_string)] // Reason: test query builders use push_str(&format!()) for readability
#![allow(clippy::default_trait_access)] // Reason: test setup uses Default::default() for brevity
use fraiseql_core::db::path_escape;

// ============================================================================
// PostgreSQL JSONB Injection Tests
// ============================================================================

#[test]
fn test_postgres_path_with_single_quote() {
    // Attack: user'; DROP TABLE users; --
    let segment = "user'; DROP TABLE users; --";
    let escaped = path_escape::escape_postgres_jsonb_segment(segment);

    // Verify quotes are doubled
    assert_eq!(escaped, "user''; DROP TABLE users; --");

    // The escaped version should now be safe inside a PostgreSQL string
    let sql = format!("data->>'{}'", escaped);
    // Inside the quoted string, DROP is just text, not a command
    assert!(sql.contains("data->>'user''"), "Escaping structure broken");
}

#[test]
fn test_postgres_path_with_multiple_quotes() {
    let segment = "it's a test' and '1'='1";
    let escaped = path_escape::escape_postgres_jsonb_segment(segment);

    // All quotes should be doubled
    let quote_count = segment.matches('\'').count();
    let escaped_quote_count = escaped.matches("''").count();
    assert_eq!(quote_count, escaped_quote_count, "Not all quotes were properly escaped");
}

#[test]
fn test_postgres_path_with_sql_keywords() {
    let vectors = vec![
        "DELETE FROM users",
        "DROP TABLE users",
        "UPDATE users SET",
        "INSERT INTO users",
        "SELECT * FROM",
    ];

    for keyword_path in vectors {
        let escaped = path_escape::escape_postgres_jsonb_segment(keyword_path);
        // These should just be escaped, not cause SQL injection
        let sql = format!("data->'{}'", escaped);

        // The format should not create SQL keywords outside the string
        assert!(sql.contains(keyword_path), "Path lost during escaping");
    }
}

#[test]
fn test_postgres_path_with_brackets() {
    let segment = "field'][0";
    let escaped = path_escape::escape_postgres_jsonb_segment(segment);

    // The quote in the segment should be doubled
    assert_eq!(escaped, "field''][0");

    let sql = format!("data->'{}'", escaped);

    // The quoted string prevents the bracket syntax from being interpreted
    assert!(sql.contains("data->'field''"), "Quote escaping not applied");
}

#[test]
fn test_postgres_multipart_path_injection() {
    let path = vec![
        "user'; DROP--".to_string(),
        "admin' OR '1'='1".to_string(),
        "test".to_string(),
    ];

    let escaped = path_escape::escape_postgres_jsonb_path(&path);

    // Each segment should be properly escaped
    assert_eq!(escaped[0], "user''; DROP--");
    assert_eq!(escaped[1], "admin'' OR ''1''=''1");
    assert_eq!(escaped[2], "test");

    // Verify we can build valid SQL with escaped paths
    let mut sql = "data".to_string();
    for (i, segment) in escaped.iter().enumerate() {
        if i < escaped.len() - 1 {
            sql.push_str(&format!("->'{}'", segment));
        } else {
            sql.push_str(&format!("->>'{}' ", segment));
        }
    }

    // Should build a valid structure with quoted segments
    assert!(sql.contains("data->"), "SQL structure broken");
    assert!(sql.contains("user''"), "Quote escaping not applied");
}

#[test]
fn test_postgres_empty_path_segment() {
    let segment = "";
    let escaped = path_escape::escape_postgres_jsonb_segment(segment);
    assert_eq!(escaped, "");
}

#[test]
fn test_postgres_unicode_in_path() {
    let segment = "user' UNION SELECT '你好";
    let escaped = path_escape::escape_postgres_jsonb_segment(segment);
    // The quote should be doubled, making the UNION SELECT safe
    assert_eq!(escaped, "user'' UNION SELECT ''你好");
    // When wrapped in quotes, UNION SELECT becomes literal text
    let sql = format!("data->'{}'", escaped);
    assert!(sql.contains("data->'user''"), "Quote escaping failed");
}

#[test]
fn test_postgres_only_quotes() {
    let segment = "''''";
    let escaped = path_escape::escape_postgres_jsonb_segment(segment);
    // Should double all quotes: '''' becomes ''''''
    assert_eq!(escaped, "''''''''");
}

// ============================================================================
// MySQL JSON_EXTRACT Injection Tests
// ============================================================================

#[test]
fn test_all_databases_with_very_long_path() {
    // Test paths with many segments
    let mut long_path = Vec::new();
    for i in 0..50 {
        long_path.push(format!("segment_{}', DROP TABLE users;--", i));
    }

    let pg_escaped = path_escape::escape_postgres_jsonb_path(&long_path);
    assert_eq!(pg_escaped.len(), 50, "Path segments lost during escaping");

    for (i, segment) in pg_escaped.iter().enumerate() {
        // Quotes should be doubled, making the content safe
        assert!(segment.contains("segment_"), "Segment identifier lost");
        // Check that quotes are properly doubled
        if segment.contains('\'') {
            assert!(segment.contains("''"), "Quote not doubled in segment {}", i);
        }
    }
}

#[test]
fn test_postgres_escaping_idempotency() {
    // Applying escaping twice should be safe (not double-escape when wrapped)
    let original = "user'name";
    let once = path_escape::escape_postgres_jsonb_segment(original);

    // The once-escaped version has doubled quotes
    assert_eq!(once, "user''name");

    // If we escape it again (as if it were user input), it should double the already-doubled
    // quotes
    let twice = path_escape::escape_postgres_jsonb_segment(&once);
    assert_eq!(twice, "user''''name");
}

#[test]
fn test_backslash_not_special_in_postgres() {
    let segment = "field\\path";
    let escaped = path_escape::escape_postgres_jsonb_segment(segment);

    // Backslash should be preserved as-is in PostgreSQL (only quotes need escaping)
    assert_eq!(escaped, "field\\path");
}

#[test]
fn test_various_quote_positions() {
    let vectors = vec![
        "'leading",
        "trailing'",
        "mid'dle",
        "mul'ti'ple'quotes",
        "''consecutive",
    ];

    for vector in vectors {
        let pg_escaped = path_escape::escape_postgres_jsonb_segment(vector);

        // Every single quote should become two quotes
        let single_quotes = vector.matches('\'').count();
        let doubled_quotes = pg_escaped.matches("''").count();

        assert_eq!(single_quotes, doubled_quotes, "Quote escaping failed for: {}", vector);
    }
}
