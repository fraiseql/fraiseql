#![allow(clippy::unwrap_used)] // Reason: test code

use serde_json::json;

use super::{NullsOrder, SORT_DIRECTIONS, nulls_last, nulls_sql, parse_sort_direction};
use crate::types::{
    OrderByClause,
    OrderDirection::{Asc, Desc},
};

#[test]
fn an_omitted_placement_is_postgresqls_for_the_direction() {
    assert!(nulls_last(Asc, None));
    assert!(!nulls_last(Desc, None));
}

#[test]
fn a_requested_placement_wins_over_the_direction() {
    assert!(nulls_last(Desc, Some(NullsOrder::Last)));
    assert!(!nulls_last(Asc, Some(NullsOrder::First)));
    assert!(nulls_last(Asc, Some(NullsOrder::Last)));
    assert!(!nulls_last(Desc, Some(NullsOrder::First)));
}

#[test]
fn only_a_placement_other_than_the_default_is_rendered() {
    assert_eq!(nulls_sql(Asc, true), "");
    assert_eq!(nulls_sql(Desc, false), "");
    assert_eq!(nulls_sql(Asc, false), " NULLS FIRST");
    assert_eq!(nulls_sql(Desc, true), " NULLS LAST");
}

#[test]
fn each_published_direction_names_its_direction_and_placement() {
    let parsed: Vec<_> = SORT_DIRECTIONS.iter().map(|d| parse_sort_direction(d).unwrap()).collect();
    assert_eq!(
        parsed,
        [
            (Asc, None),
            (Desc, None),
            (Asc, Some(NullsOrder::First)),
            (Asc, Some(NullsOrder::Last)),
            (Desc, Some(NullsOrder::First)),
            (Desc, Some(NullsOrder::Last)),
        ]
    );
}

#[test]
fn any_other_direction_names_nothing() {
    for name in [
        "",
        "asc",
        "UP",
        "ASCDESC",
        "ASC_NULLS",
        "DESC_NULLS_MIDDLE",
        "ASC_NULLS_LAST_",
    ] {
        assert_eq!(parse_sort_direction(name), None, "{name}");
    }
}

#[test]
fn an_order_by_item_and_key_carry_the_placement_their_direction_names() {
    let items = OrderByClause::from_graphql_json(&json!([
        { "field": "discontinuedAt", "direction": "desc_nulls_last" },
        { "field": "name" },
    ]))
    .unwrap();
    assert_eq!((items[0].direction, items[0].nulls), (Desc, Some(NullsOrder::Last)));
    assert_eq!((items[1].direction, items[1].nulls), (Asc, None));
    let keys =
        OrderByClause::from_graphql_json(&json!({ "discontinuedAt": "ASC_NULLS_FIRST" })).unwrap();
    assert_eq!((keys[0].direction, keys[0].nulls), (Asc, Some(NullsOrder::First)));
}

#[test]
fn an_order_by_direction_outside_the_six_is_refused_listing_them() {
    for value in [
        json!([{ "field": "name", "direction": "DESC_NULLS" }]),
        json!({ "name": "UP" }),
    ] {
        let message = OrderByClause::from_graphql_json(&value).unwrap_err().to_string();
        assert!(
            message.contains(
                "must be one of `ASC`, `DESC`, `ASC_NULLS_FIRST`, `ASC_NULLS_LAST`, \
                              `DESC_NULLS_FIRST`, `DESC_NULLS_LAST`"
            ),
            "{message}"
        );
    }
}
