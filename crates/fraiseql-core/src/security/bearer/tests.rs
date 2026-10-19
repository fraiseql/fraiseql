//! Tests for `bearer_credential`.

use super::bearer_credential;

#[test]
fn a_bearer_value_yields_its_token() {
    assert_eq!(bearer_credential("Bearer abc.def.ghi"), Some("abc.def.ghi"));
}

#[test]
fn surrounding_whitespace_is_not_the_token() {
    assert_eq!(bearer_credential("Bearer   abc  "), Some("abc"));
}

#[test]
fn an_empty_credential_is_none() {
    assert_eq!(bearer_credential("Bearer "), None);
    assert_eq!(bearer_credential("Bearer    "), None);
}

#[test]
fn another_scheme_is_none() {
    for value in [
        "Basic dXNlcjpwYXNz",
        "Token abc",
        "bearer abc",
        "Bearerabc",
        "",
    ] {
        assert_eq!(bearer_credential(value), None, "{value:?}");
    }
}
