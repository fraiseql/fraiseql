//! HS256 symmetric-key authentication configuration.
//!
//! HS256 auth is an alternative to OIDC for integration testing and internal
//! service-to-service scenarios where a shared secret is acceptable. Unlike
//! OIDC, validation is fully local — no discovery endpoint, no JWKS fetch.
//!
//! For public-facing production, prefer OIDC (`[auth]`).

use serde::{Deserialize, Serialize};

/// HS256 authentication configuration.
///
/// Loaded from the `[auth_hs256]` section of `fraiseql.toml`. Mutually
/// exclusive with `[auth]` (OIDC).
///
/// # Example (TOML)
///
/// ```toml
/// [auth_hs256]
/// secret_env = "FRAISEQL_HS256_SECRET"
/// issuer = "my-test-suite"
/// audience = "my-api"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hs256Config {
    /// Name of the environment variable holding the shared secret.
    ///
    /// The secret itself is never stored in the config file. At server
    /// startup, the value of this environment variable is used as the HS256
    /// signing key.
    pub secret_env: String,

    /// Expected `iss` claim (optional).
    #[serde(default)]
    pub issuer: Option<String>,

    /// Expected `aud` claim (optional).
    #[serde(default)]
    pub audience: Option<String>,
}

impl Hs256Config {
    /// Resolve the shared secret from the configured environment variable.
    ///
    /// # Errors
    ///
    /// Returns an error string when the environment variable is unset or empty.
    pub fn load_secret(&self) -> Result<String, String> {
        let value = std::env::var(&self.secret_env).map_err(|_| {
            format!(
                "auth_hs256: environment variable `{}` is not set",
                self.secret_env
            )
        })?;
        if value.is_empty() {
            return Err(format!(
                "auth_hs256: environment variable `{}` is empty",
                self.secret_env
            ));
        }
        Ok(value)
    }
}
