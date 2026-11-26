//! The cookie that ties a social, SAML or `OIDC` login to the browser that started it.
//!
//! A login's `state` (social, `OIDC`) or `RelayState` (SAML) is kept server-side and comes back
//! through the provider; by itself it does not say which browser brings it back. So
//! starting a login also sets a cookie holding a random value, and the stored state keeps
//! that value's SHA-256. The callback or the ACS completes only a state whose cookie is
//! present and matches, and clears the cookie on every answer, so it is single-use.
//!
//! The cookie's name carries a prefix of the state's own digest, so two logins started in
//! one browser at once each keep their cookie. It is `HttpOnly; Secure` and `__Host-`
//! prefixed. Its `SameSite` follows how the provider comes back: `Lax` is sent on the
//! provider's top-level `GET` redirect, and a cross-site form `POST` (Apple's `form_post`,
//! the SAML ACS) needs `None`. The cookie holds no credential: only proof of which browser
//! started the login.

use axum::http::{HeaderMap, HeaderValue, header};
use sha2::{Digest as _, Sha256};

use crate::constant_time::ConstantTimeOps;

/// How the provider brings the browser back, which sets the cookie's `SameSite`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnsBy {
    /// A top-level `GET` redirect: `SameSite=Lax`.
    Redirect,
    /// A cross-site form `POST`: `SameSite=None`.
    FormPost,
}

impl ReturnsBy {
    const fn same_site(self) -> &'static str {
        match self {
            Self::Redirect => "Lax",
            Self::FormPost => "None",
        }
    }
}

/// A login just started: the cookie to set, and the digest to store with the state.
#[derive(Debug)]
pub struct Started {
    /// The `Set-Cookie` value.
    pub cookie: HeaderValue,
    /// The SHA-256 of the cookie's value, hex: what the stored state keeps.
    pub digest: String,
}

/// Start the binding of the login `state`, whose cookie lives `max_age_secs`.
#[must_use]
pub fn start(state: &str, returns_by: ReturnsBy, max_age_secs: u64) -> Started {
    let pending = Pending::new();
    let cookie = pending.cookie(state, returns_by, max_age_secs);
    Started {
        cookie,
        digest: pending.digest,
    }
}

/// A binding whose state is not yet known: for a store that mints the state as it stores
/// the digest. Its cookie is written once the state is.
#[derive(Debug)]
pub struct Pending {
    value:      String,
    /// The SHA-256 of the cookie's value, hex: what the stored state keeps.
    pub digest: String,
}

impl Pending {
    /// A new random value, and its digest.
    #[must_use]
    pub fn new() -> Self {
        let value = crate::handlers::generate_secure_state();
        let digest = hex::encode(Sha256::digest(value.as_bytes()));
        Self { value, digest }
    }

    /// The `Set-Cookie` value binding the login `state` to this value.
    #[must_use]
    pub fn cookie(&self, state: &str, returns_by: ReturnsBy, max_age_secs: u64) -> HeaderValue {
        header_value(&cookie_name(state), &self.value, returns_by, max_age_secs)
    }
}

impl Default for Pending {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether the request carries the cookie of the login `state`, and its value hashes to
/// `digest`.
#[must_use]
pub fn matches(headers: &HeaderMap, state: &str, digest: &str) -> bool {
    let prefix = format!("{}=", cookie_name(state));
    let Some(value) = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|part| part.trim().strip_prefix(prefix.as_str()))
    else {
        return false;
    };
    let presented = hex::encode(Sha256::digest(value.as_bytes()));
    ConstantTimeOps::compare_str(digest, &presented)
}

/// The `Set-Cookie` value clearing the cookie of the login `state`.
#[must_use]
pub fn clear(state: &str, returns_by: ReturnsBy) -> HeaderValue {
    header_value(&cookie_name(state), "", returns_by, 0)
}

/// The cookie's name: a fixed prefix and the first 16 hex digits of the state's SHA-256.
fn cookie_name(state: &str) -> String {
    let digest = hex::encode(Sha256::digest(state.as_bytes()));
    format!("__Host-fraiseql_login_{}", &digest[..16])
}

fn header_value(name: &str, value: &str, returns_by: ReturnsBy, max_age_secs: u64) -> HeaderValue {
    // Reason: the name is hex under a fixed prefix and the value is hex (or empty), so the
    // string is always a valid header value.
    HeaderValue::from_str(&format!(
        "{name}={value}; Path=/; HttpOnly; Secure; SameSite={}; Max-Age={max_age_secs}",
        returns_by.same_site()
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("__Host-fraiseql_login_invalid=; Max-Age=0"))
}

#[cfg(test)]
mod tests;
