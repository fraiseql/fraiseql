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

use clap::{Args, Parser, builder::BoolishValueParser};

use crate::ServerConfig;

/// Parse a boolean environment variable, returning `None` if unset.
///
/// Accepts `true`, `1`, `yes`, `on` (case-insensitive) as `Some(true)`;
/// all other values as `Some(false)`.
fn parse_bool_env_opt(var: &str) -> Option<bool> {
    std::env::var(var)
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on"))
}

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

    /// Require authentication for schema metadata endpoint (overrides `introspection_require_auth`
    /// for `/api/v1/schema/metadata`).
    #[arg(long, env = "FRAISEQL_METADATA_REQUIRE_AUTH", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub metadata_require_auth: Option<bool>,

    /// Require authentication for schema export endpoints (overrides `introspection_require_auth`
    /// for `/api/v1/schema.graphql` and `/api/v1/schema.json`).
    #[arg(long, env = "FRAISEQL_SCHEMA_EXPORT_REQUIRE_AUTH", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub schema_export_require_auth: Option<bool>,

    /// Require authentication for playground endpoint (overrides `introspection_require_auth` for
    /// the playground path).
    #[arg(long, env = "FRAISEQL_PLAYGROUND_REQUIRE_AUTH", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub playground_require_auth: Option<bool>,

    /// Require authentication for subscription endpoint (overrides `introspection_require_auth`
    /// for the `WebSocket` subscription path).
    #[arg(long, env = "FRAISEQL_SUBSCRIPTION_REQUIRE_AUTH", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub subscription_require_auth: Option<bool>,

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
    /// Construct a `ServerArgs` from environment variables only (no CLI parsing).
    ///
    /// This is useful for consumers that handle their own CLI args (e.g.
    /// `fraiseql run`) but still want to pick up server-production env vars
    /// like `FRAISEQL_METRICS_ENABLED` without duplicating the parsing logic.
    ///
    /// Unset env vars produce `None` fields — only explicitly set env vars
    /// generate overrides.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            config: std::env::var("FRAISEQL_CONFIG").ok(),
            database_url: std::env::var("DATABASE_URL").ok(),
            bind_addr: std::env::var("FRAISEQL_BIND_ADDR").ok().and_then(|v| v.parse().ok()),
            schema_path: std::env::var("FRAISEQL_SCHEMA_PATH").ok(),
            metrics_enabled: parse_bool_env_opt("FRAISEQL_METRICS_ENABLED"),
            metrics_token: std::env::var("FRAISEQL_METRICS_TOKEN").ok(),
            admin_api_enabled: parse_bool_env_opt("FRAISEQL_ADMIN_API_ENABLED"),
            admin_token: std::env::var("FRAISEQL_ADMIN_TOKEN").ok(),
            introspection_enabled: parse_bool_env_opt("FRAISEQL_INTROSPECTION_ENABLED"),
            introspection_require_auth: parse_bool_env_opt("FRAISEQL_INTROSPECTION_REQUIRE_AUTH"),
            metadata_require_auth: parse_bool_env_opt("FRAISEQL_METADATA_REQUIRE_AUTH"),
            schema_export_require_auth: parse_bool_env_opt("FRAISEQL_SCHEMA_EXPORT_REQUIRE_AUTH"),
            playground_require_auth: parse_bool_env_opt("FRAISEQL_PLAYGROUND_REQUIRE_AUTH"),
            subscription_require_auth: parse_bool_env_opt("FRAISEQL_SUBSCRIPTION_REQUIRE_AUTH"),
            rate_limiting_enabled: parse_bool_env_opt("FRAISEQL_RATE_LIMITING_ENABLED"),
            rate_limit_rps_per_ip: std::env::var("FRAISEQL_RATE_LIMIT_RPS_PER_IP")
                .ok()
                .and_then(|v| v.parse().ok()),
            rate_limit_rps_per_user: std::env::var("FRAISEQL_RATE_LIMIT_RPS_PER_USER")
                .ok()
                .and_then(|v| v.parse().ok()),
            rate_limit_burst_size: std::env::var("FRAISEQL_RATE_LIMIT_BURST_SIZE")
                .ok()
                .and_then(|v| v.parse().ok()),
            log_format: std::env::var("FRAISEQL_LOG_FORMAT").ok(),
        }
    }

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
        if let Some(require_auth) = self.metadata_require_auth {
            config.metadata_require_auth = Some(require_auth);
        }
        if let Some(require_auth) = self.schema_export_require_auth {
            config.schema_export_require_auth = Some(require_auth);
        }
        if let Some(require_auth) = self.playground_require_auth {
            config.playground_require_auth = Some(require_auth);
        }
        if let Some(require_auth) = self.subscription_require_auth {
            config.subscription_require_auth = Some(require_auth);
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

        let mut rate_config = config.rate_limiting.take().unwrap_or_default();

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
    #[must_use]
    pub fn is_json_log_format(&self) -> bool {
        self.log_format.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("json"))
    }
}
