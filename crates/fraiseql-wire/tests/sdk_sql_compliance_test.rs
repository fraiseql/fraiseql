#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! SDK cross-compliance harness for the fraiseql-wire SQL operator layer.
//!
//! Verifies that each `WhereOperator` variant produces the correct, well-formed
//! PostgreSQL SQL fragment and correct parameter binding. Tests cover operator
//! categories not addressed by existing unit tests:
//!
//! - Full-text search operators (Matches, `PlainQuery`, `PhraseQuery`, `WebsearchQuery`)
//! - Network / INET operators (`IsIPv6`, `IsPrivate`, `IsPublic`, `InSubnet`, etc.)
//! - String pattern operators (Startswith, Endswith, Icontains, etc.)
//! - Multi-operator parameter chaining (sequential `$N` numbering)
//! - Null equality equivalences
//! - JSONB strict containment
//!
//! **Execution engine:** none (pure SQL generation, no DB)
//! **Infrastructure:** none
//! **Parallelism:** safe

use std::collections::HashMap;

use fraiseql_wire::operators::{Field, Value, WhereOperator};
use fraiseql_wire::operators::sql_gen::generate_where_operator_sql;

// ── Helpers ───────────────────────────────────────────────────────────────────

type ParamMap = HashMap<usize, Value>;

fn gen(op: &WhereOperator) -> (String, usize, ParamMap) {
    let mut idx = 0usize;
    let mut params = HashMap::new();
    let sql = generate_where_operator_sql(op, &mut idx, &mut params).unwrap();
    (sql, idx, params)
}

fn jf(name: &str) -> Field {
    Field::JsonbField(name.to_string())
}

fn dc(name: &str) -> Field {
    Field::DirectColumn(name.to_string())
}

fn param_is_string(params: &ParamMap, key: usize, expected: &str) {
    match &params[&key] {
        Value::String(s) => assert_eq!(s, expected, "param ${key} must equal {expected:?}"),
        other => panic!("param ${key} must be String, got {other:?}"),
    }
}

fn param_is_number(params: &ParamMap, key: usize, expected: f64) {
    match &params[&key] {
        Value::Number(n) => assert!(
            (n - expected).abs() < f64::EPSILON,
            "param ${key} must equal {expected}"
        ),
        other => panic!("param ${key} must be Number, got {other:?}"),
    }
}

// ── String pattern operators ─────────────────────────────────────

/// `Startswith` on a `JSONB` field produces `LIKE prefix%`.
#[test]
fn startswith_jsonb_generates_like_prefix() {
    let op = WhereOperator::Startswith(jf("name"), "Jo".to_string());
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "(data->'name') LIKE $1");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "Jo%");
}

/// `Istartswith` on a direct column produces `ILIKE prefix%` (case-insensitive).
#[test]
fn istartswith_direct_column_generates_ilike_prefix() {
    let op = WhereOperator::Istartswith(dc("email"), "admin".to_string());
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "email ILIKE $1");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "admin%");
}

/// `Endswith` on a `JSONB` field produces `LIKE %suffix`.
#[test]
fn endswith_jsonb_generates_like_suffix() {
    let op = WhereOperator::Endswith(jf("email"), ".com".to_string());
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "(data->'email') LIKE $1");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "%.com");
}

/// `Iendswith` on a direct column produces `ILIKE %suffix`.
#[test]
fn iendswith_direct_column_generates_ilike_suffix() {
    let op = WhereOperator::Iendswith(dc("domain"), ".ORG".to_string());
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "domain ILIKE $1");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "%.ORG");
}

/// `Icontains` produces `ILIKE '%' || $N::text || '%'`.
#[test]
fn icontains_direct_column_generates_ilike_contains() {
    let op = WhereOperator::Icontains(dc("bio"), "rust".to_string());
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "bio ILIKE '%' || $1::text || '%'");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "rust");
}

/// `Like` on a `JSONB` field passes the pattern verbatim.
#[test]
fn like_jsonb_passes_pattern_verbatim() {
    let op = WhereOperator::Like(jf("code"), "ABC-%".to_string());
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "(data->'code') LIKE $1");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "ABC-%");
}

/// `Ilike` on a direct column uses `ILIKE`.
#[test]
fn ilike_direct_column_case_insensitive() {
    let op = WhereOperator::Ilike(dc("title"), "%rust%".to_string());
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "title ILIKE $1");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "%rust%");
}

// ── Full-text search operators ───────────────────────────────────

/// `Matches` generates `@@ plainto_tsquery('english', $N)` by default.
#[test]
fn matches_generates_plainto_tsquery_english() {
    let op = WhereOperator::Matches {
        field:    dc("body"),
        query:    "rust async".to_string(),
        language: None,
    };
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "body @@ plainto_tsquery('english', $1)");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "rust async");
}

/// `Matches` with an explicit language uses the specified language.
#[test]
fn matches_with_language_uses_specified_language() {
    let op = WhereOperator::Matches {
        field:    dc("body"),
        query:    "programmation".to_string(),
        language: Some("french".to_string()),
    };
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "body @@ plainto_tsquery('french', $1)");
    assert_eq!(idx, 1);
}

/// `PhraseQuery` generates `@@ phraseto_tsquery(...)`.
#[test]
fn phrase_query_generates_phraseto_tsquery() {
    let op = WhereOperator::PhraseQuery {
        field:    dc("content"),
        query:    "database design".to_string(),
        language: None,
    };
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "content @@ phraseto_tsquery('english', $1)");
    assert_eq!(idx, 1);
}

/// `PhraseQuery` with an explicit language passes it through.
#[test]
fn phrase_query_with_explicit_language() {
    let op = WhereOperator::PhraseQuery {
        field:    dc("content"),
        query:    "hola mundo".to_string(),
        language: Some("spanish".to_string()),
    };
    let (sql, _, _) = gen(&op);
    assert_eq!(sql, "content @@ phraseto_tsquery('spanish', $1)");
}

/// `WebsearchQuery` generates `@@ websearch_to_tsquery(...)`.
#[test]
fn websearch_query_generates_websearch_to_tsquery() {
    let op = WhereOperator::WebsearchQuery {
        field:    dc("body"),
        query:    "rust OR go -python".to_string(),
        language: None,
    };
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "body @@ websearch_to_tsquery('english', $1)");
    assert_eq!(idx, 1);
}

/// `PlainQuery` generates `plainto_tsquery($N)::tsvector`.
#[test]
fn plain_query_generates_plain_tsquery() {
    let op = WhereOperator::PlainQuery {
        field: dc("tsv"),
        query: "quick brown fox".to_string(),
    };
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "tsv @@ plainto_tsquery($1)::tsvector");
    assert_eq!(idx, 1);
}

// ── Network / INET operators ─────────────────────────────────────

/// `IsIPv6` generates `family(...)::inet = 6`.
#[test]
fn is_ipv6_generates_family_check() {
    let op = WhereOperator::IsIPv6(dc("ip_address"));
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "family(ip_address::inet) = 6");
    assert_eq!(idx, 0);
}

/// `IsLoopback` checks both `IPv4` and `IPv6` loopback ranges.
#[test]
fn is_loopback_generates_dual_stack_check() {
    let op = WhereOperator::IsLoopback(dc("ip"));
    let (sql, idx, _) = gen(&op);
    assert!(sql.contains("127.0.0.0/8"), "IPv4 loopback range must be present");
    assert!(sql.contains("::1/128"), "IPv6 loopback range must be present");
    assert_eq!(idx, 0);
}

/// `IsPrivate` generates `RFC1918` range checks.
#[test]
fn is_private_generates_rfc1918_ranges() {
    let op = WhereOperator::IsPrivate(dc("src_ip"));
    let (sql, idx, _) = gen(&op);
    assert!(sql.contains("10.0.0.0/8"), "10/8 must be checked");
    assert!(sql.contains("172.16.0.0/12"), "172.16/12 must be checked");
    assert!(sql.contains("192.168.0.0/16"), "192.168/16 must be checked");
    assert_eq!(idx, 0);
}

/// `IsPublic` negates the private check with `NOT`.
#[test]
fn is_public_negates_private_ranges() {
    let op = WhereOperator::IsPublic(dc("src_ip"));
    let (sql, idx, _) = gen(&op);
    assert!(sql.starts_with("NOT ("), "IsPublic must start with NOT (");
    assert_eq!(idx, 0);
}

/// `InSubnet` generates `::inet << $N::inet`.
#[test]
fn in_subnet_generates_contained_by_operator() {
    let op = WhereOperator::InSubnet {
        field:  dc("ip"),
        subnet: "192.168.1.0/24".to_string(),
    };
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "ip::inet << $1::inet");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "192.168.1.0/24");
}

/// `ContainsSubnet` generates `::inet >> $N::inet`.
#[test]
fn contains_subnet_generates_contains_operator() {
    let op = WhereOperator::ContainsSubnet {
        field:  dc("network"),
        subnet: "10.0.0.0/8".to_string(),
    };
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "network::inet >> $1::inet");
    assert_eq!(idx, 1);
}

/// `ContainsIP` generates `::inet >> $N::inet`.
#[test]
fn contains_ip_generates_contains_operator() {
    let op = WhereOperator::ContainsIP {
        field: dc("subnet"),
        ip:    "10.0.0.1".to_string(),
    };
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "subnet::inet >> $1::inet");
    assert_eq!(idx, 1);
}

/// `IPRangeOverlap` generates `::inet && $N::inet`.
#[test]
fn ip_range_overlap_generates_overlap_operator() {
    let op = WhereOperator::IPRangeOverlap {
        field: dc("allocated"),
        range: "10.0.0.0/8".to_string(),
    };
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "allocated::inet && $1::inet");
    assert_eq!(idx, 1);
}

// ── Null operators ────────────────────────────────────────────────

/// `Eq(field, Null)` generates `IS NULL` without parameter binding.
#[test]
fn eq_null_generates_is_null_clause() {
    let op = WhereOperator::Eq(dc("deleted_at"), Value::Null);
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "deleted_at IS NULL");
    assert_eq!(idx, 0);
    assert!(params.is_empty());
}

/// `Neq(field, Null)` generates `IS NOT NULL`.
#[test]
fn neq_null_generates_is_not_null_clause() {
    let op = WhereOperator::Neq(dc("deleted_at"), Value::Null);
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "deleted_at IS NOT NULL");
    assert_eq!(idx, 0);
    assert!(params.is_empty());
}

/// `IsNull(field, true)` generates `IS NULL`.
#[test]
fn is_null_true_generates_is_null() {
    let op = WhereOperator::IsNull(dc("archived_at"), true);
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "archived_at IS NULL");
    assert_eq!(idx, 0);
}

/// `IsNull(field, false)` generates `IS NOT NULL`.
#[test]
fn is_null_false_generates_is_not_null() {
    let op = WhereOperator::IsNull(dc("archived_at"), false);
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "archived_at IS NOT NULL");
    assert_eq!(idx, 0);
}

// ── JSONB strict containment ─────────────────────────────────────

/// `StrictlyContains` generates `::jsonb @> $N::jsonb`.
#[test]
fn strictly_contains_generates_jsonb_contains_operator() {
    let op = WhereOperator::StrictlyContains(
        dc("metadata"),
        Value::String(r#"{"role":"admin"}"#.to_string()),
    );
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "metadata::jsonb @> $1::jsonb");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, r#"{"role":"admin"}"#);
}

// ── Multi-operator parameter chaining ────────────────────────────

/// Sequential operator calls share the same `param_index`, producing
/// monotonically increasing `$N` numbers.
#[test]
fn sequential_operators_produce_monotonic_param_numbers() {
    let mut idx = 0usize;
    let mut params = HashMap::new();

    let op1 = WhereOperator::Eq(dc("status"), Value::String("active".to_string()));
    let sql1 = generate_where_operator_sql(&op1, &mut idx, &mut params).unwrap();
    assert_eq!(sql1, "status = $1");
    assert_eq!(idx, 1);

    let op2 = WhereOperator::Gt(dc("age"), Value::Number(18.0));
    let sql2 = generate_where_operator_sql(&op2, &mut idx, &mut params).unwrap();
    assert_eq!(sql2, "age > $2");
    assert_eq!(idx, 2);

    let op3 = WhereOperator::Startswith(dc("name"), "Al".to_string());
    let sql3 = generate_where_operator_sql(&op3, &mut idx, &mut params).unwrap();
    assert_eq!(sql3, "name LIKE $3");
    assert_eq!(idx, 3);

    assert_eq!(params.len(), 3);
    param_is_string(&params, 1, "active");
    param_is_number(&params, 2, 18.0);
    param_is_string(&params, 3, "Al%");
}

/// `In` with N values consumes N consecutive parameter slots.
#[test]
fn in_operator_consumes_n_consecutive_slots() {
    let mut idx = 5usize; // Start mid-sequence to verify offset is preserved
    let mut params = HashMap::new();

    let op = WhereOperator::In(
        dc("category"),
        vec![
            Value::String("A".to_string()),
            Value::String("B".to_string()),
            Value::String("C".to_string()),
        ],
    );
    let sql = generate_where_operator_sql(&op, &mut idx, &mut params).unwrap();
    assert_eq!(sql, "category IN ($6, $7, $8)");
    assert_eq!(idx, 8);
    param_is_string(&params, 6, "A");
    param_is_string(&params, 7, "B");
    param_is_string(&params, 8, "C");
}

/// `Nin` produces `NOT IN` with correct parameter numbers.
#[test]
fn nin_operator_generates_not_in_clause() {
    let op = WhereOperator::Nin(
        dc("role"),
        vec![
            Value::String("guest".to_string()),
            Value::String("banned".to_string()),
        ],
    );
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "role NOT IN ($1, $2)");
    assert_eq!(idx, 2);
    param_is_string(&params, 1, "guest");
    param_is_string(&params, 2, "banned");
}

// ── Array operators ───────────────────────────────────────────────

/// `ArrayContainedBy` generates `field <@ ARRAY[$N]`.
#[test]
fn array_contained_by_generates_contained_by_operator() {
    let op = WhereOperator::ArrayContainedBy(dc("tags"), Value::String("featured".to_string()));
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "tags <@ ARRAY[$1]");
    assert_eq!(idx, 1);
    param_is_string(&params, 1, "featured");
}

/// `ArrayOverlaps` with multiple values generates `field && ARRAY[$1, $2, ...]`.
#[test]
fn array_overlaps_generates_overlap_operator() {
    let op = WhereOperator::ArrayOverlaps(
        dc("permissions"),
        vec![
            Value::String("read".to_string()),
            Value::String("write".to_string()),
        ],
    );
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "permissions && ARRAY[$1, $2]");
    assert_eq!(idx, 2);
    param_is_string(&params, 1, "read");
    param_is_string(&params, 2, "write");
}

// ── JSONB field type casting ─────────────────────────────────────

/// `JSONB` number comparison applies `::numeric` cast.
#[test]
fn jsonb_number_comparison_applies_numeric_cast() {
    let op = WhereOperator::Gt(jf("price"), Value::Number(100.0));
    let (sql, idx, _) = gen(&op);
    assert_eq!(sql, "(data->'price')::numeric > $1");
    assert_eq!(idx, 1);
}

/// `JSONB` boolean comparison applies `::boolean` cast.
#[test]
fn jsonb_boolean_comparison_applies_boolean_cast() {
    let op = WhereOperator::Eq(jf("active"), Value::Bool(true));
    let (sql, _, _) = gen(&op);
    assert_eq!(sql, "(data->'active')::boolean = $1");
}

/// Direct column number comparison does NOT apply any cast.
#[test]
fn direct_column_number_comparison_has_no_cast() {
    let op = WhereOperator::Lte(dc("age"), Value::Number(65.0));
    let (sql, _, _) = gen(&op);
    assert_eq!(sql, "age <= $1");
}

/// `JSONB` null comparison generates `IS NULL` (no cast needed).
#[test]
fn jsonb_null_comparison_generates_is_null_no_cast() {
    let op = WhereOperator::Eq(jf("optional_field"), Value::Null);
    let (sql, idx, params) = gen(&op);
    assert_eq!(sql, "(data->'optional_field') IS NULL");
    assert_eq!(idx, 0);
    assert!(params.is_empty());
}
