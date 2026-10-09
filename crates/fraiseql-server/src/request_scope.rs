//! The one place a transport turns a request into what the engine reads from it besides
//! the query: its locale (#1512) and the headers its session variables name (#1520).
//!
//! Every transport that reaches the engine builds a [`RequestScope`] with what it knows
//! (headers, an explicit argument, the resolved identity) and runs its engine calls inside
//! [`scoped`]. One value carries both, so a transport cannot scope one and forget the
//! other. The locale's resolution order is `[locale]`'s
//! ([`LocaleConfig::resolve`](fraiseql_core::schema::LocaleConfig::resolve)); the header
//! rules are [`SessionHeaders::resolve`]'s. This module only adapts transport inputs to them.

use std::future::Future;

use axum::http::HeaderMap;
use fraiseql_core::{
    error::Result,
    runtime::SessionHeaders,
    schema::{CompiledSchema, LocaleInputs},
    security::{ENRICHED_NAMESPACE_PREFIX, SecurityContext},
};

/// What a request's engine calls run in: its locale and its session headers.
#[derive(Debug, Clone, Default)]
pub struct RequestScope {
    /// The resolved request locale, or `None` when the schema declares no `[locale]`.
    pub locale:          Option<String>,
    /// The headers the schema's `source = "header"` session variables name, as sent.
    pub session_headers: SessionHeaders,
}

impl RequestScope {
    /// The scope of a request under `schema`: [`resolve`]'s locale and
    /// [`session_headers`]'s headers.
    ///
    /// # Errors
    ///
    /// As [`session_headers`].
    pub fn resolve(
        schema: &CompiledSchema,
        headers: Option<&HeaderMap>,
        argument: Option<&dyn Fn(&str) -> Option<String>>,
        security_context: Option<&SecurityContext>,
    ) -> Result<Self> {
        Ok(Self {
            locale:          resolve(schema, headers, argument, security_context),
            session_headers: session_headers(schema, headers)?,
        })
    }
}

/// The headers `schema`'s `source = "header"` session variables name, read from `headers`
/// (none for a transport without headers).
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`](fraiseql_core::error::FraiseQLError::Validation)
/// when a named header was sent twice, is not UTF-8 or is too long
/// ([`SessionHeaders::resolve`]).
pub fn session_headers(
    schema: &CompiledSchema,
    headers: Option<&HeaderMap>,
) -> Result<SessionHeaders> {
    let Some(headers) = headers else {
        return Ok(SessionHeaders::default());
    };
    SessionHeaders::resolve(&schema.session_variables, &|name| {
        headers
            .get_all(name)
            .iter()
            .map(|v| v.to_str().ok().map(str::to_string))
            .collect()
    })
}

/// The locale a request resolves to under `schema`, or `None` when the schema declares no
/// `[locale]`.
///
/// `argument` looks up the explicit request argument by name (GraphQL `extensions`, REST
/// query parameters); a transport without one passes `None`, as one without headers does.
#[must_use]
pub fn resolve(
    schema: &CompiledSchema,
    headers: Option<&HeaderMap>,
    argument: Option<&dyn Fn(&str) -> Option<String>>,
    security_context: Option<&SecurityContext>,
) -> Option<String> {
    let config = schema.locale.as_ref()?;
    let header = |name: &str| -> Option<String> {
        headers?.get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
    };
    let enrichment = |field: &str| -> Option<String> {
        let value = security_context?
            .attributes
            .get(&format!("{ENRICHED_NAMESPACE_PREFIX}{field}"))?;
        value.as_str().map(str::to_string)
    };
    Some(
        config
            .resolve(&LocaleInputs {
                argument,
                header: Some(&header),
                enrichment: Some(&enrichment),
            })
            .to_string(),
    )
}

/// The explicit-argument lookup for a JSON object of request extras (GraphQL `extensions`):
/// the named key's value when it is a string.
pub fn json_argument(
    extensions: Option<&serde_json::Value>,
) -> impl Fn(&str) -> Option<String> + '_ {
    move |name| extensions?.get(name)?.as_str().map(str::to_string)
}

/// Run `future` in `scope`: its locale, when there is one, and its session headers.
pub async fn scoped<F: Future>(scope: RequestScope, future: F) -> F::Output {
    let future = fraiseql_core::runtime::with_session_headers(scope.session_headers, future);
    match scope.locale {
        Some(locale) => fraiseql_core::runtime::with_request_locale(locale, future).await,
        None => future.await,
    }
}
