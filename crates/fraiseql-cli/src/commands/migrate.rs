//! `fraiseql migrate` - Database migration wrapper
//!
//! Wraps confiture for database migrations, providing a unified CLI
//! experience without requiring users to install confiture separately.

use std::{path::Path, process::Command};

use anyhow::{Context, Result};
use tracing::info;

use crate::output::OutputFormatter;

/// Migration subcommand
#[derive(Debug, Clone)]
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
}

/// Run the migrate command
///
/// # Errors
///
/// Returns an error if the operation fails.
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
    }
}

/// Resolve the database URL: use explicit flag, or fall back to fraiseql.toml
///
/// # Errors
///
/// Returns an error if the operation fails.
pub fn resolve_database_url(explicit: Option<&str>) -> Result<String> {
    if let Some(url) = explicit {
        return Ok(url.to_string());
    }

    // Try loading from fraiseql.toml
    let toml_path = Path::new("fraiseql.toml");
    if toml_path.exists() {
        let content = std::fs::read_to_string(toml_path).context("Failed to read fraiseql.toml")?;
        let parsed: toml::Value =
            toml::from_str(&content).context("Failed to parse fraiseql.toml")?;

        if let Some(url) = parsed
            .get("database")
            .and_then(|db| db.get("url"))
            .and_then(toml::Value::as_str)
        {
            info!("Using database URL from fraiseql.toml");
            return Ok(url.to_string());
        }
    }

    // Try DATABASE_URL env var
    if let Ok(url) = std::env::var("DATABASE_URL") {
        info!("Using DATABASE_URL environment variable");
        return Ok(url);
    }

    anyhow::bail!(
        "No database URL provided. Use --database, set [database].url in fraiseql.toml, \
         or set DATABASE_URL environment variable."
    )
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

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    // These tests mutate process-global state (cwd and env vars) and must not
    // run in parallel with each other.
    static GLOBAL_STATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_resolve_migration_dir_explicit() {
        assert_eq!(resolve_migration_dir(Some("custom/dir")), "custom/dir");
    }

    #[test]
    fn test_resolve_migration_dir_default() {
        // When no auto-discoverable dirs exist, falls back to default
        let dir = resolve_migration_dir(None);
        // Should return some string (either auto-discovered or default)
        assert!(!dir.is_empty());
    }

    #[test]
    fn test_resolve_database_url_explicit() {
        let url = resolve_database_url(Some("postgres://localhost/test")).unwrap();
        assert_eq!(url, "postgres://localhost/test");
    }

    #[test]
    fn test_resolve_database_url_no_source() {
        let _guard = GLOBAL_STATE_LOCK
            .lock()
            .expect("GLOBAL_STATE_LOCK poisoned; a previous test panicked mid-migration");

        let tmp = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        temp_env::with_vars([("DATABASE_URL", None::<&str>)], || {
            let result = resolve_database_url(None);
            assert!(result.is_err());
        });

        std::env::set_current_dir(original).unwrap();
    }

    #[test]
    fn test_resolve_database_url_from_env() {
        let _guard = GLOBAL_STATE_LOCK
            .lock()
            .expect("GLOBAL_STATE_LOCK poisoned; a previous test panicked mid-migration");

        let tmp = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        temp_env::with_vars([("DATABASE_URL", Some("postgres://env/test"))], || {
            let url = resolve_database_url(None).unwrap();
            assert_eq!(url, "postgres://env/test");
        });

        std::env::set_current_dir(original).unwrap();
    }
}
