//! Annotating a WHERE clause with declared field types changes nothing for the
//! consumers that only walk it.
//!
//! `WhereClause::Typed` carries the compiled-schema types the SQL generator needs
//! for its casts (#798). It travels *inside* the clause rather than alongside it,
//! so no adapter signature can drop it — but that also means every consumer that
//! pattern-matches on `WhereClause` has to recognise it. `WhereClause` is
//! `#[non_exhaustive]`, so a cross-crate `match` compiles with a `_` arm and a
//! consumer can silently take the wrong branch: the subscription filter would
//! refuse every filtered subscription, the partial-period optimiser would stop
//! firing, the aggregation planner would error.
//!
//! The compiler cannot catch that. This suite drives each consumer with the same
//! clause twice — bare and annotated — and asserts the answers match.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_core::db::{
    ScalarFieldType, WhereClause, WhereOperator,
    where_clause::{FieldTypeMap, SharedFieldTypes},
};
use serde_json::json;

fn types() -> SharedFieldTypes {
    Arc::new(FieldTypeMap::from_pairs([
        ("tenant_id", ScalarFieldType::Text),
        ("period_start", ScalarFieldType::Date),
        ("amount", ScalarFieldType::Numeric),
    ]))
}

fn field(name: &str, operator: WhereOperator, value: serde_json::Value) -> WhereClause {
    WhereClause::Field {
        path: vec![name.to_string()],
        operator,
        value,
    }
}

/// `is_empty` looks through the annotation.
#[test]
fn is_empty_looks_through_the_annotation() {
    let empty = WhereClause::And(vec![]);
    assert!(empty.typed(types()).is_empty(), "an annotated empty clause is still empty");

    let non_empty = field("tenant_id", WhereOperator::Eq, json!("acme"));
    assert!(
        !non_empty.typed(types()).is_empty(),
        "an annotated non-empty clause is still non-empty"
    );
}

/// `native_column_names` collects through the annotation.
#[test]
fn native_column_collection_looks_through_the_annotation() {
    let clause = WhereClause::NativeField {
        column:   "pk_user".to_string(),
        pg_cast:  "uuid".to_string(),
        operator: WhereOperator::Eq,
        value:    json!("00000000-0000-0000-0000-000000000000"),
    };
    assert_eq!(clause.native_column_names(), vec!["pk_user"]);
    let annotated = clause.typed(types());
    assert_eq!(annotated.native_column_names(), vec!["pk_user"]);
}

/// The subscription row-visibility filter accepts an annotated clause.
///
/// Its catch-all arm *refuses* the subscription rather than delivering
/// unfiltered rows (#596), which is right for `Or`/`Not` and wrong for an
/// annotation — falling through would refuse every subscription that carries a
/// user filter.
#[test]
fn the_subscription_filter_accepts_an_annotated_clause() {
    use fraiseql_core::runtime::subscription::extract_rls_conditions;

    let clause = WhereClause::And(vec![
        field("tenant_id", WhereOperator::Eq, json!("acme")),
        field("amount", WhereOperator::Eq, json!(10)),
    ]);
    let bare = extract_rls_conditions(&clause);
    let annotated = extract_rls_conditions(&clause.typed(types()));
    assert_eq!(
        bare, annotated,
        "annotating a row-visibility clause must not change which conditions are enforceable"
    );
    assert!(bare.is_ok(), "the bare clause must be enforceable to begin with: {bare:?}");
}

/// The partial-period optimiser sees the same lower bound through the annotation.
#[test]
fn the_partial_period_optimiser_looks_through_the_annotation() {
    use fraiseql_core::runtime::partial_period::{extract_lower_date_bound, split_where_clause};

    let clause = WhereClause::And(vec![
        field("period_start", WhereOperator::Gte, json!("2024-06-01")),
        field("tenant_id", WhereOperator::Eq, json!("acme")),
    ]);

    assert_eq!(
        extract_lower_date_bound(&clause, "period_start"),
        extract_lower_date_bound(&clause.clone().typed(types()), "period_start"),
        "annotating must not hide the lower bound"
    );
    assert!(
        extract_lower_date_bound(&clause, "period_start").is_some(),
        "the bare clause must expose a lower bound to begin with"
    );

    let bare = split_where_clause(&clause, "period_start").expect("bare split");
    let annotated =
        split_where_clause(&clause.typed(types()), "period_start").expect("annotated split");
    assert_eq!(bare.lower_bound, annotated.lower_bound);
    assert!(
        annotated.remaining.is_some(),
        "the remaining conditions must survive the split, annotated or not"
    );
}

/// The cache key is stable and distinguishes structurally different clauses.
#[test]
fn the_cache_key_is_deterministic_over_an_annotated_clause() {
    use fraiseql_core::cache::generate_view_query_key;

    let annotated = field("tenant_id", WhereOperator::Eq, json!("acme")).typed(types());

    let key = |c: &WhereClause| generate_view_query_key("v_user", Some(c), None, None, None, "v1");
    assert_eq!(key(&annotated), key(&annotated), "the key must be deterministic");

    let other = field("tenant_id", WhereOperator::Eq, json!("other")).typed(types());
    assert_ne!(
        key(&annotated),
        key(&other),
        "two different filters must not collide under the annotation"
    );
}
