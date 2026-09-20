#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

#[test]
fn test_adapter_creation() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test");
    assert_eq!(adapter.database_type(), DatabaseType::PostgreSQL);
    assert_eq!(adapter.chunk_size, 1024);
}

#[test]
fn test_adapter_with_chunk_size() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test").with_chunk_size(512);
    assert_eq!(adapter.chunk_size, 512);
}

#[test]
fn test_pool_metrics() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test");
    let metrics = adapter.pool_metrics();
    assert_eq!(metrics.total_connections, 0);
    assert_eq!(metrics.idle_connections, 0);
    assert_eq!(metrics.active_connections, 0);
    assert_eq!(metrics.waiting_requests, 0);
}

/// #1284: this backend refuses a relevance ordering rather than escaping the
/// search text into its raw SQL.
///
/// It assembles `ORDER BY` as a bare string — there is no bind parameter on
/// this path, which is why `WhereSqlGenerator` escapes values and documents
/// itself as not for new code. Interpolating a client's search string into an
/// `ORDER BY` is precisely what the parameter channel exists to prevent, so the
/// refusal is named. It is also consistent with the WHERE side, which already
/// refuses every full-text operator — so a `?search=` request cannot reach here.
#[test]
fn a_relevance_ordering_is_refused_rather_than_escaped() {
    let clause = OrderByClause::by_relevance(crate::types::sql_hints::RelevanceOrder {
        fields: vec!["label".to_string()],
        query:  "'; DROP TABLE users; --".to_string(),
    });
    let err = order_by_columns(Some(&[clause])).unwrap_err().to_string();
    assert!(err.contains("cannot supply"), "got: {err}");
    assert!(!err.contains("DROP TABLE"), "the refusal must not echo the search text: {err}");

    // An ordinary ordering still renders — the refusal is about the parameter,
    // not about ordering on this backend.
    //
    // The identity is appended because this adapter's reads are paged: it slices
    // LIMIT/OFFSET in memory, but each call re-runs the query, so two consecutive
    // pages are two orderings of the same tied rows unless the order is total
    // (#1287). `created_at` is not unique, so the tie-breaker is added.
    let plain = OrderByClause::new("createdAt".to_string(), crate::OrderDirection::Desc);
    assert_eq!(
        order_by_columns(Some(&[plain])).unwrap().unwrap(),
        "data->>'created_at' DESC, data->>'id' ASC"
    );
}

/// The runtime half of the mutation gate must refuse this adapter.
///
/// `DatabaseAdapter::supports_mutations` calls itself "**the authoritative mutation
/// gate**" and instructs read-only adapters to override it, naming this one. It never
/// did, so the gate answered `true` — the permissive default — for the only read-only
/// adapter in the tree, and the refusal came instead from the trait's default
/// `execute_function_call`, at the far end of the whole write pipeline.
///
/// The compile-time `SupportsMutations` bound (which this adapter deliberately does not
/// implement) is what actually kept it out of the write entries. This assertion pins the
/// runtime layer so the two agree, rather than one standing alone.
#[test]
fn wire_adapter_refuses_mutations_at_the_runtime_gate() {
    let adapter = FraiseWireAdapter::new("postgres://localhost/test");
    assert!(
        !adapter.supports_mutations(),
        "FraiseWireAdapter has no write path — its `execute_function_call` is the trait \
         default, which refuses. The authoritative runtime gate must say so too."
    );
}
