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

#[cfg(test)]
mod tests {
    use clap::{Arg, Command as ClapCommand};

    use super::*;

    fn create_test_cli() -> ClapCommand {
        ClapCommand::new("test-cli")
            .version("1.0.0")
            .about("Test CLI for unit tests")
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .long("verbose")
                    .help("Enable verbose mode")
                    .global(true)
                    .action(clap::ArgAction::SetTrue),
            )
            .subcommand(
                ClapCommand::new("compile")
                    .about("Compile files")
                    .arg(Arg::new("input").help("Input file").required(true))
                    .arg(
                        Arg::new("output")
                            .short('o')
                            .long("output")
                            .help("Output file")
                            .default_value("out.json"),
                    )
                    .after_help("EXAMPLES:\n    fraiseql compile input.json\n    fraiseql compile input.json -o output.json"),
            )
            .subcommand(
                ClapCommand::new("hidden")
                    .about("Hidden command")
                    .hide(true),
            )
    }

    #[test]
    fn test_extract_cli_help() {
        let cmd = create_test_cli();
        let help = extract_cli_help(&cmd, "1.0.0");

        assert_eq!(help.name, "test-cli");
        assert_eq!(help.version, "1.0.0");
        assert_eq!(help.about, "Test CLI for unit tests");
        assert!(!help.exit_codes.is_empty());
    }

    #[test]
    fn test_extract_global_options() {
        let cmd = create_test_cli();
        let help = extract_cli_help(&cmd, "1.0.0");

        assert!(!help.global_options.is_empty());
        let verbose = help.global_options.iter().find(|a| a.name == "verbose");
        assert!(verbose.is_some());
        let verbose = verbose.unwrap();
        assert_eq!(verbose.short, Some("-v".to_string()));
        assert_eq!(verbose.long, Some("--verbose".to_string()));
    }

    #[test]
    fn test_extract_command_help() {
        let cmd = create_test_cli();
        let compile = cmd.get_subcommands().find(|c| c.get_name() == "compile").unwrap();
        let help = extract_command_help(compile);

        assert_eq!(help.name, "compile");
        assert_eq!(help.about, "Compile files");
        assert_eq!(help.arguments.len(), 1);
        assert_eq!(help.arguments[0].name, "input");
        assert!(help.arguments[0].required);
    }

    #[test]
    fn test_extract_options() {
        let cmd = create_test_cli();
        let compile = cmd.get_subcommands().find(|c| c.get_name() == "compile").unwrap();
        let help = extract_command_help(compile);

        let output_opt = help.options.iter().find(|o| o.name == "output");
        assert!(output_opt.is_some());
        let output_opt = output_opt.unwrap();
        assert_eq!(output_opt.default_value, Some("out.json".to_string()));
    }

    #[test]
    fn test_extract_examples() {
        let cmd = create_test_cli();
        let compile = cmd.get_subcommands().find(|c| c.get_name() == "compile").unwrap();
        let help = extract_command_help(compile);

        assert_eq!(help.examples.len(), 2);
        assert!(help.examples[0].contains("fraiseql compile"));
    }

    #[test]
    fn test_list_commands() {
        let cmd = create_test_cli();
        let commands = list_commands(&cmd);

        // Should only list non-hidden commands
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "compile");
        assert!(!commands[0].has_subcommands);
    }

    #[test]
    fn test_hidden_commands_excluded() {
        let cmd = create_test_cli();
        let help = extract_cli_help(&cmd, "1.0.0");

        // Hidden command should not appear in subcommands
        let hidden = help.subcommands.iter().find(|s| s.name == "hidden");
        assert!(hidden.is_none());
    }
}
