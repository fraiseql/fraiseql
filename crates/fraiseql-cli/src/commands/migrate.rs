//! `fraiseql migrate` - Database migration wrapper
//!
//! Wraps confiture for database migrations: every verb is one `confiture migrate <verb> …`
//! invocation, assembled by `ConfitureCommand` from the option table its doc comment
//! holds. Confiture must be installed and on `PATH`; `run` says so and stops when it is not.
//!
//! The call shape follows confiture's CLI reference, `docs/reference/cli.md` in
//! <https://github.com/fraiseql/confiture> at tag `v1.19.0`, section "`confiture migrate`".
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

    /// Whether the verb has a `--no-config` option: `up`, `down`, `status` and `preflight`
    /// do, `generate` and `validate` do not (`confiture migrate <verb> --help`, 1.19.0).
    pub(crate) const fn accepts_no_config(self) -> bool {
        matches!(self, Self::Up | Self::Down | Self::Status | Self::Preflight)
    }
}

/// One `confiture migrate <verb>` invocation.
///
/// Every option this wrapper passes is a row of this table, and every row was checked
/// against `confiture migrate <verb> --help`:
///
/// | argv                          | verbs                      | source                           |
/// |-------------------------------|----------------------------|----------------------------------|
/// | `--migrations-dir <dir>`      | all six                    | the resolved migration directory |
/// | `<name>` (positional)         | `generate`                 | the migration name               |
/// | `--steps <n>`                 | `down`                     | how many migrations to roll back |
/// | `--no-config`                 | `up`, `down`, `status`     | accompanies the DSN (below)      |
/// | `--format json`               | all six                    | the CLI's global `--json`        |
///
/// No verb takes a schema directory (`validate` has `--ddl-dir`/`--schema`; the rest have
/// no schema path at all), so none is passed.
///
/// The DSN never travels on argv, where `ps aux` and `/proc/<pid>/cmdline` would show it.
/// It is exported as `CONFITURE_DATABASE_URL`, confiture's canonical variable, together
/// with `--no-config`. Under that flag confiture's connection ladder (its
/// `docs/reference/cli.md`, §"Connection source and precedence") reads the environment as
/// the sole DSN source and skips config-file discovery, so the DSN [`resolve_database_url`]
/// settled on is the one confiture connects with; no `confiture.yaml` or
/// `db/environments/*.yaml` in the working directory can shadow it. The same ladder is why
/// the ambient `DATABASE_URL` is never used for the handoff: confiture ignores it for
/// `status` (every migration "unknown (no config)", exit 0) and refuses it for `up` and
/// `down` (`CONFIG_010`). `preflight` also has `--no-config`, but the wrapper hands it no
/// DSN, so it runs under confiture's own discovery.
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

    /// The DSN confiture connects with: exported as `CONFITURE_DATABASE_URL`, with
    /// `--no-config` on argv. Only a verb that has `--no-config` can be handed one.
    pub(crate) fn database_url(mut self, url: &str) -> Self {
        debug_assert!(
            self.verb.accepts_no_config(),
            "confiture migrate {} has no --no-config, so it cannot take a DSN this way",
            self.verb.as_str()
        );
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
        if self.database_url.is_some() {
            argv.push("--no-config".to_string());
        }
        if self.json {
            argv.push("--format".to_string());
            argv.push("json".to_string());
        }
        argv
    }

    /// The process to spawn: `confiture` with [`Self::argv`], and `CONFITURE_DATABASE_URL`
    /// set when a DSN was handed over. Nothing else in the environment is touched.
    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new("confiture");
        command.args(self.argv());
        if let Some(url) = &self.database_url {
            command.env("CONFITURE_DATABASE_URL", url);
        }
        command
    }

    /// Runs confiture with stdio inherited and reports whether it exited 0.
    fn succeeds(&self) -> Result<bool> {
        let status = self.command().status().context("Failed to execute confiture")?;
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

/// The long help of `fraiseql migrate`. It lives beside the resolver whose order it states
/// and the builder whose handoff it describes, so a change to either is a change here; a
/// unit test holds the text to the resolver's order.
pub(crate) const MIGRATE_LONG_ABOUT: &str = "Run database migrations\n\n\
    Wraps confiture: every verb is one `confiture migrate <verb>` call. The database URL is \
    resolved in this order: the --database flag, then [database].url in fraiseql.toml, then \
    the DATABASE_URL environment variable. The result reaches confiture as \
    CONFITURE_DATABASE_URL with --no-config, so confiture's own config files never override \
    it.";

/// Resolves the database URL for the commands that connect: the explicit `--database`
/// flag, else `[database].url` in `fraiseql.toml`, else the `DATABASE_URL` environment
/// variable.
///
/// `fraiseql.toml` is placed above the environment because it is the project's own file,
/// while `DATABASE_URL` is whatever the shell happens to hold; an explicit flag beats both.
/// This is fraiseql's ladder, not confiture's. Confiture has one of its own — its
/// `docs/reference/cli.md`, §"Connection source and precedence" — in which a present config
/// file beats an ambient `DATABASE_URL` and a mutating verb refuses an ambient variable
/// outright. The wrapper does not let the two ladders compete: the URL this function
/// settles on is handed to confiture under `--no-config`, which makes that URL the sole
/// source (`ConfitureCommand`, below).
///
/// # Errors
///
/// Returns an error if `fraiseql.toml` exists but cannot be read or parsed, or if no
/// source provides a URL.
pub fn resolve_database_url(explicit: Option<&str>) -> Result<String> {
    if let Some(url) = explicit {
        return Ok(url.to_string());
    }
    if let Some(url) = fraiseql_toml_database_url()? {
        info!("Using database URL from fraiseql.toml");
        return Ok(url);
    }
    if let Ok(url) = std::env::var("DATABASE_URL") {
        info!("Using DATABASE_URL environment variable");
        return Ok(url);
    }
    anyhow::bail!(
        "No database URL provided. Use --database, set [database].url in fraiseql.toml, \
         or set DATABASE_URL environment variable."
    )
}

/// `[database].url` from the `fraiseql.toml` in the working directory, if both exist.
fn fraiseql_toml_database_url() -> Result<Option<String>> {
    let toml_path = Path::new("fraiseql.toml");
    if !toml_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(toml_path).context("Failed to read fraiseql.toml")?;
    let parsed: toml::Value = toml::from_str(&content).context("Failed to parse fraiseql.toml")?;
    Ok(parsed
        .get("database")
        .and_then(|db| db.get("url"))
        .and_then(toml::Value::as_str)
        .map(str::to_string))
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
