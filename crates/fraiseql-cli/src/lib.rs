//! FraiseQL CLI library - exposes internal modules for testing and reuse

#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::format_push_string)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::missing_const_for_fn)] // Reason: const fn not stable for all patterns used

pub mod commands;
pub mod config;
pub mod introspection;
pub mod output;
pub mod output_schemas;
pub mod schema;

use std::{env, process, str::FromStr};

use clap::{CommandFactory, Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Exit codes documented in help text
const EXIT_CODES_HELP: &str = "\
EXIT CODES:
    0  Success - Command completed successfully
    1  Error - Command failed with an error
    2  Validation failed - Schema or input validation failed";

/// FraiseQL CLI - Compile GraphQL schemas to optimized SQL execution
#[derive(Parser)]
#[command(name = "fraiseql")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(after_help = EXIT_CODES_HELP)]
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
    #[command(after_help = "\
EXAMPLES:
    fraiseql compile fraiseql.toml
    fraiseql compile fraiseql.toml --types types.json
    fraiseql compile schema.json -o schema.compiled.json
    fraiseql compile fraiseql.toml --check")]
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

    /// Extract schema from annotated source files
    ///
    /// Parses FraiseQL annotations in any supported language and generates schema.json.
    /// No language runtime required — pure text processing.
    #[command(after_help = "\
EXAMPLES:
    fraiseql extract schema/schema.py
    fraiseql extract schema/ --recursive
    fraiseql extract schema.rs --language rust -o schema.json")]
    Extract {
        /// Source file(s) or directory to extract from
        #[arg(value_name = "INPUT")]
        input: Vec<String>,

        /// Override language detection (python, typescript, rust, java, kotlin, go, csharp, swift,
        /// scala)
        #[arg(short, long)]
        language: Option<String>,

        /// Recursively scan directories
        #[arg(short, long)]
        recursive: bool,

        /// Output file path
        #[arg(short, long, default_value = "schema.json")]
        output: String,
    },

    /// Explain query execution plan and complexity
    ///
    /// Shows GraphQL query execution plan, SQL, and complexity analysis.
    #[command(after_help = "\
EXAMPLES:
    fraiseql explain '{ users { id name } }'
    fraiseql explain '{ user(id: 1) { posts { title } } }' --json")]
    Explain {
        /// GraphQL query string
        #[arg(value_name = "QUERY")]
        query: String,
    },

    /// Calculate query complexity score
    ///
    /// Quick analysis of query complexity (depth, field count, score).
    #[command(after_help = "\
EXAMPLES:
    fraiseql cost '{ users { id name } }'
    fraiseql cost '{ deeply { nested { query { here } } } }' --json")]
    Cost {
        /// GraphQL query string
        #[arg(value_name = "QUERY")]
        query: String,
    },

    /// Analyze schema for optimization opportunities
    ///
    /// Provides recommendations across 6 categories:
    /// performance, security, federation, complexity, caching, indexing
    #[command(after_help = "\
EXAMPLES:
    fraiseql analyze schema.compiled.json
    fraiseql analyze schema.compiled.json --json")]
    Analyze {
        /// Path to schema.compiled.json
        #[arg(value_name = "SCHEMA")]
        schema: String,
    },

    /// Analyze schema type dependencies
    ///
    /// Exports dependency graph, detects cycles, and finds unused types.
    /// Supports multiple output formats for visualization and CI integration.
    #[command(after_help = "\
EXAMPLES:
    fraiseql dependency-graph schema.compiled.json
    fraiseql dependency-graph schema.compiled.json -f dot > graph.dot
    fraiseql dependency-graph schema.compiled.json -f mermaid
    fraiseql dependency-graph schema.compiled.json --json")]
    DependencyGraph {
        /// Path to schema.compiled.json
        #[arg(value_name = "SCHEMA")]
        schema: String,

        /// Output format (json, dot, mermaid, d2, console)
        #[arg(short, long, value_name = "FORMAT", default_value = "json")]
        format: String,
    },

    /// Export federation dependency graph
    ///
    /// Visualize federation structure in multiple formats.
    #[command(after_help = "\
EXAMPLES:
    fraiseql federation graph schema.compiled.json
    fraiseql federation graph schema.compiled.json -f dot
    fraiseql federation graph schema.compiled.json -f mermaid")]
    Federation {
        /// Schema path (positional argument passed to subcommand)
        #[command(subcommand)]
        command: FederationCommands,
    },

    /// Lint schema for FraiseQL design quality
    ///
    /// Analyzes schema using FraiseQL-calibrated design rules.
    /// Detects JSONB batching issues, compilation problems, auth boundaries, etc.
    #[command(after_help = "\
EXAMPLES:
    fraiseql lint schema.json
    fraiseql lint schema.compiled.json --federation
    fraiseql lint schema.json --fail-on-critical
    fraiseql lint schema.json --json")]
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
    #[command(after_help = "\
EXAMPLES:
    fraiseql generate-views -s schema.json -e User --view va_users
    fraiseql generate-views -s schema.json -e Order --view tv_orders --refresh-strategy scheduled")]
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
    ///
    /// Performs comprehensive schema validation including:
    /// - JSON structure validation
    /// - Type reference validation
    /// - Circular dependency detection (with --check-cycles)
    /// - Unused type detection (with --check-unused)
    #[command(after_help = "\
EXAMPLES:
    fraiseql validate schema.json
    fraiseql validate schema.json --check-unused
    fraiseql validate schema.json --strict
    fraiseql validate facts -s schema.json -d postgres://localhost/db")]
    Validate {
        #[command(subcommand)]
        command: Option<ValidateCommands>,

        /// Schema.json file path to validate (if no subcommand)
        #[arg(value_name = "INPUT")]
        input: Option<String>,

        /// Check for circular dependencies between types
        #[arg(long, default_value = "true")]
        check_cycles: bool,

        /// Check for unused types (no incoming references)
        #[arg(long)]
        check_unused: bool,

        /// Strict mode: treat warnings as errors (unused types become errors)
        #[arg(long)]
        strict: bool,

        /// Only analyze specific type(s) - comma-separated list
        #[arg(long, value_name = "TYPES", value_delimiter = ',')]
        types: Vec<String>,
    },

    /// Introspect database for fact tables and output suggestions
    #[command(after_help = "\
EXAMPLES:
    fraiseql introspect facts -d postgres://localhost/db
    fraiseql introspect facts -d postgres://localhost/db -f json")]
    Introspect {
        #[command(subcommand)]
        command: IntrospectCommands,
    },

    /// Generate authoring-language source from schema.json
    ///
    /// The inverse of `fraiseql extract`: reads a schema.json and produces annotated
    /// source code in any of the 9 supported authoring languages.
    #[command(after_help = "\
EXAMPLES:
    fraiseql generate schema.json --language python
    fraiseql generate schema.json --language rust -o schema.rs
    fraiseql generate schema.json --language typescript")]
    Generate {
        /// Path to schema.json
        #[arg(value_name = "INPUT")]
        input: String,

        /// Target language (python, typescript, rust, java, kotlin, go, csharp, swift, scala)
        #[arg(short, long)]
        language: String,

        /// Output file path (default: schema.<ext> based on language)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Initialize a new FraiseQL project
    ///
    /// Creates project directory with fraiseql.toml, schema.json,
    /// database DDL structure, and authoring skeleton.
    #[command(after_help = "\
EXAMPLES:
    fraiseql init my-app
    fraiseql init my-app --language typescript --database postgres
    fraiseql init my-app --size xs --no-git")]
    Init {
        /// Project name (used as directory name)
        #[arg(value_name = "PROJECT_NAME")]
        project_name: String,

        /// Authoring language (python, typescript, rust, java, kotlin, go, csharp, swift, scala)
        #[arg(short, long, default_value = "python")]
        language: String,

        /// Target database (postgres, mysql, sqlite, sqlserver)
        #[arg(long, default_value = "postgres")]
        database: String,

        /// Project size: xs (single file), s (flat dirs), m (per-entity dirs)
        #[arg(long, default_value = "s")]
        size: String,

        /// Skip git init
        #[arg(long)]
        no_git: bool,
    },

    /// Run database migrations
    ///
    /// Wraps confiture for a unified migration experience.
    /// Reads database URL from --database, fraiseql.toml, or DATABASE_URL env var.
    #[command(after_help = "\
EXAMPLES:
    fraiseql migrate up --database postgres://localhost/mydb
    fraiseql migrate down --steps 1
    fraiseql migrate status
    fraiseql migrate create add_posts_table")]
    Migrate {
        #[command(subcommand)]
        command: MigrateCommands,
    },

    /// Generate Software Bill of Materials
    ///
    /// Parses Cargo.lock and fraiseql.toml to produce a compliance-ready SBOM.
    #[command(after_help = "\
EXAMPLES:
    fraiseql sbom
    fraiseql sbom --format spdx
    fraiseql sbom --format cyclonedx --output sbom.json")]
    Sbom {
        /// Output format (cyclonedx, spdx)
        #[arg(short, long, default_value = "cyclonedx")]
        format: String,

        /// Output file path (default: stdout)
        #[arg(short, long, value_name = "FILE")]
        output: Option<String>,
    },

    /// Compile schema and immediately start the GraphQL server
    ///
    /// Compiles the schema in-memory (no disk artifact) and starts the HTTP server.
    /// With --watch, the server hot-reloads whenever the schema file changes.
    ///
    /// Server and database settings can be declared in fraiseql.toml under [server]
    /// and [database] sections.  CLI flags take precedence over TOML settings, which
    /// take precedence over defaults.  The database URL is resolved in this order:
    /// --database flag > DATABASE_URL env var > [database].url in fraiseql.toml.
    #[command(after_help = "\
EXAMPLES:
    fraiseql run
    fraiseql run fraiseql.toml --database postgres://localhost/mydb
    fraiseql run --port 3000 --watch
    fraiseql run schema.json --introspection

TOML CONFIG:
    [server]
    host = \"127.0.0.1\"
    port = 9000

    [server.cors]
    origins = [\"https://app.example.com\"]

    [database]
    url      = \"${DATABASE_URL}\"
    pool_min = 2
    pool_max = 20")]
    Run {
        /// Input file path (fraiseql.toml or schema.json); auto-detected if omitted
        #[arg(value_name = "INPUT")]
        input: Option<String>,

        /// Database URL (overrides [database].url in fraiseql.toml and DATABASE_URL env var)
        #[arg(short, long, value_name = "DATABASE_URL")]
        database: Option<String>,

        /// Port to listen on (overrides [server].port in fraiseql.toml)
        #[arg(short, long, value_name = "PORT")]
        port: Option<u16>,

        /// Bind address (overrides [server].host in fraiseql.toml)
        #[arg(long, value_name = "HOST")]
        bind: Option<String>,

        /// Watch input file for changes and hot-reload the server
        #[arg(short, long)]
        watch: bool,

        /// Enable the GraphQL introspection endpoint (no auth required)
        #[arg(long)]
        introspection: bool,
    },

    /// Validate a trusted documents manifest
    ///
    /// Checks that the manifest JSON is well-formed and that each key
    /// is a valid SHA-256 hex string matching its query body.
    #[command(after_help = "\
EXAMPLES:
    fraiseql validate-documents manifest.json")]
    ValidateDocuments {
        /// Path to the trusted documents manifest JSON file
        #[arg(value_name = "MANIFEST")]
        manifest: String,
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

#[derive(Subcommand)]
enum MigrateCommands {
    /// Apply pending migrations
    Up {
        /// Database connection URL
        #[arg(short, long, value_name = "DATABASE_URL")]
        database: Option<String>,

        /// Migration directory
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,
    },

    /// Roll back migrations
    Down {
        /// Database connection URL
        #[arg(short, long, value_name = "DATABASE_URL")]
        database: Option<String>,

        /// Migration directory
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,

        /// Number of migrations to roll back
        #[arg(long, default_value = "1")]
        steps: u32,
    },

    /// Show migration status
    Status {
        /// Database connection URL
        #[arg(short, long, value_name = "DATABASE_URL")]
        database: Option<String>,

        /// Migration directory
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,
    },

    /// Create a new migration file
    Create {
        /// Migration name
        #[arg(value_name = "NAME")]
        name: String,

        /// Migration directory
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,
    },
}

/// Run the FraiseQL CLI. Called from both the `fraiseql-cli` and `fraiseql` binary entry points.
pub async fn run() {
    use crate::{commands, output};

    if let Some(code) = handle_introspection_flags() {
        process::exit(code);
    }

    let cli = Cli::parse();

    init_logging(cli.verbose, cli.debug);

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

        Commands::Extract {
            input,
            language,
            recursive,
            output,
        } => commands::extract::run(&input, language.as_deref(), recursive, &output),

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

        Commands::DependencyGraph { schema, format } => {
            match commands::dependency_graph::GraphFormat::from_str(&format) {
                Ok(fmt) => match commands::dependency_graph::run(&schema, fmt) {
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

        Commands::Lint {
            schema,
            federation: _,
            cost: _,
            cache: _,
            auth: _,
            compilation: _,
            fail_on_critical,
            fail_on_warning,
            verbose: _,
        } => {
            let opts = commands::lint::LintOptions {
                fail_on_critical,
                fail_on_warning,
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
        } => match commands::generate_views::RefreshStrategy::parse(&refresh_strategy) {
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

        Commands::Validate {
            command,
            input,
            check_cycles,
            check_unused,
            strict,
            types,
        } => match command {
            Some(ValidateCommands::Facts { schema, database }) => {
                commands::validate_facts::run(std::path::Path::new(&schema), &database).await
            },
            None => match input {
                Some(input) => {
                    let opts = commands::validate::ValidateOptions {
                        check_cycles,
                        check_unused,
                        strict,
                        filter_types: types,
                    };
                    match commands::validate::run_with_options(&input, opts) {
                        Ok(result) => {
                            println!(
                                "{}",
                                output::OutputFormatter::new(cli.json, cli.quiet).format(&result)
                            );
                            if result.status == "validation-failed" {
                                Err(anyhow::anyhow!("Validation failed"))
                            } else {
                                Ok(())
                            }
                        },
                        Err(e) => Err(e),
                    }
                },
                None => Err(anyhow::anyhow!("INPUT required when no subcommand provided")),
            },
        },

        Commands::Introspect { command } => match command {
            IntrospectCommands::Facts { database, format } => {
                match commands::introspect_facts::OutputFormat::parse(&format) {
                    Ok(fmt) => commands::introspect_facts::run(&database, fmt).await,
                    Err(e) => Err(anyhow::anyhow!(e)),
                }
            },
        },

        Commands::Generate {
            input,
            language,
            output,
        } => match commands::init::Language::from_str(&language) {
            Ok(lang) => commands::generate::run(&input, lang, output.as_deref()),
            Err(e) => Err(anyhow::anyhow!(e)),
        },

        Commands::Init {
            project_name,
            language,
            database,
            size,
            no_git,
        } => {
            match (
                commands::init::Language::from_str(&language),
                commands::init::Database::from_str(&database),
                commands::init::ProjectSize::from_str(&size),
            ) {
                (Ok(lang), Ok(db), Ok(sz)) => {
                    let config = commands::init::InitConfig {
                        project_name,
                        language: lang,
                        database: db,
                        size: sz,
                        no_git,
                    };
                    commands::init::run(&config)
                },
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => Err(anyhow::anyhow!(e)),
            }
        },

        Commands::Migrate { command } => match command {
            MigrateCommands::Up { database, dir } => {
                let db_url = commands::migrate::resolve_database_url(database.as_deref());
                match db_url {
                    Ok(url) => {
                        let mig_dir = commands::migrate::resolve_migration_dir(dir.as_deref());
                        let action = commands::migrate::MigrateAction::Up {
                            database_url: url,
                            dir:          mig_dir,
                        };
                        commands::migrate::run(&action)
                    },
                    Err(e) => Err(e),
                }
            },
            MigrateCommands::Down {
                database,
                dir,
                steps,
            } => {
                let db_url = commands::migrate::resolve_database_url(database.as_deref());
                match db_url {
                    Ok(url) => {
                        let mig_dir = commands::migrate::resolve_migration_dir(dir.as_deref());
                        let action = commands::migrate::MigrateAction::Down {
                            database_url: url,
                            dir: mig_dir,
                            steps,
                        };
                        commands::migrate::run(&action)
                    },
                    Err(e) => Err(e),
                }
            },
            MigrateCommands::Status { database, dir } => {
                let db_url = commands::migrate::resolve_database_url(database.as_deref());
                match db_url {
                    Ok(url) => {
                        let mig_dir = commands::migrate::resolve_migration_dir(dir.as_deref());
                        let action = commands::migrate::MigrateAction::Status {
                            database_url: url,
                            dir:          mig_dir,
                        };
                        commands::migrate::run(&action)
                    },
                    Err(e) => Err(e),
                }
            },
            MigrateCommands::Create { name, dir } => {
                let mig_dir = commands::migrate::resolve_migration_dir(dir.as_deref());
                let action = commands::migrate::MigrateAction::Create { name, dir: mig_dir };
                commands::migrate::run(&action)
            },
        },

        Commands::Sbom { format, output } => match commands::sbom::SbomFormat::from_str(&format) {
            Ok(fmt) => commands::sbom::run(fmt, output.as_deref()),
            Err(e) => Err(anyhow::anyhow!(e)),
        },

        Commands::Run {
            input,
            database,
            port,
            bind,
            watch,
            introspection,
        } => {
            commands::run::run(input.as_deref(), database, port, bind, watch, introspection).await
        },

        Commands::ValidateDocuments { manifest } => {
            match commands::validate_documents::run(&manifest) {
                Ok(true) => Ok(()),
                Ok(false) => {
                    process::exit(2);
                },
                Err(e) => Err(e),
            }
        },

        Commands::Serve { schema, port } => commands::serve::run(&schema, port).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        if cli.debug {
            eprintln!("\nDebug info:");
            eprintln!("{e:?}");
        }
        process::exit(1);
    }
}

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

fn handle_introspection_flags() -> Option<i32> {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help-json") {
        let cmd = Cli::command();
        let version = env!("CARGO_PKG_VERSION");
        let help = crate::introspection::extract_cli_help(&cmd, version);
        let result = crate::output::CommandResult::success(
            "help",
            serde_json::to_value(&help).expect("CliHelp is always serializable"),
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("CommandResult is always serializable")
        );
        return Some(0);
    }

    if args.iter().any(|a| a == "--list-commands") {
        let cmd = Cli::command();
        let commands = crate::introspection::list_commands(&cmd);
        let result = crate::output::CommandResult::success(
            "list-commands",
            serde_json::to_value(&commands).expect("command list is always serializable"),
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("CommandResult is always serializable")
        );
        return Some(0);
    }

    let idx = args.iter().position(|a| a == "--show-output-schema")?;
    let available = crate::output_schemas::list_schema_commands().join(", ");

    let Some(cmd_name) = args.get(idx + 1) else {
        let result = crate::output::CommandResult::error(
            "show-output-schema",
            &format!("Missing command name. Available: {available}"),
            "MISSING_ARGUMENT",
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("CommandResult is always serializable")
        );
        return Some(1);
    };

    if let Some(schema) = crate::output_schemas::get_output_schema(cmd_name) {
        let result = crate::output::CommandResult::success(
            "show-output-schema",
            serde_json::to_value(&schema).expect("output schema is always serializable"),
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("CommandResult is always serializable")
        );
        return Some(0);
    }

    let result = crate::output::CommandResult::error(
        "show-output-schema",
        &format!("Unknown command: {cmd_name}. Available: {available}"),
        "UNKNOWN_COMMAND",
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&result).expect("CommandResult is always serializable")
    );
    Some(1)
}
