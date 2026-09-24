//! `fraiseql migrate` - Database migration wrapper
//!
//! Wraps confiture for database migrations: every verb is one `confiture migrate <verb> …`
//! invocation, assembled by `ConfitureCommand` from the option table its doc comment
//! holds. Confiture must be installed and on `PATH`; `run` says so and stops when it is not.
//!
//! The call shape follows confiture's CLI reference, `docs/reference/cli.md` in
//! <https://github.com/fraiseql/confiture> at tag `v0.44.0`, section "`confiture migrate`".
//! That is the confiture the `integration (postgres)` leg installs from
//! `tools/confiture-requirements.txt` and runs `tests/migrate_against_confiture.rs` against;
//! a bump of that pin is where a change to the table gets checked (#1376).

use std::{path::Path, process::Command};

use anyhow::{Context, Result};
use tracing::info;

use crate::output::OutputFormatter;

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

    let (command, done, failed) = match action {
        MigrateAction::Up { database_url, dir } => {
            info!("Running migrations up from {dir}");
            formatter.progress(&format!("Applying migrations from {dir}..."));
            (
                ConfitureCommand::new(ConfitureVerb::Up, dir, formatter).database_url(database_url),
                "Migrations applied successfully.".to_string(),
                "Migration failed. Check the output above for details.",
            )
        },
        MigrateAction::Down {
            database_url,
            dir,
            steps,
        } => {
            info!("Rolling back {steps} migration(s) from {dir}");
            formatter.progress(&format!("Rolling back {steps} migration(s)..."));
            (
                ConfitureCommand::new(ConfitureVerb::Down, dir, formatter)
                    .database_url(database_url)
                    .steps(*steps),
                "Rollback completed successfully.".to_string(),
                "Rollback failed. Check the output above for details.",
            )
        },
        MigrateAction::Status { database_url, dir } => {
            info!("Checking migration status for {dir}");
            (
                ConfitureCommand::new(ConfitureVerb::Status, dir, formatter)
                    .database_url(database_url),
                String::new(),
                "Failed to get migration status.",
            )
        },
        MigrateAction::Create { name, dir } => {
            info!("Creating migration: {name} in {dir}");
            ensure_migrations_dir(dir)?;
            (
                ConfitureCommand::new(ConfitureVerb::Generate, dir, formatter).name(name),
                format!("Migration created in {dir}/"),
                "Failed to create migration.",
            )
        },
        MigrateAction::Generate { name, dir } => {
            info!("Generating migration: {name} in {dir}");
            ensure_migrations_dir(dir)?;
            formatter.progress(&format!("Generating migration '{name}' in {dir}..."));
            (
                ConfitureCommand::new(ConfitureVerb::Generate, dir, formatter).name(name),
                format!("Migration generated in {dir}/"),
                "Failed to generate migration.",
            )
        },
        MigrateAction::Validate { dir } => {
            info!("Validating migrations in {dir}");
            (
                ConfitureCommand::new(ConfitureVerb::Validate, dir, formatter),
                String::new(),
                "Migration validation failed. Check the output above for details.",
            )
        },
        MigrateAction::Preflight { dir } => {
            info!("Running preflight checks for {dir}");
            formatter.progress(&format!("Running preflight checks on {dir}..."));
            (
                ConfitureCommand::new(ConfitureVerb::Preflight, dir, formatter),
                "Preflight checks passed.".to_string(),
                "Preflight checks failed. Check the output above for details.",
            )
        },
    };

    if command.succeeds()? {
        if !done.is_empty() {
            formatter.progress(&done);
        }
        Ok(())
    } else {
        anyhow::bail!("{failed}")
    }
}

/// The `confiture migrate` verbs this wrapper invokes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfitureVerb {
    /// `confiture migrate up`
    Up,
    /// `confiture migrate down`
    Down,
    /// `confiture migrate status`
    Status,
    /// `confiture migrate generate` — the verb that writes a new migration file
    Generate,
    /// `confiture migrate validate`
    Validate,
    /// `confiture migrate preflight`
    Preflight,
}

impl ConfitureVerb {
    /// The verb as confiture spells it.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Status => "status",
            Self::Generate => "generate",
            Self::Validate => "validate",
            Self::Preflight => "preflight",
        }
    }
}

/// One `confiture migrate <verb>` invocation.
///
/// Every option this wrapper passes is a row of this table, and every row was checked
/// against `confiture migrate <verb> --help`:
///
/// | argv                          | verbs         | source                              |
/// |-------------------------------|---------------|-------------------------------------|
/// | `--migrations-dir <dir>`      | all six       | the resolved migration directory    |
/// | `<name>` (positional)         | `generate`    | the migration name                  |
/// | `--steps <n>`                 | `down`        | how many migrations to roll back    |
/// | `--format json`               | all six       | the CLI's global `--json`           |
///
/// No verb takes a schema directory (`validate` has `--ddl-dir`/`--schema`; the rest have
/// no schema path at all), so none is passed. The DSN never travels on argv: it is
/// exported as an environment variable, so it is not visible in `ps aux` or
/// `/proc/<pid>/cmdline`.
#[derive(Clone)]
pub(crate) struct ConfitureCommand {
    verb:           ConfitureVerb,
    migrations_dir: String,
    name:           Option<String>,
    steps:          Option<u32>,
    json:           bool,
    database_url:   Option<String>,
}

impl ConfitureCommand {
    /// A `confiture migrate <verb> --migrations-dir <dir>` call, `--format json` under `--json`.
    pub(crate) fn new(
        verb: ConfitureVerb,
        migrations_dir: &str,
        formatter: &OutputFormatter,
    ) -> Self {
        Self {
            verb,
            migrations_dir: migrations_dir.to_string(),
            name: None,
            steps: None,
            json: formatter.is_json(),
            database_url: None,
        }
    }

    /// The positional migration name (`generate`).
    pub(crate) fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// `--steps <n>` (`down`).
    pub(crate) const fn steps(mut self, steps: u32) -> Self {
        self.steps = Some(steps);
        self
    }

    /// The DSN confiture connects with, exported as an environment variable.
    pub(crate) fn database_url(mut self, url: &str) -> Self {
        self.database_url = Some(url.to_string());
        self
    }

    /// The arguments after the program name, in the order confiture receives them.
    pub(crate) fn argv(&self) -> Vec<String> {
        let mut argv = vec!["migrate".to_string(), self.verb.as_str().to_string()];
        if let Some(name) = &self.name {
            argv.push(name.clone());
        }
        argv.push("--migrations-dir".to_string());
        argv.push(self.migrations_dir.clone());
        if let Some(steps) = self.steps {
            argv.push("--steps".to_string());
            argv.push(steps.to_string());
        }
        if self.json {
            argv.push("--format".to_string());
            argv.push("json".to_string());
        }
        argv
    }

    /// Runs confiture with stdio inherited and reports whether it exited 0.
    fn succeeds(&self) -> Result<bool> {
        let mut command = Command::new("confiture");
        command.args(self.argv());
        if let Some(url) = &self.database_url {
            command.env("DATABASE_URL", url);
        }
        let status = command.status().context("Failed to execute confiture")?;
        Ok(status.success())
    }
}

impl std::fmt::Debug for ConfitureCommand {
    /// The DSN is a credential: it is shown as present or absent, never spelled out.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfitureCommand")
            .field("verb", &self.verb)
            .field("migrations_dir", &self.migrations_dir)
            .field("name", &self.name)
            .field("steps", &self.steps)
            .field("json", &self.json)
            .field("database_url", &self.database_url.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

fn ensure_migrations_dir(dir: &str) -> Result<()> {
    std::fs::create_dir_all(dir).context(format!("Failed to create migration directory: {dir}"))
}

/// Resolve the database URL: use explicit flag, or fall back to fraiseql.toml
///
/// # Errors
///
/// Returns an error if `fraiseql.toml` exists but cannot be read or parsed, or
/// if no database URL can be found from any source (flag, TOML, or `DATABASE_URL`).
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
