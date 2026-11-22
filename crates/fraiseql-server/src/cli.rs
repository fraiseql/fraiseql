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

use clap::{Args, FromArgMatches, Parser, builder::BoolishValueParser};

use crate::ServerConfig;

/// The dotted keys whose values differ between `a` and `b`, deepest first reached.
#[must_use]
pub fn differing_keys(a: &serde_json::Value, b: &serde_json::Value) -> Vec<String> {
    fn walk(a: &serde_json::Value, b: &serde_json::Value, path: &str, out: &mut Vec<String>) {
        use serde_json::Value;
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => {
                let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
                keys.sort();
                keys.dedup();
                for key in keys {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    walk(
                        a.get(key).unwrap_or(&Value::Null),
                        b.get(key).unwrap_or(&Value::Null),
                        &child,
                        out,
                    );
                }
            },
            (a, b) if a != b => out.push(path.to_owned()),
            _ => {},
        }
    }
    let mut out = Vec::new();
    walk(a, b, "", &mut out);
    out
}

/// `error`, from reading `command`'s arguments from the environment alone, as a message that
/// names the variable an operator set rather than the flag clap spells it as.
fn env_error(command: &clap::Command, error: &clap::Error) -> String {
    use clap::error::{ContextKind, ContextValue, ErrorKind};
    let declared = || command.get_arguments().filter_map(|arg| Some((arg, arg.get_env()?)));
    // The argument clap names, as `--flag <VALUE>`; a value that is not valid UTF-8 names
    // none, so the variable holding one is found by reading each.
    let named = match error.get(ContextKind::InvalidArg) {
        Some(ContextValue::String(spelled)) => declared().find_map(|(arg, env)| {
            let long = format!("--{}", arg.get_long()?);
            (spelled == &long || spelled.starts_with(&format!("{long} "))).then_some(env)
        }),
        _ => None,
    };
    let variable = named
        .or_else(|| {
            declared()
                .map(|(_, env)| env)
                .find(|env| matches!(std::env::var(env), Err(std::env::VarError::NotUnicode(_))))
        })
        .map(|env| env.to_string_lossy().into_owned());
    let Some(variable) = variable else {
        return error.to_string();
    };
    if error.kind() == ErrorKind::InvalidUtf8 {
        return format!("{variable} is not valid UTF-8");
    }
    let written = std::env::var(&variable).unwrap_or_default();
    let reason = match error.get(ContextKind::ValidValue) {
        Some(ContextValue::Strings(valid)) => format!("expected one of {}", valid.join(", ")),
        _ => std::error::Error::source(error)
            .map_or_else(|| error.kind().to_string(), ToString::to_string),
    };
    format!("{variable} has an invalid value {written:?}: {reason}")
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

    /// Most connections the PostgreSQL pool opens. Overrides the `pool_max_size` config key
    /// (default 20). Every replica opens its own pool, so replicas × this, plus every other
    /// client, must stay under the database's `max_connections` (#1558).
    #[arg(long, env = "FRAISEQL_POOL_MAX_SIZE")]
    pub pool_max_size: Option<usize>,

    /// Attempts a mutation's transaction gets when PostgreSQL aborts it with `40P01` or
    /// `40001`, the first included. Overrides `[transaction_retry] max_attempts`; without
    /// that section, opts in with its other defaults. `1` turns retry off (#1879).
    #[arg(long, env = "FRAISEQL_TRANSACTION_RETRY_MAX_ATTEMPTS")]
    pub transaction_retry_max_attempts: Option<u32>,

    /// Arrow Flight gRPC bind address (`host:port`).
    #[cfg(feature = "arrow")]
    #[arg(long, env = "FRAISEQL_FLIGHT_BIND_ADDR")]
    pub flight_bind_addr: Option<SocketAddr>,

    /// Fail boot if any declared `sql_source` (query view / mutation function) is
    /// not backed by the database, or is one the server's role may not use (`USAGE`
    /// on its schema, `SELECT` / `EXECUTE`), printing a precise list. Default OFF;
    /// Postgres-only. Overrides the `validate_sql_sources` config key. (#487, #1426)
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

    /// Require authentication for the federation service SDL (overrides
    /// `introspection_require_auth` for `{ _service { sdl } }`).
    #[arg(long, env = "FRAISEQL_SERVICE_SDL_REQUIRE_AUTH", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub service_sdl_require_auth: Option<bool>,

    /// Whether a refused name carries a "Did you mean" hint (overrides
    /// `suggestions_enabled`; unset, the deployment decides: off in production).
    #[arg(long, env = "FRAISEQL_SUGGESTIONS_ENABLED", value_parser = BoolishValueParser::new(), num_args = 0..=1, default_missing_value = "true")]
    pub suggestions_enabled: Option<bool>,

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

    /// Rate limit: maximum tracked buckets per map (memory ceiling, ~200 bytes each).
    #[arg(long, env = "FRAISEQL_RATE_LIMIT_MAX_BUCKETS")]
    pub rate_limit_max_buckets: Option<usize>,

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
        let command = Self::augment_args(clap::Command::new("fraiseql-server"));
        command
            .clone()
            .try_get_matches_from(["fraiseql-server"])
            .and_then(|matches| Self::from_arg_matches(&matches))
            .map_err(|error| env_error(&command, &error))
    }

    /// The environment variables these arguments read, as their declarations name them: the
    /// argument's id and the variable.
    #[must_use]
    pub fn env_vars() -> Vec<(String, String)> {
        Self::augment_args(clap::Command::new("fraiseql-server"))
            .get_arguments()
            .filter_map(|arg| {
                let env = arg.get_env()?.to_string_lossy().into_owned();
                Some((arg.get_id().to_string(), env))
            })
            .collect()
    }

    /// The configuration file keys each environment variable overrides, found by applying the
    /// argument alone to a default configuration, with two values, and reading which keys
    /// differ: the variable, and those keys, dotted. A variable that sets no file key (the
    /// config file's path, the log format, the rate-limit overrides) has none.
    #[must_use]
    pub fn overridden_keys() -> Vec<(String, Vec<String>)> {
        /// Pairs of values, tried in order until both of a pair parse.
        const VALUES: [(&str, &str); 4] = [
            ("true", "false"),
            ("2", "3"),
            ("127.0.0.1:1", "127.0.0.1:2"),
            ("x", "y"),
        ];
        let command = Self::augment_args(clap::Command::new("fraiseql-server"));
        let applied = |long: &str, value: &str| {
            let matches = command
                .clone()
                .try_get_matches_from(["fraiseql-server".to_owned(), format!("--{long}={value}")])
                .ok()?;
            let mut config = ServerConfig::default();
            Self::from_arg_matches(&matches).ok()?.apply_to_config(&mut config);
            serde_json::to_value(&config).ok()
        };
        command
            .get_arguments()
            .filter_map(|arg| {
                let env = arg.get_env()?.to_string_lossy().into_owned();
                let long = arg.get_long()?;
                let keys = VALUES
                    .iter()
                    .find_map(|(a, b)| Some((applied(long, a)?, applied(long, b)?)))
                    .map(|(a, b)| differing_keys(&a, &b))
                    .unwrap_or_default();
                Some((env, keys))
            })
            .collect()
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
        if let Some(max) = self.pool_max_size {
            config.pool_max_size = max;
        }
        if let Some(attempts) = self.transaction_retry_max_attempts {
            config.transaction_retry.get_or_insert_with(Default::default).max_attempts = attempts;
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
        if let Some(require_auth) = self.service_sdl_require_auth {
            config.service_sdl_require_auth = Some(require_auth);
        }
        if let Some(enabled) = self.suggestions_enabled {
            config.suggestions_enabled = Some(enabled);
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
            max_buckets:  self.rate_limit_max_buckets,
        };
    }

    /// Whether the log format is JSON.
    #[must_use]
    pub fn is_json_log_format(&self) -> bool {
        self.log_format.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("json"))
    }
}

#[cfg(test)]
mod env_tests;
