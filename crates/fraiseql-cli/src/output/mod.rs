//! Output formatting for CLI commands
//!
//! Supports three output modes:
//! - JSON: Machine-readable structured output for agents
//! - Text: Human-readable formatted output
//! - Quiet: No output (exit code only)

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Context for command execution - holds formatter and logging options
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CliContext {
    /// Output formatter (JSON/text/quiet mode)
    pub formatter: OutputFormatter,
    /// Enable verbose logging
    pub verbose:   bool,
    /// Enable debug logging
    pub debug:     bool,
}

impl CliContext {
    /// Create a new CLI context
    #[allow(
        dead_code,
        clippy::too_many_arguments,
        clippy::fn_params_excessive_bools,
        clippy::missing_const_for_fn
    )]
    pub fn new(json_mode: bool, quiet_mode: bool, verbose: bool, debug: bool) -> Self {
        Self {
            formatter: OutputFormatter::new(json_mode, quiet_mode),
            verbose,
            debug,
        }
    }

    /// Print a result and return the exit code
    #[allow(dead_code)]
    pub fn print_result(&self, result: &CommandResult) -> i32 {
        let output = self.formatter.format(result);
        if !output.is_empty() {
            println!("{output}");
        }
        result.exit_code
    }
}

/// Formats command output in different modes
#[derive(Debug, Clone)]
pub struct OutputFormatter {
    json_mode:  bool,
    quiet_mode: bool,
}

impl OutputFormatter {
    /// Create a new output formatter
    ///
    /// # Arguments
    /// * `json_mode` - If true, output JSON; otherwise output text
    /// * `quiet_mode` - If true and not in JSON mode, suppress all output
    pub const fn new(json_mode: bool, quiet_mode: bool) -> Self {
        Self {
            json_mode,
            quiet_mode,
        }
    }

    /// Format a command result for output
    pub fn format(&self, result: &CommandResult) -> String {
        match (self.json_mode, self.quiet_mode) {
            // JSON mode always outputs JSON regardless of quiet flag
            (true, _) => serde_json::to_string(result).unwrap_or_else(|_| {
                json!({
                    "status": "error",
                    "command": "unknown",
                    "message": "Failed to serialize response"
                })
                .to_string()
            }),
            // Quiet mode suppresses output
            (false, true) => String::new(),
            // Text mode with output
            (false, false) => Self::format_text(result),
        }
    }

    fn format_text(result: &CommandResult) -> String {
        match result.status.as_str() {
            "success" => {
                let mut output = format!("✓ {} succeeded", result.command);

                if !result.warnings.is_empty() {
                    output.push_str("\n\nWarnings:");
                    for warning in &result.warnings {
                        output.push_str(&format!("\n  • {warning}"));
                    }
                }

                output
            },
            "validation-failed" => {
                let mut output = format!("✗ {} validation failed", result.command);

                if !result.errors.is_empty() {
                    output.push_str("\n\nErrors:");
                    for error in &result.errors {
                        output.push_str(&format!("\n  • {error}"));
                    }
                }

                output
            },
            "error" => {
                let mut output = format!("✗ {} error", result.command);

                if let Some(msg) = &result.message {
                    output.push_str(&format!("\n  {msg}"));
                }

                if let Some(code) = &result.code {
                    output.push_str(&format!("\n  Code: {code}"));
                }

                output
            },
            _ => format!("? {} - unknown status: {}", result.command, result.status),
        }
    }
}

/// Result of a CLI command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    /// Status of the command: "success", "error", "validation-failed"
    pub status: String,

    /// Name of the command that was executed
    pub command: String,

    /// Primary data/output from the command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,

    /// Error message (if status is "error")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Error code (if status is "error")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    /// Validation errors (if status is "validation-failed")
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,

    /// Warnings that occurred during execution
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,

    /// Exit code for the process: 0=success, 1=error, 2=validation-failed
    #[serde(skip)]
    #[allow(dead_code)]
    pub exit_code: i32,
}

impl CommandResult {
    /// Create a successful command result with data
    pub fn success(command: &str, data: Value) -> Self {
        Self {
            status:    "success".to_string(),
            command:   command.to_string(),
            data:      Some(data),
            message:   None,
            code:      None,
            errors:    Vec::new(),
            warnings:  Vec::new(),
            exit_code: 0,
        }
    }

    /// Create a successful command result with warnings
    pub fn success_with_warnings(command: &str, data: Value, warnings: Vec<String>) -> Self {
        Self {
            status: "success".to_string(),
            command: command.to_string(),
            data: Some(data),
            message: None,
            code: None,
            errors: Vec::new(),
            warnings,
            exit_code: 0,
        }
    }

    /// Create an error result
    pub fn error(command: &str, message: &str, code: &str) -> Self {
        Self {
            status:    "error".to_string(),
            command:   command.to_string(),
            data:      None,
            message:   Some(message.to_string()),
            code:      Some(code.to_string()),
            errors:    Vec::new(),
            warnings:  Vec::new(),
            exit_code: 1,
        }
    }

    /// Create a validation failure result
    #[allow(dead_code)]
    pub fn validation_failed(command: &str, errors: Vec<String>) -> Self {
        Self {
            status: "validation-failed".to_string(),
            command: command.to_string(),
            data: None,
            message: None,
            code: None,
            errors,
            warnings: Vec::new(),
            exit_code: 2,
        }
    }

    /// Create an error result from an anyhow::Error
    #[allow(dead_code)]
    pub fn from_error(command: &str, error: anyhow::Error) -> Self {
        Self::error(command, &error.to_string(), "INTERNAL_ERROR")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_formatter_json_mode_success() {
        let formatter = OutputFormatter::new(true, false);

        let result = CommandResult::success(
            "compile",
            json!({
                "files_compiled": 2,
                "output_file": "schema.compiled.json"
            }),
        );

        let output = formatter.format(&result);
        assert!(!output.is_empty());

        // Verify it's valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("Output must be valid JSON");
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["command"], "compile");
    }

    #[test]
    fn test_output_formatter_text_mode_success() {
        let formatter = OutputFormatter::new(false, false);

        let result = CommandResult::success("compile", json!({}));
        let output = formatter.format(&result);

        assert!(!output.is_empty());
        assert!(output.contains("compile"));
        assert!(output.contains("✓"));
    }

    #[test]
    fn test_output_formatter_quiet_mode() {
        let formatter = OutputFormatter::new(false, true);

        let result = CommandResult::success("compile", json!({}));
        let output = formatter.format(&result);

        assert_eq!(output, "");
    }

    #[test]
    fn test_output_formatter_json_mode_error() {
        let formatter = OutputFormatter::new(true, false);

        let result = CommandResult::error("compile", "Parse error", "PARSE_ERROR");

        let output = formatter.format(&result);
        assert!(!output.is_empty());

        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("Output must be valid JSON");
        assert_eq!(parsed["status"], "error");
        assert_eq!(parsed["command"], "compile");
        assert_eq!(parsed["code"], "PARSE_ERROR");
    }

    #[test]
    fn test_output_formatter_validation_failure() {
        let formatter = OutputFormatter::new(true, false);

        let result = CommandResult::validation_failed(
            "validate",
            vec![
                "Invalid type: User".to_string(),
                "Missing field: id".to_string(),
            ],
        );

        let output = formatter.format(&result);

        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("Output must be valid JSON");
        assert_eq!(parsed["status"], "validation-failed");
        assert!(parsed["errors"].is_array());
        assert_eq!(parsed["errors"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_command_result_preserves_data() {
        let data = json!({
            "count": 42,
            "nested": {
                "value": "test"
            }
        });

        let result = CommandResult::success("test", data.clone());

        // Data should be preserved exactly
        assert_eq!(result.data, Some(data));
    }

    #[test]
    fn test_output_formatter_with_warnings() {
        let formatter = OutputFormatter::new(true, false);

        let result = CommandResult::success_with_warnings(
            "compile",
            json!({ "status": "ok" }),
            vec!["Optimization opportunity: add index to User.id".to_string()],
        );

        let output = formatter.format(&result);
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("Valid JSON");

        assert_eq!(parsed["status"], "success");
        assert!(parsed["warnings"].is_array());
    }

    #[test]
    fn test_text_mode_shows_status() {
        let formatter = OutputFormatter::new(false, false);

        let result = CommandResult::success("compile", json!({}));
        let output = formatter.format(&result);

        // Should contain some indication of success
        assert!(output.to_lowercase().contains("success") || output.contains("✓"));
    }

    #[test]
    fn test_text_mode_shows_error() {
        let formatter = OutputFormatter::new(false, false);

        let result = CommandResult::error("compile", "File not found", "FILE_NOT_FOUND");
        let output = formatter.format(&result);

        assert!(
            output.to_lowercase().contains("error")
                || output.contains("✗")
                || output.contains("file")
        );
    }

    #[test]
    fn test_quiet_mode_suppresses_all_output() {
        let formatter = OutputFormatter::new(false, true);

        let success = CommandResult::success("compile", json!({}));
        let error = CommandResult::error("validate", "Invalid", "INVALID");

        assert_eq!(formatter.format(&success), "");
        assert_eq!(formatter.format(&error), "");
    }

    #[test]
    fn test_json_mode_ignores_quiet_flag() {
        // JSON mode should always output JSON, even with quiet=true
        let formatter = OutputFormatter::new(true, true);

        let result = CommandResult::success("compile", json!({}));
        let output = formatter.format(&result);

        // Should still produce JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("Should be valid JSON");
        assert_eq!(parsed["status"], "success");
    }

    #[test]
    fn test_command_result_from_anyhow_error() {
        let error = anyhow::anyhow!("Database connection failed");
        let result = CommandResult::from_error("serve", error);

        assert_eq!(result.status, "error");
        assert_eq!(result.command, "serve");
    }

    #[test]
    fn test_validation_failed_exit_code() {
        let result = CommandResult::validation_failed("validate", vec!["Error 1".to_string()]);

        // Validation failures should have a specific exit code
        assert_eq!(result.exit_code, 2);
    }

    #[test]
    fn test_error_exit_code() {
        let result = CommandResult::error("compile", "Failed", "FAILED");

        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_success_exit_code() {
        let result = CommandResult::success("compile", json!({}));

        assert_eq!(result.exit_code, 0);
    }
}
