//! Command-line interface for observer runtime management.
//!
//! Provides commands for:
//! - Runtime status monitoring
//! - Event inspection and debugging
//! - Dead letter queue management
//! - Configuration validation
//! - Prometheus metrics inspection

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod commands;

#[cfg(test)]
mod tests;

/// FraiseQL Observer CLI - Runtime management and monitoring
#[derive(Parser, Debug)]
#[command(name = "fraiseql-observers")]
#[command(about = "CLI for managing and monitoring FraiseQL Observer runtime", long_about = None)]
#[command(version)]
#[command(author)]
pub struct Cli {
    /// Path to observer configuration file
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output format (text, json)
    #[arg(short, long, global = true, default_value = "text")]
    pub format: OutputFormat,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Output format for CLI responses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output
    Text,
    /// JSON output
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(format!("Invalid format '{other}'. Use 'text' or 'json'")),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
        }
    }
}

/// Available CLI commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show observer runtime status
    Status {
        /// Listener ID to check (optional, all if not specified)
        #[arg(short, long)]
        listener: Option<String>,

        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },

    /// Debug event inspection and processing
    DebugEvent {
        /// Event ID to inspect
        #[arg(short, long)]
        event_id: Option<String>,

        /// Show event history (recent N events)
        #[arg(short, long)]
        history: Option<usize>,

        /// Filter by entity type
        #[arg(short, long)]
        entity_type: Option<String>,

        /// Filter by event kind (created, updated, deleted)
        #[arg(short, long)]
        kind: Option<String>,
    },

    /// Manage dead letter queue
    Dlq {
        /// Dead letter queue subcommand
        #[command(subcommand)]
        subcommand: DlqSubcommand,
    },

    /// Validate observer configuration
    ValidateConfig {
        /// Configuration file to validate
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,

        /// Show detailed validation report
        #[arg(short, long)]
        detailed: bool,
    },

    /// Inspect Prometheus metrics
    #[cfg(feature = "metrics")]
    Metrics {
        /// Specific metric to inspect (leave empty for all)
        #[arg(short, long)]
        metric: Option<String>,

        /// Show metric help/documentation
        #[arg(short, long)]
        help: bool,
    },
}

/// DLQ management subcommands
#[derive(Subcommand, Debug)]
pub enum DlqSubcommand {
    /// List failed items in DLQ
    List {
        /// Limit number of items shown
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Offset for pagination
        #[arg(short, long)]
        offset: Option<usize>,

        /// Filter by observer name
        #[arg(short, long)]
        observer: Option<String>,

        /// Show items after timestamp (ISO 8601)
        #[arg(short, long)]
        after: Option<String>,
    },

    /// Show details of a specific DLQ item
    Show {
        /// ID of the DLQ item
        item_id: String,
    },

    /// Retry a failed item
    Retry {
        /// ID of the item to retry
        item_id: String,

        /// Force retry regardless of max attempts
        #[arg(long)]
        force: bool,
    },

    /// Retry all items matching filters
    RetryAll {
        /// Only retry items from this observer
        #[arg(short, long)]
        observer: Option<String>,

        /// Only retry items after timestamp
        #[arg(short, long)]
        after: Option<String>,

        /// Dry run (show what would be retried)
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove an item from DLQ
    Remove {
        /// ID of the item to remove
        item_id: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Show DLQ statistics
    Stats {
        /// Show breakdown by observer
        #[arg(short, long)]
        by_observer: bool,

        /// Show breakdown by error type
        #[arg(short, long)]
        by_error: bool,
    },
}

/// Run the CLI
pub async fn run() -> crate::error::Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        // Initialize tracing with debug level
        tracing::debug!("CLI started in verbose mode");
    }

    match cli.command {
        Commands::Status { listener, detailed } => {
            commands::status::execute(cli.format, listener, detailed).await
        },
        Commands::DebugEvent {
            event_id,
            history,
            entity_type,
            kind,
        } => commands::debug_event::execute(cli.format, event_id, history, entity_type, kind).await,
        Commands::Dlq { subcommand } => commands::dlq::execute(cli.format, subcommand).await,
        Commands::ValidateConfig { file, detailed } => {
            commands::validate_config::execute(cli.format, file, detailed).await
        },
        #[cfg(feature = "metrics")]
        Commands::Metrics { metric, help } => {
            commands::metrics::execute(cli.format, metric, help).await
        },
    }
}
