//! SSRF-safe HTTP client construction.
//!
//! All outbound HTTP clients that contact external services (OIDC JWKS endpoints,
//! Vault, federation subgraphs, webhooks) must be built with this module to ensure
//! a consistent security baseline.

use std::time::Duration;

use reqwest::{Client, ClientBuilder, redirect};

/// Build an HTTP client that is safe for SSRF-sensitive outbound requests.
///
/// # Policy
///
/// - **Redirects disabled**: a redirect response (`3xx`) is treated as an error, preventing
///   redirect-chain attacks that bypass URL-validation guards applied to the initial URL (e.g. a
///   subgraph redirecting to `169.254.169.254`).
/// - **HTTPS only**: plain HTTP connections are rejected, preventing protocol-downgrade via
///   redirect.
/// - **Caller-supplied timeout**: applied to all requests; prevents slow-loris hangs on malicious
///   or misconfigured endpoints.
///
/// # Errors
///
/// Returns a [`reqwest::Error`] if TLS initialisation fails (extremely rare;
/// indicates a platform TLS misconfiguration).
pub fn build_ssrf_safe_client(timeout: Duration) -> reqwest::Result<Client> {
    ClientBuilder::new()
        .redirect(redirect::Policy::none())
        .https_only(true)
        .timeout(timeout)
        .build()
}
