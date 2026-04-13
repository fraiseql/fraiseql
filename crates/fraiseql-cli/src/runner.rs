//! CLI command dispatch and helper utilities.

use std::{env, process, str::FromStr};

use clap::{CommandFactory, Parser};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::cli::{
    Cli, Commands, FederationCommands, IntrospectCommands, MigrateCommands, ValidateCommands,
};

/// Run the FraiseQL CLI. Called from both the `fraiseql-cli` and `fraiseql` binary entry points.
#[allow(clippy::cognitive_complexity)] // Reason: CLI dispatch with many subcommand branches
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
            federation,
            cost,
            cache,
            auth,
            compilation,
            fail_on_critical,
            fail_on_warning,
            verbose: _,
        } => {
            let opts = commands::lint::LintOptions {
                fail_on_critical,
                fail_on_warning,
                filter: commands::lint::LintCategoryFilter {
                    federation,
                    cost,
                    cache,
                    auth,
                    compilation,
                },
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

                let formatter = output::OutputFormatter::new(cli.json, cli.quiet);
                commands::generate_views::run(config, &formatter)
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
                let formatter = output::OutputFormatter::new(cli.json, cli.quiet);
                commands::validate_facts::run(std::path::Path::new(&schema), &database, &formatter)
                    .await
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
                    Ok(fmt) => {
                        let formatter = output::OutputFormatter::new(cli.json, cli.quiet);
                        commands::introspect_facts::run(&database, fmt, &formatter).await
                    },
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

        Commands::Migrate { command } => {
            let formatter = output::OutputFormatter::new(cli.json, cli.quiet);
            match command {
                MigrateCommands::Up { database, dir } => {
                    let db_url = commands::migrate::resolve_database_url(database.as_deref());
                    match db_url {
                        Ok(url) => {
                            let mig_dir = commands::migrate::resolve_migration_dir(dir.as_deref());
                            let action = commands::migrate::MigrateAction::Up {
                                database_url: url,
                                dir:          mig_dir,
                            };
                            commands::migrate::run(&action, &formatter)
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
                            commands::migrate::run(&action, &formatter)
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
                            commands::migrate::run(&action, &formatter)
                        },
                        Err(e) => Err(e),
                    }
                },
                MigrateCommands::Create { name, dir } => {
                    let mig_dir = commands::migrate::resolve_migration_dir(dir.as_deref());
                    let action = commands::migrate::MigrateAction::Create { name, dir: mig_dir };
                    commands::migrate::run(&action, &formatter)
                },
            }
        },

        Commands::Sbom { format, output } => match commands::sbom::SbomFormat::from_str(&format) {
            Ok(fmt) => commands::sbom::run(fmt, output.as_deref()),
            Err(e) => Err(anyhow::anyhow!(e)),
        },

        #[cfg(feature = "run-server")]
        Commands::Run {
            input,
            watch,
            server,
        } => commands::run::run(input.as_deref(), server, watch).await,

        Commands::ValidateDocuments { manifest } => {
            let formatter = output::OutputFormatter::new(cli.json, cli.quiet);
            match commands::validate_documents::run(&manifest, &formatter) {
                Ok(true) => Ok(()),
                Ok(false) => {
                    process::exit(2);
                },
                Err(e) => Err(e),
            }
        },

        Commands::Serve { schema, port } => commands::serve::run(&schema, port).await,

        Commands::Doctor {
            config,
            schema,
            db_url,
            json: json_flag,
        } => {
            let all_passed = commands::doctor::run(&config, &schema, db_url.as_deref(), json_flag);
            if all_passed {
                Ok(())
            } else {
                process::exit(1);
            }
        },
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

/// Serialize a value to a JSON `Value`, printing to stderr and exiting with code 2 on failure.
fn serialize_or_exit<T: serde::Serialize>(value: &T, context: &str) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|e| {
        eprintln!("fraiseql: failed to serialize {context}: {e}");
        std::process::exit(2);
    })
}

/// Serialize a value to pretty-printed JSON, printing to stderr and exiting with code 2 on failure.
fn pretty_or_exit<T: serde::Serialize>(value: &T, context: &str) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| {
        eprintln!("fraiseql: failed to format {context}: {e}");
        std::process::exit(2);
    })
}

fn handle_introspection_flags() -> Option<i32> {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help-json") {
        let cmd = Cli::command();
        let version = env!("CARGO_PKG_VERSION");
        let help = crate::introspection::extract_cli_help(&cmd, version);
        let result =
            crate::output::CommandResult::success("help", serialize_or_exit(&help, "help output"));
        println!("{}", pretty_or_exit(&result, "command result"));
        return Some(0);
    }

    if args.iter().any(|a| a == "--list-commands") {
        let cmd = Cli::command();
        let commands = crate::introspection::list_commands(&cmd);
        let result = crate::output::CommandResult::success(
            "list-commands",
            serialize_or_exit(&commands, "command list"),
        );
        println!("{}", pretty_or_exit(&result, "command result"));
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
        println!("{}", pretty_or_exit(&result, "command result"));
        return Some(1);
    };

    if let Some(schema) = crate::output_schemas::get_output_schema(cmd_name) {
        let result = crate::output::CommandResult::success(
            "show-output-schema",
            serialize_or_exit(&schema, "output schema"),
        );
        println!("{}", pretty_or_exit(&result, "command result"));
        return Some(0);
    }

    let result = crate::output::CommandResult::error(
        "show-output-schema",
        &format!("Unknown command: {cmd_name}. Available: {available}"),
        "UNKNOWN_COMMAND",
    );
    println!("{}", pretty_or_exit(&result, "command result"));
    Some(1)
}
