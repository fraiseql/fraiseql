//! FraiseQL CLI - Schema compilation and development tools
//!
//! This CLI compiles schema.json files (from Python/TypeScript/etc decorators)
//! into optimized schema.compiled.json files for the FraiseQL Rust runtime.

use clap::{Parser, Subcommand};
use std::process;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod commands;
mod schema;

/// FraiseQL CLI - Compile GraphQL schemas to optimized SQL execution
#[derive(Parser)]
#[command(name = "fraiseql")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Enable debug logging
    #[arg(short, long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile schema.json to optimized schema.compiled.json
    Compile {
        /// Input schema.json file path
        #[arg(value_name = "INPUT")]
        input: String,

        /// Output schema.compiled.json file path
        #[arg(short, long, value_name = "OUTPUT", default_value = "schema.compiled.json")]
        output: String,

        /// Validate only, don't write output
        #[arg(long)]
        check: bool,
    },

    /// Validate schema.json or fact tables
    Validate {
        #[command(subcommand)]
        command: Option<ValidateCommands>,

        /// Schema.json file path to validate (if no subcommand)
        #[arg(value_name = "INPUT")]
        input: Option<String>,
    },

    /// Introspect database for fact tables and output suggestions
    Introspect {
        #[command(subcommand)]
        command: IntrospectCommands,
    },

    /// Development server with hot-reload (Phase 9 Part 3)
    #[command(hide = true)] // Hide until implemented
    Serve {
        /// Schema.json file path to watch
        #[arg(value_name = "SCHEMA")]
        schema: String,

        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
}

#[derive(Subcommand)]
enum ValidateCommands {
    /// Validate that declared fact tables match database schema
    Facts {
        /// Schema.json file path
        #[arg(short, long, value_name = "SCHEMA")]
        schema: String,

        /// Database connection string
        #[arg(short, long, value_name = "DATABASE_URL")]
        database: String,
    },
}

#[derive(Subcommand)]
enum IntrospectCommands {
    /// Introspect database for fact tables (tf_* tables)
    Facts {
        /// Database connection string
        #[arg(short, long, value_name = "DATABASE_URL")]
        database: String,

        /// Output format (python, json)
        #[arg(short, long, value_name = "FORMAT", default_value = "python")]
        format: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(cli.verbose, cli.debug);

    // Run command
    let result = match cli.command {
        Commands::Compile {
            input,
            output,
            check,
        } => commands::compile::run(&input, &output, check).await,

        Commands::Validate { command, input } => match command {
            Some(ValidateCommands::Facts { schema, database }) => {
                commands::validate_facts::run(std::path::Path::new(&schema), &database).await
            }
            None => match input {
                Some(input) => commands::validate::run(&input).await,
                None => Err(anyhow::anyhow!("INPUT required when no subcommand provided")),
            },
        },

        Commands::Introspect { command } => {
            match command {
                IntrospectCommands::Facts { database, format } => {
                    match commands::introspect_facts::OutputFormat::from_str(&format) {
                        Ok(fmt) => commands::introspect_facts::run(&database, fmt).await,
                        Err(e) => Err(anyhow::anyhow!(e)),
                    }
                }
            }
        }

        Commands::Serve { schema, port } => commands::serve::run(&schema, port).await,
    };

    // Handle errors
    if let Err(e) = result {
        eprintln!("Error: {e}");
        if cli.debug {
            eprintln!("\nDebug info:");
            eprintln!("{e:?}");
        }
        process::exit(1);
    }
}

/// Initialize tracing subscriber for logging
fn init_logging(verbose: bool, debug: bool) {
    let filter = if debug {
        "fraiseql=debug,fraiseql_core=debug"
    } else if verbose {
        "fraiseql=info,fraiseql_core=info"
    } else {
        "fraiseql=warn,fraiseql_core=warn"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
