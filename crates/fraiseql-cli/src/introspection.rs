//! CLI introspection for AI agents
//!
//! Extracts command metadata from clap to enable machine-readable help output.

use clap::Command;

use crate::output::{ArgumentHelp, CliHelp, CommandHelp, CommandSummary, get_exit_codes};

/// Extract complete CLI help from a clap Command
pub fn extract_cli_help(cmd: &Command, version: &str) -> CliHelp {
    CliHelp {
        name:           cmd.get_name().to_string(),
        version:        version.to_string(),
        about:          cmd.get_about().map_or_else(String::new, ToString::to_string),
        global_options: extract_global_options(cmd),
        subcommands:    cmd
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set())
            .map(extract_command_help)
            .collect(),
        exit_codes:     get_exit_codes(),
    }
}

/// Extract help for a single command
pub fn extract_command_help(cmd: &Command) -> CommandHelp {
    let (arguments, options) = extract_arguments(cmd);

    CommandHelp {
        name: cmd.get_name().to_string(),
        about: cmd.get_about().map_or_else(String::new, ToString::to_string),
        arguments,
        options,
        subcommands: cmd
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set())
            .map(extract_command_help)
            .collect(),
        examples: extract_examples(cmd),
    }
}

/// List all available commands with summaries
pub fn list_commands(cmd: &Command) -> Vec<CommandSummary> {
    cmd.get_subcommands()
        .filter(|sub| !sub.is_hide_set())
        .map(|sub| CommandSummary {
            name:            sub.get_name().to_string(),
            description:     sub.get_about().map_or_else(String::new, ToString::to_string),
            has_subcommands: sub.get_subcommands().count() > 0,
        })
        .collect()
}

/// Extract global options from the root command
fn extract_global_options(cmd: &Command) -> Vec<ArgumentHelp> {
    cmd.get_arguments()
        .filter(|arg| arg.is_global_set())
        .map(|arg| ArgumentHelp {
            name:            arg.get_id().to_string(),
            short:           arg.get_short().map(|c| format!("-{c}")),
            long:            arg.get_long().map(|s| format!("--{s}")),
            help:            arg.get_help().map_or_else(String::new, ToString::to_string),
            required:        arg.is_required_set(),
            default_value:   arg
                .get_default_values()
                .first()
                .and_then(|v| v.to_str())
                .map(String::from),
            takes_value:     arg.get_num_args().is_some_and(|n| n.min_values() > 0),
            possible_values: arg
                .get_possible_values()
                .iter()
                .map(|v| v.get_name().to_string())
                .collect(),
        })
        .collect()
}

/// Extract arguments and options from a command
fn extract_arguments(cmd: &Command) -> (Vec<ArgumentHelp>, Vec<ArgumentHelp>) {
    let mut arguments = Vec::new();
    let mut options = Vec::new();

    for arg in cmd.get_arguments() {
        // Skip global arguments (they're listed separately)
        if arg.is_global_set() {
            continue;
        }

        // Skip the built-in help and version flags
        let id = arg.get_id().as_str();
        if id == "help" || id == "version" {
            continue;
        }

        let arg_help = ArgumentHelp {
            name:            arg.get_id().to_string(),
            short:           arg.get_short().map(|c| format!("-{c}")),
            long:            arg.get_long().map(|s| format!("--{s}")),
            help:            arg.get_help().map_or_else(String::new, ToString::to_string),
            required:        arg.is_required_set(),
            default_value:   arg
                .get_default_values()
                .first()
                .and_then(|v| v.to_str())
                .map(String::from),
            takes_value:     arg.get_num_args().is_some_and(|n| n.min_values() > 0),
            possible_values: arg
                .get_possible_values()
                .iter()
                .map(|v| v.get_name().to_string())
                .collect(),
        };

        // Positional arguments have no short or long flag
        if arg.get_short().is_none() && arg.get_long().is_none() {
            arguments.push(arg_help);
        } else {
            options.push(arg_help);
        }
    }

    (arguments, options)
}

/// Extract examples from command's after_help text
fn extract_examples(cmd: &Command) -> Vec<String> {
    // Look for EXAMPLES section in after_help
    if let Some(after_help) = cmd.get_after_help() {
        let text = after_help.to_string();
        if let Some(examples_start) = text.find("EXAMPLES:") {
            let examples_section = &text[examples_start + 9..];
            return examples_section
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && line.starts_with("fraiseql"))
                .map(String::from)
                .collect();
        }
    }
    Vec::new()
}
