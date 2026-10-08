//! The resolved request locale, carried to the engine (#1512).
//!
//! A transport resolves the locale once per request ([`LocaleConfig::resolve`]) and runs the
//! request's engine calls inside [`with_request_locale`]. The engine reads it with
//! [`request_locale`], which every read path uses for the `fraiseql.locale` session setting
//! and the result-cache key.
//!
//! It is a task-local, like [`crate::runtime::dispatched_at`]'s depth, and for the same
//! reason: a client controls neither. A transport whose engine calls cross a task boundary
//! (`/ws` after the upgrade, an async-operation worker) carries the resolved value across and
//! scopes it again on the far side.
//!
//! [`LocaleConfig::resolve`]: crate::schema::LocaleConfig::resolve

use std::future::Future;

use crate::schema::CompiledSchema;

tokio::task_local! {
    static REQUEST_LOCALE: String;
}

/// Run `future` as the work of a request whose locale resolved to `locale`.
pub async fn with_request_locale<F: Future>(locale: impl Into<String>, future: F) -> F::Output {
    REQUEST_LOCALE.scope(locale.into(), future).await
}

/// The locale the current request runs in, when `schema` declares `[locale]`: the scoped
/// value if it is one of `allowed`, otherwise `default`. `None` when the schema declares no
/// locale.
///
/// A scoped value outside `allowed` cannot come from a transport (they scope what
/// `LocaleConfig::resolve` returns), so it is treated as absent rather than trusted: only a
/// configured tag ever reaches SQL. An engine call made outside any scope (an embedder, a
/// background job) runs in `default`.
#[must_use]
pub fn request_locale(schema: &CompiledSchema) -> Option<String> {
    let config = schema.locale.as_ref()?;
    let scoped = REQUEST_LOCALE
        .try_with(|tag| config.chain(tag).map(|_| tag.clone()))
        .ok()
        .flatten();
    Some(scoped.unwrap_or_else(|| config.default.clone()))
}
