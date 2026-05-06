#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod output_tests {
    use serde_json::json;

    use super::super::*;

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
}
