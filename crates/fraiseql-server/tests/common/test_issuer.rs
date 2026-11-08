//! A server's `[auth]` section pointed at the shared ES256 test issuer.

use fraiseql_core::security::OidcConfig;
use fraiseql_test_support::issuer::TestIssuer;

/// The `[auth]` section of a deployment that trusts `issuer`, for its issuer and
/// audience.
///
/// The JWKS URI is left unset, so the server finds the key set the way a
/// deployment does: by discovery from the issuer. `required` is the section's own
/// `required`: whether a request without a bearer is refused.
#[must_use]
pub fn oidc_config(issuer: &TestIssuer, required: bool) -> OidcConfig {
    OidcConfig {
        issuer: Some(issuer.issuer().to_string()),
        audience: Some(issuer.audience().to_string()),
        required,
        allowed_algorithms: vec!["ES256".to_string()],
        ..OidcConfig::default()
    }
}
