//! `[auth_issuer]`: this server as an OIDC issuer for the sessions it mints (#1964).

use serde::{Deserialize, Serialize};

/// The environment variable the key-encryption key is read from when
/// `key_encryption_key_env` is unset.
pub const DEFAULT_KEY_ENCRYPTION_KEY_ENV: &str = "FRAISEQL_AUTH_SIGNING_KEK";

/// The first-party issuer: the server signs the sessions it mints with its own ES256
/// keys, publishes them, and validates its own tokens like any other issuer's.
///
/// Loaded from the `[auth_issuer]` section of the server config.
///
/// # Example (TOML)
///
/// ```toml
/// [auth_issuer]
/// url = "https://api.example.com"
/// audience = "my-api"
/// key_encryption_key_env = "FRAISEQL_AUTH_SIGNING_KEK"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerConfig {
    /// The issuer's public URL: the `iss` of every token it mints, and where its
    /// discovery document is served from.
    pub url: String,

    /// The `aud` of every token it mints, and the only audience its tokens are
    /// accepted for.
    pub audience: String,

    /// The environment variable holding the key-encryption key (64 hex digits) the
    /// signing keys are sealed under.
    #[serde(default = "default_key_encryption_key_env")]
    pub key_encryption_key_env: String,
}

fn default_key_encryption_key_env() -> String {
    DEFAULT_KEY_ENCRYPTION_KEY_ENV.to_string()
}

impl IssuerConfig {
    /// Validate the `[auth_issuer]` section.
    ///
    /// # Errors
    ///
    /// A message naming the field, when `url` is not an absolute `https` URL (plain
    /// `http` only on a loopback address) or carries a query or fragment, when
    /// `audience` is empty, or when `key_encryption_key_env` is empty.
    pub fn validate(&self) -> Result<(), String> {
        let url = url::Url::parse(&self.url).map_err(|e| {
            format!("auth_issuer: `url` must be an absolute URL, got {:?}: {e}", self.url)
        })?;
        let loopback = url.host_str().is_some_and(fraiseql_guard::net::is_loopback_host);
        if !(url.scheme() == "https" || (url.scheme() == "http" && loopback)) {
            return Err(format!(
                "auth_issuer: `url` must use https (plain http only on a loopback address), \
                 got {:?}",
                self.url
            ));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(format!(
                "auth_issuer: `url` must not carry a query or fragment, got {:?}",
                self.url
            ));
        }
        if self.audience.trim().is_empty() {
            return Err("auth_issuer: `audience` must name the API its tokens are for".to_string());
        }
        if self.key_encryption_key_env.trim().is_empty() {
            return Err("auth_issuer: `key_encryption_key_env` must name the environment \
                        variable holding the key-encryption key"
                .to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
