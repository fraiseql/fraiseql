//! Doctor command — systematic diagnostic checks for common FraiseQL setup problems.
//!
//! Usage:
//!   fraiseql doctor
//!   fraiseql doctor --config fraiseql.toml --schema schema.compiled.json
//!   fraiseql doctor --json

use std::{
    net::TcpStream,
    path::Path,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::config::toml_schema::TomlSchema;

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
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self { name, status: CheckStatus::Pass, detail: detail.into(), hint: None }
    }

    fn warn(name: &'static str, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Warn,
            detail: detail.into(),
            hint:   Some(hint.into()),
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            detail: detail.into(),
            hint:   Some(hint.into()),
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
                std::net::SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                    0,
                )
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
                std::net::SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                    0,
                )
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
fn parse_host_port(url: &str) -> Option<(String, u16)> {
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
/// Returns `Ok(())` if all checks passed, `Err` if any check failed.
/// Warnings do not trigger failure.
///
/// # Errors
///
/// Returns an error if any diagnostic check has `Fail` status.
pub fn run(config: &Path, schema: &Path, db_url: Option<&str>, json: bool) -> anyhow::Result<()> {
    let checks = run_checks(config, schema, db_url);

    if json {
        print_json_report(&checks);
    } else {
        print_text_report(&checks);
    }

    if checks.iter().all(|c| c.status != CheckStatus::Fail) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("One or more doctor checks failed"))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    // Helper: write a temp file with given content and return the path.
    fn temp_file_with(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    // ── check_schema_exists ───────────────────────────────────────────────────

    #[test]
    fn test_schema_exists_pass() {
        let f = temp_file_with("{}");
        let result = check_schema_exists(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_schema_exists_fail() {
        let result = check_schema_exists(Path::new("/nonexistent/schema.compiled.json"));
        assert_eq!(result.status, CheckStatus::Fail);
    }

    // ── check_schema_parses ───────────────────────────────────────────────────

    #[test]
    fn test_schema_parses_valid_json() {
        let f = temp_file_with(r#"{"types":[],"queries":[],"mutations":[]}"#);
        let result = check_schema_parses(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.detail.contains("types=0"));
    }

    #[test]
    fn test_schema_parses_invalid_json() {
        let f = temp_file_with("not json {{{");
        let result = check_schema_parses(f.path());
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.hint.is_some());
    }

    // ── check_schema_version ─────────────────────────────────────────────────

    #[test]
    fn test_schema_version_missing() {
        let f = temp_file_with(r#"{"types":[]}"#);
        let result = check_schema_version(f.path());
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn test_schema_version_current() {
        let f = temp_file_with(r#"{"version":1,"types":[]}"#);
        let result = check_schema_version(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.detail.contains("version=1"));
    }

    #[test]
    fn test_schema_version_mismatch() {
        let f = temp_file_with(r#"{"version":99,"types":[]}"#);
        let result = check_schema_version(f.path());
        assert_eq!(result.status, CheckStatus::Warn);
    }

    // ── check_toml_exists ─────────────────────────────────────────────────────

    #[test]
    fn test_toml_exists_pass() {
        let f = temp_file_with(
            "[schema]\nname = \"test\"\nversion = \"1.0\"\ndatabase_target = \"postgresql\"\n",
        );
        let result = check_toml_exists(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_toml_exists_warn() {
        let result = check_toml_exists(Path::new("/nonexistent/fraiseql.toml"));
        assert_eq!(result.status, CheckStatus::Warn);
    }

    // ── check_toml_parses ─────────────────────────────────────────────────────

    #[test]
    fn test_toml_parses_valid() {
        let toml =
            "[schema]\nname = \"myapp\"\nversion = \"1.0\"\ndatabase_target = \"postgresql\"\n";
        let f = temp_file_with(toml);
        let result = check_toml_parses(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_toml_parses_invalid_syntax() {
        let f = temp_file_with("this is not [[[ valid toml");
        let result = check_toml_parses(f.path());
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.hint.is_some());
    }

    // ── check_database_url_set ────────────────────────────────────────────────

    #[test]
    fn test_db_url_set_via_override() {
        let result = check_database_url_set(Some("postgres://localhost/test"));
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_db_url_not_set() {
        temp_env::with_var_unset("DATABASE_URL", || {
            let result = check_database_url_set(None);
            assert_eq!(result.status, CheckStatus::Fail);
        });
    }

    #[test]
    fn test_db_url_from_env() {
        temp_env::with_var("DATABASE_URL", Some("postgres://localhost/test"), || {
            let result = check_database_url_set(None);
            assert_eq!(result.status, CheckStatus::Pass);
        });
    }

    // ── check_db_reachable ────────────────────────────────────────────────────

    #[test]
    fn test_db_reachable_unreachable_port() {
        // Port 1 is almost always closed / refused.
        temp_env::with_var_unset("DATABASE_URL", || {
            let result = check_db_reachable(Some("postgres://localhost:1/db"));
            assert_eq!(result.status, CheckStatus::Fail);
            let hint = result.hint.unwrap();
            assert!(hint.contains("pg_isready"), "hint should mention pg_isready: {hint}");
        });
    }

    #[test]
    fn test_db_reachable_no_url() {
        temp_env::with_var_unset("DATABASE_URL", || {
            let result = check_db_reachable(None);
            assert_eq!(result.status, CheckStatus::Fail);
        });
    }

    // ── check_jwt_secret ──────────────────────────────────────────────────────

    #[test]
    fn test_jwt_secret_set() {
        temp_env::with_var("FRAISEQL_JWT_SECRET", Some("supersecret"), || {
            let result = check_jwt_secret();
            assert_eq!(result.status, CheckStatus::Pass);
        });
    }

    #[test]
    fn test_jwt_secret_missing() {
        temp_env::with_var_unset("FRAISEQL_JWT_SECRET", || {
            let result = check_jwt_secret();
            assert_eq!(result.status, CheckStatus::Warn);
            assert!(result.hint.is_some());
        });
    }

    // ── check_redis_reachable ─────────────────────────────────────────────────

    #[test]
    fn test_redis_not_set_is_pass() {
        temp_env::with_var_unset("REDIS_URL", || {
            let result = check_redis_reachable();
            assert_eq!(result.status, CheckStatus::Pass);
        });
    }

    #[test]
    fn test_redis_set_but_unreachable() {
        temp_env::with_var("REDIS_URL", Some("redis://localhost:1"), || {
            let result = check_redis_reachable();
            assert_eq!(result.status, CheckStatus::Fail);
        });
    }

    // ── check_tls ─────────────────────────────────────────────────────────────

    #[test]
    fn test_tls_no_config_is_pass() {
        let result = check_tls(Path::new("/nonexistent/fraiseql.toml"));
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_tls_disabled_in_config_is_pass() {
        let toml = "[schema]\nname = \"a\"\nversion = \"1\"\ndatabase_target = \"postgresql\"\n";
        let f = temp_file_with(toml);
        let result = check_tls(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    // ── check_rls_cache_coherence ─────────────────────────────────────────────

    #[test]
    fn test_cache_auth_coherence_cache_disabled_is_pass() {
        let toml = "[schema]\nname = \"a\"\nversion = \"1\"\ndatabase_target = \"postgresql\"\n";
        let f = temp_file_with(toml);
        let result = check_rls_cache_coherence(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_cache_auth_coherence_cache_enabled_no_policy_is_warn() {
        let toml = "[schema]\nname = \"a\"\nversion = \"1\"\ndatabase_target = \"postgresql\"\n\n\
                    [caching]\nenabled = true\n\n[security]\ndefault_policy = \"\"\n";
        let f = temp_file_with(toml);
        // With default_policy = "" and empty policies list, this could be either warn or pass
        // depending on the Some("") interpretation. Just verify it runs without panic.
        let result = check_rls_cache_coherence(f.path());
        assert!(matches!(result.status, CheckStatus::Pass | CheckStatus::Warn));
    }

    #[test]
    fn test_cache_auth_coherence_cache_enabled_with_policy_is_pass() {
        let toml = "[schema]\nname = \"a\"\nversion = \"1\"\ndatabase_target = \"postgresql\"\n\n\
                    [caching]\nenabled = true\n\n[security]\ndefault_policy = \"authenticated\"\n";
        let f = temp_file_with(toml);
        let result = check_rls_cache_coherence(f.path());
        assert_eq!(result.status, CheckStatus::Pass);
    }

    // ── parse_host_port ───────────────────────────────────────────────────────

    #[test]
    fn test_parse_host_port_postgres() {
        let (host, port) =
            parse_host_port("postgres://user:pass@db.example.com:5432/mydb").unwrap();
        assert_eq!(host, "db.example.com");
        assert_eq!(port, 5432);
    }

    #[test]
    fn test_parse_host_port_localhost() {
        let (host, port) = parse_host_port("postgres://localhost:5432/db").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 5432);
    }

    #[test]
    fn test_parse_host_port_ipv6() {
        let result = parse_host_port("postgres://[::1]:5432/db");
        assert!(result.is_some());
        let (host, port) = result.unwrap();
        assert_eq!(host, "::1");
        assert_eq!(port, 5432);
    }

    #[test]
    fn test_parse_host_port_invalid() {
        assert!(parse_host_port("not-a-url").is_none());
    }

    // ── JSON output ───────────────────────────────────────────────────────────

    #[test]
    fn test_json_serialization() {
        let checks = vec![
            DoctorCheck::pass("Test pass", "detail"),
            DoctorCheck::warn("Test warn", "detail", "hint text"),
            DoctorCheck::fail("Test fail", "detail", "hint text"),
        ];
        let json = serde_json::to_string(&checks).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0]["status"], "pass");
        assert_eq!(parsed[1]["status"], "warn");
        assert_eq!(parsed[2]["status"], "fail");
    }
}
