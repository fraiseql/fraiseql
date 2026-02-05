//! FraiseQL CLI - Schema compilation and development tools
//!
//! This CLI compiles schema.json files (from Python/TypeScript/etc decorators)
//! into optimized schema.compiled.json files for the FraiseQL Rust runtime.

#![warn(clippy::all)]
#![warn(clippy::pedantic)]
// Allow common pedantic lints for CLI tooling
#![allow(clippy::format_push_string)] // Sometimes clearer than write!
#![allow(clippy::option_if_let_else)] // Sometimes clearer
#![allow(clippy::needless_pass_by_value)] // Clap requires owned strings
#![allow(clippy::must_use_candidate)] // CLI functions don't need #[must_use]
#![allow(clippy::module_name_repetitions)] // Common in Rust APIs
#![allow(clippy::missing_errors_doc)] // CLI functions
#![allow(clippy::doc_markdown)] // Would require extensive backtick additions
#![allow(clippy::too_many_lines)] // CLI commands can be long
#![allow(clippy::unnecessary_wraps)] // Sometimes needed for API consistency
#![allow(clippy::match_same_arms)] // Sometimes clearer to be explicit
#![allow(clippy::similar_names)] // Variable naming style
#![allow(clippy::struct_excessive_bools)] // IntermediateAutoParams uses bools for flags
#![allow(clippy::derive_partial_eq_without_eq)] // Some structs shouldn't implement Eq

use std::{process, str::FromStr};

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod commands;
mod config;
mod output;
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

    /// Output as JSON (machine-readable)
    #[arg(long, global = true)]
    json: bool,

    /// Suppress output (exit code only)
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile schema to optimized schema.compiled.json
    ///
    /// Supports three workflows:
    /// 1. TOML-only: fraiseql compile fraiseql.toml
    /// 2. Language + TOML: fraiseql compile fraiseql.toml --types types.json
    /// 3. Legacy JSON: fraiseql compile schema.json
    Compile {
        /// Input file path: fraiseql.toml (TOML) or schema.json (legacy)
        #[arg(value_name = "INPUT")]
        input: String,

        /// Optional types.json from language implementation (used with fraiseql.toml)
        #[arg(long, value_name = "TYPES")]
        types: Option<String>,

        /// Directory for auto-discovery of schema files (recursive *.json)
        #[arg(long, value_name = "DIR")]
        schema_dir: Option<String>,

        /// Type files (repeatable): fraiseql compile fraiseql.toml --type-file a.json --type-file
        /// b.json
        #[arg(long = "type-file", value_name = "FILE")]
        type_files: Vec<String>,

        /// Query files (repeatable)
        #[arg(long = "query-file", value_name = "FILE")]
        query_files: Vec<String>,

        /// Mutation files (repeatable)
        #[arg(long = "mutation-file", value_name = "FILE")]
        mutation_files: Vec<String>,

        /// Output schema.compiled.json file path
        #[arg(
            short,
            long,
            value_name = "OUTPUT",
            default_value = "schema.compiled.json"
        )]
        output: String,

        /// Validate only, don't write output
        #[arg(long)]
        check: bool,

        /// Optional database URL for indexed column validation
        /// When provided, validates that indexed columns exist in database views
        #[arg(long, value_name = "DATABASE_URL")]
        database: Option<String>,
    },

    /// Explain query execution plan and complexity
    ///
    /// Shows GraphQL query execution plan, SQL, and complexity analysis.
    Explain {
        /// GraphQL query string
        #[arg(value_name = "QUERY")]
        query: String,
    },

    /// Calculate query complexity score
    ///
    /// Quick analysis of query complexity (depth, field count, score).
    Cost {
        /// GraphQL query string
        #[arg(value_name = "QUERY")]
        query: String,
    },

    /// Analyze schema for optimization opportunities
    ///
    /// Provides recommendations across 6 categories:
    /// performance, security, federation, complexity, caching, indexing
    Analyze {
        /// Path to schema.compiled.json
        #[arg(value_name = "SCHEMA")]
        schema: String,
    },

    /// Export federation dependency graph
    ///
    /// Visualize federation structure in multiple formats.
    Federation {
        /// Schema path (positional argument passed to subcommand)
        #[command(subcommand)]
        command: FederationCommands,
    },

    /// Lint schema for FraiseQL design quality
    ///
    /// Analyzes schema using FraiseQL-calibrated design rules.
    /// Detects JSONB batching issues, compilation problems, auth boundaries, etc.
    Lint {
        /// Path to schema.json or schema.compiled.json
        #[arg(value_name = "SCHEMA")]
        schema: String,

        /// Only show federation audit
        #[arg(long)]
        federation: bool,

        /// Only show cost audit
        #[arg(long)]
        cost: bool,

        /// Only show cache audit
        #[arg(long)]
        cache: bool,

        /// Only show auth audit
        #[arg(long)]
        auth: bool,

        /// Only show compilation audit
        #[arg(long)]
        compilation: bool,

        /// Exit with error if any critical issues found
        #[arg(long)]
        fail_on_critical: bool,

        /// Exit with error if any warning or critical issues found
        #[arg(long)]
        fail_on_warning: bool,

        /// Show detailed issue descriptions
        #[arg(long)]
        verbose: bool,
    },

    /// Generate DDL for Arrow views (va_*, tv_*, ta_*)
    GenerateViews {
        /// Path to schema.json
        #[arg(short, long, value_name = "SCHEMA")]
        schema: String,

        /// Entity name from schema
        #[arg(short, long, value_name = "NAME")]
        entity: String,

        /// View name (must start with va_, tv_, or ta_)
        #[arg(long, value_name = "NAME")]
        view: String,

        /// Refresh strategy (trigger-based or scheduled)
        #[arg(long, value_name = "STRATEGY", default_value = "trigger-based")]
        refresh_strategy: String,

        /// Output file path (default: {view}.sql)
        #[arg(short, long, value_name = "PATH")]
        output: Option<String>,

        /// Include helper/composition views
        #[arg(long, default_value = "true")]
        include_composition_views: bool,

        /// Include monitoring functions
        #[arg(long, default_value = "true")]
        include_monitoring: bool,

        /// Validate only, don't write file
        #[arg(long)]
        validate: bool,

        /// Show generation steps (use global --verbose flag)
        #[arg(long, action = clap::ArgAction::SetTrue)]
        gen_verbose: bool,
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

    /// Development server with hot-reload
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
enum FederationCommands {
    /// Export federation graph
    Graph {
        /// Path to schema.compiled.json
        #[arg(value_name = "SCHEMA")]
        schema: String,

        /// Output format (json, dot, mermaid)
        #[arg(short, long, value_name = "FORMAT", default_value = "json")]
        format: String,
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
            types,
            schema_dir,
            type_files,
            query_files,
            mutation_files,
            output,
            check,
            database,
        } => {
            commands::compile::run(
                &input,
                types.as_deref(),
                schema_dir.as_deref(),
                type_files,
                query_files,
                mutation_files,
                &output,
                check,
                database.as_deref(),
            )
            .await
        },

        Commands::Explain { query } => match commands::explain::run(&query) {
            Ok(result) => {
                println!("{}", output::OutputFormatter::new(cli.json, cli.quiet).format(&result));
                Ok(())
            },
            Err(e) => Err(e),
        },

        Commands::Cost { query } => match commands::cost::run(&query) {
            Ok(result) => {
                println!("{}", output::OutputFormatter::new(cli.json, cli.quiet).format(&result));
                Ok(())
            },
            Err(e) => Err(e),
        },

        Commands::Analyze { schema } => match commands::analyze::run(&schema) {
            Ok(result) => {
                println!("{}", output::OutputFormatter::new(cli.json, cli.quiet).format(&result));
                Ok(())
            },
            Err(e) => Err(e),
        },

        Commands::Lint {
            schema,
            federation,
            cost,
            cache,
            auth,
            compilation,
            fail_on_critical,
            fail_on_warning,
            verbose,
        } => {
            let opts = commands::lint::LintOptions {
                federation,
                cost,
                cache,
                auth,
                compilation,
                fail_on_critical,
                fail_on_warning,
                verbose,
            };
            match commands::lint::run(&schema, opts) {
                Ok(result) => {
                    println!(
                        "{}",
                        output::OutputFormatter::new(cli.json, cli.quiet).format(&result)
                    );
                    Ok(())
                },
                Err(e) => Err(e),
            }
        },

        Commands::Federation { command } => match command {
            FederationCommands::Graph { schema, format } => {
                match commands::federation::graph::GraphFormat::from_str(&format) {
                    Ok(fmt) => match commands::federation::graph::run(&schema, fmt) {
                        Ok(result) => {
                            println!(
                                "{}",
                                output::OutputFormatter::new(cli.json, cli.quiet).format(&result)
                            );
                            Ok(())
                        },
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(anyhow::anyhow!(e)),
                }
            },
        },

        Commands::GenerateViews {
            schema,
            entity,
            view,
            refresh_strategy,
            output,
            include_composition_views,
            include_monitoring,
            validate,
            gen_verbose,
        } => match commands::generate_views::RefreshStrategy::from_str(&refresh_strategy) {
            Ok(refresh_strat) => {
                let config = commands::generate_views::GenerateViewsConfig {
                    schema_path: schema,
                    entity,
                    view,
                    refresh_strategy: refresh_strat,
                    output,
                    include_composition_views,
                    include_monitoring,
                    validate_only: validate,
                    verbose: cli.verbose || gen_verbose,
                };

                commands::generate_views::run(config)
            },
            Err(e) => Err(anyhow::anyhow!(e)),
        },

        Commands::Validate { command, input } => match command {
            Some(ValidateCommands::Facts { schema, database }) => {
                commands::validate_facts::run(std::path::Path::new(&schema), &database).await
            },
            None => match input {
                Some(input) => commands::validate::run(&input).await,
                None => Err(anyhow::anyhow!("INPUT required when no subcommand provided")),
            },
        },

        Commands::Introspect { command } => match command {
            IntrospectCommands::Facts { database, format } => {
                match commands::introspect_facts::OutputFormat::from_str(&format) {
                    Ok(fmt) => commands::introspect_facts::run(&database, fmt).await,
                    Err(e) => Err(anyhow::anyhow!(e)),
                }
            },
        },

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
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
