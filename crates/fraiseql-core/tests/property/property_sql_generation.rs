//! Property-based tests for SQL generation safety and correctness.
//!
//! These properties verify that the WHERE clause SQL generators produce
//! safe, well-formed output for any valid input combination.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use fraiseql_core::db::{
    PostgresDialect, WhereClause, WhereOperator, postgres::PostgresWhereGenerator,
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

/// Scalar JSON values for WHERE comparisons: narrow (letters, digits and `_.@-`), or
/// hostile (any number, any string weighted towards SQL syntax; see
/// [`arb_hostile_string`]).
fn scalar_value(hostile: bool) -> BoxedStrategy<Value> {
    if hostile {
        prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            arb_hostile_number(),
            arb_hostile_string().prop_map(Value::String),
        ]
        .boxed()
    } else {
        prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| json!(n)),
            "[a-zA-Z0-9 _.@-]{0,50}".prop_map(Value::String),
        ]
        .boxed()
    }
}

/// String values only, narrow or hostile as [`scalar_value`].
fn string_value(hostile: bool) -> BoxedStrategy<Value> {
    if hostile {
        arb_hostile_string().prop_map(Value::String).boxed()
    } else {
        "[a-zA-Z0-9 _.@-]{0,50}".prop_map(Value::String).boxed()
    }
}

fn arb_scalar_value() -> impl Strategy<Value = Value> {
    scalar_value(false)
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

/// A leaf: a comparison, a string match, `in` / `nin`, or a null test, its operands narrow
/// or hostile as [`scalar_value`].
fn leaf_clause(hostile: bool) -> BoxedStrategy<WhereClause> {
    let field = |operator: BoxedStrategy<WhereOperator>, value: BoxedStrategy<Value>| {
        (arb_path(), operator, value).prop_map(|(path, operator, value)| WhereClause::Field {
            path,
            operator,
            value,
        })
    };
    let membership = prop_oneof![Just(WhereOperator::In), Just(WhereOperator::Nin)].boxed();
    let elements = prop::collection::vec(string_value(hostile), 1..=5)
        .prop_map(Value::Array)
        .boxed();
    prop_oneof![
        field(arb_comparison_operator().boxed(), scalar_value(hostile)),
        field(arb_string_operator().boxed(), string_value(hostile)),
        field(membership, elements),
        (arb_path(), any::<bool>()).prop_map(|(path, is_null)| WhereClause::Field {
            path,
            operator: WhereOperator::IsNull,
            value: json!(is_null),
        }),
    ]
    .boxed()
}

/// A tree of leaves under `And`, `Or` and `Not`, up to three levels deep.
fn where_clause(hostile: bool) -> BoxedStrategy<WhereClause> {
    leaf_clause(hostile)
        .prop_recursive(3, 16, 4, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 1..=4).prop_map(WhereClause::And),
                prop::collection::vec(inner.clone(), 1..=4).prop_map(WhereClause::Or),
                inner.prop_map(|c| WhereClause::Not(Box::new(c))),
            ]
        })
        .boxed()
}

fn arb_leaf_clause() -> impl Strategy<Value = WhereClause> {
    leaf_clause(false)
}

fn arb_where_clause() -> impl Strategy<Value = WhereClause> {
    where_clause(false)
}

/// A tree whose operands are hostile: what the generator properties draw from.
fn arb_hostile_where_clause() -> impl Strategy<Value = WhereClause> {
    where_clause(true)
}

// ============================================================================
// Operand values: hostile strings and numbers, and a tree carrying them
// ============================================================================

/// Any string, weighted towards SQL quoting, comment and pattern syntax, NUL, combining
/// marks and astral characters.
fn arb_hostile_string() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<String>(),
        prop::collection::vec(
            prop_oneof![
                Just("'"),
                Just("''"),
                Just("\\"),
                Just("$$"),
                Just("$t$"),
                Just("E'"),
                Just("--"),
                Just("/*"),
                Just("*/"),
                Just(";"),
                Just("%"),
                Just("_"),
                Just("\0"),
                Just("e\u{301}"),
                Just("\u{1F600}"),
                Just("a"),
            ],
            0..12,
        )
        .prop_map(|parts| parts.concat()),
    ]
}

/// Any JSON number: every `i64`, the integers just past 2^53, and finite floats.
fn arb_hostile_number() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(|n| json!(n)),
        prop_oneof![
            Just(9_007_199_254_740_993_i64),
            Just(-9_007_199_254_740_993_i64)
        ]
        .prop_map(|n| json!(n)),
        any::<f64>()
            .prop_filter("JSON holds finite numbers", |f| f.is_finite())
            .prop_map(|f| json!(f)),
    ]
}

/// Operand values to substitute, consumed in order and reused cyclically.
#[derive(Debug, Clone)]
struct Operands {
    strings: Vec<String>,
    numbers: Vec<Value>,
}

fn arb_operands() -> impl Strategy<Value = Operands> {
    (
        prop::collection::vec(arb_hostile_string(), 1..16),
        prop::collection::vec(arb_hostile_number(), 1..16),
    )
        .prop_map(|(strings, numbers)| Operands { strings, numbers })
}

/// `clause` with every operand value replaced by one of the same JSON shape from
/// `operands`: a string by a string, a number by a number, a boolean by its negation, an
/// array element by element. The null-test operand is not a value but the test's sense,
/// so it is kept.
fn revalue(clause: &WhereClause, operands: &Operands, next: &mut usize) -> WhereClause {
    fn value(v: &Value, operands: &Operands, next: &mut usize) -> Value {
        *next += 1;
        match v {
            Value::String(_) => json!(operands.strings[*next % operands.strings.len()]),
            Value::Number(_) => operands.numbers[*next % operands.numbers.len()].clone(),
            Value::Bool(b) => json!(!b),
            Value::Array(items) => {
                Value::Array(items.iter().map(|i| value(i, operands, next)).collect())
            },
            other => other.clone(),
        }
    }
    match clause {
        WhereClause::Field {
            path,
            operator,
            value: v,
        } => WhereClause::Field {
            path:     path.clone(),
            operator: operator.clone(),
            value:    if matches!(operator, WhereOperator::IsNull | WhereOperator::IsNotNull) {
                v.clone()
            } else {
                value(v, operands, next)
            },
        },
        WhereClause::And(cs) => {
            WhereClause::And(cs.iter().map(|c| revalue(c, operands, next)).collect())
        },
        WhereClause::Or(cs) => {
            WhereClause::Or(cs.iter().map(|c| revalue(c, operands, next)).collect())
        },
        WhereClause::Not(c) => WhereClause::Not(Box::new(revalue(c, operands, next))),
        other => other.clone(),
    }
}

// ============================================================================
// PostgresWhereGenerator: Parameterization Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Property: the SQL text does not depend on any operand value.
    ///
    /// The same tree carrying two independent sets of hostile operands (quotes, backslashes,
    /// dollar quotes, comment syntax, NUL, any Unicode, any number) renders byte-identical
    /// SQL, with one parameter per operand either way. A value that reached the statement
    /// text, escaped or not, would make the two differ. No refusal is expected: every
    /// substitute has its original's JSON shape.
    #[test]
    fn prop_postgres_sql_text_independent_of_operand_values(
        clause in arb_where_clause(),
        a in arb_operands(),
        b in arb_operands(),
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let (with_a, with_b) = (revalue(&clause, &a, &mut 0), revalue(&clause, &b, &mut 0));
        let (sql_a, params_a) = gen.generate(&with_a).unwrap();
        let (sql_b, params_b) = gen.generate(&with_b).unwrap();
        prop_assert_eq!(&sql_a, &sql_b, "the operands changed the statement");
        prop_assert_eq!(params_a.len(), params_b.len());
    }

    /// Property: the placeholders are exactly `$1`..`$n`, one per parameter.
    ///
    /// Every generated tree renders (none is refused: their operands fit their operators),
    /// and its statement names each of `$1`..`$n` and no `$n+1`, where `n` is the number of
    /// parameters: so `in` binds one per element and a null test binds none.
    #[test]
    fn prop_postgres_placeholders_match_params(clause in arb_hostile_where_clause()) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let (sql, params) = gen.generate(&clause).unwrap();
        prop_assert_eq!(
            count_placeholders(&sql), params.len(),
            "placeholders and parameters disagree\nSQL: {}", sql
        );
        prop_assert!(!sql.contains(&format!("${}", params.len() + 1)), "SQL: {}", sql);
    }

    /// Property: a key is quote-doubled into its path literal, whatever it holds.
    ///
    /// Keys come from the compiled schema, not the client, but the generator must still
    /// keep any key inside its literal: quotes, backslashes and any Unicode included.
    #[test]
    fn prop_postgres_path_segment_quoted(
        segment in arb_hostile_string().prop_filter("a key is not empty", |s| !s.is_empty()),
        operator in arb_comparison_operator(),
    ) {
        let gen = PostgresWhereGenerator::new(PostgresDialect);
        let clause = WhereClause::Field {
            path: vec![segment.clone()],
            operator,
            value: json!("v"),
        };
        let (sql, params) = gen.generate(&clause).unwrap();
        let quoted = format!("'{}'", segment.replace('\'', "''"));
        prop_assert!(sql.contains(&quoted), "want {} in {}", quoted, sql);
        prop_assert_eq!(params, vec![json!("v")]);
    }

    /// Property: a generator that has already rendered a tree renders the next one as a
    /// fresh generator does: its parameter numbering restarts.
    #[test]
    fn prop_postgres_generator_reusable(
        first in arb_hostile_where_clause(),
        second in arb_hostile_where_clause(),
    ) {
        let used = PostgresWhereGenerator::new(PostgresDialect);
        used.generate(&first).unwrap();
        let fresh = PostgresWhereGenerator::new(PostgresDialect);
        prop_assert_eq!(used.generate(&second).unwrap(), fresh.generate(&second).unwrap());
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
            // Reason: non_exhaustive requires catch-all for cross-crate matches
            _ => {}
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
    ///
    /// "Known" is read from `WHERE_OPERATORS`, the table #828 made the single
    /// vocabulary — not from a list maintained here. The hand-maintained version
    /// went stale the moment that table gained `ne` as an alias for `neq`: the
    /// generator draws `[a-z]{1,10}`, so it eventually produced `"ne"`, found it
    /// accepted, and failed. Probabilistically, which is why it passed CI on the
    /// commit that broke it.
    #[test]
    fn prop_operator_rejects_unknown(
        name in "[a-z]{1,10}",
    ) {
        prop_assume!(fraiseql_core::db::where_clause::operator_spec(&name).is_none());

        let result = WhereOperator::from_str(&name);
        prop_assert!(result.is_err(), "Unknown operator '{}' should be rejected", name);
    }

    /// Property: every name the table advertises parses.
    ///
    /// The other direction of the same invariant, and the reason the assumption
    /// above cannot be vacuous: if `operator_spec` ever returned `Some` for
    /// everything, this test would still have to find each advertised name
    /// runnable.
    #[test]
    fn prop_every_advertised_operator_parses(
        idx in 0usize..fraiseql_core::db::where_clause::WHERE_OPERATORS.len(),
    ) {
        let spec = &fraiseql_core::db::where_clause::WHERE_OPERATORS[idx];
        for advertised in spec.all_names() {
            prop_assert!(
                WhereOperator::from_str(advertised).is_ok(),
                "advertised operator '{}' does not parse", advertised
            );
        }
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

        let generator = PostgresWhereGenerator::new(PostgresDialect);
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

        let gen1 = PostgresWhereGenerator::new(PostgresDialect);
        let gen2 = PostgresWhereGenerator::new(PostgresDialect);

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
