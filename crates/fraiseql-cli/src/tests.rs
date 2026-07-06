#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod introspection_tests {
    use clap::{Arg, Command as ClapCommand};

    use super::super::introspection::*;

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

mod output_schemas_tests {
    use super::super::output_schemas::*;

    #[test]
    fn test_get_output_schema_compile() {
        let schema = get_output_schema("compile");
        assert!(schema.is_some());
        let schema = schema.unwrap();
        assert_eq!(schema.command, "compile");
        assert_eq!(schema.format, "json");
    }

    #[test]
    fn test_get_output_schema_unknown() {
        let schema = get_output_schema("unknown-command");
        assert!(schema.is_none());
    }

    #[test]
    fn test_list_schema_commands() {
        let commands = list_schema_commands();
        assert!(commands.contains(&"compile"));
        assert!(commands.contains(&"validate"));
        assert!(commands.contains(&"lint"));
    }

    #[test]
    fn test_success_schema_structure() {
        let schema = get_output_schema("cost").unwrap();
        let success = &schema.success;

        assert_eq!(success["type"], "object");
        assert!(success["required"].is_array());
        assert!(success["properties"].is_object());
    }

    #[test]
    fn test_error_schema_structure() {
        let schema = get_output_schema("compile").unwrap();
        let error = &schema.error;

        assert_eq!(error["type"], "object");
        assert!(error["properties"]["message"].is_object());
        assert!(error["properties"]["code"].is_object());
    }
}

mod runner_tests {
    use anyhow::Context as _;

    use super::super::runner::{error_causes, format_cli_error};

    #[test]
    fn format_cli_error_json_mode_produces_structured_object() {
        let output = format_cli_error("something went wrong", &[], None, true, 1);
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("must be valid JSON");
        assert_eq!(parsed["error"]["message"], "something went wrong");
        assert_eq!(parsed["error"]["code"], 1);
    }

    #[test]
    fn format_cli_error_json_mode_uses_exit_code() {
        let output = format_cli_error("validation failed", &[], None, true, 2);
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("must be valid JSON");
        assert_eq!(parsed["error"]["code"], 2);
    }

    #[test]
    fn format_cli_error_plain_mode_produces_human_readable_text() {
        // No causes → byte-identical to the pre-chain output (backward compatible).
        let output = format_cli_error("file not found", &[], None, false, 1);
        assert_eq!(output, "Error: file not found");
    }

    #[test]
    fn format_cli_error_plain_mode_appends_debug_info() {
        let output = format_cli_error("oops", &[], Some("stack trace here"), false, 1);
        assert!(output.contains("Error: oops"));
        assert!(output.contains("Debug info:"));
        assert!(output.contains("stack trace here"));
    }

    #[test]
    fn format_cli_error_json_mode_omits_debug_info() {
        // In JSON mode, debug_info is not included — keep the output machine-parseable.
        let output = format_cli_error("oops", &[], Some("secret internals"), true, 1);
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("must be valid JSON");
        let serialized = parsed.to_string();
        assert!(!serialized.contains("secret internals"));
    }

    #[test]
    fn format_cli_error_plain_mode_surfaces_cause_chain() {
        // The regression this fixes: the underlying reason must reach the user, not
        // just the top-level context.
        let causes = vec![
            "Type 'Order' implements interface 'CascadeNode' but is missing field 'id'".to_string(),
        ];
        let output = format_cli_error(
            "Failed to convert schema to compiled format",
            &causes,
            None,
            false,
            1,
        );
        assert!(output.starts_with("Error: Failed to convert schema to compiled format"));
        assert!(output.contains("caused by: Type 'Order' implements interface 'CascadeNode'"));
    }

    #[test]
    fn format_cli_error_json_mode_includes_causes_array() {
        // `--json` must carry the chain too — previously it dropped it entirely.
        let causes = vec!["root reason".to_string()];
        let output = format_cli_error("top context", &causes, None, true, 1);
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("must be valid JSON");
        assert_eq!(parsed["error"]["message"], "top context");
        assert_eq!(parsed["error"]["causes"][0], "root reason");
    }

    #[test]
    fn format_cli_error_plain_mode_lists_every_cause_in_order() {
        let causes = vec!["middle".to_string(), "root".to_string()];
        let output = format_cli_error("top", &causes, None, false, 1);
        let mid = output.find("caused by: middle").expect("middle cause present");
        let root = output.find("caused by: root").expect("root cause present");
        assert!(mid < root, "causes are rendered outermost-first");
    }

    #[test]
    fn error_causes_extracts_chain_below_top_context() {
        // `bail!("root")` wrapped in two contexts: top is the message, the rest are causes.
        let err = Err::<(), _>(anyhow::anyhow!("root reason"))
            .context("middle context")
            .context("top context")
            .unwrap_err();
        assert_eq!(err.to_string(), "top context");
        assert_eq!(
            error_causes(&err),
            vec!["middle context".to_string(), "root reason".to_string(),]
        );
    }
}
