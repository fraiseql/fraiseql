//! Clap-based CLI argument parsing for `fraiseql-server`.
//!
//! The [`Cli`] struct defines all command-line flags and their corresponding
//! environment variable fallbacks.  Clap's `env` attribute provides automatic
//! **CLI flag > env var > default** precedence.
//!
//! # Sharing with `fraiseql-cli`
//!
//! `Cli` is re-exported from `fraiseql_server` so that the `fraiseql run`
//! subcommand can embed it via `#[command(flatten)]`, eliminating duplicated
//! env-var handling between the two binaries.

use std::net::SocketAddr;

use clap::builder::BoolishValueParser;
use clap::{Args, Parser};

use crate::ServerConfig;

// ── Top-level CLI ────────────────────────────────────────────────────────────

/// FraiseQL Server — compiled GraphQL execution engine.
#[derive(Parser, Debug, Clone)]
#[command(name = "fraiseql-server", version, about)]
pub struct Cli {
    /// Server configuration overrides (shared with `fraiseql run`).
    #[command(flatten)]
    pub server: ServerArgs,

    /// Enable MCP (Model Context Protocol) stdio transport.
    ///
    /// When set (to any value), the server starts in MCP stdio mode instead of
    /// HTTP.  Equivalent to setting `FRAISEQL_MCP_STDIO=1`.
    #[cfg(feature = "mcp")]
    #[arg(long, env = "FRAISEQL_MCP_STDIO", hide = true)]
    pub mcp_stdio: Option<String>,
}

// ── Shared server arguments ──────────────────────────────────────────────────

/// Server configuration flags shared between `fraiseql-server` and
/// `fraiseql run`.
///
/// Every flag has a corresponding environment variable (clap's `env`
/// attribute).  The resolution order is: **CLI flag > env var > config
/// file > built-in default**.
#[derive(Args, Debug, Clone, Default)]
pub struct ServerArgs {
    // ── Core ─────────────────────────────────────────────────────────────

    /// Path to TOML configuration file.
    #[arg(long, env = "FRAISEQL_CONFIG")]
    pub config: Option<String>,

    /// Database connection URL.
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: Option<String>,

    /// Server bind address (`host:port`).
    #[arg(long, env = "FRAISEQL_BIND_ADDR")]
    pub bind_addr: Option<SocketAddr>,

    /// Path to compiled schema JSON file.
    #[arg(long, env = "FRAISEQL_SCHEMA_PATH")]
    pub schema_path: Option<String>,

    // ── Metrics ──────────────────────────────────────────────────────────

    /// Enable Prometheus metrics endpoint.
    #[arg(long, env = "FRAISEQL_METRICS_ENABLED", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub metrics_enabled: Option<bool>,

    /// Bearer token for metrics endpoint authentication.
    #[arg(long, env = "FRAISEQL_METRICS_TOKEN")]
    pub metrics_token: Option<String>,

    // ── Admin API ────────────────────────────────────────────────────────

    /// Enable admin API endpoints.
    #[arg(long, env = "FRAISEQL_ADMIN_API_ENABLED", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub admin_api_enabled: Option<bool>,

    /// Bearer token for admin API authentication.
    #[arg(long, env = "FRAISEQL_ADMIN_TOKEN")]
    pub admin_token: Option<String>,

    // ── Introspection ────────────────────────────────────────────────────

    /// Enable GraphQL introspection endpoint.
    #[arg(long, env = "FRAISEQL_INTROSPECTION_ENABLED", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub introspection_enabled: Option<bool>,

    /// Require authentication for introspection endpoint.
    #[arg(long, env = "FRAISEQL_INTROSPECTION_REQUIRE_AUTH", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub introspection_require_auth: Option<bool>,

    // ── Rate limiting ────────────────────────────────────────────────────

    /// Enable per-IP and per-user rate limiting.
    #[arg(long, env = "FRAISEQL_RATE_LIMITING_ENABLED", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub rate_limiting_enabled: Option<bool>,

    /// Rate limit: maximum requests per second per IP.
    #[arg(long, env = "FRAISEQL_RATE_LIMIT_RPS_PER_IP")]
    pub rate_limit_rps_per_ip: Option<u32>,

    /// Rate limit: maximum requests per second per authenticated user.
    #[arg(long, env = "FRAISEQL_RATE_LIMIT_RPS_PER_USER")]
    pub rate_limit_rps_per_user: Option<u32>,

    /// Rate limit: token bucket burst capacity.
    #[arg(long, env = "FRAISEQL_RATE_LIMIT_BURST_SIZE")]
    pub rate_limit_burst_size: Option<u32>,

    // ── Logging ──────────────────────────────────────────────────────────

    /// Log output format: `json` for structured JSON, `text` for
    /// human-readable (default).
    #[arg(long, env = "FRAISEQL_LOG_FORMAT")]
    pub log_format: Option<String>,
}

impl ServerArgs {
    /// Apply CLI/env overrides to a [`ServerConfig`] loaded from file or
    /// defaults.
    ///
    /// Fields that were not provided on the command line *and* not set via
    /// environment variables are left untouched in `config`.
    pub fn apply_to_config(&self, config: &mut ServerConfig) {
        // Core overrides
        if let Some(ref db_url) = self.database_url {
            config.database_url.clone_from(db_url);
        }
        if let Some(addr) = self.bind_addr {
            config.bind_addr = addr;
        }
        if let Some(ref path) = self.schema_path {
            config.schema_path = path.into();
        }

        // Metrics
        if let Some(enabled) = self.metrics_enabled {
            config.metrics_enabled = enabled;
        }
        if self.metrics_token.is_some() {
            config.metrics_token.clone_from(&self.metrics_token);
        }

        // Admin API
        if let Some(enabled) = self.admin_api_enabled {
            config.admin_api_enabled = enabled;
        }
        if self.admin_token.is_some() {
            config.admin_token.clone_from(&self.admin_token);
        }

        // Introspection
        if let Some(enabled) = self.introspection_enabled {
            config.introspection_enabled = enabled;
        }
        if let Some(require_auth) = self.introspection_require_auth {
            config.introspection_require_auth = require_auth;
        }

        // Rate limiting — apply all four overrides atomically.
        self.apply_rate_limit_overrides(config);
    }

    /// Apply rate-limiting CLI/env overrides to `config`.
    fn apply_rate_limit_overrides(&self, config: &mut ServerConfig) {
        if self.rate_limiting_enabled.is_none()
            && self.rate_limit_rps_per_ip.is_none()
            && self.rate_limit_rps_per_user.is_none()
            && self.rate_limit_burst_size.is_none()
        {
            return;
        }

        let mut rate_config = config
            .rate_limiting
            .take()
            .unwrap_or_default();

        if let Some(enabled) = self.rate_limiting_enabled {
            rate_config.enabled = enabled;
        }
        if let Some(v) = self.rate_limit_rps_per_ip {
            rate_config.rps_per_ip = v;
        }
        if let Some(v) = self.rate_limit_rps_per_user {
            rate_config.rps_per_user = v;
        }
        if let Some(v) = self.rate_limit_burst_size {
            rate_config.burst_size = v;
        }

        config.rate_limiting = Some(rate_config);
    }

    /// Whether the log format is JSON.
    pub fn is_json_log_format(&self) -> bool {
        self.log_format
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case("json"))
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[allow(clippy::field_reassign_with_default)] // Reason: test readability — explicit field-by-field overrides
#[cfg(test)]
mod tests {
    use crate::middleware::RateLimitConfig;

    use super::*;

    // ── Cli::parse_from ──────────────────────────────────────────────────

    #[test]
    fn cli_parse_config_flag() {
        let cli = Cli::parse_from(["fraiseql-server", "--config", "/etc/fraiseql.toml"]);
        assert_eq!(cli.server.config.as_deref(), Some("/etc/fraiseql.toml"));
    }

    #[test]
    fn cli_parse_database_url_flag() {
        let cli = Cli::parse_from(["fraiseql-server", "--database-url", "postgres://localhost/db"]);
        assert_eq!(
            cli.server.database_url.as_deref(),
            Some("postgres://localhost/db")
        );
    }

    #[test]
    fn cli_parse_bind_addr_flag() {
        let cli = Cli::parse_from(["fraiseql-server", "--bind-addr", "127.0.0.1:3000"]);
        assert_eq!(
            cli.server.bind_addr,
            Some("127.0.0.1:3000".parse().unwrap())
        );
    }

    #[test]
    fn cli_defaults_are_none_when_no_flags_or_env() {
        // Clear env vars that would interfere (run in isolation via temp_env
        // in the integration tests; here we just verify the parse shape).
        let cli = Cli::parse_from(["fraiseql-server"]);
        // All Option fields should be None when nothing is set
        // (env vars from the test runner may populate some, so we only check
        // fields that are unlikely to be in the environment).
        assert!(cli.server.config.is_none());
        assert!(cli.server.schema_path.is_none());
        assert!(cli.server.metrics_token.is_none());
        assert!(cli.server.admin_token.is_none());
    }

    #[test]
    fn cli_parse_bool_flag_with_value() {
        let cli =
            Cli::parse_from(["fraiseql-server", "--metrics-enabled", "true"]);
        assert_eq!(cli.server.metrics_enabled, Some(true));

        let cli =
            Cli::parse_from(["fraiseql-server", "--metrics-enabled", "false"]);
        assert_eq!(cli.server.metrics_enabled, Some(false));
    }

    #[test]
    fn cli_parse_bool_flag_without_value() {
        // `--metrics-enabled` with no value should default to true
        let cli = Cli::parse_from(["fraiseql-server", "--metrics-enabled"]);
        assert_eq!(cli.server.metrics_enabled, Some(true));
    }

    #[test]
    fn cli_parse_rate_limit_flags() {
        let cli = Cli::parse_from([
            "fraiseql-server",
            "--rate-limit-rps-per-ip",
            "200",
            "--rate-limit-burst-size",
            "1000",
        ]);
        assert_eq!(cli.server.rate_limit_rps_per_ip, Some(200));
        assert_eq!(cli.server.rate_limit_burst_size, Some(1000));
        assert!(cli.server.rate_limit_rps_per_user.is_none());
    }

    #[test]
    fn cli_parse_log_format() {
        let cli = Cli::parse_from(["fraiseql-server", "--log-format", "json"]);
        assert_eq!(cli.server.log_format.as_deref(), Some("json"));
        assert!(cli.server.is_json_log_format());
    }

    // ── ServerArgs::apply_to_config ──────────────────────────────────────

    #[test]
    fn apply_overrides_database_url() {
        let args = ServerArgs {
            database_url: Some("postgres://override/db".into()),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        args.apply_to_config(&mut config);
        assert_eq!(config.database_url, "postgres://override/db");
    }

    #[test]
    fn apply_leaves_config_unchanged_when_no_overrides() {
        let args = ServerArgs::default();
        let mut config = ServerConfig::default();
        let original_db = config.database_url.clone();
        let original_addr = config.bind_addr;
        args.apply_to_config(&mut config);
        assert_eq!(config.database_url, original_db);
        assert_eq!(config.bind_addr, original_addr);
    }

    #[test]
    fn apply_metrics_enabled_override() {
        let args = ServerArgs {
            metrics_enabled: Some(true),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        assert!(!config.metrics_enabled);
        args.apply_to_config(&mut config);
        assert!(config.metrics_enabled);
    }

    #[test]
    fn apply_rate_limit_creates_config_when_absent() {
        let args = ServerArgs {
            rate_limit_rps_per_ip: Some(50),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        config.rate_limiting = None;
        args.apply_to_config(&mut config);
        let rl = config.rate_limiting.unwrap();
        assert_eq!(rl.rps_per_ip, 50);
        // Other fields should have sensible defaults
        assert!(rl.enabled);
        assert_eq!(rl.burst_size, 500);
    }

    #[test]
    fn apply_rate_limit_preserves_existing_fields() {
        let args = ServerArgs {
            rate_limit_burst_size: Some(999),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        config.rate_limiting = Some(RateLimitConfig {
            enabled:               true,
            rps_per_ip:            42,
            rps_per_user:          420,
            burst_size:            100,
            cleanup_interval_secs: 60,
            trust_proxy_headers:   true,
            trusted_proxy_cidrs:   Vec::new(),
        });
        args.apply_to_config(&mut config);
        let rl = config.rate_limiting.unwrap();
        assert_eq!(rl.burst_size, 999);
        assert_eq!(rl.rps_per_ip, 42);
        assert_eq!(rl.rps_per_user, 420);
        assert!(rl.trust_proxy_headers);
    }

    #[test]
    fn apply_introspection_overrides() {
        let args = ServerArgs {
            introspection_enabled: Some(true),
            introspection_require_auth: Some(false),
            ..Default::default()
        };
        let mut config = ServerConfig::default();
        args.apply_to_config(&mut config);
        assert!(config.introspection_enabled);
        assert!(!config.introspection_require_auth);
    }

    #[test]
    fn is_json_log_format_case_insensitive() {
        let args = ServerArgs {
            log_format: Some("JSON".into()),
            ..Default::default()
        };
        assert!(args.is_json_log_format());

        let args = ServerArgs {
            log_format: Some("text".into()),
            ..Default::default()
        };
        assert!(!args.is_json_log_format());

        let args = ServerArgs::default();
        assert!(!args.is_json_log_format());
    }
}
