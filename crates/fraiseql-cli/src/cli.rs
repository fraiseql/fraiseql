//! CLI argument definitions: `Cli` struct, `Commands` enum, and all sub-command enums.

use clap::{Parser, Subcommand};

/// Exit codes documented in help text
pub(crate) const EXIT_CODES_HELP: &str = "\
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
pub(crate) struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub(crate) verbose: bool,

    /// Enable debug logging
    #[arg(short, long, global = true)]
    pub(crate) debug: bool,

    /// Output as JSON (machine-readable)
    #[arg(long, global = true)]
    pub(crate) json: bool,

    /// Suppress output (exit code only)
    #[arg(short, long, global = true)]
    pub(crate) quiet: bool,

    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
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
    #[cfg(feature = "run-server")]
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

    /// Generate OpenAPI 3.0.3 specification from compiled schema
    ///
    /// Reads a compiled schema with REST configuration and outputs an OpenAPI
    /// specification documenting all REST endpoints, parameters, and schemas.
    #[command(after_help = "\
EXAMPLES:
    fraiseql openapi schema.compiled.json
    fraiseql openapi schema.compiled.json -o openapi.json
    fraiseql openapi schema.compiled.json -o -")]
    Openapi {
        /// Path to schema.compiled.json
        #[arg(value_name = "SCHEMA")]
        schema: String,

        /// Output file path (use - for stdout)
        #[arg(short, long, default_value = "openapi.json")]
        output: String,
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

    /// Run diagnostic checks on your FraiseQL setup
    ///
    /// Validates configuration, connectivity, and common setup issues.
    /// Checks: schema exists/parses/version, TOML config, DATABASE_URL,
    /// database reachability, JWT secret, Redis, TLS, and cache+auth coherence.
    #[command(after_help = "\
EXAMPLES:
    fraiseql doctor
    fraiseql doctor --config fraiseql.toml --schema schema.compiled.json
    fraiseql doctor --json")]
    Doctor {
        /// Path to fraiseql.toml config file
        #[arg(long, default_value = "fraiseql.toml")]
        config: String,

        /// Path to schema.compiled.json
        #[arg(long, default_value = "schema.compiled.json")]
        schema: String,

        /// Override DATABASE_URL for connectivity check
        #[arg(long, value_name = "DATABASE_URL")]
        database: Option<String>,
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
pub(crate) enum ValidateCommands {
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
pub(crate) enum FederationCommands {
    /// Export federation graph
    Graph {
        /// Path to schema.compiled.json
        #[arg(value_name = "SCHEMA")]
        schema: String,

        /// Output format (json, dot, mermaid)
        #[arg(short, long, value_name = "FORMAT", default_value = "json")]
        format: String,
    },

    /// Start a federation gateway
    ///
    /// Loads a gateway configuration file, validates subgraph schemas,
    /// and starts an HTTP server that routes GraphQL queries across
    /// multiple FraiseQL subgraph instances.
    #[command(after_help = "\
EXAMPLES:
    fraiseql federation gateway gateway.toml
    fraiseql federation gateway gateway.toml --check")]
    Gateway {
        /// Path to gateway configuration TOML file
        #[arg(value_name = "CONFIG")]
        config: String,

        /// Validate configuration only, don't start the server
        #[arg(long)]
        check: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum IntrospectCommands {
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
pub(crate) enum MigrateCommands {
    /// Apply pending migrations
    Up {
        /// Database connection URL
        #[arg(long, value_name = "DATABASE_URL")]
        database: Option<String>,

        /// Migration directory
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,
    },

    /// Roll back migrations
    Down {
        /// Database connection URL
        #[arg(long, value_name = "DATABASE_URL")]
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
        #[arg(long, value_name = "DATABASE_URL")]
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
