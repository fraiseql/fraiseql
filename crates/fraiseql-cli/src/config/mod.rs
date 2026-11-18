//! Configuration loading and management
//!
//! This module handles loading configuration from fraiseql.toml files,
//! including security settings, project metadata, and compilation options.

pub mod discovery;
pub mod inject_defaults;
pub mod runtime;
pub mod security;
pub mod security_merge;
pub mod toml_schema;

use anyhow::Result;
pub use discovery::ConfigSource;
use fraiseql_core::utils::config_diagnostic::{ConfigDiagnostic, DiagnosticCode};
pub use runtime::{DatabaseRuntimeConfig, ServerRuntimeConfig};
use serde::{Deserialize, Serialize};
pub use toml_schema::TomlSchema;

/// `[project]`: the metadata of every `fraiseql.toml`, the compile input or the file read
/// beside a schema document.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectMetadata {
    /// Project name (`fraiseql sbom` names the component after it).
    pub name:            String,
    /// Project version (`fraiseql sbom` versions the component after it).
    pub version:         String,
    /// Target database backend. Only `"postgresql"` is accepted: MySQL, SQLite and SQL
    /// Server were removed in v2.15.0 (#374), and [`Self::validate_database_target`]
    /// refuses them.
    pub database_target: Option<String>,
}

impl ProjectMetadata {
    /// Refuse a `database_target` other than `postgresql`.
    ///
    /// # Errors
    ///
    /// Returns an error naming the removed backends and `docs/database-compatibility.md`
    /// when the target is anything but `postgresql` (compared case-insensitively).
    pub fn validate_database_target(&self) -> Result<()> {
        match &self.database_target {
            Some(target) => validate_database_target("[project]", target),
            None => Ok(()),
        }
    }
}

impl Default for ProjectMetadata {
    fn default() -> Self {
        Self {
            name:            "my-fraiseql-app".to_string(),
            version:         "1.0.0".to_string(),
            database_target: None,
        }
    }
}

/// The tables that author a schema: a TOML schema takes them, a `fraiseql.toml` read beside
/// a schema document does not, since the document authors the schema there. Each is refused
/// by name with where it belongs, where `deny_unknown_fields` would only say the key is
/// unknown.
const AUTHORING_SECTIONS: &[&str] = &[
    "types",
    "queries",
    "mutations",
    "includes",
    "domain_discovery",
    "crud",
    "analytics",
];

/// Refuse the first top-level key of [`AUTHORING_SECTIONS`] that `table` sets.
///
/// # Errors
///
/// Returns an error naming the table and where it belongs.
pub(crate) fn refuse_misplaced_sections(table: &toml::Table) -> Result<()> {
    if let Some(name) = AUTHORING_SECTIONS.iter().find(|name| table.contains_key(**name)) {
        return Err(ConfigDiagnostic::new(
            DiagnosticCode::Misplaced,
            name,
            "",
            format!(
                "fraiseql.toml: `{name}` does not belong in a project configuration, the \
                 fraiseql.toml read beside a schema document: it authors the schema, which the \
                 schema document does here. Declare it there, or author the schema in TOML and \
                 pass that file as the compile input (`fraiseql compile <schema>.toml`)."
            ),
        )
        .with_belongs_in(SCHEMA_DOCUMENT)
        .into());
    }
    Ok(())
}

/// Keys `fraiseql.toml` once accepted that `fraiseql run` never applied: the section, the
/// key, and what to do instead.
///
/// Each is refused by name when it is written, even at its old default, so a project
/// that relied on one learns it never had an effect. Without this, `deny_unknown_fields`
/// would still refuse them, but as an unknown field, with no word of what to use.
const UNAPPLIED_KEYS: &[(&str, &str, &str)] = &[
    (
        "server",
        "request_timeout_ms",
        "set `request_timeout_secs` (in seconds) in the server configuration file, the file \
         FRAISEQL_CONFIG names",
    ),
    ("server", "keep_alive_secs", "remove it: the server has no keep-alive setting"),
    (
        "server.cors",
        "credentials",
        "remove it: the server never sends `Access-Control-Allow-Credentials`, so a \
         browser app on another origin cannot send the session cookie; serve the app \
         from the same origin as the API, or authenticate with a bearer token",
    ),
    (
        "database",
        "idle_timeout_ms",
        "remove it: the database pool has no idle timeout",
    ),
];

/// What a retired spelling is told.
enum Retired {
    /// The section moved: write this instead.
    Write(&'static str),
    /// The key was renamed: it is spelled this.
    SpelledAs(&'static str),
    /// Nothing reads the key: it is refused rather than accepted and ignored.
    Unread,
}

/// Spellings `fraiseql.toml` no longer takes, each with what replaced it: the section, the
/// key (or the section itself, when `key` is empty), and what it is told.
///
/// Refused by name, with no alias, so a file that used one learns its replacement instead
/// of an unknown field. Checked in every role the file plays.
const RETIRED_SPELLINGS: &[(&str, &str, Retired)] = &[
    ("schema", "", Retired::Write("`[project]`")),
    (
        "observability",
        "",
        Retired::Write(
            "`metrics_enabled`, `metrics_token`, `tracing_enabled`, `otlp_endpoint` and \
             `tracing_service_name` at the top level of the server's configuration file, the \
             file FRAISEQL_CONFIG names (logging through `RUST_LOG`)",
        ),
    ),
    ("project", "description", Retired::Unread),
    ("", "naming_convention", Retired::Write("`[naming] convention`")),
    ("security.constant_time", "", Retired::Unread),
    (
        "security.audit_logging",
        "",
        Retired::Write("`[security.enterprise] audit_logging_enabled`"),
    ),
    (
        "security.rate_limiting",
        "failed_login_max_requests",
        Retired::SpelledAs("`failed_login_max_attempts`"),
    ),
    (
        "security.rate_limiting",
        "failed_login_window_secs",
        Retired::SpelledAs("`failed_login_lockout_secs`"),
    ),
    // The project configuration's former `[fraiseql]` prefix: every table it held is a
    // top-level table now, as in a TOML schema.
    ("fraiseql.security.constant_time", "", Retired::Unread),
    ("fraiseql.security", "", Retired::Write("`[security]`")),
    ("fraiseql.tenancy", "", Retired::Write("`[tenancy]`")),
    ("fraiseql.session_variables", "", Retired::Write("`[session_variables]`")),
    ("fraiseql.naming", "", Retired::Write("`[naming]`")),
    ("fraiseql.cost_weights", "", Retired::Write("`[cost_weights]`")),
    ("fraiseql.mutations", "", Retired::Write("`[mutation_defaults]`")),
    ("fraiseql", "schema_file", Retired::Unread),
    ("fraiseql", "output_file", Retired::Unread),
    (
        "fraiseql",
        "",
        Retired::Write("each of its tables at the top level, without the `fraiseql.` prefix"),
    ),
];

/// How `table` writes `key` of `section` (the section itself when `key` is empty), or `None`
/// when it does not.
fn written_spelling(table: &toml::Table, section: &str, key: &str) -> Option<String> {
    let mut found = Some(table);
    for part in section.split('.').filter(|part| !part.is_empty()) {
        found = found.and_then(|t| t.get(part)).and_then(toml::Value::as_table);
    }
    let found = found?;
    if key.is_empty() {
        Some(format!("[{section}]"))
    } else if !found.contains_key(key) {
        None
    } else if section.is_empty() {
        Some(key.to_string())
    } else {
        Some(format!("[{section}] {key}"))
    }
}

/// Where a misplaced `fraiseql.toml` setting belongs, as a refusal names it.
const SERVER_FILE: &str = "the server's configuration file (the file FRAISEQL_CONFIG names)";
/// Where a schema-authoring table beside a schema document belongs.
const SCHEMA_DOCUMENT: &str = "the schema document";

/// A configuration refusal of `table` (and `key`, when not empty), with nothing to write
/// instead.
pub(crate) fn refusal(
    code: DiagnosticCode,
    table: &str,
    key: &str,
    message: String,
) -> anyhow::Error {
    ConfigDiagnostic::new(code, table, key, message).into()
}

/// `error`, its configuration diagnostic (if it carries one) naming the file `path`.
///
/// A refusal is raised where the table is read, which does not know the path; the loader
/// that read the file names it on the way out.
pub(crate) fn name_the_file(mut error: anyhow::Error, path: &str) -> anyhow::Error {
    if let Some(diagnostic) = error.downcast_mut::<ConfigDiagnostic>() {
        diagnostic.file.get_or_insert_with(|| path.to_owned());
    }
    error
}

/// Refuse the first spelling of [`RETIRED_SPELLINGS`] that `table` writes.
///
/// # Errors
///
/// Returns an error naming the retired spelling and what replaced it.
pub(crate) fn refuse_retired_spellings(table: &toml::Table) -> Result<()> {
    for (section, key, retired) in RETIRED_SPELLINGS {
        let Some(spelling) = written_spelling(table, section, key) else {
            continue;
        };
        match retired {
            Retired::Write(instead) => {
                return Err(ConfigDiagnostic::new(
                    DiagnosticCode::Retired,
                    section,
                    key,
                    format!("fraiseql.toml: `{spelling}` is no longer accepted; write {instead}"),
                )
                .with_replacement(instead)
                .into());
            },
            Retired::SpelledAs(instead) => {
                return Err(ConfigDiagnostic::new(
                    DiagnosticCode::Retired,
                    section,
                    key,
                    format!(
                        "fraiseql.toml: `{spelling}` is no longer accepted; it is spelled {instead}"
                    ),
                )
                .with_replacement(instead)
                .into());
            },
            Retired::Unread => {
                return Err(refusal(
                    DiagnosticCode::Unread,
                    section,
                    key,
                    format!("fraiseql.toml: `{spelling}` is no longer accepted: nothing reads it"),
                ));
            },
        }
    }
    Ok(())
}

/// Where a spelling of the server's configuration file belongs instead.
enum ServerFile {
    /// `fraiseql.toml` makes the same setting, spelled this.
    SpelledHere(&'static str),
    /// Only the server's file makes the setting.
    ReadThere,
}

/// Spellings of the server's configuration file (the file `FRAISEQL_CONFIG` names) that
/// `fraiseql.toml` does not take: the section, the key (or the section itself, when `key` is
/// empty), and where the setting belongs. The two files spell some settings differently, and
/// `[tenancy]` holds unrelated keys in each, so a key copied from one into the other is named
/// rather than reported as an unknown field.
const SERVER_FILE_SPELLINGS: &[(&str, &str, ServerFile)] = &[
    ("", "bind_addr", ServerFile::SpelledHere("`[server] host` and `port`")),
    ("", "database_url", ServerFile::SpelledHere("`[database] url`")),
    ("", "pool_min_size", ServerFile::SpelledHere("`[database] pool_min`")),
    ("", "pool_max_size", ServerFile::SpelledHere("`[database] pool_max`")),
    (
        "",
        "pool_timeout_secs",
        ServerFile::SpelledHere("`[database] connect_timeout_ms`, in milliseconds"),
    ),
    ("tls", "", ServerFile::SpelledHere("`[server.tls]`")),
    ("database_tls", "", ServerFile::SpelledHere("`[database] ssl_mode`")),
    (
        "transaction_retry",
        "",
        ServerFile::SpelledHere("`[database.transaction_retry]`"),
    ),
    ("server.tls", "cert_path", ServerFile::SpelledHere("`cert_file`")),
    ("server.tls", "key_path", ServerFile::SpelledHere("`key_file`")),
    ("database", "postgres_ssl_mode", ServerFile::SpelledHere("`ssl_mode`")),
    ("tenancy", "runtime", ServerFile::ReadThere),
    ("tenancy", "storage_quota", ServerFile::ReadThere),
];

/// Refuse the first spelling of [`SERVER_FILE_SPELLINGS`] that `table` writes.
///
/// # Errors
///
/// Returns an error naming the spelling, the server's configuration file, and what
/// `fraiseql.toml` writes instead.
pub(crate) fn refuse_server_file_spellings(table: &toml::Table) -> Result<()> {
    for (section, key, belongs) in SERVER_FILE_SPELLINGS {
        let Some(spelling) = written_spelling(table, section, key) else {
            continue;
        };
        match belongs {
            ServerFile::SpelledHere(instead) => {
                return Err(ConfigDiagnostic::new(
                    DiagnosticCode::Misplaced,
                    section,
                    key,
                    format!(
                        "fraiseql.toml: `{spelling}` is how the server's configuration file (the \
                         file FRAISEQL_CONFIG names) spells this setting; in fraiseql.toml write \
                         {instead}"
                    ),
                )
                .with_replacement(instead)
                .with_belongs_in(SERVER_FILE)
                .into());
            },
            ServerFile::ReadThere => {
                return Err(ConfigDiagnostic::new(
                    DiagnosticCode::Misplaced,
                    section,
                    key,
                    format!(
                        "fraiseql.toml: `{spelling}` is read from the server's configuration \
                         file, the file FRAISEQL_CONFIG names; set it there"
                    ),
                )
                .with_belongs_in(SERVER_FILE)
                .into());
            },
        }
    }
    Ok(())
}

/// Refuse the first key of [`UNAPPLIED_KEYS`] that `table` sets.
///
/// # Errors
///
/// Returns an error naming the key and what to do instead.
pub(crate) fn refuse_unapplied_keys(table: &toml::Table) -> Result<()> {
    for (section, key, instead) in UNAPPLIED_KEYS {
        let mut found = Some(table);
        for part in section.split('.') {
            found = found.and_then(|t| t.get(part)).and_then(toml::Value::as_table);
        }
        if found.is_some_and(|t| t.contains_key(*key)) {
            return Err(refusal(
                DiagnosticCode::Unapplied,
                section,
                key,
                format!(
                    "fraiseql.toml: `[{section}] {key}` was never applied by `fraiseql run` and \
                     is no longer accepted; {instead}"
                ),
            ));
        }
    }
    Ok(())
}

/// Refuse a `database_target` other than `postgresql` (compared case-insensitively).
///
/// The rule for `[project] database_target`, which `fraiseql init` writes into every new
/// `fraiseql.toml`. `section` names the key in the message.
///
/// # Errors
///
/// Returns an error naming the removed backends and `docs/database-compatibility.md`.
pub(crate) fn validate_database_target(section: &str, target: &str) -> Result<()> {
    let target = target.trim();
    if target.eq_ignore_ascii_case("postgresql") {
        return Ok(());
    }
    anyhow::bail!(
        "{section} database_target = {target:?} is not supported: FraiseQL targets \
         PostgreSQL only. The `mysql`, `sqlite` and `sqlserver` backends were removed in \
         v2.15.0 (#374), so a schema declaring one would compile PostgreSQL SQL. Set \
         `database_target = \"postgresql\"` or remove the key. See \
         docs/database-compatibility.md."
    )
}

/// Expand `${VAR}` environment variable placeholders in a string.
///
/// Unknown variables are left as-is: a `fraiseql.toml` is compiled where the deployment's
/// variables may not be set.
///
/// # Errors
///
/// Returns an error naming a variable whose value could break the TOML around it.
pub(crate) fn expand_env_vars(content: &str) -> Result<String> {
    fraiseql_core::utils::env_expand::expand_env_vars(
        content,
        fraiseql_core::utils::env_expand::UnsetVariable::KeepLiteral,
    )
    .map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod tests;
