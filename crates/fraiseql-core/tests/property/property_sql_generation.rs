//! Property-based tests for SQL generation safety and correctness.
//!
//! These properties verify that the WHERE clause SQL generators produce
//! safe, well-formed output for any valid input combination.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use fraiseql_core::db::{
    WhereClause, WhereOperator, postgres::PostgresWhereGenerator,
    where_sql_generator::WhereSqlGenerator,
};
use proptest::prelude::*;
use serde_json::{Value, json};

// ============================================================================
// Strategies for generating arbitrary WhereClause ASTs
// ============================================================================

/// Strategy for generating safe field path segments (valid SQL identifiers).
fn arb_path_segment() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,20}".prop_map(String::from)
}

/// Strategy for generating a non-empty field path.
fn arb_path() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(arb_path_segment(), 1..=4)
}

/// Strategy for generating scalar JSON values suitable for WHERE comparisons.
fn arb_scalar_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        "[a-zA-Z0-9 _.@-]{0,50}".prop_map(Value::String),
    ]
}

/// Strategy for generating string values only.
fn arb_string_value() -> impl Strategy<Value = Value> {
    "[a-zA-Z0-9 _.@-]{0,50}".prop_map(Value::String)
}

/// Strategy for generating array values (for IN/NIN operators).
fn arb_array_value() -> impl Strategy<Value = Value> {
    prop::collection::vec(arb_string_value(), 1..=5).prop_map(Value::Array)
}

/// Operators that work with any scalar value.
fn arb_comparison_operator() -> impl Strategy<Value = WhereOperator> {
    prop_oneof![
        Just(WhereOperator::Eq),
        Just(WhereOperator::Neq),
        Just(WhereOperator::Gt),
        Just(WhereOperator::Gte),
        Just(WhereOperator::Lt),
        Just(WhereOperator::Lte),
    ]
}

/// Operators that require string values.
fn arb_string_operator() -> impl Strategy<Value = WhereOperator> {
    prop_oneof![
        Just(WhereOperator::Contains),
        Just(WhereOperator::Icontains),
        Just(WhereOperator::Startswith),
        Just(WhereOperator::Istartswith),
        Just(WhereOperator::Endswith),
        Just(WhereOperator::Iendswith),
        Just(WhereOperator::Like),
        Just(WhereOperator::Ilike),
    ]
}

/// Strategy for generating a `WhereClause::Field` with comparison operators.
fn arb_comparison_field() -> impl Strategy<Value = WhereClause> {
    (arb_path(), arb_comparison_operator(), arb_scalar_value()).prop_map(
        |(path, operator, value)| WhereClause::Field {
            path,
            operator,
            value,
        },
    )
}

/// Strategy for generating a `WhereClause::Field` with string operators.
fn arb_string_field() -> impl Strategy<Value = WhereClause> {
    (arb_path(), arb_string_operator(), arb_string_value()).prop_map(|(path, operator, value)| {
        WhereClause::Field {
            path,
            operator,
            value,
        }
    })
}

/// Strategy for generating a `WhereClause::Field` with IN/NIN.
fn arb_in_field() -> impl Strategy<Value = WhereClause> {
    (
        arb_path(),
        prop_oneof![Just(WhereOperator::In), Just(WhereOperator::Nin)],
        arb_array_value(),
    )
        .prop_map(|(path, operator, value)| WhereClause::Field {
            path,
            operator,
            value,
        })
}

/// Strategy for generating a `WhereClause::Field` with `IsNull`.
fn arb_isnull_field() -> impl Strategy<Value = WhereClause> {
    (arb_path(), any::<bool>()).prop_map(|(path, is_null)| WhereClause::Field {
        path,
        operator: WhereOperator::IsNull,
        value: json!(is_null),
    })
}

/// Strategy for generating any valid leaf `WhereClause`.
fn arb_leaf_clause() -> impl Strategy<Value = WhereClause> {
    prop_oneof![
        arb_comparison_field(),
        arb_string_field(),
        arb_in_field(),
        arb_isnull_field(),
    ]
}

/// Strategy for generating a `WhereClause` tree (with nesting).
fn arb_where_clause() -> impl Strategy<Value = WhereClause> {
    arb_leaf_clause().prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 1..=4).prop_map(WhereClause::And),
            prop::collection::vec(inner.clone(), 1..=4).prop_map(WhereClause::Or),
            inner.prop_map(|c| WhereClause::Not(Box::new(c))),
        ]
    })
}

// ============================================================================
// PostgresWhereGenerator: Parameterization Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Property: Generated SQL never contains raw string values inline.
    /// All values must be parameterized ($1, $2, etc.).
    #[test]
    fn prop_postgres_never_inlines_string_values(
        path in arb_path(),
        value in "[a-zA-Z0-9]{1,20}",
    ) {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path,
            operator: WhereOperator::Eq,
            value: Value::String(value.clone()),
        };

        let (sql, params) = gen.generate(&clause).unwrap();

        // The value must appear in params, not inline in SQL.
        // Check that SQL uses a placeholder and the value is in params.
        prop_assert!(
            sql.contains("$1"),
            "SQL must use parameterized placeholder, got: {}", sql
        );
        prop_assert_eq!(params.len(), 1);
        prop_assert_eq!(&params[0], &json!(value));
    }

    /// Property: Parameter count in SQL matches params vector length.
    #[test]
    fn prop_postgres_param_count_matches(clause in arb_where_clause()) {
        let gen = PostgresWhereGenerator::new();
        let result = gen.generate(&clause);

        if let Ok((sql, params)) = result {
            let placeholder_count = count_placeholders(&sql);
            prop_assert_eq!(
                placeholder_count, params.len(),
                "Placeholder count ({}) != params length ({})\nSQL: {}",
                placeholder_count, params.len(), sql
            );
        }
    }

    /// Property: Parameter placeholders are sequential ($1, $2, $3...).
    #[test]
    fn prop_postgres_params_sequential(clause in arb_where_clause()) {
        let gen = PostgresWhereGenerator::new();
        let result = gen.generate(&clause);

        if let Ok((sql, params)) = result {
            for i in 1..=params.len() {
                let placeholder = format!("${}", i);
                prop_assert!(
                    sql.contains(&placeholder),
                    "Missing sequential placeholder {} in SQL: {}", placeholder, sql
                );
            }
        }
    }

    /// Property: SQL injection payloads in string values are parameterized, not inlined.
    #[test]
    fn prop_postgres_injection_safe_string_values(
        path in arb_path(),
        prefix in "[a-zA-Z]{0,10}",
        injection in prop_oneof![
            Just("'; DROP TABLE users; --"),
            Just("' OR '1'='1"),
            Just("'; DELETE FROM data WHERE '1'='1"),
            Just("\\'; TRUNCATE users; --"),
        ],
    ) {
        let gen = PostgresWhereGenerator::new();
        let payload = format!("{}{}", prefix, injection);
        let clause = WhereClause::Field {
            path,
            operator: WhereOperator::Eq,
            value: Value::String(payload.clone()),
        };

        let (sql, params) = gen.generate(&clause).unwrap();

        prop_assert!(
            !sql.contains("DROP"),
            "SQL injection payload must not appear in SQL: {}", sql
        );
        prop_assert!(
            !sql.contains("DELETE"),
            "SQL injection payload must not appear in SQL: {}", sql
        );
        prop_assert!(
            !sql.contains("TRUNCATE"),
            "SQL injection payload must not appear in SQL: {}", sql
        );
        prop_assert_eq!(&params[0], &json!(payload));
    }

    /// Property: SQL injection payloads in field paths are properly escaped.
    /// Single quotes in path segments are doubled, preventing SQL breakout.
    #[test]
    fn prop_postgres_injection_safe_path_segments(
        prefix in "[a-zA-Z]{1,5}",
        suffix in "[a-zA-Z]{1,5}",
        operator in arb_comparison_operator(),
    ) {
        let gen = PostgresWhereGenerator::new();
        // Path with embedded single quote
        let path_segment = format!("{}'{}",  prefix, suffix);
        let clause = WhereClause::Field {
            path: vec![path_segment.clone()],
            operator,
            value: json!("safe_value"),
        };

        let result = gen.generate(&clause);

        if let Ok((sql, _)) = result {
            // The single quote must be escaped (doubled) in the SQL
            let escaped = path_segment.replace('\'', "''");
            prop_assert!(
                sql.contains(&escaped),
                "Path should contain escaped segment '{}', got: {}", escaped, sql
            );
            // Values are still parameterized
            prop_assert!(sql.contains("$1"), "Values should still be parameterized: {}", sql);
        }
    }

    /// Property: AND/OR clauses produce balanced parentheses.
    #[test]
    fn prop_postgres_balanced_parentheses(clause in arb_where_clause()) {
        let gen = PostgresWhereGenerator::new();
        let result = gen.generate(&clause);

        if let Ok((sql, _)) = result {
            let open = sql.chars().filter(|c| *c == '(').count();
            let close = sql.chars().filter(|c| *c == ')').count();
            prop_assert_eq!(
                open, close,
                "Unbalanced parentheses in SQL: {}", sql
            );
        }
    }

    /// Property: Empty AND produces TRUE, empty OR produces FALSE.
    #[test]
    fn prop_postgres_empty_logic_identity(
        use_and in any::<bool>(),
    ) {
        let gen = PostgresWhereGenerator::new();
        let clause = if use_and {
            WhereClause::And(vec![])
        } else {
            WhereClause::Or(vec![])
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        prop_assert!(params.is_empty());

        if use_and {
            prop_assert_eq!(sql, "TRUE");
        } else {
            prop_assert_eq!(sql, "FALSE");
        }
    }

    /// Property: NOT wraps inner clause in NOT (...).
    #[test]
    fn prop_postgres_not_wraps_inner(inner in arb_leaf_clause()) {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Not(Box::new(inner));
        let result = gen.generate(&clause);

        if let Ok((sql, _)) = result {
            prop_assert!(
                sql.starts_with("NOT ("),
                "NOT clause should start with 'NOT (': {}", sql
            );
            prop_assert!(
                sql.ends_with(')'),
                "NOT clause should end with ')': {}", sql
            );
        }
    }

    /// Property: IsNull produces no parameters.
    #[test]
    fn prop_postgres_isnull_no_params(
        path in arb_path(),
        is_null in any::<bool>(),
    ) {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path,
            operator: WhereOperator::IsNull,
            value: json!(is_null),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        prop_assert!(params.is_empty(), "IsNull should produce no params, got: {:?}", params);

        if is_null {
            prop_assert!(sql.contains("IS NULL"), "Expected IS NULL in: {}", sql);
            prop_assert!(!sql.contains("IS NOT NULL"), "Should not contain IS NOT NULL: {}", sql);
        } else {
            prop_assert!(sql.contains("IS NOT NULL"), "Expected IS NOT NULL in: {}", sql);
        }
    }

    /// Property: IN operator with N elements produces N parameters.
    #[test]
    fn prop_postgres_in_param_count(
        path in arb_path(),
        values in prop::collection::vec(arb_string_value(), 1..=10),
    ) {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path,
            operator: WhereOperator::In,
            value: Value::Array(values.clone()),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        prop_assert_eq!(
            params.len(), values.len(),
            "IN should produce one param per array element. SQL: {}", sql
        );
    }

    /// Property: String operators (LIKE/ILIKE) always parameterize the search term.
    #[test]
    fn prop_postgres_like_parameterized(
        path in arb_path(),
        operator in arb_string_operator(),
        value in "[a-zA-Z0-9]{1,20}",
    ) {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path,
            operator,
            value: Value::String(value.clone()),
        };

        let (sql, params) = gen.generate(&clause).unwrap();

        // The search term must be parameterized, not inlined.
        // We verify by checking the value appears in params and SQL uses $1.
        prop_assert!(
            sql.contains("$1"),
            "LIKE/ILIKE must use parameterized placeholder, got: {}", sql
        );
        prop_assert!(!params.is_empty(), "LIKE/ILIKE must produce at least one param");
        prop_assert!(
            sql.contains("LIKE") || sql.contains("ILIKE"),
            "String operators should produce LIKE or ILIKE: {}", sql
        );
        // Verify the value is in params
        let expected_val = Value::String(value);
        prop_assert!(
            params.contains(&expected_val),
            "Search term must appear in params"
        );
    }

    /// Property: Numeric comparisons cast to ::numeric for type safety.
    #[test]
    fn prop_postgres_numeric_casts(
        path in arb_path(),
        operator in arb_comparison_operator(),
        value in any::<i64>(),
    ) {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path,
            operator,
            value: json!(value),
        };

        let (sql, _params) = gen.generate(&clause).unwrap();
        prop_assert!(
            sql.contains("::numeric"),
            "Numeric comparisons should cast to ::numeric: {}", sql
        );
    }

    /// Property: Boolean comparisons cast to ::boolean for type safety.
    #[test]
    fn prop_postgres_boolean_casts(
        path in arb_path(),
        value in any::<bool>(),
    ) {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path,
            operator: WhereOperator::Eq,
            value: json!(value),
        };

        let (sql, _params) = gen.generate(&clause).unwrap();
        prop_assert!(
            sql.contains("::boolean"),
            "Boolean comparisons should cast to ::boolean: {}", sql
        );
    }

    /// Property: Generator is reusable — calling generate() resets param counter.
    #[test]
    fn prop_postgres_generator_reusable(
        clause1 in arb_leaf_clause(),
        clause2 in arb_leaf_clause(),
    ) {
        let gen = PostgresWhereGenerator::new();

        let result1 = gen.generate(&clause1);
        let result2 = gen.generate(&clause2);

        if let (Ok((sql1, _)), Ok((sql2, _))) = (&result1, &result2) {
            if sql1.contains('$') {
                prop_assert!(sql1.contains("$1"), "First call should start at $1: {}", sql1);
            }
            if sql2.contains('$') {
                prop_assert!(sql2.contains("$1"), "Second call should reset to $1: {}", sql2);
            }
        }
    }
}

// ============================================================================
// WhereSqlGenerator (generic): Structural Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Property: Generic SQL generator never panics on valid clause trees.
    #[test]
    fn prop_generic_generator_no_panic(clause in arb_where_clause()) {
        let _ = WhereSqlGenerator::to_sql(&clause);
    }

    /// Property: Generic generator escapes single quotes in string values.
    #[test]
    fn prop_generic_generator_escapes_quotes(
        path in arb_path(),
        value in ".*'.*",
    ) {
        let clause = WhereClause::Field {
            path,
            operator: WhereOperator::Eq,
            value: Value::String(value.clone()),
        };

        let result = WhereSqlGenerator::to_sql(&clause);
        if let Ok(sql) = result {
            let quote_count = value.chars().filter(|c| *c == '\'').count();
            let sql_quote_count = sql.chars().filter(|c| *c == '\'').count();
            prop_assert!(
                sql_quote_count >= quote_count * 2,
                "Single quotes must be escaped. Value has {} quotes, SQL has {} quotes: {}",
                quote_count, sql_quote_count, sql
            );
        }
    }

    /// Property: Generic generator AND/OR produce correct keyword.
    #[test]
    fn prop_generic_and_or_keywords(
        clauses in prop::collection::vec(arb_leaf_clause(), 2..=4),
        use_and in any::<bool>(),
    ) {
        let clause = if use_and {
            WhereClause::And(clauses)
        } else {
            WhereClause::Or(clauses)
        };

        let result = WhereSqlGenerator::to_sql(&clause);
        if let Ok(sql) = result {
            if use_and {
                prop_assert!(
                    sql.contains(" AND "),
                    "AND clause should contain ' AND ': {}", sql
                );
            } else {
                prop_assert!(
                    sql.contains(" OR "),
                    "OR clause should contain ' OR ': {}", sql
                );
            }
        }
    }

    /// Property: Generic generator balanced parentheses.
    #[test]
    fn prop_generic_balanced_parentheses(clause in arb_where_clause()) {
        let result = WhereSqlGenerator::to_sql(&clause);
        if let Ok(sql) = result {
            let open = sql.chars().filter(|c| *c == '(').count();
            let close = sql.chars().filter(|c| *c == ')').count();
            prop_assert_eq!(
                open, close,
                "Unbalanced parentheses in SQL: {}", sql
            );
        }
    }

    /// Property: Generic generator escapes single quotes in field path segments.
    /// A path containing a single quote must have it doubled in the output.
    #[test]
    fn prop_generic_path_escaping(
        prefix in "[a-zA-Z]{1,10}",
        suffix in "[a-zA-Z]{1,10}",
    ) {
        let injection = format!("{}'{}",  prefix, suffix);
        let clause = WhereClause::Field {
            path: vec![injection.clone()],
            operator: WhereOperator::Eq,
            value: json!("value"),
        };

        let result = WhereSqlGenerator::to_sql(&clause);
        if let Ok(sql) = result {
            // The single quote in the path should be escaped (doubled)
            let escaped = injection.replace('\'', "''");
            prop_assert!(
                sql.contains(&escaped),
                "Path should contain escaped version '{}', got: {}", escaped, sql
            );
        }
    }
}

// ============================================================================
// WhereClause AST Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Property: WhereClause serializes and deserializes via JSON without data loss.
    #[test]
    fn prop_where_clause_json_roundtrip(clause in arb_leaf_clause()) {
        let json_str = serde_json::to_string(&clause).expect("serialization failed");
        let restored: WhereClause =
            serde_json::from_str(&json_str).expect("deserialization failed");
        prop_assert_eq!(clause, restored);
    }

    /// Property: WhereClause::is_empty is true only for empty And/Or.
    #[test]
    fn prop_where_clause_is_empty_consistency(clause in arb_where_clause()) {
        match &clause {
            WhereClause::And(v) | WhereClause::Or(v) => {
                prop_assert_eq!(clause.is_empty(), v.is_empty());
            }
            WhereClause::Not(_) | WhereClause::Field { .. } => {
                prop_assert!(!clause.is_empty());
            }
        }
    }

    /// Property: WhereOperator::from_str roundtrips for known operators.
    #[test]
    fn prop_operator_from_str_roundtrip(
        op_name in prop_oneof![
            Just("eq"), Just("neq"), Just("gt"), Just("gte"), Just("lt"), Just("lte"),
            Just("in"), Just("nin"),
            Just("contains"), Just("icontains"), Just("startswith"), Just("istartswith"),
            Just("endswith"), Just("iendswith"), Just("like"), Just("ilike"),
            Just("isnull"),
            Just("array_contains"), Just("array_contained_by"), Just("array_overlaps"),
            Just("matches"), Just("plain_query"), Just("phrase_query"), Just("websearch_query"),
        ],
    ) {
        let parsed = WhereOperator::from_str(op_name);
        prop_assert!(parsed.is_ok(), "Known operator '{}' should parse", op_name);
    }

    /// Property: WhereOperator::from_str rejects unknown operators.
    #[test]
    fn prop_operator_rejects_unknown(
        name in "[a-z]{1,10}",
    ) {
        let known = [
            "eq", "neq", "gt", "gte", "lt", "lte", "in", "nin",
            "contains", "icontains", "startswith", "istartswith",
            "endswith", "iendswith", "like", "ilike", "isnull",
        ];
        prop_assume!(!known.contains(&name.as_str()));

        let more_known = ["matches", "overlaps", "lca"];
        prop_assume!(!more_known.contains(&name.as_str()));

        let prefixed = [
            "array_", "len_", "cosine_", "l2_", "l1_", "hamming_", "inner_",
            "jaccard_", "plain_", "phrase_", "websearch_", "is_", "in_",
            "contains_", "strictly_", "ancestor_", "descendant_", "matches_",
            "depth_",
        ];
        prop_assume!(!prefixed.iter().any(|p| name.starts_with(p)));

        let result = WhereOperator::from_str(&name);
        prop_assert!(result.is_err(), "Unknown operator '{}' should be rejected", name);
    }

    /// Property: String operators are correctly classified.
    #[test]
    fn prop_string_operator_classification(op in arb_string_operator()) {
        prop_assert!(
            op.is_string_operator(),
            "{:?} should be classified as string operator", op
        );
    }

    /// Property: IN/NIN operators expect array values.
    #[test]
    fn prop_in_expects_array(use_in in any::<bool>()) {
        let op = if use_in { WhereOperator::In } else { WhereOperator::Nin };
        prop_assert!(op.expects_array(), "{:?} should expect array values", op);
    }

    /// Property: Comparison operators don't expect arrays.
    #[test]
    fn prop_comparison_not_array(op in arb_comparison_operator()) {
        prop_assert!(!op.expects_array(), "{:?} should not expect array values", op);
    }
}

// ============================================================================
// SQL Generation Robustness and Edge Cases
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: Field WHERE clauses always generate valid SQL with placeholders.
    #[test]
    fn prop_field_where_generates_valid_sql(
        path in arb_path(),
        op in arb_comparison_operator(),
        value in arb_scalar_value(),
    ) {
        let clause = WhereClause::Field {
            path,
            operator: op,
            value,
        };

        let generator = PostgresWhereGenerator::new();
        if let Ok((sql, params)) = generator.generate(&clause) {
            // Should not be empty
            prop_assert!(!sql.is_empty(), "Generated SQL should not be empty");
            // Should have balanced parentheses
            let open = sql.chars().filter(|c| *c == '(').count();
            let close = sql.chars().filter(|c| *c == ')').count();
            prop_assert_eq!(open, close, "Parentheses should be balanced in: {}", sql);
            // Should have at least one parameter
            prop_assert!(!params.is_empty(), "Should have parameters: {}", sql);
        }
    }

    /// Property: Deeply nested field paths generate valid SQL.
    #[test]
    fn prop_deep_field_paths_valid_sql(
        path_segments in prop::collection::vec(arb_path_segment(), 1..10),
        op in arb_comparison_operator(),
        value in arb_scalar_value(),
    ) {
        let clause = WhereClause::Field {
            path: path_segments,
            operator: op,
            value,
        };

        let generator = PostgresWhereGenerator::new();
        // Should either succeed or fail safely, never panic
        let _result = generator.generate(&clause);
    }

    /// Property: SQL generation is deterministic (same input → same output).
    #[test]
    fn prop_sql_generation_deterministic(
        path in arb_path(),
        op in arb_comparison_operator(),
        value in arb_scalar_value(),
    ) {
        let clause = WhereClause::Field {
            path,
            operator: op,
            value,
        };

        let gen1 = PostgresWhereGenerator::new();
        let gen2 = PostgresWhereGenerator::new();

        let result1 = gen1.generate(&clause);
        let result2 = gen2.generate(&clause);

        // Both should succeed or both should fail
        match (&result1, &result2) {
            (Ok((sql1, _params1)), Ok((sql2, _params2))) => {
                prop_assert_eq!(sql1, sql2, "SQL should be deterministic");
            }
            (Err(_), Err(_)) => {
                // Both failed, which is deterministic
            }
            (Ok(_), Err(_)) | (Err(_), Ok(_)) => {
                prop_assert!(false, "SQL generation should be deterministic: got Ok vs Err");
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Count the number of `$N` parameter placeholders in a SQL string.
fn count_placeholders(sql: &str) -> usize {
    let mut count = 0;
    let mut i = 1;
    loop {
        let placeholder = format!("${}", i);
        if sql.contains(&placeholder) {
            count += 1;
            i += 1;
        } else {
            break;
        }
    }
    count
}
