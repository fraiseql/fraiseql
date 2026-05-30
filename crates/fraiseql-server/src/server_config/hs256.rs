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
            format!("auth_hs256: environment variable `{}` is not set", self.secret_env)
        })?;
        if value.is_empty() {
            return Err(format!("auth_hs256: environment variable `{}` is empty", self.secret_env));
        }
        Ok(value)
    }

    /// Validate the `[auth_hs256]` config shape at startup.
    ///
    /// `audience` is required.  When two HS256-protected services share a
    /// signing secret (common in test fixtures, internal service meshes,
    /// monorepo CI), a token minted for service A is accepted by service
    /// B if B leaves `audience` unset — exactly the cross-service
    /// token-confusion attack the v2.3 S40 OIDC hardening closes for the
    /// OIDC path.  Mirroring that guard here closes the same gap for the
    /// shared-secret testing path.
    ///
    /// `secret_env` is required to be non-empty so configuration errors
    /// surface at validation time rather than at first auth request.
    ///
    /// `issuer` remains optional — `OidcConfig::validate` also treats
    /// the matching field as optional.
    ///
    /// # Errors
    ///
    /// Returns a human-readable error string when:
    /// - `secret_env` is empty.
    /// - `audience` is `None`.
    pub fn validate(&self) -> Result<(), String> {
        if self.secret_env.is_empty() {
            return Err("auth_hs256: `secret_env` is required and must name the \
                        environment variable holding the shared secret"
                .to_owned());
        }
        if self.audience.is_none() {
            return Err("auth_hs256: `audience` is REQUIRED for security. Set it to your API \
                 identifier to prevent cross-service token-confusion attacks where a \
                 token minted by one service is accepted by another. \
                 Example: audience = \"my-api\" or audience = \"https://api.example.com\""
                .to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
