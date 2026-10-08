//! The one place a transport turns a request into its locale (#1512).
//!
//! Every transport that reaches the engine calls [`resolve`] with what it knows (headers,
//! an explicit argument, the resolved identity) and runs its engine calls inside
//! [`scoped`]. The resolution order and its fall-through are `[locale]`'s
//! ([`LocaleConfig::resolve`](fraiseql_core::schema::LocaleConfig::resolve)); this module
//! only adapts transport inputs to it.

use std::future::Future;

use axum::http::HeaderMap;
use fraiseql_core::{
    schema::{CompiledSchema, LocaleInputs},
    security::{ENRICHED_NAMESPACE_PREFIX, SecurityContext},
};

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

/// Run `future` in `locale`, when there is one.
pub async fn scoped<F: Future>(locale: Option<String>, future: F) -> F::Output {
    match locale {
        Some(locale) => fraiseql_core::runtime::with_request_locale(locale, future).await,
        None => future.await,
    }
}
