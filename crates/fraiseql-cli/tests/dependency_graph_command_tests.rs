//! Integration tests for the dependency-graph command

use std::io::Write;

use tempfile::NamedTempFile;

/// Create a simple test schema
fn create_test_schema() -> String {
    serde_json::json!({
        "types": [
            {
                "name": "User",
                "sql_source": "v_user",
                "jsonb_column": "data",
                "fields": [
                    {"name": "id", "field_type": "ID"},
                    {"name": "profile", "field_type": {"Object": "Profile"}, "nullable": true}
                ],
                "implements": []
            },
            {
                "name": "Profile",
                "sql_source": "v_profile",
                "jsonb_column": "data",
                "fields": [
                    {"name": "bio", "field_type": "String", "nullable": true}
                ],
                "implements": []
            },
            {
                "name": "OrphanType",
                "sql_source": "v_orphan",
                "jsonb_column": "data",
                "fields": [
                    {"name": "data", "field_type": "String", "nullable": true}
                ],
                "implements": []
            }
        ],
        "queries": [
            {
                "name": "users",
                "sql_source": "v_user",
                "return_type": "[User]",
                "arguments": [],
                "max_results": 1000
            }
        ],
        "mutations": [],
        "subscriptions": [],
        "enums": [],
        "input_types": [],
        "interfaces": [],
        "unions": [],
        "directives": [],
        "observers": []
    })
    .to_string()
}

/// Create a schema with a cycle for testing
fn create_cyclic_schema() -> String {
    serde_json::json!({
        "types": [
            {
                "name": "A",
                "sql_source": "v_a",
                "jsonb_column": "data",
                "fields": [
                    {"name": "id", "field_type": "ID"},
                    {"name": "b", "field_type": {"Object": "B"}, "nullable": true}
                ],
                "implements": []
            },
            {
                "name": "B",
                "sql_source": "v_b",
                "jsonb_column": "data",
                "fields": [
                    {"name": "id", "field_type": "ID"},
                    {"name": "a", "field_type": {"Object": "A"}, "nullable": true}
                ],
                "implements": []
            }
        ],
        "queries": [
            {
                "name": "items",
                "sql_source": "v_a",
                "return_type": "[A]",
                "arguments": [],
                "max_results": 1000
            }
        ],
        "mutations": [],
        "subscriptions": [],
        "enums": [],
        "input_types": [],
        "interfaces": [],
        "unions": [],
        "directives": [],
        "observers": []
    })
    .to_string()
}

#[test]
fn test_dependency_graph_run_success() {
    use fraiseql_cli::commands::dependency_graph::{GraphFormat, run};

    let schema_json = create_test_schema();
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(schema_json.as_bytes()).unwrap();

    let result = run(temp_file.path().to_str().unwrap(), GraphFormat::Json);
    assert!(result.is_ok());

    let cmd_result = result.unwrap();
    assert_eq!(cmd_result.status, "success");

    // Should have warnings about unused type
    assert!(!cmd_result.warnings.is_empty());
    assert!(cmd_result.warnings.iter().any(|w| w.contains("OrphanType")));
}

#[test]
fn test_dependency_graph_detects_cycles() {
    use fraiseql_cli::commands::dependency_graph::{GraphFormat, run};

    let schema_json = create_cyclic_schema();
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(schema_json.as_bytes()).unwrap();

    let result = run(temp_file.path().to_str().unwrap(), GraphFormat::Json);
    assert!(result.is_ok());

    let cmd_result = result.unwrap();
    assert_eq!(cmd_result.status, "validation-failed");
    assert!(cmd_result.errors.iter().any(|e| e.contains("Circular dependency")));
}

#[test]
fn test_dependency_graph_dot_format() {
    use fraiseql_cli::commands::dependency_graph::{GraphFormat, run};

    let schema_json = create_test_schema();
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(schema_json.as_bytes()).unwrap();

    let result = run(temp_file.path().to_str().unwrap(), GraphFormat::Dot);
    assert!(result.is_ok());

    let cmd_result = result.unwrap();
    let data = cmd_result.data.unwrap();
    let dot_output = data.as_str().unwrap();

    assert!(dot_output.contains("digraph"));
    assert!(dot_output.contains("User"));
    assert!(dot_output.contains("Profile"));
}

#[test]
fn test_dependency_graph_mermaid_format() {
    use fraiseql_cli::commands::dependency_graph::{GraphFormat, run};

    let schema_json = create_test_schema();
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(schema_json.as_bytes()).unwrap();

    let result = run(temp_file.path().to_str().unwrap(), GraphFormat::Mermaid);
    assert!(result.is_ok());

    let cmd_result = result.unwrap();
    let data = cmd_result.data.unwrap();
    let mermaid_output = data.as_str().unwrap();

    assert!(mermaid_output.contains("```mermaid"));
    assert!(mermaid_output.contains("graph LR"));
}

#[test]
fn test_dependency_graph_d2_format() {
    use fraiseql_cli::commands::dependency_graph::{GraphFormat, run};

    let schema_json = create_test_schema();
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(schema_json.as_bytes()).unwrap();

    let result = run(temp_file.path().to_str().unwrap(), GraphFormat::D2);
    assert!(result.is_ok());

    let cmd_result = result.unwrap();
    let data = cmd_result.data.unwrap();
    let d2_output = data.as_str().unwrap();

    assert!(d2_output.contains("# Schema Dependency Graph"));
    assert!(d2_output.contains("direction: right"));
    assert!(d2_output.contains("User"));
    assert!(d2_output.contains("Profile"));
}

#[test]
fn test_dependency_graph_console_format() {
    use fraiseql_cli::commands::dependency_graph::{GraphFormat, run};

    let schema_json = create_test_schema();
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(schema_json.as_bytes()).unwrap();

    let result = run(temp_file.path().to_str().unwrap(), GraphFormat::Console);
    assert!(result.is_ok());

    let cmd_result = result.unwrap();
    let data = cmd_result.data.unwrap();
    let console_output = data.as_str().unwrap();

    assert!(console_output.contains("Schema Dependency Graph Analysis"));
    assert!(console_output.contains("Total types:"));
    assert!(console_output.contains("UNUSED TYPES"));
}

#[test]
fn test_dependency_graph_json_structure() {
    use fraiseql_cli::commands::dependency_graph::{GraphFormat, run};

    let schema_json = create_test_schema();
    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(schema_json.as_bytes()).unwrap();

    let result = run(temp_file.path().to_str().unwrap(), GraphFormat::Json);
    assert!(result.is_ok());

    let cmd_result = result.unwrap();
    let data = cmd_result.data.unwrap();

    // Verify JSON structure
    assert!(data.get("type_count").is_some());
    assert!(data.get("nodes").is_some());
    assert!(data.get("edges").is_some());
    assert!(data.get("cycles").is_some());
    assert!(data.get("unused_types").is_some());
    assert!(data.get("stats").is_some());

    // Check stats
    let stats = data.get("stats").unwrap();
    assert!(stats.get("total_types").is_some());
    assert!(stats.get("total_edges").is_some());
    assert!(stats.get("max_depth").is_some());
}

#[test]
fn test_dependency_graph_file_not_found() {
    use fraiseql_cli::commands::dependency_graph::{GraphFormat, run};

    let result = run("/nonexistent/path/schema.json", GraphFormat::Json);
    assert!(result.is_err());
}
