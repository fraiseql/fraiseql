//! `Cache-Control` and `Vary` on `/graphql` responses (#1564).
//!
//! A response is reusable only when nothing about it can belong to someone else. The
//! decision fails closed: anything not shown to be safe to store is `no-store`.
//!
//! | The response | `Cache-Control` |
//! |---|---|
//! | to a `POST` or `QUERY`, or an SSE stream | `no-store` |
//! | carrying `errors`, partial or not | `no-store` |
//! | of an operation not every root of which declares a positive `cache_ttl_seconds` | `no-store` |
//! | to a caller with an identity, a tenant, or headers that select session variables | `private, max-age=<ttl>` |
//! | otherwise | `public, max-age=<ttl>` |
//!
//! "An identity" is any credential the server reads, whether or not it authenticated:
//! a security context, an `Authorization` header, a cookie (the OIDC middleware reads
//! `__Host-access_token`), or an `X-API-Key`. Marking such a response `public` would let a
//! shared cache store it under its URL and hand it to the next caller.
//!
//! `Vary` names every request header that can change the answer: the credentials, the
//! tenant header, and the headers the schema reads a locale or a session variable from.

use axum::http::{HeaderMap, HeaderValue, header};
use fraiseql_core::schema::{CompiledSchema, LocaleSource, SessionVariableSource};

/// Whether, and by whom, a response may be stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::routes::graphql) enum Caching {
    /// Store nothing.
    NoStore,
    /// The caller's own cache only (a browser), for `max_age` seconds.
    Private {
        /// Seconds.
        max_age: u64,
    },
    /// Any cache, shared ones included, for `max_age` seconds.
    Public {
        /// Seconds.
        max_age: u64,
    },
}

/// What a `GET` response's caching is decided from.
pub(in crate::routes::graphql) struct Answered<'a> {
    /// The response body.
    pub body:      &'a serde_json::Value,
    /// [`Executor::response_cache_ttl`](fraiseql_core::runtime::Executor::response_cache_ttl)
    /// for the executed operation.
    pub ttl:       Option<u64>,
    /// Whether the request carried a resolved principal.
    pub principal: bool,
    /// Whether the request was dispatched to a tenant.
    pub tenant:    bool,
}

/// The caching a `GET` response may carry; see the module documentation.
pub(in crate::routes::graphql) fn decide(
    answered: &Answered<'_>,
    headers: &HeaderMap,
    schema: &CompiledSchema,
) -> Caching {
    if answered.body.get("errors").is_some() {
        return Caching::NoStore;
    }
    let Some(max_age) = answered.ttl.filter(|ttl| *ttl > 0) else {
        return Caching::NoStore;
    };
    let credential = [header::AUTHORIZATION, header::COOKIE]
        .iter()
        .any(|name| headers.contains_key(name))
        || headers.contains_key("x-api-key");
    let session_header = schema
        .session_variables
        .variables
        .iter()
        .any(|v| matches!(v.source, SessionVariableSource::Header { .. }));
    if answered.principal
        || credential
        || answered.tenant
        || headers.contains_key("x-tenant-id")
        || session_header
    {
        Caching::Private { max_age }
    } else {
        Caching::Public { max_age }
    }
}

/// Set `Cache-Control` (and, on a reusable response, `Vary`) on `headers`.
pub(in crate::routes::graphql) fn apply(
    headers: &mut HeaderMap,
    caching: Caching,
    schema: Option<&CompiledSchema>,
) {
    let value = match caching {
        Caching::NoStore => {
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            return;
        },
        Caching::Private { max_age } => format!("private, max-age={max_age}"),
        Caching::Public { max_age } => format!("public, max-age={max_age}"),
    };
    if let Ok(value) = HeaderValue::from_str(&value) {
        headers.insert(header::CACHE_CONTROL, value);
    } else {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return;
    }
    if let Ok(vary) = HeaderValue::from_str(&vary(schema)) {
        headers.insert(header::VARY, vary);
    } else {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
}

/// The request headers that can change a response under `schema`.
fn vary(schema: Option<&CompiledSchema>) -> String {
    let mut names = vec![
        "Authorization".to_string(),
        "Cookie".to_string(),
        "X-API-Key".to_string(),
        "X-Tenant-ID".to_string(),
    ];
    if let Some(schema) = schema {
        if let Some(locale) = schema.locale.as_ref() {
            names.extend(locale.resolve.iter().filter_map(|source| match source {
                LocaleSource::Header { header } => Some(header.clone()),
                LocaleSource::Argument { .. } | LocaleSource::Enrichment { .. } => None,
            }));
        }
        names.extend(schema.session_variables.variables.iter().filter_map(|v| match &v.source {
            SessionVariableSource::Header { header } => Some(header.clone()),
            SessionVariableSource::Jwt { .. }
            | SessionVariableSource::Literal { .. }
            | SessionVariableSource::Enrichment { .. } => None,
        }));
    }
    let mut seen = std::collections::HashSet::new();
    names.retain(|name| seen.insert(name.to_ascii_lowercase()));
    names.join(", ")
}
