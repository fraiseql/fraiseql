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

/// Parse a boolean environment variable, returning `Ok(None)` if unset.
///
/// Delegates to the same [`BoolishValueParser`] clap applies to these
/// variables under `fraiseql-server`, so the two binaries cannot disagree
/// (#874): `y`/`t`/`on` are true, `n`/`f`/`off` are false, and a
/// set-but-unrecognised value — a typo like `ture`, or `"true "` with a
/// trailing space from a hand-edited compose `.env` — is a hard error, never
/// a silent override to `false`.
pub(crate) fn parse_bool_env_opt(var: &str) -> Result<Option<bool>, String> {
    use clap::builder::TypedValueParser;
    let Some(raw) = std::env::var_os(var) else {
        return Ok(None);
    };
    BoolishValueParser::new()
        .parse_ref(&clap::Command::new("fraiseql-server"), None, &raw)
        .map(Some)
        .map_err(|_| {
            format!(
                "{var} must be a boolean (true/false, 1/0, yes/no, on/off, t/f, y/n), got {:?}",
                raw.to_string_lossy()
            )
        })
}

/// Parse an environment variable via `FromStr`, returning `Ok(None)` if unset.
///
/// A set-but-unparseable value is a hard error (#874): silently discarding it
/// would apply a default the operator explicitly tried to override.
pub(crate) fn parse_env_opt<T: std::str::FromStr>(var: &str) -> Result<Option<T>, String>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(var) {
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(format!("{var} is not valid UTF-8: {e}")),
        Ok(raw) => raw
            .parse()
            .map(Some)
            .map_err(|e| format!("{var} has an unparseable value {raw:?}: {e}")),
    }
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

    /// Arrow Flight gRPC bind address (`host:port`).
    #[cfg(feature = "arrow")]
    #[arg(long, env = "FRAISEQL_FLIGHT_BIND_ADDR")]
    pub flight_bind_addr: Option<SocketAddr>,

    /// Fail boot if any declared `sql_source` (query view / mutation function) is
    /// not backed by the database, printing a precise list. Default OFF;
    /// Postgres-only. Overrides the `validate_sql_sources` config key. (#487)
    #[arg(long, env = "FRAISEQL_VALIDATE_SQL_SOURCES", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub validate_sql_sources: Option<bool>,

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

    // ── Shutdown ─────────────────────────────────────────────────────────
    /// Seconds to wait for in-flight requests to drain on SIGTERM/SIGINT
    /// before aborting them.
    #[arg(long, env = "FRAISEQL_SHUTDOWN_TIMEOUT_SECS")]
    pub shutdown_timeout_secs: Option<u64>,
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
    ///
    /// # Errors
    ///
    /// Returns an error naming the variable when a set env var carries an
    /// unparseable value (#874). Discarding it silently would flip the
    /// operator's explicit override into the built-in default — for the
    /// `*_require_auth` family, the exact opposite of the stated intent.
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            config: std::env::var("FRAISEQL_CONFIG").ok(),
            database_url: std::env::var("DATABASE_URL").ok(),
            bind_addr: parse_env_opt("FRAISEQL_BIND_ADDR")?,
            schema_path: std::env::var("FRAISEQL_SCHEMA_PATH").ok(),
            #[cfg(feature = "arrow")]
            flight_bind_addr: parse_env_opt("FRAISEQL_FLIGHT_BIND_ADDR")?,
            validate_sql_sources: parse_bool_env_opt("FRAISEQL_VALIDATE_SQL_SOURCES")?,
            metrics_enabled: parse_bool_env_opt("FRAISEQL_METRICS_ENABLED")?,
            metrics_token: std::env::var("FRAISEQL_METRICS_TOKEN").ok(),
            admin_api_enabled: parse_bool_env_opt("FRAISEQL_ADMIN_API_ENABLED")?,
            admin_token: std::env::var("FRAISEQL_ADMIN_TOKEN").ok(),
            introspection_enabled: parse_bool_env_opt("FRAISEQL_INTROSPECTION_ENABLED")?,
            introspection_require_auth: parse_bool_env_opt("FRAISEQL_INTROSPECTION_REQUIRE_AUTH")?,
            metadata_require_auth: parse_bool_env_opt("FRAISEQL_METADATA_REQUIRE_AUTH")?,
            schema_export_require_auth: parse_bool_env_opt("FRAISEQL_SCHEMA_EXPORT_REQUIRE_AUTH")?,
            playground_require_auth: parse_bool_env_opt("FRAISEQL_PLAYGROUND_REQUIRE_AUTH")?,
            subscription_require_auth: parse_bool_env_opt("FRAISEQL_SUBSCRIPTION_REQUIRE_AUTH")?,
            rate_limiting_enabled: parse_bool_env_opt("FRAISEQL_RATE_LIMITING_ENABLED")?,
            rate_limit_rps_per_ip: parse_env_opt("FRAISEQL_RATE_LIMIT_RPS_PER_IP")?,
            rate_limit_rps_per_user: parse_env_opt("FRAISEQL_RATE_LIMIT_RPS_PER_USER")?,
            rate_limit_burst_size: parse_env_opt("FRAISEQL_RATE_LIMIT_BURST_SIZE")?,
            log_format: std::env::var("FRAISEQL_LOG_FORMAT").ok(),
            shutdown_timeout_secs: parse_env_opt("FRAISEQL_SHUTDOWN_TIMEOUT_SECS")?,
        })
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
        #[cfg(feature = "arrow")]
        if let Some(addr) = self.flight_bind_addr {
            config.flight_bind_addr = addr;
        }
        // #487: the CLI flag / FRAISEQL_VALIDATE_SQL_SOURCES env var (both surface
        // here as `Some`) override the `validate_sql_sources` config key.
        if let Some(enabled) = self.validate_sql_sources {
            config.validate_sql_sources = enabled;
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

        // Shutdown drain window (#838: the config field's rustdoc had promised
        // this override long before anything read it).
        if let Some(secs) = self.shutdown_timeout_secs {
            config.shutdown_timeout_secs = secs;
        }

        // Rate limiting — apply all four overrides atomically.
        self.apply_rate_limit_overrides(config);
    }

    /// Record the rate-limiting CLI/env overrides on `config`.
    ///
    /// Recorded rather than merged into `config.rate_limiting`: the merge produced a
    /// `RateLimitConfig` in which an overridden field was indistinguishable from a
    /// defaulted one, so the resolver could not apply the overrides *over* the
    /// compiled schema and simply let the schema win — leaving
    /// `FRAISEQL_RATE_LIMITING_ENABLED=false` with no effect (#774).
    const fn apply_rate_limit_overrides(&self, config: &mut ServerConfig) {
        config.rate_limit_overrides = crate::middleware::RateLimitOverrides {
            enabled:      self.rate_limiting_enabled,
            rps_per_ip:   self.rate_limit_rps_per_ip,
            rps_per_user: self.rate_limit_rps_per_user,
            burst_size:   self.rate_limit_burst_size,
        };
    }

    /// Whether the log format is JSON.
    #[must_use]
    pub fn is_json_log_format(&self) -> bool {
        self.log_format.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("json"))
    }
}
