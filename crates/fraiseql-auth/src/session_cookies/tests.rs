#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};

use super::{SessionCookies, refresh_token_cookie, session_response};
use crate::session::TokenPair;

fn pair() -> TokenPair {
    TokenPair {
        access_token:  "header.payload.signature".to_string(),
        refresh_token: "opaque-refresh_token".to_string(),
        expires_in:    900,
    }
}

const COOKIES: SessionCookies = SessionCookies {
    refresh_max_age_secs: 86_400,
};

async fn body_of(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn the_cookies_carry_the_pair_with_their_attributes() {
    let [access, refresh] = COOKIES.set(&pair()).unwrap();
    assert_eq!(
        access,
        "__Host-access_token=header.payload.signature; Path=/; HttpOnly; Secure; \
         SameSite=Strict; Max-Age=900"
    );
    assert_eq!(
        refresh,
        "__Secure-refresh_token=opaque-refresh_token; Path=/auth/v1/session; HttpOnly; \
         Secure; SameSite=Strict; Max-Age=86400"
    );
}

#[test]
fn a_token_no_cookie_can_carry_sets_nothing() {
    for bad in ["", "a;b", "a b", "a\"b", "a,b", "a\\b"] {
        let mut pair = pair();
        pair.refresh_token = bad.to_string();
        assert!(COOKIES.set(&pair).is_none(), "{bad:?}");
    }
}

#[tokio::test]
async fn in_json_transport_the_body_is_the_pair() {
    let response = session_response(None, pair());
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get(header::SET_COOKIE).is_none());
    assert_eq!(
        body_of(response).await,
        r#"{"access_token":"header.payload.signature","refresh_token":"opaque-refresh_token","expires_in":900}"#
    );
}

#[tokio::test]
async fn in_cookie_transport_the_body_carries_no_token() {
    let response = session_response(Some(&COOKIES), pair());
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get_all(header::SET_COOKIE).iter().count(), 2);
    assert_eq!(body_of(response).await, r#"{"expires_in":900}"#);
}

#[tokio::test]
async fn an_unsendable_token_is_a_500_not_a_body() {
    let mut pair = pair();
    pair.access_token = "a;b".to_string();
    let response = session_response(Some(&COOKIES), pair);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(response.headers().get(header::SET_COOKIE).is_none());
    assert!(!body_of(response).await.contains("opaque-refresh_token"));
}

#[test]
fn the_refresh_cookie_is_read_from_any_cookie_header() {
    let mut headers = HeaderMap::new();
    headers.append(header::COOKIE, HeaderValue::from_static("a=1"));
    headers.append(
        header::COOKIE,
        HeaderValue::from_static("__Host-access_token=x; __Secure-refresh_token=tok"),
    );
    assert_eq!(refresh_token_cookie(&headers).as_deref(), Some("tok"));
}

#[test]
fn an_absent_or_empty_refresh_cookie_is_none() {
    let mut headers = HeaderMap::new();
    assert_eq!(refresh_token_cookie(&headers), None);
    headers.insert(header::COOKIE, HeaderValue::from_static("__Secure-refresh_token="));
    assert_eq!(refresh_token_cookie(&headers), None);
    headers.insert(header::COOKIE, HeaderValue::from_static("x__Secure-refresh_token=tok"));
    assert_eq!(refresh_token_cookie(&headers), None, "a longer name is another cookie");
}

#[test]
fn clearing_empties_both_cookies_at_their_paths() {
    let [access, refresh] = SessionCookies::clear();
    assert_eq!(
        access,
        "__Host-access_token=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0"
    );
    assert_eq!(
        refresh,
        "__Secure-refresh_token=; Path=/auth/v1/session; HttpOnly; Secure; SameSite=Strict; \
         Max-Age=0"
    );
}
