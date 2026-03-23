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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn get_public_default_ttl() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      true,
                has_auth:    false,
                query_ttl:   None,
                default_ttl: 60,
                cdn_max_age: None,
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=60");
        assert_eq!(headers.get("vary").unwrap().to_str().unwrap(), "Authorization, Accept, Prefer");
    }

    #[test]
    fn get_private_with_auth() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      true,
                has_auth:    true,
                query_ttl:   None,
                default_ttl: 60,
                cdn_max_age: None,
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "private, max-age=60");
    }

    #[test]
    fn get_custom_ttl_from_query() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      true,
                has_auth:    false,
                query_ttl:   Some(120),
                default_ttl: 60,
                cdn_max_age: None,
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=120");
    }

    #[test]
    fn mutation_no_store() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      false,
                has_auth:    false,
                query_ttl:   None,
                default_ttl: 60,
                cdn_max_age: None,
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
        assert!(headers.get("vary").is_none());
    }

    #[test]
    fn mutation_no_store_with_auth() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      false,
                has_auth:    true,
                query_ttl:   None,
                default_ttl: 60,
                cdn_max_age: None,
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
    }

    #[test]
    fn zero_ttl_disables_caching() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      true,
                has_auth:    false,
                query_ttl:   Some(0),
                default_ttl: 60,
                cdn_max_age: None,
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=0");
    }

    #[test]
    fn s_maxage_on_public_get() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      true,
                has_auth:    false,
                query_ttl:   None,
                default_ttl: 60,
                cdn_max_age: Some(300),
            },
        );
        assert_eq!(
            headers.get("cache-control").unwrap().to_str().unwrap(),
            "public, max-age=60, s-maxage=300"
        );
    }

    #[test]
    fn no_s_maxage_on_private_get() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      true,
                has_auth:    true,
                query_ttl:   None,
                default_ttl: 60,
                cdn_max_age: Some(300),
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "private, max-age=60");
    }

    #[test]
    fn no_s_maxage_when_none() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      true,
                has_auth:    false,
                query_ttl:   None,
                default_ttl: 60,
                cdn_max_age: None,
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "public, max-age=60");
    }

    #[test]
    fn no_s_maxage_on_mutations() {
        let mut headers = HeaderMap::new();
        apply_cache_headers(
            &mut headers,
            &CacheContext {
                is_get:      false,
                has_auth:    false,
                query_ttl:   None,
                default_ttl: 60,
                cdn_max_age: Some(300),
            },
        );
        assert_eq!(headers.get("cache-control").unwrap().to_str().unwrap(), "no-store");
    }
}
