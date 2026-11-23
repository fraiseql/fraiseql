use super::demand;

#[test]
#[should_panic(expected = "TVIEWS_UNPRELOADED_DATABASE_URL is not set, but \
                           FRAISEQL_TEST_REQUIRE_TVIEWS_UNPRELOADED is")]
fn an_unset_url_fails_where_it_is_required() {
    let _ = demand(None, true);
}

#[test]
fn an_unset_url_skips_where_it_is_not_required() {
    assert_eq!(demand(None, false), None);
}

#[test]
fn a_set_url_is_returned() {
    let url = "postgresql://localhost:5442/test_fraiseql".to_string();
    assert_eq!(demand(Some(url.clone()), true), Some(url));
}
