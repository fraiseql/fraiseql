//! A key's NULL placement through the `ORDER BY`, the keyset order and the signature.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::{keyset_keys, keyset_order, keyset_predicate, keyset_signature};
use crate::{
    order_by::{Tiebreak, render_order_by_columns},
    types::{DatabaseType, NullsOrder, OrderByClause, OrderDirection},
};

fn clause(direction: OrderDirection, nulls: Option<NullsOrder>) -> OrderByClause {
    let mut clause = OrderByClause::new("note".to_string(), direction);
    clause.nulls = nulls;
    clause
}

fn rendered(clause: OrderByClause) -> String {
    render_order_by_columns(Some(&[clause]), DatabaseType::PostgreSQL, 1, Tiebreak::None)
        .unwrap()
        .unwrap()
        .columns
}

#[test]
fn the_order_by_names_a_placement_only_where_it_is_not_the_directions_default() {
    use NullsOrder::{First, Last};
    use OrderDirection::{Asc, Desc};
    assert_eq!(rendered(clause(Desc, Some(Last))), "data->>'note' DESC NULLS LAST");
    assert_eq!(rendered(clause(Asc, Some(First))), "data->>'note' ASC NULLS FIRST");
    for unchanged in [
        clause(Asc, None),
        clause(Asc, Some(Last)),
        clause(Desc, Some(First)),
    ] {
        assert!(!rendered(unchanged).contains("NULLS"));
    }
}

fn order(direction: OrderDirection, nulls: Option<NullsOrder>, forward: bool) -> String {
    let keys = keyset_keys(Some(&[clause(direction, nulls)])).unwrap();
    keyset_order(&keys, "pk", forward)
}

/// A backward page reads the ordering in reverse, its NULLs' placement included: `ASC NULLS
/// FIRST` backward is `DESC NULLS LAST`, which is not PostgreSQL's default for `DESC`.
#[test]
fn a_backward_page_flips_the_placement_with_the_direction() {
    use NullsOrder::{First, Last};
    use OrderDirection::{Asc, Desc};
    assert_eq!(order(Asc, Some(First), true), "data->>'note' ASC NULLS FIRST, pk ASC");
    assert_eq!(order(Asc, Some(First), false), "data->>'note' DESC NULLS LAST, pk DESC");
    assert_eq!(order(Desc, Some(Last), true), "data->>'note' DESC NULLS LAST, pk ASC");
    assert_eq!(order(Desc, Some(Last), false), "data->>'note' ASC NULLS FIRST, pk DESC");
    // The defaults read backward are the other direction's defaults: nothing to name.
    assert_eq!(order(Asc, None, false), "data->>'note' DESC, pk DESC");
    assert_eq!(order(Desc, None, false), "data->>'note' ASC, pk DESC");
}

fn signature(direction: OrderDirection, nulls: Option<NullsOrder>) -> String {
    keyset_signature(&keyset_keys(Some(&[clause(direction, nulls)])).unwrap())
}

/// The signature a cursor's fingerprint hashes: an omitted placement and the direction's
/// default are one ordering (so cursors issued before `nulls` existed resume), and the two
/// placements are two.
#[test]
fn the_signature_tells_the_placements_apart_and_the_default_from_nothing_not() {
    use NullsOrder::{First, Last};
    use OrderDirection::{Asc, Desc};
    assert_eq!(signature(Asc, None), "data->>'note' ASC");
    assert_eq!(signature(Asc, Some(Last)), signature(Asc, None));
    assert_eq!(signature(Desc, Some(First)), signature(Desc, None));
    assert_ne!(signature(Asc, Some(First)), signature(Asc, Some(Last)));
    assert_ne!(signature(Desc, Some(First)), signature(Desc, Some(Last)));
}

fn predicate(
    direction: OrderDirection,
    nulls: NullsOrder,
    bound: Option<&str>,
    forward: bool,
) -> String {
    let keys = keyset_keys(Some(&[clause(direction, Some(nulls))])).unwrap();
    keyset_predicate(&keys, &[bound.map(str::to_string)], "pk", |p| p.to_string(), forward, 1)
        .unwrap()
        .0
}

/// The NULLs are past a value only when the page meets them after the values: `ASC NULLS
/// FIRST` puts them before every value read forward and after every value read backward.
/// From a NULL, every value is past it when the NULLs come first in the read, and none when
/// they come last.
#[test]
fn the_null_branches_follow_an_explicit_placement_both_ways() {
    use NullsOrder::{First, Last};
    use OrderDirection::{Asc, Desc};
    let k = "data->>'note'";
    assert_eq!(
        predicate(Asc, First, Some("a"), true),
        format!("({k} > $1::text OR ({k} = $1::text AND pk > $2))")
    );
    assert_eq!(
        predicate(Asc, First, None, true),
        format!("({k} IS NOT NULL OR ({k} IS NULL AND pk > $1))")
    );
    assert_eq!(
        predicate(Asc, First, Some("a"), false),
        format!("(({k} < $1::text OR {k} IS NULL) OR ({k} = $1::text AND pk < $2))")
    );
    assert_eq!(predicate(Asc, First, None, false), format!("(({k} IS NULL AND pk < $1))"));
    assert_eq!(
        predicate(Desc, Last, Some("a"), true),
        format!("(({k} < $1::text OR {k} IS NULL) OR ({k} = $1::text AND pk > $2))")
    );
    assert_eq!(predicate(Desc, Last, None, true), format!("(({k} IS NULL AND pk > $1))"));
    assert_eq!(
        predicate(Desc, Last, Some("a"), false),
        format!("({k} > $1::text OR ({k} = $1::text AND pk < $2))")
    );
    assert_eq!(
        predicate(Desc, Last, None, false),
        format!("({k} IS NOT NULL OR ({k} IS NULL AND pk < $1))")
    );
}
