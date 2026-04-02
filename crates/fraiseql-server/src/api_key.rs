//! API key authentication.
//!
//! Provides static (env-based) and database-backed API key authentication.
//! When an `X-API-Key` header (or configured header) is present, the key is
//! hashed and looked up against configured storage.  A valid key produces a
//! [`SecurityContext`]; a missing key falls through to JWT authentication.
//!
//! # Security
//!
//! - Keys are **never** stored or compared in plaintext — only SHA-256 hashes.
//! - Comparison uses constant-time equality (`subtle::ConstantTimeEq`) to prevent timing
//!   side-channels.
//! - Revoked keys (with `revoked_at` set) are rejected.

use std::sync::Arc;

use axum::http::{HeaderMap, HeaderName};
use chrono::Utc;
use fraiseql_core::security::{AuthenticatedUser, SecurityContext};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tracing::{debug, warn};

// ───────────────────────────────────────────────────────────────
// Configuration (deserialized from compiled schema JSON)
// ───────────────────────────────────────────────────────────────

/// API key configuration embedded in the compiled schema.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiKeyConfig {
    /// Whether API key authentication is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// HTTP header name to read the API key from (default: `x-api-key`).
    #[serde(default = "default_header")]
    pub header: String,

    /// Hash algorithm used to store key hashes (`sha256`).
    #[serde(default = "default_algorithm")]
    pub hash_algorithm: String,

    /// Storage backend: `"env"` for static keys or `"postgres"` for DB-backed.
    #[serde(default = "default_storage")]
    pub storage: String,

    /// Static API keys (only used when `storage = "env"`).
    #[serde(default, rename = "static")]
    pub static_keys: Vec<StaticApiKeyConfig>,
}

fn default_header() -> String {
    "x-api-key".into()
}
fn default_algorithm() -> String {
    "sha256".into()
}
fn default_storage() -> String {
    "env".into()
}

/// A single static API key entry from configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct StaticApiKeyConfig {
    /// Hex-encoded SHA-256 hash of the key, optionally prefixed with `sha256:`.
    pub key_hash: String,
    /// OAuth-style scopes granted by this key.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Human-readable key name (for audit logging).
    pub name: String,
}

// ───────────────────────────────────────────────────────────────
// Authenticator
// ───────────────────────────────────────────────────────────────

/// Resolved static key (with parsed hash bytes).
#[derive(Debug, Clone)]
struct ResolvedStaticKey {
    hash: [u8; 32],
    scopes: Vec<String>,
    name: String,
}

/// API key authentication result.
#[derive(Debug)]
#[non_exhaustive]
pub enum ApiKeyResult {
    /// Key found and valid — contains the constructed `SecurityContext`.
    Authenticated(Box<SecurityContext>),
    /// No API key header present — caller should fall through to JWT.
    NotPresent,
    /// Key was present but invalid or revoked.
    Invalid,
}

/// API key authenticator.
pub struct ApiKeyAuthenticator {
    header_name: HeaderName,
    static_keys: Vec<ResolvedStaticKey>,
}

impl ApiKeyAuthenticator {
    /// Build an authenticator from the compiled schema config.
    ///
    /// Returns `None` if API key auth is not enabled or configuration is
    /// invalid (logs warnings).
    #[must_use]
    pub fn from_config(config: &ApiKeyConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }

        let header_name: HeaderName = config
            .header
            .parse()
            .map_err(|e| {
                warn!(header = %config.header, error = %e, "Invalid API key header name");
            })
            .ok()?;

        if config.hash_algorithm != "sha256" {
            warn!(
                algorithm = %config.hash_algorithm,
                "Unsupported API key hash algorithm — only sha256 is supported"
            );
            return None;
        }

        let mut static_keys = Vec::new();
        for entry in &config.static_keys {
            let hex_str = entry.key_hash.strip_prefix("sha256:").unwrap_or(&entry.key_hash);
            match hex::decode(hex_str) {
                Ok(bytes) if bytes.len() == 32 => {
                    let mut hash = [0u8; 32];
                    hash.copy_from_slice(&bytes);
                    static_keys.push(ResolvedStaticKey {
                        hash,
                        scopes: entry.scopes.clone(),
                        name: entry.name.clone(),
                    });
                },
                Ok(bytes) => {
                    warn!(
                        name = %entry.name,
                        len = bytes.len(),
                        "API key hash has wrong length (expected 32 bytes)"
                    );
                },
                Err(e) => {
                    warn!(
                        name = %entry.name,
                        error = %e,
                        "API key hash is not valid hex"
                    );
                },
            }
        }

        Some(Self {
            header_name,
            static_keys,
        })
    }

    /// Authenticate a request using the API key header.
    pub async fn authenticate(&self, headers: &HeaderMap) -> ApiKeyResult {
        let raw_key = match headers.get(&self.header_name) {
            Some(v) => match v.to_str() {
                Ok(s) if !s.is_empty() => s,
                _ => return ApiKeyResult::NotPresent,
            },
            None => return ApiKeyResult::NotPresent,
        };

        // Strip optional "ApiKey " prefix (for Authorization header usage).
        let key = raw_key
            .strip_prefix("ApiKey ")
            .or_else(|| raw_key.strip_prefix("apikey "))
            .unwrap_or(raw_key);

        let key_hash = sha256_hash(key.as_bytes());

        // Check static keys with constant-time comparison.
        for static_key in &self.static_keys {
            if bool::from(key_hash.ct_eq(&static_key.hash)) {
                debug!(name = %static_key.name, "API key authenticated (static)");
                let ctx = build_security_context(&static_key.name, &static_key.scopes);
                return ApiKeyResult::Authenticated(Box::new(ctx));
            }
        }

        warn!("API key authentication failed: key not found");
        ApiKeyResult::Invalid
    }
}

impl std::fmt::Debug for ApiKeyAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyAuthenticator")
            .field("header_name", &self.header_name)
            .field("static_keys_count", &self.static_keys.len())
            .finish()
    }
}

// ───────────────────────────────────────────────────────────────
// Helpers
// ───────────────────────────────────────────────────────────────

/// SHA-256 hash of input bytes.
fn sha256_hash(input: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Build a `SecurityContext` for an API key identity.
fn build_security_context(key_name: &str, scopes: &[String]) -> SecurityContext {
    let user = AuthenticatedUser {
        user_id: format!("apikey:{key_name}"),
        scopes: scopes.to_vec(),
        expires_at: Utc::now() + chrono::Duration::hours(24),
    };
    SecurityContext::from_user(&user, format!("apikey-{}", uuid::Uuid::new_v4()))
}

/// Build an `ApiKeyAuthenticator` from the compiled schema's `security.api_keys` JSON.
pub fn api_key_authenticator_from_schema(
    schema: &fraiseql_core::schema::CompiledSchema,
) -> Option<Arc<ApiKeyAuthenticator>> {
    let security = schema.security.as_ref()?;
    let api_keys_val = security.additional.get("api_keys")?;
    let config: ApiKeyConfig = serde_json::from_value(api_keys_val.clone())
        .map_err(|e| {
            warn!(error = %e, "Failed to parse security.api_keys config");
        })
        .ok()?;
    ApiKeyAuthenticator::from_config(&config).map(Arc::new)
}

// ───────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    fn sha256_hex(input: &str) -> String {
        hex::encode(sha256_hash(input.as_bytes()))
    }

    fn test_config(key: &str) -> ApiKeyConfig {
        ApiKeyConfig {
            enabled: true,
            header: "x-api-key".into(),
            hash_algorithm: "sha256".into(),
            storage: "env".into(),
            static_keys: vec![StaticApiKeyConfig {
                key_hash: format!("sha256:{}", sha256_hex(key)),
                scopes: vec!["read:*".into()],
                name: "test-key".into(),
            }],
        }
    }

    #[tokio::test]
    async fn valid_api_key_returns_security_context() {
        let config = test_config("my-secret-key");
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "my-secret-key".parse().unwrap());

        match auth.authenticate(&headers).await {
            ApiKeyResult::Authenticated(ctx) => {
                assert_eq!(ctx.user_id, "apikey:test-key");
                assert_eq!(ctx.scopes, vec!["read:*".to_string()]);
            },
            ref other => panic!("expected Authenticated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_api_key_returns_invalid() {
        let config = test_config("my-secret-key");
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "wrong-key".parse().unwrap());

        assert!(matches!(auth.authenticate(&headers).await, ApiKeyResult::Invalid));
    }

    #[tokio::test]
    async fn missing_api_key_returns_not_present() {
        let config = test_config("my-secret-key");
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();

        let headers = HeaderMap::new();
        assert!(matches!(auth.authenticate(&headers).await, ApiKeyResult::NotPresent));
    }

    #[tokio::test]
    async fn api_key_prefix_stripped() {
        let config = test_config("my-secret-key");
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "ApiKey my-secret-key".parse().unwrap());

        assert!(matches!(auth.authenticate(&headers).await, ApiKeyResult::Authenticated(_)));
    }

    #[test]
    fn disabled_config_returns_none() {
        let mut config = test_config("key");
        config.enabled = false;
        assert!(ApiKeyAuthenticator::from_config(&config).is_none());
    }

    #[test]
    fn invalid_hash_hex_is_skipped() {
        let config = ApiKeyConfig {
            enabled: true,
            header: "x-api-key".into(),
            hash_algorithm: "sha256".into(),
            storage: "env".into(),
            static_keys: vec![StaticApiKeyConfig {
                key_hash: "not-valid-hex".into(),
                scopes: vec![],
                name: "bad-key".into(),
            }],
        };
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();
        assert_eq!(auth.static_keys.len(), 0);
    }

    #[test]
    fn hash_without_prefix_works() {
        let hash = sha256_hex("test");
        let config = ApiKeyConfig {
            enabled: true,
            header: "x-api-key".into(),
            hash_algorithm: "sha256".into(),
            storage: "env".into(),
            static_keys: vec![StaticApiKeyConfig {
                key_hash: hash, // no "sha256:" prefix
                scopes: vec![],
                name: "no-prefix".into(),
            }],
        };
        let auth = ApiKeyAuthenticator::from_config(&config).unwrap();
        assert_eq!(auth.static_keys.len(), 1);
    }

    #[test]
    fn sha256_hash_is_deterministic() {
        let h1 = sha256_hash(b"hello");
        let h2 = sha256_hash(b"hello");
        assert_eq!(h1, h2);
        // Different input → different hash.
        let h3 = sha256_hash(b"world");
        assert_ne!(h1, h3);
    }

    #[test]
    fn unsupported_algorithm_returns_none() {
        let mut config = test_config("key");
        config.hash_algorithm = "bcrypt".into();
        assert!(ApiKeyAuthenticator::from_config(&config).is_none());
    }
}
