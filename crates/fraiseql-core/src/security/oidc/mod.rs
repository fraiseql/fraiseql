//! OIDC Discovery and JWKS Support
//!
//! This module provides OpenID Connect discovery and JSON Web Key Set (JWKS)
//! support for validating JWT tokens from any OIDC-compliant provider.
//!
//! Supported providers include:
//! - Auth0
//! - Keycloak
//! - Okta
//! - AWS Cognito
//! - Microsoft Entra ID (Azure AD)
//! - Google Identity
//! - Any OIDC-compliant provider
//!
//! # Architecture
//!
//! ```text
//! JWT Token from Client
//!     ↓
//! OidcValidator::validate_token()
//!     ├─ Extract kid (key ID) from JWT header
//!     ├─ Fetch/cache JWKS from provider
//!     ├─ Find matching key by kid
//!     ├─ Verify JWT signature
//!     └─ Validate claims (iss, aud, exp)
//!     ↓
//! AuthenticatedUser (if valid)
//! ```
//!
//! # Example
//!
//! ```no_run
//! use fraiseql_core::security::oidc::{OidcConfig, OidcValidator};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = OidcConfig {
//!     issuer: "https://your-tenant.auth0.com/".to_string(),
//!     audience: Some("your-api-identifier".to_string()),
//!     ..Default::default()
//! };
//!
//! let validator = OidcValidator::new(config).await?;
//! let user = validator.validate_token("eyJhbG...").await?;
//! # Ok(())
//! # }
//! ```

pub(crate) mod audience;
pub(crate) mod jwks;
pub(crate) mod providers;
pub(crate) mod token;

#[cfg(test)]
mod tests;

// Public re-exports — external callers see no change in paths.
pub use audience::{Audience, JwtClaims};
pub use jwks::{Jwk, Jwks, OidcDiscoveryDocument};
pub use providers::OidcConfig;
pub use token::OidcValidator;

// ---------------------------------------------------------------------------
// SSRF validation for OIDC outbound URLs (discovery + JWKS fetch)
// ---------------------------------------------------------------------------

use crate::security::errors::SecurityError;

/// Returns `true` when `FRAISEQL_OIDC_ALLOW_LOCALHOST` is set to `"1"` or
/// `"true"`, allowing OIDC URLs to target localhost and private IPs.
///
/// This is only intended for **local development and testing** (e.g. wiremock).
fn allow_localhost_env() -> bool {
    std::env::var("FRAISEQL_OIDC_ALLOW_LOCALHOST")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Validate an OIDC URL (discovery endpoint or JWKS URI) against SSRF risks.
///
/// Rejects URLs targeting private, loopback, link-local, and other reserved
/// addresses to prevent server-side request forgery via a malicious or
/// compromised OIDC provider configuration.
///
/// Set `FRAISEQL_OIDC_ALLOW_LOCALHOST=1` for local dev/test (e.g. wiremock).
///
/// # Errors
///
/// Returns `SecurityError::SecurityConfigError` if the URL is invalid, has no
/// host, targets `localhost`, or resolves to a forbidden IP range.
pub(crate) fn validate_oidc_url(url: &str) -> Result<(), SecurityError> {
    if allow_localhost_env() {
        return Ok(());
    }

    let parsed = reqwest::Url::parse(url).map_err(|e| {
        SecurityError::SecurityConfigError(format!("Invalid OIDC URL '{url}': {e}"))
    })?;

    let host_raw = parsed.host_str().ok_or_else(|| {
        SecurityError::SecurityConfigError(format!("OIDC URL has no host: {url}"))
    })?;

    // Strip IPv6 brackets added by the url crate (e.g. "[::1]" → "::1").
    let host = if host_raw.starts_with('[') && host_raw.ends_with(']') {
        &host_raw[1..host_raw.len() - 1]
    } else {
        host_raw
    };

    let lower_host = host.to_ascii_lowercase();
    if lower_host == "localhost" || lower_host.ends_with(".localhost") {
        return Err(SecurityError::SecurityConfigError(format!(
            "OIDC URL targets a loopback host ({host}) — SSRF blocked"
        )));
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_oidc_ssrf_blocked_ip(&ip) {
            return Err(SecurityError::SecurityConfigError(format!(
                "OIDC URL targets a private/reserved IP ({ip}) — SSRF blocked"
            )));
        }
    }

    Ok(())
}

/// Returns `true` for IP ranges that OIDC outbound requests must never contact.
fn is_oidc_ssrf_blocked_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127                                        // loopback 127/8
            || o[0] == 10                                      // RFC 1918 10/8
            || (o[0] == 172 && (16..=31).contains(&o[1]))     // RFC 1918 172.16/12
            || (o[0] == 192 && o[1] == 168)                   // RFC 1918 192.168/16
            || (o[0] == 169 && o[1] == 254)                   // link-local 169.254/16
            || (o[0] == 100 && (64..=127).contains(&o[1]))    // CGNAT 100.64/10
            || o == [0, 0, 0, 0]                               // unspecified
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()                                   // ::1
            || v6.is_unspecified()                             // ::
            || {
                let s = v6.segments();
                (s[0] & 0xfe00) == 0xfc00                      // ULA fc00::/7
                || (s[0] & 0xffc0) == 0xfe80                  // link-local fe80::/10
                || (s[0] == 0 && s[1] == 0 && s[2] == 0      // ::ffff:0:0/96
                    && s[3] == 0 && s[4] == 0 && s[5] == 0xffff)
            }
        }
    }
}
