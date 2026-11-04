//! Sessions delivered in `HttpOnly` cookies (`[auth.local] session_transport = "cookie"`).
//!
//! In cookie transport no response body carries a token, so a script running in the page
//! cannot read one:
//!
//! - the access token is in [`ACCESS_COOKIE`], `Path=/`, which the server's session layer reads on
//!   the routes that refuse a non-`JSON` body;
//! - the refresh token is in [`REFRESH_COOKIE`], scoped to [`REFRESH_COOKIE_PATH`], so the browser
//!   sends it to the refresh and logout routes and nowhere else.
//!
//! Both are `HttpOnly; Secure; SameSite=Strict`. The access cookie's `__Host-` prefix also
//! forbids a `Domain`, so no other host on the same site can set or receive it. The refresh cookie
//! cannot carry that prefix, which requires `Path=/`; `__Secure-` still requires `Secure`.
use axum::{
    Json,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::session::TokenPair;

/// The cookie holding the access token. The same name the `PKCE` callback sets.
pub const ACCESS_COOKIE: &str = "__Host-access_token";
/// The cookie holding the refresh token.
pub const REFRESH_COOKIE: &str = "__Secure-refresh_token";
/// The only path the refresh cookie is sent to: the refresh and logout routes.
pub const REFRESH_COOKIE_PATH: &str = "/auth/v1/session";

/// Cookie transport's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionCookies {
    /// The refresh cookie's `Max-Age`: `[auth.session] absolute_ttl_secs`. The server
    /// refuses a refresh past that limit whatever the cookie says.
    pub refresh_max_age_secs: u64,
}

impl SessionCookies {
    /// The `Set-Cookie` values carrying `pair`, access cookie first.
    ///
    /// `None` when a token holds a byte a cookie value cannot, which no token this crate
    /// mints does: the caller answers 500 rather than send a cookie the browser would cut.
    #[must_use]
    pub fn set(&self, pair: &TokenPair) -> Option<[HeaderValue; 2]> {
        Some([
            cookie(ACCESS_COOKIE, &pair.access_token, "/", pair.expires_in)?,
            cookie(
                REFRESH_COOKIE,
                &pair.refresh_token,
                REFRESH_COOKIE_PATH,
                self.refresh_max_age_secs,
            )?,
        ])
    }

    /// The `Set-Cookie` values emptying both cookies, at the paths they were set on.
    #[must_use]
    pub const fn clear() -> [HeaderValue; 2] {
        [
            HeaderValue::from_static(
                "__Host-access_token=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
            ),
            HeaderValue::from_static(
                "__Secure-refresh_token=; Path=/auth/v1/session; HttpOnly; Secure; \
                 SameSite=Strict; Max-Age=0",
            ),
        ]
    }
}

/// The refresh token in the request's [`REFRESH_COOKIE`], if any.
#[must_use]
pub fn refresh_token_cookie(headers: &HeaderMap) -> Option<String> {
    let prefix = format!("{REFRESH_COOKIE}=");
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|part| part.trim().strip_prefix(prefix.as_str()).map(ToOwned::to_owned))
        .filter(|token| !token.is_empty())
}

/// The 200 answer handing `pair` to the client: the pair as its `JSON` body when
/// `cookies` is `None`, else the two cookies and a body of `expires_in` alone.
#[must_use]
pub fn session_response(cookies: Option<&SessionCookies>, pair: TokenPair) -> Response {
    let Some(cookies) = cookies else {
        return (StatusCode::OK, Json(pair)).into_response();
    };
    let expires_in = pair.expires_in;
    with_cookies(cookies, &pair, serde_json::json!({ "expires_in": expires_in }))
}

/// The 200 answer of cookie transport: `body`, which carries no token, and the cookies
/// holding `pair`.
#[must_use]
pub fn with_cookies(
    cookies: &SessionCookies,
    pair: &TokenPair,
    body: serde_json::Value,
) -> Response {
    let Some([access, refresh]) = cookies.set(pair) else {
        tracing::error!("a minted token holds a byte no cookie value may carry");
        return (StatusCode::INTERNAL_SERVER_ERROR, "session delivery failed").into_response();
    };
    let mut response = (StatusCode::OK, Json(body)).into_response();
    let headers = response.headers_mut();
    headers.append(header::SET_COOKIE, access);
    headers.append(header::SET_COOKIE, refresh);
    response
}

/// One `Set-Cookie` value, or `None` when `value` holds a byte outside RFC 6265's
/// `cookie-octet`.
fn cookie(name: &str, value: &str, path: &str, max_age_secs: u64) -> Option<HeaderValue> {
    let cookie_octet =
        |b: u8| matches!(b, 0x21 | 0x23..=0x2B | 0x2D..=0x3A | 0x3C..=0x5B | 0x5D..=0x7E);
    if value.is_empty() || !value.bytes().all(cookie_octet) {
        return None;
    }
    HeaderValue::from_str(&format!(
        "{name}={value}; Path={path}; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age_secs}"
    ))
    .ok()
}

#[cfg(test)]
mod tests;
