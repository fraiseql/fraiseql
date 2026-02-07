//! Integration tests for AI agent introspection features
//!
//! Tests the --help-json, --list-commands, and --show-output-schema flags
//! that enable machine-readable CLI discovery.

use std::process::Command;

use serde_json::Value;

fn run_cli(args: &[&str]) -> (String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(args)
        .output()
        .expect("Failed to execute CLI");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, code)
}

mod help_json {
    use super::*;

    #[test]
    fn outputs_valid_json() {
        let (stdout, code) = run_cli(&["--help-json"]);

        assert_eq!(code, 0, "Exit code should be 0");
        let parsed: Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["command"], "help");
    }

    #[test]
    fn includes_cli_metadata() {
        let (stdout, _) = run_cli(&["--help-json"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let data = &parsed["data"];
        assert_eq!(data["name"], "fraiseql");
        assert!(data["version"].is_string());
        assert!(!data["version"].as_str().unwrap().is_empty());
    }

    #[test]
    fn includes_subcommands() {
        let (stdout, _) = run_cli(&["--help-json"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let subcommands = parsed["data"]["subcommands"].as_array().unwrap();
        assert!(!subcommands.is_empty());

        // Check that known commands exist
        let command_names: Vec<&str> = subcommands
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();

        assert!(command_names.contains(&"compile"));
        assert!(command_names.contains(&"validate"));
        assert!(command_names.contains(&"lint"));
    }

    #[test]
    fn includes_global_options() {
        let (stdout, _) = run_cli(&["--help-json"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let global_options = parsed["data"]["global_options"].as_array().unwrap();
        let option_names: Vec<&str> = global_options
            .iter()
            .map(|o| o["name"].as_str().unwrap())
            .collect();

        assert!(option_names.contains(&"verbose"));
        assert!(option_names.contains(&"debug"));
        assert!(option_names.contains(&"json"));
    }

    #[test]
    fn includes_exit_codes() {
        let (stdout, _) = run_cli(&["--help-json"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let exit_codes = parsed["data"]["exit_codes"].as_array().unwrap();
        assert!(!exit_codes.is_empty());

        let codes: Vec<i64> = exit_codes
            .iter()
            .map(|e| e["code"].as_i64().unwrap())
            .collect();

        assert!(codes.contains(&0)); // success
        assert!(codes.contains(&1)); // error
        assert!(codes.contains(&2)); // validation failed
    }

    #[test]
    fn compile_command_has_examples() {
        let (stdout, _) = run_cli(&["--help-json"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let subcommands = parsed["data"]["subcommands"].as_array().unwrap();
        let compile = subcommands.iter().find(|c| c["name"] == "compile").unwrap();

        let examples = compile["examples"].as_array().unwrap();
        assert!(!examples.is_empty(), "compile command should have examples");
    }
}

mod list_commands {
    use super::*;

    #[test]
    fn outputs_valid_json() {
        let (stdout, code) = run_cli(&["--list-commands"]);

        assert_eq!(code, 0);
        let parsed: Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["command"], "list-commands");
    }

    #[test]
    fn lists_all_visible_commands() {
        let (stdout, _) = run_cli(&["--list-commands"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let commands = parsed["data"].as_array().unwrap();
        assert!(!commands.is_empty());

        let command_names: Vec<&str> = commands
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();

        // Check core commands exist
        assert!(command_names.contains(&"compile"));
        assert!(command_names.contains(&"validate"));
        assert!(command_names.contains(&"lint"));
        assert!(command_names.contains(&"analyze"));
        assert!(command_names.contains(&"explain"));
        assert!(command_names.contains(&"cost"));
    }

    #[test]
    fn excludes_hidden_commands() {
        let (stdout, _) = run_cli(&["--list-commands"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let commands = parsed["data"].as_array().unwrap();

        // "serve" is hidden in the CLI
        assert!(
            !commands
                .iter()
                .map(|c| c["name"].as_str().unwrap())
                .any(|x| x == "serve"),
            "Hidden commands should not be listed"
        );
    }

    #[test]
    fn includes_command_descriptions() {
        let (stdout, _) = run_cli(&["--list-commands"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let commands = parsed["data"].as_array().unwrap();

        for cmd in commands {
            let description = cmd["description"].as_str().unwrap();
            assert!(
                !description.is_empty(),
                "Command {} should have a description",
                cmd["name"]
            );
        }
    }

    #[test]
    fn indicates_subcommand_presence() {
        let (stdout, _) = run_cli(&["--list-commands"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let commands = parsed["data"].as_array().unwrap();
        let federation = commands.iter().find(|c| c["name"] == "federation").unwrap();

        assert_eq!(
            federation["has_subcommands"], true,
            "federation should indicate it has subcommands"
        );
    }
}

mod show_output_schema {
    use super::*;

    #[test]
    fn outputs_schema_for_compile() {
        let (stdout, code) = run_cli(&["--show-output-schema", "compile"]);

        assert_eq!(code, 0);
        let parsed: Value = serde_json::from_str(&stdout).expect("Output should be valid JSON");
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["command"], "show-output-schema");
        assert_eq!(parsed["data"]["command"], "compile");
    }

    #[test]
    fn schema_has_required_fields() {
        let (stdout, _) = run_cli(&["--show-output-schema", "validate"]);
        let parsed: Value = serde_json::from_str(&stdout).unwrap();

        let data = &parsed["data"];
        assert!(data["schema_version"].is_string());
        assert_eq!(data["format"], "json");
        assert!(data["success"].is_object());
        assert!(data["error"].is_object());
    }

    #[test]
    fn error_for_unknown_command() {
        let (stdout, code) = run_cli(&["--show-output-schema", "nonexistent"]);

        assert_eq!(code, 1);
        let parsed: Value = serde_json::from_str(&stdout).expect("Error should still be JSON");
        assert_eq!(parsed["status"], "error");
        assert_eq!(parsed["code"], "UNKNOWN_COMMAND");
    }

    #[test]
    fn error_for_missing_command_name() {
        let (stdout, code) = run_cli(&["--show-output-schema"]);

        assert_eq!(code, 1);
        let parsed: Value = serde_json::from_str(&stdout).expect("Error should still be JSON");
        assert_eq!(parsed["status"], "error");
        assert_eq!(parsed["code"], "MISSING_ARGUMENT");
    }

    #[test]
    fn all_documented_commands_have_schemas() {
        let documented_commands = vec![
            "compile",
            "validate",
            "lint",
            "analyze",
            "explain",
            "cost",
            "dependency-graph",
        ];

        for cmd in documented_commands {
            let (stdout, code) = run_cli(&["--show-output-schema", cmd]);
            assert_eq!(
                code, 0,
                "Command '{}' should have an output schema",
                cmd
            );

            let parsed: Value = serde_json::from_str(&stdout).unwrap();
            assert_eq!(parsed["data"]["command"], cmd);
        }
    }
}

mod exit_codes_in_help {
    use super::*;

    #[test]
    fn regular_help_shows_exit_codes() {
        let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
            .args(["--help"])
            .output()
            .expect("Failed to execute CLI");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("EXIT CODES:"),
            "Help should include EXIT CODES section"
        );
        assert!(stdout.contains('0'), "Help should document exit code 0");
        assert!(stdout.contains('1'), "Help should document exit code 1");
        assert!(stdout.contains('2'), "Help should document exit code 2");
    }
}

mod examples_in_help {
    use super::*;

    #[test]
    fn compile_help_shows_examples() {
        let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
            .args(["compile", "--help"])
            .output()
            .expect("Failed to execute CLI");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("EXAMPLES:"),
            "compile --help should include EXAMPLES section"
        );
        assert!(
            stdout.contains("fraiseql compile"),
            "Examples should show fraiseql compile usage"
        );
    }

    #[test]
    fn validate_help_shows_examples() {
        let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
            .args(["validate", "--help"])
            .output()
            .expect("Failed to execute CLI");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("EXAMPLES:"),
            "validate --help should include EXAMPLES section"
        );
    }

    #[test]
    fn lint_help_shows_examples() {
        let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
            .args(["lint", "--help"])
            .output()
            .expect("Failed to execute CLI");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("EXAMPLES:"),
            "lint --help should include EXAMPLES section"
        );
    }
}
