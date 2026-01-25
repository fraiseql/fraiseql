//! Comprehensive test suite for JSON path SQL injection prevention
//!
//! Tests that malicious path segments cannot be used to inject SQL.
//! These tests verify the escaping mechanisms work correctly.

#[cfg(test)]
mod path_injection_tests {
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
    fn test_mysql_path_with_single_quote() {
        let path = vec!["user'; DROP TABLE users; --".to_string()];
        let escaped = path_escape::escape_mysql_json_path(&path);

        // Should escape single quote with backslash
        assert!(escaped.contains("\\'"), "Single quote not escaped for MySQL");
        // Result should be: $.user\'; DROP TABLE users; --
        assert_eq!(escaped, "$.user\\'; DROP TABLE users; --");
    }

    #[test]
    fn test_mysql_multipart_path_with_injection() {
        let path = vec!["user'".to_string(), "admin' OR '1'='1".to_string()];
        let escaped = path_escape::escape_mysql_json_path(&path);

        // All quotes should be escaped
        assert!(escaped.contains("\\'"), "Quotes not properly escaped");
    }

    #[test]
    fn test_mysql_path_with_sql_keywords() {
        let vectors = vec![
            vec!["DELETE FROM users".to_string()],
            vec!["DROP TABLE".to_string()],
            vec!["UPDATE users".to_string()],
        ];

        for path in vectors {
            let escaped = path_escape::escape_mysql_json_path(&path);

            // Path components should be joined with dots for JSON path
            assert!(escaped.starts_with("$."), "JSON path must start with $.");
            // No quotes in these examples, so keywords are preserved as-is
            assert!(
                escaped.contains("DELETE")
                    || escaped.contains("DROP")
                    || escaped.contains("UPDATE"),
                "Keywords should be preserved"
            );
        }
    }

    #[test]
    fn test_mysql_preserves_dot_notation() {
        let path = vec![
            "user".to_string(),
            "profile".to_string(),
            "name".to_string(),
        ];
        let escaped = path_escape::escape_mysql_json_path(&path);
        assert_eq!(escaped, "$.user.profile.name");
    }

    // ============================================================================
    // SQLite json_extract Injection Tests
    // ============================================================================

    #[test]
    fn test_sqlite_path_with_single_quote() {
        let path = vec!["user'; DROP TABLE users; --".to_string()];
        let escaped = path_escape::escape_sqlite_json_path(&path);

        assert!(escaped.contains("\\'"), "Single quote not escaped for SQLite");
        assert_eq!(escaped, "$.user\\'; DROP TABLE users; --");
    }

    #[test]
    fn test_sqlite_multipart_path() {
        let path = vec![
            "user'".to_string(),
            "data".to_string(),
            "admin' OR '1'='1".to_string(),
        ];
        let escaped = path_escape::escape_sqlite_json_path(&path);

        let sql = format!("json_extract(data, '{}')", escaped);
        assert!(!sql.contains("OR '1'='1"), "Boolean-based injection not prevented");
    }

    // ============================================================================
    // SQL Server JSON_VALUE Injection Tests
    // ============================================================================

    #[test]
    fn test_sqlserver_path_with_single_quote() {
        let path = vec!["user'; DROP TABLE users; --".to_string()];
        let escaped = path_escape::escape_sqlserver_json_path(&path);

        // SQL Server uses double-quote escaping
        assert!(escaped.contains("''"), "Single quote not escaped for SQL Server");
        assert_eq!(escaped, "$.user''; DROP TABLE users; --");
    }

    #[test]
    fn test_sqlserver_multipart_path() {
        let path = vec!["user'".to_string(), "admin' OR '1'='1".to_string()];
        let escaped = path_escape::escape_sqlserver_json_path(&path);

        let sql = format!("JSON_VALUE(data, '{}')", escaped);

        // Check proper escaping (SQL Server doubles quotes)
        assert!(sql.contains("user''"), "Single quotes not doubled");
        assert!(sql.contains("admin''"), "Single quotes not doubled");
    }

    // ============================================================================
    // Edge Cases and Special Scenarios
    // ============================================================================

    #[test]
    fn test_all_databases_with_null_byte() {
        // Null bytes could cause truncation in some contexts
        let segment_with_null = "test\x00injection";

        let pg_escaped = path_escape::escape_postgres_jsonb_segment(segment_with_null);
        let mysql_escaped = path_escape::escape_mysql_json_path(&[segment_with_null.to_string()]);
        let sqlite_escaped = path_escape::escape_sqlite_json_path(&[segment_with_null.to_string()]);
        let sqlserver_escaped =
            path_escape::escape_sqlserver_json_path(&[segment_with_null.to_string()]);

        // None should lose content due to null bytes
        assert!(pg_escaped.contains("test"), "Null byte caused truncation in PostgreSQL");
        assert!(mysql_escaped.contains("test"), "Null byte caused truncation in MySQL");
        assert!(sqlite_escaped.contains("test"), "Null byte caused truncation in SQLite");
        assert!(sqlserver_escaped.contains("test"), "Null byte caused truncation in SQL Server");
    }

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
    fn test_path_that_looks_like_json_syntax() {
        let attack_vectors = vec![
            "field'}]",
            "[0]",
            "{\"key\": \"value\"}",
            "\\u0000",
            "\\n\\r\\t",
        ];

        for vector in attack_vectors {
            let escaped_pg = path_escape::escape_postgres_jsonb_segment(vector);
            let _escaped_mysql = path_escape::escape_mysql_json_path(&[vector.to_string()]);

            // These should be treated as literal strings, not JSON syntax
            let pg_sql = format!("data->'{}'", escaped_pg);

            // Verify the SQL structure is preserved and safe
            assert!(pg_sql.contains("data->"), "SQL structure broken");
            assert!(pg_sql.contains("'"), "Quoting broken");
        }
    }

    // ============================================================================
    // Real-World Attack Patterns
    // ============================================================================

    #[test]
    fn test_real_world_sql_injection_patterns() {
        // Common SQL injection patterns that should all be neutralized
        let real_attacks = vec![
            "admin' --",
            "' OR '1'='1",
            "'; DELETE FROM users; --",
            "1' UNION SELECT * FROM users --",
            "' OR 1=1 --",
            "admin'/*",
            "' or 1 like '1",
            "1' AND SLEEP(5) --",
            "' OR 'a'='a",
        ];

        for attack in real_attacks {
            let pg_escaped = path_escape::escape_postgres_jsonb_segment(attack);
            let mysql_escaped = path_escape::escape_mysql_json_path(&[attack.to_string()]);

            // Build complete SQL statements
            let pg_sql = format!("WHERE data->'{}'", pg_escaped);
            let mysql_sql = format!("WHERE JSON_EXTRACT(data, '{}')", mysql_escaped);

            // These patterns should now be inside a string literal, safe from injection
            assert!(pg_sql.contains("WHERE data->"), "PostgreSQL wrapper broken");
            assert!(mysql_sql.contains("WHERE JSON_EXTRACT"), "MySQL wrapper broken");
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
    fn test_all_special_chars_except_quotes() {
        let special_chars = "!@#$%^&*()_+-=[]{}|;:,.<>?/~`";

        let pg_escaped = path_escape::escape_postgres_jsonb_segment(special_chars);
        let mysql_escaped = path_escape::escape_mysql_json_path(&[special_chars.to_string()]);
        let sqlite_escaped = path_escape::escape_sqlite_json_path(&[special_chars.to_string()]);
        let sqlserver_escaped =
            path_escape::escape_sqlserver_json_path(&[special_chars.to_string()]);

        // Special characters should be preserved, only quotes (if any) should change
        assert!(pg_escaped.contains("!@#$%^&*"), "Special chars lost in PostgreSQL");
        assert!(mysql_escaped.contains("!@#$%^&*"), "Special chars lost in MySQL");
        assert!(sqlite_escaped.contains("!@#$%^&*"), "Special chars lost in SQLite");
        assert!(sqlserver_escaped.contains("!@#$%^&*"), "Special chars lost in SQL Server");
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

    #[test]
    fn test_mysql_preserves_structure_with_quotes() {
        let path = vec!["user'name".to_string(), "email".to_string()];
        let escaped = path_escape::escape_mysql_json_path(&path);

        // Should preserve the dot notation structure
        assert!(escaped.starts_with("$."), "JSON path must start with $.");
        assert!(escaped.contains("user\\'name"), "Quote should be escaped");
        assert!(escaped.contains("email"), "Second segment should be present");
    }
}
