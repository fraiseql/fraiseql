//! The result-cache key tells two NULL placements of one ordering apart.

use super::generate_view_query_key;
use crate::backend::{NullsOrder, OrderByClause, OrderDirection};

fn key(nulls: Option<NullsOrder>, direction: OrderDirection) -> u64 {
    let mut clause = OrderByClause::new("discontinued_at".into(), direction);
    clause.nulls = nulls;
    generate_view_query_key("v_product", None, None, None, Some(&[clause]), &[], None, "v1")
}

#[test]
fn nulls_first_and_nulls_last_are_two_cached_pages() {
    assert_ne!(
        key(Some(NullsOrder::First), OrderDirection::Desc),
        key(Some(NullsOrder::Last), OrderDirection::Desc)
    );
}

#[test]
fn the_directions_default_spelled_out_is_the_page_that_omits_it() {
    assert_eq!(key(Some(NullsOrder::Last), OrderDirection::Asc), key(None, OrderDirection::Asc));
    assert_eq!(
        key(Some(NullsOrder::First), OrderDirection::Desc),
        key(None, OrderDirection::Desc)
    );
}
