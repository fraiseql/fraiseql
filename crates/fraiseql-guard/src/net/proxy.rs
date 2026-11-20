//! The proxy an environment configures, which pinned outbound clients do not use.
//!
//! A client pinned to the addresses its own lookup checked cannot go through a proxy:
//! the proxy, not the client, would resolve the destination. Observer and function
//! deliveries therefore connect directly. A deployment that set a proxy for them would
//! see deliveries fail with no word why, so the server says so once at boot.

use std::sync::Once;

/// The variables `reqwest` reads a proxy from, in both cases.
const PROXY_VARIABLES: [&str; 6] = [
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "ALL_PROXY",
    "all_proxy",
];

/// The first proxy variable `var` holds a non-blank value for.
pub fn configured_proxy(var: impl Fn(&str) -> Option<String>) -> Option<&'static str> {
    PROXY_VARIABLES
        .into_iter()
        .find(|name| var(name).is_some_and(|value| !value.trim().is_empty()))
}

/// The warning for a proxy configured in `variable`.
#[must_use]
pub fn direct_connection_warning(variable: &str) -> String {
    format!(
        "observer and function deliveries connect directly and do not use the proxy \
         {variable} configures"
    )
}

/// Warn, once per process, when the environment configures a proxy: called where the
/// observer runtime or the functions runtime starts.
pub fn warn_if_proxy_configured() {
    static WARNED: Once = Once::new();
    if let Some(variable) = configured_proxy(|name| std::env::var(name).ok()) {
        WARNED.call_once(|| {
            tracing::warn!(variable, "{}", direct_connection_warning(variable));
        });
    }
}

#[cfg(test)]
mod tests;
