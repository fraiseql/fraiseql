//! Doctor command — systematic diagnostic checks for common FraiseQL setup problems.
//!
//! Usage:
//!   fraiseql doctor
//!   fraiseql doctor --config fraiseql.toml --schema schema.compiled.json
//!   fraiseql doctor --json

use std::{net::TcpStream, path::Path, time::Duration};

use fraiseql_observers::migrations::ENTITY_CHANGE_LOG_CONTRACT;
use serde::{Deserialize, Serialize};

use crate::{
    config::toml_schema::TomlSchema,
    schema::pg_catalog::{LiveColumn, PgCatalog, PlpgsqlCheckOutcome},
};

// ─── Types ────────────────────────────────────────────────────────────────────

/// Outcome of a single diagnostic check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum CheckStatus {
    /// Check passed.
    Pass,
    /// Check produced a non-fatal warning.
    Warn,
    /// Check failed (fatal).
    Fail,
}

/// A single diagnostic check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    /// Short display name shown in the report.
    pub name:   &'static str,
    /// Outcome of the check.
    pub status: CheckStatus,
    /// One-line detail appended after the name.
    pub detail: String,
    /// Optional actionable hint shown on the next line when status is not Pass.
    pub hint:   Option<String>,
}

impl DoctorCheck {
    pub(crate) fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Pass,
            detail: detail.into(),
            hint: None,
        }
    }

    pub(crate) fn warn(
        name: &'static str,
        detail: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            name,
            status: CheckStatus::Warn,
            detail: detail.into(),
            hint: Some(hint.into()),
        }
    }

    pub(crate) fn fail(
        name: &'static str,
        detail: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            detail: detail.into(),
            hint: Some(hint.into()),
        }
    }
}

// ─── Individual checks ────────────────────────────────────────────────────────

/// Check that the compiled schema file exists and is readable.
pub fn check_schema_exists(path: &Path) -> DoctorCheck {
    if path.exists() {
        DoctorCheck::pass("Schema file exists", path.display().to_string())
    } else {
        DoctorCheck::fail(
            "Schema file exists",
            format!("not found: {}", path.display()),
            "Run `fraiseql compile fraiseql.toml` to generate schema.compiled.json",
        )
    }
}

/// Check that the compiled schema file is valid JSON.
pub fn check_schema_parses(path: &Path) -> DoctorCheck {
    match std::fs::read_to_string(path) {
        Err(e) => DoctorCheck::fail(
            "Schema parses",
            format!("cannot read: {e}"),
            "Check file permissions or run `fraiseql compile`",
        ),
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Err(e) => DoctorCheck::fail(
                "Schema parses",
                format!("JSON parse error: {e}"),
                "Run `fraiseql compile fraiseql.toml` to regenerate the schema",
            ),
            Ok(schema) => {
                let types = schema.get("types").and_then(|v| v.as_array()).map_or(0, Vec::len);
                let queries = schema.get("queries").and_then(|v| v.as_array()).map_or(0, Vec::len);
                let mutations =
                    schema.get("mutations").and_then(|v| v.as_array()).map_or(0, Vec::len);
                DoctorCheck::pass(
                    "Schema parses",
                    format!("types={types}, queries={queries}, mutations={mutations}"),
                )
            },
        },
    }
}

/// Check the schema format version field.
pub fn check_schema_version(path: &Path) -> DoctorCheck {
    let Ok(content) = std::fs::read_to_string(path) else {
        return DoctorCheck::warn(
            "Schema format version",
            "could not read schema file",
            "Ensure schema.compiled.json is readable",
        );
    };
    let Ok(schema) = serde_json::from_str::<serde_json::Value>(&content) else {
        return DoctorCheck::warn(
            "Schema format version",
            "schema is not valid JSON — version check skipped",
            "Run `fraiseql compile` to regenerate",
        );
    };

    match schema.get("version").and_then(serde_json::Value::as_u64) {
        None => DoctorCheck::warn(
            "Schema format version",
            "no version field (older schema)",
            "Run `fraiseql compile fraiseql.toml` to get a versioned schema",
        ),
        Some(v) if v == 1 => {
            DoctorCheck::pass("Schema format version", format!("version={v} (current)"))
        },
        Some(v) => DoctorCheck::warn(
            "Schema format version",
            format!("version={v} (expected 1)"),
            "Run `fraiseql compile fraiseql.toml` to recompile with the current compiler",
        ),
    }
}

/// Check whether `fraiseql.toml` exists.
pub fn check_toml_exists(path: &Path) -> DoctorCheck {
    if path.exists() {
        DoctorCheck::pass("fraiseql.toml found", path.display().to_string())
    } else {
        DoctorCheck::warn(
            "fraiseql.toml found",
            format!("not found: {} (using defaults)", path.display()),
            "Create fraiseql.toml with `fraiseql init` or provide --config",
        )
    }
}

/// Parse `fraiseql.toml`. Only called when the file actually exists.
pub fn check_toml_parses(path: &Path) -> DoctorCheck {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return DoctorCheck::fail(
                "TOML syntax valid",
                format!("cannot read: {e}"),
                "Check file permissions",
            );
        },
    };
    match TomlSchema::parse_toml(&content) {
        Ok(_) => DoctorCheck::pass("TOML syntax valid", ""),
        Err(e) => {
            // Keep only the first line of the error to avoid overwhelming output.
            let first_line = e.to_string();
            let short = first_line.lines().next().unwrap_or("parse error");
            DoctorCheck::fail(
                "TOML syntax valid",
                format!("parse error: {short}"),
                "Fix TOML syntax in fraiseql.toml and retry",
            )
        },
    }
}

/// Check whether `DATABASE_URL` is set in the environment.
pub fn check_database_url_set(db_url_override: Option<&str>) -> DoctorCheck {
    let val = db_url_override
        .map(std::borrow::Cow::Borrowed)
        .or_else(|| std::env::var("DATABASE_URL").ok().map(std::borrow::Cow::Owned));
    if val.is_some() {
        DoctorCheck::pass("DATABASE_URL set", "")
    } else {
        DoctorCheck::fail(
            "DATABASE_URL set",
            "not set",
            "Set DATABASE_URL=postgres://user:pass@host:port/dbname in your environment",
        )
    }
}

/// Attempt a TCP connection to the database host:port extracted from the URL.
///
/// This does **not** run any SQL — it only validates that a TCP socket can be
/// opened within a 5-second timeout.
pub fn check_db_reachable(db_url_override: Option<&str>) -> DoctorCheck {
    let url_str = match db_url_override
        .map(std::borrow::Cow::Borrowed)
        .or_else(|| std::env::var("DATABASE_URL").ok().map(std::borrow::Cow::Owned))
    {
        Some(u) => u.into_owned(),
        None => {
            return DoctorCheck::fail(
                "DATABASE_URL reachable",
                "DATABASE_URL not set — cannot check connectivity",
                "Set DATABASE_URL first",
            );
        },
    };

    match parse_host_port(&url_str) {
        None => DoctorCheck::warn(
            "DATABASE_URL reachable",
            format!("could not parse host:port from URL: {url_str}"),
            "Ensure DATABASE_URL is a valid postgres:// or mysql:// URL",
        ),
        Some((host, port)) => {
            let addr = format!("{host}:{port}");
            // Parse the socket addr; fall back to a guaranteed-refused addr on parse failure.
            let sock_addr = addr.parse().unwrap_or_else(|_| {
                std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0)
            });
            match TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5)) {
                Ok(_) => DoctorCheck::pass("DATABASE_URL reachable", addr),
                Err(e) => DoctorCheck::fail(
                    "DATABASE_URL reachable",
                    format!("connection refused ({addr}): {e}"),
                    format!(
                        "Check that the database is running: pg_isready -h {host} -p {port}\n\
                         Or set DATABASE_URL=postgres://user:pass@host:port/dbname"
                    ),
                ),
            }
        },
    }
}

/// Check whether `FRAISEQL_JWT_SECRET` is set.
pub fn check_jwt_secret() -> DoctorCheck {
    if std::env::var("FRAISEQL_JWT_SECRET").is_ok() {
        DoctorCheck::pass("FRAISEQL_JWT_SECRET", "set")
    } else {
        DoctorCheck::warn(
            "FRAISEQL_JWT_SECRET",
            "not set (auth will reject all tokens)",
            "Set FRAISEQL_JWT_SECRET in your environment or .env file",
        )
    }
}

/// Check Redis if `REDIS_URL` is set.
pub fn check_redis_reachable() -> DoctorCheck {
    let Ok(url_str) = std::env::var("REDIS_URL") else {
        return DoctorCheck::pass("FRAISEQL_REDIS_URL", "not set (OK: cache disabled)");
    };

    match parse_host_port(&url_str) {
        None => DoctorCheck::warn(
            "FRAISEQL_REDIS_URL",
            format!("could not parse host:port from REDIS_URL: {url_str}"),
            "Ensure REDIS_URL is a valid redis:// URL",
        ),
        Some((host, port)) => {
            let addr = format!("{host}:{port}");
            let sock_addr = addr.parse().unwrap_or_else(|_| {
                std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 0)
            });
            match TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5)) {
                Ok(_) => DoctorCheck::pass("FRAISEQL_REDIS_URL", format!("reachable ({addr})")),
                Err(e) => DoctorCheck::fail(
                    "FRAISEQL_REDIS_URL",
                    format!("set but not reachable ({addr}): {e}"),
                    "Check that Redis is running or unset REDIS_URL to disable caching",
                ),
            }
        },
    }
}

/// Check TLS: if the TOML config enables TLS, the cert file must exist.
pub fn check_tls(config_path: &Path) -> DoctorCheck {
    // Only run this check when the config file exists and is readable.
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return DoctorCheck::pass("TLS certificate", "not configured (OK: TLS disabled)");
    };
    let Ok(schema) = TomlSchema::parse_toml(&content) else {
        return DoctorCheck::pass("TLS certificate", "TOML unreadable — TLS check skipped");
    };

    if !schema.server.tls.enabled {
        return DoctorCheck::pass("TLS certificate", "not configured (OK: TLS disabled)");
    }

    let cert = &schema.server.tls.cert_file;
    if cert.is_empty() {
        return DoctorCheck::fail(
            "TLS certificate",
            "TLS enabled but cert_file is empty",
            "Set [server.tls] cert_file and key_file in fraiseql.toml",
        );
    }
    if Path::new(cert).exists() {
        DoctorCheck::pass("TLS certificate", format!("found: {cert}"))
    } else {
        DoctorCheck::fail(
            "TLS certificate",
            format!("TLS enabled but cert_file not found: {cert}"),
            "Provide a valid PEM certificate at the configured path",
        )
    }
}

/// Cross-check: warn if caching is enabled without any authorization policy.
///
/// When caching is active but no authorization policies are configured, cached
/// results may be served to unauthenticated users — a potential data-leak.
pub fn check_rls_cache_coherence(config_path: &Path) -> DoctorCheck {
    // Config not present — nothing to cross-check.
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return DoctorCheck::pass("Cache + auth coherence", "no config (defaults: cache disabled)");
    };
    let Ok(schema) = TomlSchema::parse_toml(&content) else {
        return DoctorCheck::pass("Cache + auth coherence", "TOML unreadable — check skipped");
    };

    let caching_enabled = schema.caching.enabled;
    let has_auth_policy =
        !schema.security.policies.is_empty() || schema.security.default_policy.is_some();

    match (caching_enabled, has_auth_policy) {
        (false, _) => {
            DoctorCheck::pass("Cache + auth coherence", "cache disabled — no cross-user risk")
        },
        (true, true) => {
            DoctorCheck::pass("Cache + auth coherence", "caching + auth policy both configured")
        },
        (true, false) => DoctorCheck::warn(
            "Cache + auth coherence",
            "caching enabled without authorization policy — cached results may leak across users",
            "Add [security.policies] entries or set [security] default_policy in fraiseql.toml",
        ),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract (host, port) from a URL like `postgres://user:pass@host:5432/db`.
///
/// Returns `None` if the URL cannot be parsed.
pub(crate) fn parse_host_port(url: &str) -> Option<(String, u16)> {
    // Strip the scheme prefix and credentials; we only need the host:port.
    let after_scheme = url.split("://").nth(1)?;
    // Drop path/query after the first `/` following host:port.
    let host_part = after_scheme.split('/').next()?;
    // Drop user:pass@.
    let host_port = host_part.split('@').next_back()?;

    // Handle IPv6 addresses: [::1]:5432
    if host_port.starts_with('[') {
        let bracket_end = host_port.find(']')?;
        let host = host_port[1..bracket_end].to_string();
        let after_bracket = &host_port[bracket_end + 1..];
        let port = after_bracket.trim_start_matches(':').parse::<u16>().ok()?;
        return Some((host, port));
    }

    let mut parts = host_port.rsplitn(2, ':');
    let port = parts.next()?.parse::<u16>().ok()?;
    let host = parts.next().unwrap_or("localhost").to_string();
    Some((host, port))
}

// ─── Output ───────────────────────────────────────────────────────────────────

/// Print the doctor report in text format to stdout.
pub fn print_text_report(checks: &[DoctorCheck]) {
    println!("\nChecking FraiseQL setup...\n");

    for check in checks {
        let symbol = match check.status {
            CheckStatus::Pass => "✓",
            CheckStatus::Warn => "!",
            CheckStatus::Fail => "✗",
        };
        let detail = if check.detail.is_empty() {
            String::new()
        } else {
            format!("    {}", check.detail)
        };
        println!("  [{symbol}] {:<30}{detail}", check.name);
        if let Some(hint) = &check.hint {
            for line in hint.lines() {
                println!("       → {line}");
            }
        }
    }

    let errors = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
    let warnings = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();

    println!();
    match (errors, warnings) {
        (0, 0) => println!("All checks passed."),
        (0, w) => println!("Summary: 0 errors, {w} warning(s)"),
        (e, 0) => println!("Summary: {e} error(s), 0 warnings"),
        (e, w) => println!("Summary: {e} error(s), {w} warning(s)"),
    }
}

/// Print the doctor report as JSON to stdout.
pub fn print_json_report(checks: &[DoctorCheck]) {
    let json = serde_json::to_string_pretty(checks).unwrap_or_else(|_| "[]".to_string());
    println!("{json}");
}

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Run all doctor checks and return the list of results.
pub fn run_checks(
    config_path: &Path,
    schema_path: &Path,
    db_url_override: Option<&str>,
) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    // Schema checks
    checks.push(check_schema_exists(schema_path));
    if schema_path.exists() {
        checks.push(check_schema_parses(schema_path));
        checks.push(check_schema_version(schema_path));
    }

    // TOML config checks
    checks.push(check_toml_exists(config_path));
    if config_path.exists() {
        checks.push(check_toml_parses(config_path));
    }

    // Environment / connectivity checks
    checks.push(check_database_url_set(db_url_override));
    checks.push(check_db_reachable(db_url_override));
    checks.push(check_jwt_secret());
    checks.push(check_redis_reachable());

    // TLS and coherence checks (only meaningful when config is present)
    checks.push(check_tls(config_path));
    checks.push(check_rls_cache_coherence(config_path));

    checks
}

/// Execute the doctor command.
///
/// Returns `true` if all checks passed (exit 0), `false` if any check failed
/// (exit 1). Warnings do not trigger an exit-1.
/// Execute the doctor command including the optional `--against-db` live-DB
/// passes: the change-log contract drift check (#380) and the PL/pgSQL
/// body-resolution pass (#409).
///
/// Runs the standard checks, then — when `against_db` is set — appends the
/// change-log contract drift report and the internal-call resolution results
/// for `schemas`. Returns `true` if all checks passed (exit 0), `false` if any
/// failed (exit 1).
pub async fn run_with_db_checks(
    config: &Path,
    schema: &Path,
    db_url: Option<&str>,
    against_db: Option<&str>,
    schemas: &[String],
    json: bool,
) -> bool {
    let mut checks = run_checks(config, schema, db_url);
    if let Some(url) = against_db {
        checks.extend(changelog_contract_checks(url).await);
        checks.extend(body_resolution_checks(url, schemas).await);
    }

    if json {
        print_json_report(&checks);
    } else {
        print_text_report(&checks);
    }

    checks.iter().all(|c| c.status != CheckStatus::Fail)
}

/// Run the PL/pgSQL body-resolution pass and map its outcome to doctor checks.
///
/// Never panics: connection or analysis failures become `Fail` checks, an
/// absent `plpgsql_check` extension becomes a `Warn` (skipped), and each
/// unresolved internal call becomes its own `Fail`.
async fn body_resolution_checks(db_url: &str, schemas: &[String]) -> Vec<DoctorCheck> {
    const NAME: &str = "PL/pgSQL body resolution";

    let catalog = match PgCatalog::connect(db_url) {
        Ok(c) => c,
        Err(e) => {
            return vec![DoctorCheck::fail(
                NAME,
                format!("cannot connect: {e}"),
                "Pass a reachable postgres:// URL to --against-db",
            )];
        },
    };

    match catalog.plpgsql_check_unresolved_calls(schemas).await {
        Err(e) => vec![DoctorCheck::fail(
            NAME,
            format!("pass failed: {e}"),
            "Ensure the connecting role may CREATE EXTENSION plpgsql_check",
        )],
        Ok(PlpgsqlCheckOutcome::Unavailable) => vec![DoctorCheck::warn(
            NAME,
            "plpgsql_check not installed — skipped",
            "Install the plpgsql_check extension to enable internal-call checking",
        )],
        Ok(PlpgsqlCheckOutcome::Ran { errors }) if errors.is_empty() => vec![DoctorCheck::pass(
            NAME,
            format!("all internal calls resolve in {}", schemas.join(", ")),
        )],
        Ok(PlpgsqlCheckOutcome::Ran { errors }) => errors
            .iter()
            .map(|e| {
                let location = e.lineno.map_or_else(String::new, |l| format!(" (line {l})"));
                DoctorCheck::fail(
                    NAME,
                    format!("{}{location}: {}", e.caller, e.message),
                    "A migration changed a function signature — update its internal callers",
                )
            })
            .collect(),
    }
}

const CHANGELOG_CONTRACT_NAME: &str = "Change-log contract";

/// Run the change-log contract drift check against a live database (#380).
///
/// Connects, reads `core.tb_entity_change_log` from `information_schema.columns`,
/// and classifies the drift via [`changelog_contract_drift`]. A connection or
/// introspection failure becomes a single `Fail` check (never panics).
async fn changelog_contract_checks(db_url: &str) -> Vec<DoctorCheck> {
    let catalog = match PgCatalog::connect(db_url) {
        Ok(c) => c,
        Err(e) => {
            return vec![DoctorCheck::fail(
                CHANGELOG_CONTRACT_NAME,
                format!("cannot connect: {e}"),
                "Pass a reachable postgres:// URL to --against-db",
            )];
        },
    };

    match catalog.table_columns("core", "tb_entity_change_log").await {
        Ok(live) => changelog_contract_drift(&live),
        Err(e) => vec![DoctorCheck::fail(
            CHANGELOG_CONTRACT_NAME,
            format!("introspection failed: {e}"),
            "Ensure the connecting role can read information_schema for the core schema",
        )],
    }
}

/// Compare a live `core.tb_entity_change_log` against the shipped contract and
/// map the drift to doctor checks.
///
/// Pure — no database access — so the full classification matrix is unit-tested
/// without a connection. The expected column set is the single source of truth
/// in [`fraiseql_observers::migrations::ENTITY_CHANGE_LOG_CONTRACT`].
///
/// - **Type mismatch** on a pre-existing column → `Fail`. The contract migration is purely additive
///   (`ADD COLUMN IF NOT EXISTS`), so it no-ops on a column that already exists and **cannot retype
///   it** — e.g. a legacy `object_id text` the contract wants as `uuid` (this bit the #149 e2e). A
///   manual `ALTER COLUMN … TYPE` is required.
/// - **Missing** contract column → `Warn`: `fraiseql migrate up` adds it.
/// - **Extra** non-contract column → `Warn`: the migration leaves it untouched.
/// - Otherwise → `Pass`.
///
/// An empty `live` means the table is absent → a single `Warn` pointing at the
/// migration.
pub(crate) fn changelog_contract_drift(live: &[LiveColumn]) -> Vec<DoctorCheck> {
    if live.is_empty() {
        return vec![DoctorCheck::warn(
            CHANGELOG_CONTRACT_NAME,
            "core.tb_entity_change_log not found",
            "Run `fraiseql migrate up` to install the change-log contract",
        )];
    }

    let mut checks = Vec::new();

    // Type drift on pre-existing columns — the additive migration cannot fix it.
    for contract in ENTITY_CHANGE_LOG_CONTRACT {
        let Some(col) = live.iter().find(|c| c.name == contract.name) else {
            continue;
        };
        if col.udt_name == contract.udt {
            continue;
        }
        checks.push(DoctorCheck::fail(
            CHANGELOG_CONTRACT_NAME,
            format!(
                "column `{}` is `{}`, contract expects `{}`",
                contract.name, col.udt_name, contract.udt
            ),
            format!(
                "The additive migration cannot retype an existing column; ALTER it manually, \
                 e.g. ALTER TABLE core.tb_entity_change_log ALTER COLUMN {name} TYPE {udt} \
                 USING {name}::{udt};",
                name = contract.name,
                udt = contract.udt,
            ),
        ));
    }

    // Missing contract columns — the migration adds them (additive).
    let missing: Vec<&str> = ENTITY_CHANGE_LOG_CONTRACT
        .iter()
        .map(|c| c.name)
        .filter(|name| !live.iter().any(|c| c.name == *name))
        .collect();
    if !missing.is_empty() {
        checks.push(DoctorCheck::warn(
            CHANGELOG_CONTRACT_NAME,
            format!("{} contract column(s) missing: {}", missing.len(), missing.join(", ")),
            "Run `fraiseql migrate up` — the additive migration adds these columns",
        ));
    }

    // Extra columns — present live, not in the contract; the migration keeps them.
    let extra: Vec<&str> = live
        .iter()
        .map(|c| c.name.as_str())
        .filter(|name| !ENTITY_CHANGE_LOG_CONTRACT.iter().any(|c| c.name == *name))
        .collect();
    if !extra.is_empty() {
        checks.push(DoctorCheck::warn(
            CHANGELOG_CONTRACT_NAME,
            format!("{} non-contract column(s) present: {}", extra.len(), extra.join(", ")),
            "Left untouched by the migration — app-specific columns are safe to keep",
        ));
    }

    if checks.is_empty() {
        checks.push(DoctorCheck::pass(
            CHANGELOG_CONTRACT_NAME,
            format!(
                "all {} contract columns present and correctly typed",
                ENTITY_CHANGE_LOG_CONTRACT.len()
            ),
        ));
    }

    checks
}
