#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod metadata_tests {
    use serde_json::json;

    use super::super::metadata::*;

    #[test]
    fn test_format_table_contains_expected_values() {
        let metadata = json!({
            "User.email": {"encrypted": true},
            "User.ssn": {"requires_scope": "read:pii", "on_deny": "mask"}
        });

        let table = format_table(&metadata);

        assert!(table.contains("User.email"), "Missing User.email:\n{table}");
        assert!(table.contains("true"), "Missing encrypted=true:\n{table}");
        assert!(table.contains("User.ssn"), "Missing User.ssn:\n{table}");
        assert!(table.contains("read:pii"), "Missing scope read:pii:\n{table}");
        assert!(table.contains("mask"), "Missing on_deny=mask:\n{table}");
    }

    #[test]
    fn test_format_table_headers_present() {
        let metadata = json!({"User.email": {"encrypted": true}});
        let table = format_table(&metadata);

        assert!(table.contains("Field"), "Missing Field header");
        assert!(table.contains("Encrypted"), "Missing Encrypted header");
        assert!(table.contains("Scope"), "Missing Scope header");
        assert!(table.contains("On Deny"), "Missing On Deny header");
    }

    #[test]
    fn test_format_table_empty_metadata() {
        let table = format_table(&json!({}));
        assert!(
            table.contains("No metadata"),
            "Empty metadata should report no entries:\n{table}"
        );
    }

    #[test]
    fn test_format_table_missing_optional_fields_show_dash() {
        let metadata = json!({"User.name": {}});
        let table = format_table(&metadata);

        assert!(table.contains("User.name"), "Missing field name:\n{table}");
        // All optional columns should default to "-"
        let data_line = table.lines().find(|l| l.contains("User.name")).unwrap();
        assert!(data_line.contains('-'), "Missing dash for unset columns: {data_line}");
    }

    #[test]
    fn test_format_table_rows_sorted_alphabetically() {
        let metadata = json!({
            "User.ssn": {"requires_scope": "read:pii"},
            "User.email": {"encrypted": true}
        });
        let table = format_table(&metadata);
        let email_pos = table.find("User.email").unwrap();
        let ssn_pos = table.find("User.ssn").unwrap();
        assert!(email_pos < ssn_pos, "Rows should be sorted: email before ssn");
    }

    #[test]
    fn test_format_table_separator_line_present() {
        let metadata = json!({"User.email": {"encrypted": true}});
        let table = format_table(&metadata);
        // The separator line consists only of dashes and spaces
        let has_separator =
            table.lines().any(|l| !l.is_empty() && l.chars().all(|c| c == '-' || c == ' '));
        assert!(has_separator, "Missing separator line:\n{table}");
    }
}
