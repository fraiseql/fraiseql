//! The request headers a `source = "header"` session variable reads (#1520).
//!
//! A transport resolves them once per request ([`SessionHeaders::resolve`]) from its own
//! headers and runs the request's engine calls inside [`with_session_headers`]. The
//! session-variable builder reads them with [`scoped_session_header`].
//!
//! They are carried apart from `SecurityContext.attributes`, which holds the principal's
//! **claims**: a header and a claim of the same name are different inputs, and reading one
//! for the other is the defect this module exists to end. Like the request locale, they are
//! a task-local; a transport whose engine calls cross a task boundary (an async-operation
//! worker, an SSE `@stream` continuation) carries the resolved value across and scopes it
//! again on the far side.
//!
//! A header is **client-controlled**: any caller can send any value. A header-sourced
//! session variable must never feed row-level security or tenant scoping.

use std::{collections::BTreeMap, future::Future};

use serde::{Deserialize, Serialize};

use crate::{
    error::{FraiseQLError, Result},
    schema::{SessionVariableSource, SessionVariablesConfig},
};

/// The longest header value a session variable accepts. A longer one is refused, never
/// truncated: a truncated value is a different value.
pub const MAX_SESSION_HEADER_BYTES: usize = 1024;

/// The values of the headers a request's `source = "header"` session variables name, keyed
/// by lower-cased header name. A header the request did not send is absent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionHeaders(BTreeMap<String, String>);

impl SessionHeaders {
    /// Resolve the headers `config` names from a request's headers.
    ///
    /// `values` returns every value the request sent for a header name, matched
    /// case-insensitively, or `None` for a value that is not valid UTF-8. Only headers
    /// some `source = "header"` mapping names are read.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] when a named header was sent more than once,
    /// is not valid UTF-8, or is longer than [`MAX_SESSION_HEADER_BYTES`]. Each is refused
    /// rather than joined, decoded lossily or truncated, because each would set the
    /// variable to a value the client did not send.
    pub fn resolve(
        config: &SessionVariablesConfig,
        values: &dyn Fn(&str) -> Vec<Option<String>>,
    ) -> Result<Self> {
        let mut resolved = BTreeMap::new();
        for mapping in &config.variables {
            let SessionVariableSource::Header { header } = &mapping.source else {
                continue;
            };
            let name = header.to_ascii_lowercase();
            if resolved.contains_key(&name) {
                continue;
            }
            let refuse = |why: &str| FraiseQLError::Validation {
                message: format!(
                    "request header '{name}' (session variable '{}') {why}",
                    mapping.name
                ),
                path:    None,
            };
            let mut sent = values(&name).into_iter();
            let Some(first) = sent.next() else {
                continue;
            };
            if sent.next().is_some() {
                return Err(refuse("was sent more than once"));
            }
            let Some(value) = first else {
                return Err(refuse("is not valid UTF-8"));
            };
            if value.len() > MAX_SESSION_HEADER_BYTES {
                return Err(refuse(&format!("is longer than {MAX_SESSION_HEADER_BYTES} bytes")));
            }
            resolved.insert(name, value);
        }
        Ok(Self(resolved))
    }

    /// Whether no header was resolved.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The value sent for `header` (any case), if the request sent it.
    #[must_use]
    pub fn get(&self, header: &str) -> Option<&str> {
        self.0.get(&header.to_ascii_lowercase()).map(String::as_str)
    }
}

tokio::task_local! {
    static SESSION_HEADERS: SessionHeaders;
}

/// Run `future` as the work of a request that sent `headers`.
pub async fn with_session_headers<F: Future>(headers: SessionHeaders, future: F) -> F::Output {
    SESSION_HEADERS.scope(headers, future).await
}

/// Run `f` with `headers` in scope: the synchronous twin of [`with_session_headers`].
pub fn with_session_headers_sync<R>(headers: SessionHeaders, f: impl FnOnce() -> R) -> R {
    SESSION_HEADERS.sync_scope(headers, f)
}

/// The headers the current request was scoped with by its transport, for carrying them
/// across a task boundary. `None` outside any scope.
#[must_use]
pub fn scoped_session_headers() -> Option<SessionHeaders> {
    SESSION_HEADERS.try_with(Clone::clone).ok()
}

/// The value the current request sent for `header`. `None` when it sent none, or outside
/// any scope (an engine call no transport made carries no request headers).
#[must_use]
pub fn scoped_session_header(header: &str) -> Option<String> {
    SESSION_HEADERS.try_with(|h| h.get(header).map(str::to_string)).ok().flatten()
}

#[cfg(test)]
mod tests;
