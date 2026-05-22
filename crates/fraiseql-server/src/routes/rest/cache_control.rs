//! HTTP caching headers for the REST transport.
//!
//! Sets `Cache-Control`, `Vary`, and related headers on REST responses:
//! - GET requests: `Cache-Control: public|private, max-age={ttl}` with `Vary`
//! - Mutating requests: `Cache-Control: no-store`
//!
//! The `private` directive is used when the request includes an `Authorization`
//! header (response varies by user), `public` otherwise.

use std::fmt::Write;

use axum::http::{HeaderMap, HeaderValue};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Context for computing cache headers on a REST response.
pub struct CacheContext {
    /// Whether this is a GET request.
    pub is_get:      bool,
    /// Whether the request included an `Authorization` header.
    pub has_auth:    bool,
    /// Per-query cache TTL override (from `QueryDefinition.cache_ttl_seconds`).
    pub query_ttl:   Option<u64>,
    /// Default TTL from `RestConfig.default_cache_ttl`.
    pub default_ttl: u64,
    /// CDN/shared-cache TTL (`s-maxage`). Only emitted on public GET responses.
    pub cdn_max_age: Option<u64>,
}

/// Apply `Cache-Control` and `Vary` headers to a response header map.
///
/// For GET requests, sets `Cache-Control: public|private, max-age={ttl}` and
/// `Vary: Authorization, Accept, Prefer`.
///
/// For mutating requests (POST, PUT, PATCH, DELETE), sets
/// `Cache-Control: no-store`.
pub fn apply_cache_headers(headers: &mut HeaderMap, ctx: &CacheContext) {
    if ctx.is_get {
        let max_age = ctx.query_ttl.unwrap_or(ctx.default_ttl);
        let visibility = if ctx.has_auth { "private" } else { "public" };
        let mut value = format!("{visibility}, max-age={max_age}");

        // s-maxage only on public responses — CDNs ignore private responses,
        // but omitting it is cleaner and avoids confusion.
        if !ctx.has_auth {
            if let Some(s_maxage) = ctx.cdn_max_age {
                write!(value, ", s-maxage={s_maxage}").expect("write to String");
            }
        }

        if let Ok(val) = HeaderValue::from_str(&value) {
            headers.insert("cache-control", val);
        }
        headers.insert("vary", HeaderValue::from_static("Authorization, Accept, Prefer"));
    } else {
        headers.insert("cache-control", HeaderValue::from_static("no-store"));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
