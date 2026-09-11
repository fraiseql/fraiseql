#![allow(clippy::panic)] // Reason: test code, a panic is the failure mechanism
//! Tests for the pagination index advice (#1307).
//!
//! No database: the catalog reads live in `fraiseql_db` and the rule that decides
//! what counts as advice is pure, so it is pinned here — in the crate's `--lib`
//! tests, which is what the required leg runs.

use fraiseql_core::schema::{AutoParams, PaginationOrder, QueryDefinition};
use fraiseql_db::postgres::IndexInfo;

use super::{Advice, advise, tie_break_key};

fn index(name: &str, keys: &[&str]) -> IndexInfo {
    IndexInfo {
        name:   name.to_string(),
        unique: false,
        keys:   keys.iter().map(|k| (*k).to_string()).collect(),
    }
}

/// A paginating list query ordered by a native column, sortable on `status`.
fn invoices() -> QueryDefinition {
    let mut q = QueryDefinition::new("invoices", "Invoice");
    q.sql_source = Some("v_invoice".to_string());
    q.pagination_order = Some(PaginationOrder::Column("pk_invoice".to_string()));
    q.auto_params = AutoParams {
        has_order_by: true,
        has_limit: true,
        ..AutoParams::default()
    };
    q.native_columns.insert("status".to_string(), "status".to_string());
    q
}

#[test]
fn a_query_with_no_page_order_has_nothing_to_advise() {
    let mut q = invoices();
    q.pagination_order = None;
    assert_eq!(tie_break_key(&q), None);
    assert!(advise(&q, "tb_invoice", &[index("ix", &["status"])]).is_empty());
}

/// The larger cost: nothing leads with the tie-break, so every page sorts the
/// whole relation.
#[test]
fn an_unindexed_tie_break_is_reported_with_its_ddl() {
    let found = advise(&invoices(), "tb_invoice", &[]);
    let first = found.first().expect("an unindexed tie-break is advice");
    assert_eq!(
        first.advice,
        Advice::TieBreakUnindexed {
            tie_break: "pk_invoice".to_string(),
        }
    );
    assert_eq!(first.ddl, vec!["CREATE INDEX ON tb_invoice (pk_invoice);"]);
}

/// An index that *contains* the tie-break but does not lead with it does not
/// order by it: `(status, pk_invoice)` orders `pk_invoice` only within a `status`.
#[test]
fn a_tie_break_in_a_trailing_position_is_still_unindexed_for_the_default_order() {
    let found = advise(&invoices(), "tb_invoice", &[index("ix", &["status", "pk_invoice"])]);
    assert!(
        found.iter().any(|a| matches!(a.advice, Advice::TieBreakUnindexed { .. })),
        "a trailing tie-break does not serve the default ordering: {found:?}"
    );
}

/// The #1307 case itself: the sort key is indexed alone, so the tie-break turns
/// an index scan into an incremental sort.
#[test]
fn an_indexed_sort_key_without_the_tie_break_is_reported() {
    let indexes = [
        index("ix_pk", &["pk_invoice"]),
        index("ix_status", &["status"]),
    ];
    let found = advise(&invoices(), "tb_invoice", &indexes);

    let composite = found
        .iter()
        .find(|a| matches!(a.advice, Advice::MissingComposite { .. }))
        .expect("an indexed sort key with no composite is advice");
    assert_eq!(
        composite.advice,
        Advice::MissingComposite {
            sort_keys: vec!["status".to_string()],
            tie_break: "pk_invoice".to_string(),
        }
    );
    assert_eq!(composite.ddl, vec!["CREATE INDEX ON tb_invoice (status, pk_invoice);"]);
    assert!(
        !found.iter().any(|a| matches!(a.advice, Advice::TieBreakUnindexed { .. })),
        "the tie-break leads ix_pk, so it is not unindexed: {found:?}"
    );
}

/// The composite exists — the state the advice asks for produces no advice, which
/// is what makes the report actionable rather than permanent.
#[test]
fn the_composite_index_silences_both_findings() {
    let indexes = [
        index("ix_pk", &["pk_invoice"]),
        index("ix_pair", &["status", "pk_invoice"]),
    ];
    assert_eq!(advise(&invoices(), "tb_invoice", &indexes), Vec::new());
}

/// A sort key that is not indexed at all is already a full sort with or without
/// the tie-break, so a composite is not the remedy and this pass does not claim it.
#[test]
fn an_unindexed_sort_key_is_not_this_passs_finding() {
    let found = advise(&invoices(), "tb_invoice", &[index("ix_pk", &["pk_invoice"])]);
    assert_eq!(found, Vec::new(), "{found:?}");
}

/// Without `orderBy` there is no client ordering to tie-break, so only the default
/// ordering is in question.
#[test]
fn a_query_without_order_by_gets_no_composite_advice() {
    let mut q = invoices();
    q.auto_params.has_order_by = false;
    let found =
        advise(&q, "tb_invoice", &[index("ix_pk", &["pk_invoice"]), index("ix_s", &["status"])]);
    assert!(
        !found.iter().any(|a| matches!(a.advice, Advice::MissingComposite { .. })),
        "{found:?}"
    );
}

// ── The JSONB identity: an expression key, not a column ──────────────────────

fn json_identity_query() -> QueryDefinition {
    let mut q = invoices();
    q.pagination_order = Some(PaginationOrder::JsonIdentity);
    q
}

#[test]
fn the_json_identity_tie_break_is_rendered_as_an_expression() {
    assert_eq!(tie_break_key(&json_identity_query()), Some("(data ->> 'id')".to_string()));
}

/// `pg_get_indexdef` prints the key as `(data ->> 'id'::text)` while the schema
/// knows it as `data->>'id'`. Matching the raw strings would report every
/// expression index as absent and advise creating one that already exists.
#[test]
fn an_existing_expression_index_is_recognised_across_postgres_rendering() {
    let indexes = [index("ix_expr", &["(data ->> 'id'::text)"])];
    let found = advise(&json_identity_query(), "tb_invoice", &indexes);
    assert!(
        !found.iter().any(|a| matches!(a.advice, Advice::TieBreakUnindexed { .. })),
        "the expression index already orders by the identity: {found:?}"
    );
}

#[test]
fn a_missing_expression_index_is_advised_with_parenthesised_ddl() {
    let found = advise(&json_identity_query(), "tb_invoice", &[]);
    let first = found.first().expect("advice");
    assert_eq!(first.ddl, vec!["CREATE INDEX ON tb_invoice (((data ->> 'id')));"]);
}

#[test]
fn a_composite_over_the_json_identity_names_the_expression_second() {
    let found = advise(
        &json_identity_query(),
        "tb_invoice",
        &[
            index("ix_id", &["(data ->> 'id'::text)"]),
            index("ix_status", &["status"]),
        ],
    );
    let composite = found
        .iter()
        .find(|a| matches!(a.advice, Advice::MissingComposite { .. }))
        .expect("advice");
    assert_eq!(composite.ddl, vec!["CREATE INDEX ON tb_invoice (status, ((data ->> 'id')));"]);
}

/// `native_columns` is a `HashMap`, so an unsorted report would differ between
/// runs of the same schema and a reader could not diff two doctor runs.
#[test]
fn the_sort_keys_are_reported_in_a_stable_order() {
    let mut q = invoices();
    for c in ["status", "issued_at", "amount"] {
        q.native_columns.insert(c.to_string(), c.to_string());
    }
    let indexes = [
        index("ix_pk", &["pk_invoice"]),
        index("ix_status", &["status"]),
        index("ix_issued", &["issued_at"]),
        index("ix_amount", &["amount"]),
    ];
    let Some(Advice::MissingComposite { sort_keys, .. }) =
        advise(&q, "tb_invoice", &indexes).into_iter().map(|a| a.advice).next()
    else {
        panic!("expected composite advice");
    };
    assert_eq!(sort_keys, vec!["amount", "issued_at", "status"]);
}
