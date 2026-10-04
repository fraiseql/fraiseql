//! `fraiseql migrate` - Database migration wrapper
//!
//! Wraps confiture for database migrations, providing a unified CLI
//! experience without requiring users to install confiture separately.

use std::{path::Path, process::Command};

use anyhow::{Context, Result};
use fraiseql_db::postgres::PostgresTlsConfig;
use tracing::info;

use crate::{config::DatabaseRuntimeConfig, output::OutputFormatter};

/// Migration subcommand
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum MigrateAction {
    /// Apply pending migrations
    Up {
        /// Database connection URL
        database_url: String,
        /// Migration directory
        dir:          String,
    },
    /// Roll back migrations
    Down {
        /// Database connection URL
        database_url: String,
        /// Migration directory
        dir:          String,
        /// Number of steps to roll back
        steps:        u32,
    },
    /// Show migration status
    Status {
        /// Database connection URL
        database_url: String,
        /// Migration directory
        dir:          String,
    },
    /// Create a new migration file
    Create {
        /// Migration name
        name: String,
        /// Migration directory
        dir:  String,
    },
    /// Generate a new migration from schema diff
    Generate {
        /// Migration name
        name: String,
        /// Migration directory
        dir:  String,
    },
    /// Validate migration files for naming, idempotency, and drift
    Validate {
        /// Migration directory
        dir: String,
    },
    /// Pre-deploy safety check on pending migrations
    Preflight {
        /// Migration directory
        dir: String,
    },
}

/// Run the migrate command
///
/// # Errors
///
/// Returns an error if `confiture` is not installed, or if the underlying
/// `confiture` subprocess fails (non-zero exit status or spawn failure).
pub fn run(action: &MigrateAction, formatter: &OutputFormatter) -> Result<()> {
    // Check if confiture is installed
    if !is_confiture_installed() {
        print_install_instructions(formatter);
        anyhow::bail!("confiture is not installed. See instructions above.");
    }

    match action {
        MigrateAction::Up { database_url, dir } => run_up(database_url, dir, formatter),
        MigrateAction::Down {
            database_url,
            dir,
            steps,
        } => run_down(database_url, dir, *steps, formatter),
        MigrateAction::Status { database_url, dir } => run_status(database_url, dir),
        MigrateAction::Create { name, dir } => run_create(name, dir, formatter),
        MigrateAction::Generate { name, dir } => run_generate(name, dir, formatter),
        MigrateAction::Validate { dir } => run_validate(dir),
        MigrateAction::Preflight { dir } => run_preflight(dir, formatter),
    }
}

/// Resolve the database URL: use explicit flag, or fall back to fraiseql.toml
///
/// # Errors
///
/// Returns an error if `fraiseql.toml` exists but cannot be read or parsed, or
/// if no database URL can be found from any source (flag, TOML, or `DATABASE_URL`).
pub fn resolve_database_url(explicit: Option<&str>) -> Result<String> {
    resolve_database(explicit).map(|db| db.url)
}

/// A database URL and the transport security to connect to it with.
#[derive(Debug, Clone)]
pub struct ResolvedDatabase {
    /// The connection string.
    pub url: String,
    /// `[database] ssl_mode` when the URL came from `fraiseql.toml`; otherwise unset,
    /// which leaves the URL's own `sslmode` in charge.
    pub tls: PostgresTlsConfig,
}

/// Resolve the database URL like [`resolve_database_url`], together with its TLS
/// settings (#1429).
///
/// A URL taken from `fraiseql.toml`'s `[database]` section brings that section's
/// `ssl_mode` with it: the two are one setting written in two keys. A URL from the flag
/// or `DATABASE_URL` carries its own `?sslmode=`.
///
/// # Errors
///
/// Returns an error if `fraiseql.toml` exists but cannot be read or parsed, if its
/// `ssl_mode` names a mode the connector cannot honour, or if no database URL can be
/// found from any source (flag, TOML, or `DATABASE_URL`).
pub fn resolve_database(explicit: Option<&str>) -> Result<ResolvedDatabase> {
    resolve_configured_database(explicit)?.ok_or_else(|| {
        anyhow::anyhow!(
            "No database URL provided. Use --database, set [database].url in fraiseql.toml, \
             or set DATABASE_URL environment variable."
        )
    })
}

/// [`resolve_database`] for a command whose database is optional.
///
/// "No source names a database" is `Ok(None)` rather than an error. A configuration that
/// names one badly — an unparseable `fraiseql.toml`, an unknown `ssl_mode` — is still an
/// error.
///
/// # Errors
///
/// Returns an error if `fraiseql.toml` exists but cannot be read or parsed, or if its
/// `ssl_mode` names a mode the connector cannot honour.
pub fn resolve_configured_database(explicit: Option<&str>) -> Result<Option<ResolvedDatabase>> {
    if let Some(url) = explicit {
        return Ok(Some(ResolvedDatabase {
            url: url.to_string(),
            tls: PostgresTlsConfig::default(),
        }));
    }

    // Try loading from fraiseql.toml
    let toml_path = Path::new("fraiseql.toml");
    if toml_path.exists() {
        let content = std::fs::read_to_string(toml_path).context("Failed to read fraiseql.toml")?;
        let parsed: toml::Value =
            toml::from_str(&content).context("Failed to parse fraiseql.toml")?;

        let database = DatabaseRuntimeConfig::from_document(&parsed)
            .context("Failed to parse [database] in fraiseql.toml")?;
        if let Some(url) = &database.url {
            info!("Using database URL from fraiseql.toml");
            return Ok(Some(ResolvedDatabase {
                url: url.clone(),
                tls: database.postgres_tls()?,
            }));
        }
    }

    // Try DATABASE_URL env var
    if let Ok(url) = std::env::var("DATABASE_URL") {
        info!("Using DATABASE_URL environment variable");
        return Ok(Some(ResolvedDatabase {
            url,
            tls: PostgresTlsConfig::default(),
        }));
    }

    Ok(None)
}

/// Resolve the migration directory: use explicit flag, or auto-discover
pub fn resolve_migration_dir(explicit: Option<&str>) -> String {
    if let Some(dir) = explicit {
        return dir.to_string();
    }

    // Auto-discover common directory names
    for candidate in &["db/0_schema", "db/migrations", "migrations"] {
        if Path::new(candidate).is_dir() {
            info!("Auto-discovered migration directory: {candidate}");
            return (*candidate).to_string();
        }
    }

    // Default
    "db/0_schema".to_string()
}

fn is_confiture_installed() -> bool {
    Command::new("confiture")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn print_install_instructions(formatter: &OutputFormatter) {
    formatter.progress("confiture is not installed.");
    formatter.progress("");
    formatter.progress("Install it with one of:");
    formatter.progress("  cargo install confiture          # From crates.io");
    formatter.progress("  brew install confiture            # macOS (if available)");
    formatter.progress("");
    formatter.progress("Learn more: https://github.com/fraiseql/confiture");
}

fn run_up(database_url: &str, dir: &str, formatter: &OutputFormatter) -> Result<()> {
    info!("Running migrations up from {dir}");
    formatter.progress(&format!("Applying migrations from {dir}..."));

    // SECURITY: Pass database URL via environment variable, not argv, so it
    // is not visible to other users via `ps aux` or `/proc/<pid>/cmdline`.
    let status = Command::new("confiture")
        .args(["up", "--source", dir])
        .env("DATABASE_URL", database_url)
        .status()
        .context("Failed to execute confiture")?;

    if status.success() {
        formatter.progress("Migrations applied successfully.");
        Ok(())
    } else {
        anyhow::bail!("Migration failed. Check the output above for details.")
    }
}

fn run_down(database_url: &str, dir: &str, steps: u32, formatter: &OutputFormatter) -> Result<()> {
    info!("Rolling back {steps} migration(s) from {dir}");
    formatter.progress(&format!("Rolling back {steps} migration(s)..."));

    let steps_str = steps.to_string();
    let status = Command::new("confiture")
        .args(["down", "--source", dir, "--steps", &steps_str])
        .env("DATABASE_URL", database_url)
        .status()
        .context("Failed to execute confiture")?;

    if status.success() {
        formatter.progress("Rollback completed successfully.");
        Ok(())
    } else {
        anyhow::bail!("Rollback failed. Check the output above for details.")
    }
}

fn run_status(database_url: &str, dir: &str) -> Result<()> {
    info!("Checking migration status for {dir}");

    let status = Command::new("confiture")
        .args(["status", "--source", dir])
        .env("DATABASE_URL", database_url)
        .status()
        .context("Failed to execute confiture")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("Failed to get migration status.")
    }
}

fn run_create(name: &str, dir: &str, formatter: &OutputFormatter) -> Result<()> {
    info!("Creating migration: {name} in {dir}");

    // Ensure directory exists
    std::fs::create_dir_all(dir).context(format!("Failed to create migration directory: {dir}"))?;

    let status = Command::new("confiture")
        .args(["create", name, "--source", dir])
        .status()
        .context("Failed to execute confiture")?;

    if status.success() {
        formatter.progress(&format!("Migration created in {dir}/"));
        Ok(())
    } else {
        anyhow::bail!("Failed to create migration.")
    }
}

fn run_generate(name: &str, dir: &str, formatter: &OutputFormatter) -> Result<()> {
    info!("Generating migration: {name} in {dir}");

    // Ensure directory exists
    std::fs::create_dir_all(dir).context(format!("Failed to create migration directory: {dir}"))?;

    formatter.progress(&format!("Generating migration '{name}' in {dir}..."));

    // SECURITY: No database URL involved — generation is a pure file operation.
    let status = Command::new("confiture")
        .args(["migrate", "generate", name, "--migrations-dir", dir])
        .status()
        .context("Failed to execute confiture")?;

    if status.success() {
        formatter.progress(&format!("Migration generated in {dir}/"));
        Ok(())
    } else {
        anyhow::bail!("Failed to generate migration.")
    }
}

fn run_validate(dir: &str) -> Result<()> {
    info!("Validating migrations in {dir}");

    let status = Command::new("confiture")
        .args(["migrate", "validate", "--source", dir])
        .status()
        .context("Failed to execute confiture")?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("Migration validation failed. Check the output above for details.")
    }
}

fn run_preflight(dir: &str, formatter: &OutputFormatter) -> Result<()> {
    info!("Running preflight checks for {dir}");
    formatter.progress(&format!("Running preflight checks on {dir}..."));

    let status = Command::new("confiture")
        .args(["migrate", "preflight", "--migrations-dir", dir])
        .status()
        .context("Failed to execute confiture")?;

    if status.success() {
        formatter.progress("Preflight checks passed.");
        Ok(())
    } else {
        anyhow::bail!("Preflight checks failed. Check the output above for details.")
    }
}
