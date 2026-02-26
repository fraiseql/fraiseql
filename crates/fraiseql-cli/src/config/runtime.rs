//! Runtime configuration for the HTTP server and database connection pool.
//!
//! These structs are shared between `FraiseQLConfig` (Workflow B: JSON + fraiseql.toml)
//! and `TomlSchema` (Workflow A: TOML-only).  All fields have sensible defaults so
//! existing `fraiseql.toml` files without `[server]` or `[database]` sections continue
//! to work unchanged.

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ─── TLS ─────────────────────────────────────────────────────────────────────

/// TLS/HTTPS configuration for the HTTP server.
///
/// ```toml
/// [server.tls]
/// enabled  = true
/// cert_file = "/etc/fraiseql/cert.pem"
/// key_file  = "/etc/fraiseql/key.pem"
/// min_version = "1.2"   # "1.2" or "1.3"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TlsRuntimeConfig {
    /// Enable TLS (HTTPS).  Default: `false`.
    pub enabled: bool,

    /// Path to the PEM-encoded certificate file.
    pub cert_file: String,

    /// Path to the PEM-encoded private key file.
    pub key_file: String,

    /// Minimum TLS version: `"1.2"` (default) or `"1.3"`.
    pub min_version: String,
}

impl Default for TlsRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled:     false,
            cert_file:   String::new(),
            key_file:    String::new(),
            min_version: "1.2".to_string(),
        }
    }
}

// ─── CORS ────────────────────────────────────────────────────────────────────

/// CORS configuration for the HTTP server.
///
/// ```toml
/// [server.cors]
/// origins     = ["https://app.example.com"]
/// credentials = true
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CorsRuntimeConfig {
    /// Allowed origins.  Empty list → all origins allowed (development default).
    pub origins: Vec<String>,

    /// Allow credentials (cookies, `Authorization` header).  Default: `false`.
    pub credentials: bool,
}

// ─── Server ──────────────────────────────────────────────────────────────────

/// HTTP server runtime configuration.
///
/// The `[server]` section in `fraiseql.toml` is **optional**.  When absent,
/// the server listens on `0.0.0.0:8080` with no TLS and permissive CORS
/// (suitable for local development).
///
/// CLI flags (`--port`, `--bind`) take precedence over these settings.
///
/// # Example
///
/// ```toml
/// [server]
/// host               = "127.0.0.1"
/// port               = 9000
/// request_timeout_ms = 30_000
/// keep_alive_secs    = 75
///
/// [server.cors]
/// origins     = ["https://app.example.com"]
/// credentials = true
///
/// [server.tls]
/// enabled   = true
/// cert_file = "/etc/fraiseql/cert.pem"
/// key_file  = "/etc/fraiseql/key.pem"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerRuntimeConfig {
    /// Bind host.  Default: `"0.0.0.0"`.
    pub host: String,

    /// TCP port.  Default: `8080`.
    pub port: u16,

    /// Request timeout in milliseconds.  Default: `30 000` (30 s).
    pub request_timeout_ms: u64,

    /// TCP keep-alive in seconds.  Default: `75`.
    pub keep_alive_secs: u64,

    /// CORS settings.
    pub cors: CorsRuntimeConfig,

    /// TLS settings.
    pub tls: TlsRuntimeConfig,
}

impl Default for ServerRuntimeConfig {
    fn default() -> Self {
        Self {
            host:               "0.0.0.0".to_string(),
            port:               8080,
            request_timeout_ms: 30_000,
            keep_alive_secs:    75,
            cors:               CorsRuntimeConfig::default(),
            tls:                TlsRuntimeConfig::default(),
        }
    }
}

impl ServerRuntimeConfig {
    /// Validate the server runtime configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `port` is zero
    /// - `tls.enabled` but `cert_file` or `key_file` is empty
    /// - `tls.min_version` is not `"1.2"` or `"1.3"`
    pub fn validate(&self) -> Result<()> {
        if self.port == 0 {
            anyhow::bail!("[server] port must be non-zero");
        }

        if self.tls.enabled {
            if self.tls.cert_file.is_empty() {
                anyhow::bail!("[server.tls] cert_file is required when tls.enabled = true");
            }
            if self.tls.key_file.is_empty() {
                anyhow::bail!("[server.tls] key_file is required when tls.enabled = true");
            }
            if self.tls.min_version != "1.2" && self.tls.min_version != "1.3" {
                anyhow::bail!(
                    "[server.tls] min_version must be \"1.2\" or \"1.3\", got \"{}\"",
                    self.tls.min_version
                );
            }
        }

        Ok(())
    }
}

// ─── Database ────────────────────────────────────────────────────────────────

/// Database connection pool runtime configuration.
///
/// The `[database]` section in `fraiseql.toml` is **optional**.  When absent,
/// connection parameters fall back to the `DATABASE_URL` environment variable
/// or the `--database` CLI flag.
///
/// Supports `${VAR}` environment variable interpolation in the `url` field:
///
/// ```toml
/// [database]
/// url      = "${DATABASE_URL}"
/// pool_min = 2
/// pool_max = 20
/// ssl_mode = "prefer"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseRuntimeConfig {
    /// Database connection URL.  Supports `${VAR}` interpolation.
    ///
    /// If not set here, the runtime falls back to the `DATABASE_URL` environment
    /// variable or the `--database` CLI flag.
    pub url: Option<String>,

    /// Minimum connection pool size.  Default: `2`.
    pub pool_min: usize,

    /// Maximum connection pool size.  Default: `20`.
    pub pool_max: usize,

    /// Connection acquisition timeout in milliseconds.  Default: `5 000` (5 s).
    pub connect_timeout_ms: u64,

    /// Idle connection lifetime in milliseconds.  Default: `600 000` (10 min).
    pub idle_timeout_ms: u64,

    /// PostgreSQL SSL mode: `"disable"`, `"allow"`, `"prefer"`, or `"require"`.
    /// Default: `"prefer"`.
    pub ssl_mode: String,
}

impl Default for DatabaseRuntimeConfig {
    fn default() -> Self {
        Self {
            url:                None,
            pool_min:           2,
            pool_max:           20,
            connect_timeout_ms: 5_000,
            idle_timeout_ms:    600_000,
            ssl_mode:           "prefer".to_string(),
        }
    }
}

impl DatabaseRuntimeConfig {
    /// Validate the database runtime configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `pool_min > pool_max`
    /// - `ssl_mode` is not one of the recognised values
    pub fn validate(&self) -> Result<()> {
        const VALID_SSL: &[&str] = &["disable", "allow", "prefer", "require"];

        if self.pool_min > self.pool_max {
            anyhow::bail!(
                "[database] pool_min ({}) must be <= pool_max ({})",
                self.pool_min,
                self.pool_max
            );
        }

        if !VALID_SSL.contains(&self.ssl_mode.as_str()) {
            anyhow::bail!(
                "[database] ssl_mode must be one of {:?}, got \"{}\"",
                VALID_SSL,
                self.ssl_mode
            );
        }

        Ok(())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ServerRuntimeConfig defaults ─────────────────────────────────────────

    #[test]
    fn test_server_runtime_config_default() {
        let cfg = ServerRuntimeConfig::default();
        assert_eq!(cfg.host, "0.0.0.0");
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.request_timeout_ms, 30_000);
        assert_eq!(cfg.keep_alive_secs, 75);
        assert!(!cfg.tls.enabled);
        assert!(cfg.cors.origins.is_empty());
        assert!(!cfg.cors.credentials);
    }

    #[test]
    fn test_server_runtime_config_validate_ok() {
        assert!(ServerRuntimeConfig::default().validate().is_ok());
    }

    #[test]
    fn test_server_runtime_config_validate_port_zero() {
        let cfg = ServerRuntimeConfig { port: 0, ..Default::default() };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("port"), "got: {err}");
    }

    #[test]
    fn test_server_runtime_config_validate_tls_missing_cert() {
        let cfg = ServerRuntimeConfig {
            tls: TlsRuntimeConfig {
                enabled:     true,
                cert_file:   String::new(),
                key_file:    "key.pem".to_string(),
                min_version: "1.2".to_string(),
            },
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("cert_file"), "got: {err}");
    }

    #[test]
    fn test_server_runtime_config_validate_tls_missing_key() {
        let cfg = ServerRuntimeConfig {
            tls: TlsRuntimeConfig {
                enabled:     true,
                cert_file:   "cert.pem".to_string(),
                key_file:    String::new(),
                min_version: "1.2".to_string(),
            },
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("key_file"), "got: {err}");
    }

    #[test]
    fn test_server_runtime_config_validate_bad_tls_version() {
        let cfg = ServerRuntimeConfig {
            tls: TlsRuntimeConfig {
                enabled:     true,
                cert_file:   "cert.pem".to_string(),
                key_file:    "key.pem".to_string(),
                min_version: "1.0".to_string(),
            },
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("min_version"), "got: {err}");
    }

    #[test]
    fn test_server_runtime_config_parses_toml() {
        let toml_str = r#"
host               = "127.0.0.1"
port               = 9000
request_timeout_ms = 60_000

[cors]
origins     = ["https://example.com"]
credentials = true

[tls]
enabled = false
"#;
        let cfg: ServerRuntimeConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.request_timeout_ms, 60_000);
        assert_eq!(cfg.cors.origins, ["https://example.com"]);
        assert!(cfg.cors.credentials);
        assert!(!cfg.tls.enabled);
    }

    // ── DatabaseRuntimeConfig ────────────────────────────────────────────────

    #[test]
    fn test_database_runtime_config_default() {
        let cfg = DatabaseRuntimeConfig::default();
        assert!(cfg.url.is_none());
        assert_eq!(cfg.pool_min, 2);
        assert_eq!(cfg.pool_max, 20);
        assert_eq!(cfg.connect_timeout_ms, 5_000);
        assert_eq!(cfg.idle_timeout_ms, 600_000);
        assert_eq!(cfg.ssl_mode, "prefer");
    }

    #[test]
    fn test_database_runtime_config_validate_ok() {
        assert!(DatabaseRuntimeConfig::default().validate().is_ok());
    }

    #[test]
    fn test_database_runtime_config_validate_pool_range() {
        let cfg = DatabaseRuntimeConfig { pool_min: 10, pool_max: 5, ..Default::default() };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("pool_min"), "got: {err}");
    }

    #[test]
    fn test_database_runtime_config_validate_ssl_mode() {
        let cfg =
            DatabaseRuntimeConfig { ssl_mode: "bogus".to_string(), ..Default::default() };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("ssl_mode"), "got: {err}");
    }

    #[test]
    fn test_database_runtime_config_parses_toml() {
        let toml_str = r#"
url      = "postgresql://localhost/mydb"
pool_min = 5
pool_max = 50
ssl_mode = "require"
"#;
        let cfg: DatabaseRuntimeConfig = toml::from_str(toml_str).expect("parse failed");
        assert_eq!(cfg.url, Some("postgresql://localhost/mydb".to_string()));
        assert_eq!(cfg.pool_min, 5);
        assert_eq!(cfg.pool_max, 50);
        assert_eq!(cfg.ssl_mode, "require");
    }
}
