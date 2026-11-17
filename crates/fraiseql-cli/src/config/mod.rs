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

/// What a top-level key of a TOML schema that does not belong in a project config is told.
enum Misplaced {
    /// It authors the schema, which beside a schema document the document does.
    Authoring,
    /// It is refused in a TOML schema too, with this pointer.
    Elsewhere(&'static str),
}

/// The tables a TOML schema takes that a `fraiseql.toml` read beside a schema document
/// does not: each is refused by name with where it belongs, where `deny_unknown_fields`
/// would only say the key is unknown.
const MISPLACED_SECTIONS: &[(&str, Misplaced)] = &[
    ("types", Misplaced::Authoring),
    ("queries", Misplaced::Authoring),
    ("mutations", Misplaced::Authoring),
    ("includes", Misplaced::Authoring),
    ("domain_discovery", Misplaced::Authoring),
    ("crud", Misplaced::Authoring),
    ("analytics", Misplaced::Authoring),
    (
        "observability",
        Misplaced::Elsewhere(
            "metrics and tracing are configured on the server, under `[metrics]` and \
             `[tracing]` in the file FRAISEQL_CONFIG names",
        ),
    ),
];

/// Refuse the first top-level key of [`MISPLACED_SECTIONS`] that `table` sets.
///
/// # Errors
///
/// Returns an error naming the table and where it belongs.
pub(crate) fn refuse_misplaced_sections(table: &toml::Table) -> Result<()> {
    for (name, misplaced) in MISPLACED_SECTIONS {
        if !table.contains_key(*name) {
            continue;
        }
        let pointer = match misplaced {
            Misplaced::Authoring => {
                "it authors the schema, which the schema document does \
                                     here. Declare it there, or author the schema in TOML and \
                                     pass that file as the compile input (`fraiseql compile \
                                     <schema>.toml`)"
            },
            Misplaced::Elsewhere(pointer) => pointer,
        };
        anyhow::bail!(
            "fraiseql.toml: `{name}` does not belong in a project configuration, the \
             fraiseql.toml read beside a schema document: {pointer}."
        );
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

/// Refuse the first spelling of [`RETIRED_SPELLINGS`] that `table` writes.
///
/// # Errors
///
/// Returns an error naming the retired spelling and what replaced it.
pub(crate) fn refuse_retired_spellings(table: &toml::Table) -> Result<()> {
    for (section, key, retired) in RETIRED_SPELLINGS {
        let mut found = Some(table);
        for part in section.split('.').filter(|part| !part.is_empty()) {
            found = found.and_then(|t| t.get(part)).and_then(toml::Value::as_table);
        }
        let Some(found) = found else { continue };
        let spelling = if key.is_empty() {
            format!("[{section}]")
        } else if !found.contains_key(*key) {
            continue;
        } else if section.is_empty() {
            (*key).to_string()
        } else {
            format!("[{section}] {key}")
        };
        match retired {
            Retired::Write(instead) => {
                anyhow::bail!("fraiseql.toml: `{spelling}` is no longer accepted; write {instead}")
            },
            Retired::SpelledAs(instead) => anyhow::bail!(
                "fraiseql.toml: `{spelling}` is no longer accepted; it is spelled {instead}"
            ),
            Retired::Unread => {
                anyhow::bail!("fraiseql.toml: `{spelling}` is no longer accepted: nothing reads it")
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
            anyhow::bail!(
                "fraiseql.toml: `[{section}] {key}` was never applied by `fraiseql run` and is \
                 no longer accepted; {instead}"
            );
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
/// Unknown variables are left as-is (no panic, silent passthrough).
#[allow(clippy::expect_used)] // Reason: regex pattern is a compile-time constant guaranteed to be valid
pub(crate) fn expand_env_vars(content: &str) -> Result<String> {
    use std::sync::LazyLock;

    static ENV_VAR_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("env var regex is valid")
    });

    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;

    for cap in ENV_VAR_REGEX.captures_iter(content) {
        // INVARIANT: Regex captures iterator yields Captures where group 0 (the full match) is
        // always present
        let m = cap.get(0).expect("INVARIANT: Regex captures group 0 is always present");
        result.push_str(&content[last_end..m.start()]);
        let var_name = &cap[1];
        match std::env::var(var_name) {
            Ok(val) => {
                validate_env_var_value(var_name, &val)?;
                result.push_str(&val);
            },
            Err(_) => {
                result.push_str(&format!("${{{}}}", var_name));
            },
        }
        last_end = m.end();
    }
    result.push_str(&content[last_end..]);
    Ok(result)
}

fn validate_env_var_value(var_name: &str, value: &str) -> Result<()> {
    if value.contains('\n') {
        anyhow::bail!("Environment variable {} contains newline character", var_name);
    }
    if value.contains('\r') {
        anyhow::bail!("Environment variable {} contains carriage return character", var_name);
    }
    if value.contains('\0') {
        anyhow::bail!("Environment variable {} contains null character", var_name);
    }
    // Check for unescaped TOML metacharacters: ", ', \, ], [, {, }
    if value.contains('"')
        || value.contains('\'')
        || value.contains('\\')
        || value.contains(']')
        || value.contains('[')
        || value.contains('{')
        || value.contains('}')
    {
        anyhow::bail!(
            "Environment variable {} contains TOML metacharacter that could break TOML parsing",
            var_name
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
