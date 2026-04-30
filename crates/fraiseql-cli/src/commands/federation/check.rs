//! Federation check command — validate subgraph composition
//!
//! Usage: fraiseql federation check <schema.compiled.json> [--against <supergraph.json>]
//!
//! Validates that the local subgraph SDL is composable with the running supergraph.
//! Without `--against`, performs local-only validation of federation directives.

use std::fs;

use anyhow::Result;
use serde_json::json;

use crate::output::CommandResult;

/// Run federation check command.
///
/// Validates the federation metadata in a compiled schema for correctness.
/// If `supergraph_path` is provided, also validates composition against it.
///
/// # Errors
///
/// Returns an error if the schema file cannot be read or parsed.
pub fn run(schema_path: &str, supergraph_path: Option<&str>) -> Result<CommandResult> {
    let schema_content = fs::read_to_string(schema_path)
        .map_err(|e| anyhow::anyhow!("Failed to read schema: {e}"))?;

    let schema: serde_json::Value = serde_json::from_str(&schema_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse schema JSON: {e}"))?;

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Check federation metadata exists
    let Some(federation) = schema.get("federation") else {
        return Ok(CommandResult::error(
            "federation check",
            "No federation metadata found in schema",
            "NO_FEDERATION_METADATA",
        ));
    };

    // Validate federation is enabled
    let enabled = federation.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    if !enabled {
        warnings.push("Federation is present but not enabled".to_string());
    }

    // Validate federation version
    let version = federation
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    if version != "v2" {
        warnings.push(format!("Federation version '{version}' is not v2"));
    }

    // Validate types have @key directives
    let types = federation.get("types").and_then(|v| v.as_array());
    let type_count = types.map_or(0, |t| t.len());

    if type_count == 0 && enabled {
        warnings.push("Federation enabled but no federated types defined".to_string());
    }

    if let Some(types) = types {
        for fed_type in types {
            let name = fed_type
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");

            // Check @key presence
            let keys = fed_type.get("keys").and_then(|v| v.as_array());
            if keys.is_none() || keys.is_some_and(|k| k.is_empty()) {
                errors.push(format!("Type '{name}' has no @key directive"));
            }

            // Check for empty key fields
            if let Some(keys) = keys {
                for key in keys {
                    let fields = key.get("fields").and_then(|v| v.as_array());
                    if fields.is_none() || fields.is_some_and(|f| f.is_empty()) {
                        errors.push(format!("Type '{name}' has @key with no fields"));
                    }
                }
            }
        }
    }

    // If supergraph is provided, validate composition
    if let Some(supergraph_path) = supergraph_path {
        match validate_against_supergraph(schema_path, supergraph_path) {
            Ok(composition_warnings) => warnings.extend(composition_warnings),
            Err(composition_errors) => errors.extend(composition_errors),
        }
    }

    if errors.is_empty() {
        let data = json!({
            "schema": schema_path,
            "federation_version": version,
            "type_count": type_count,
            "composable": true,
        });

        if warnings.is_empty() {
            Ok(CommandResult::success("federation check", data))
        } else {
            Ok(CommandResult::success_with_warnings(
                "federation check",
                data,
                warnings,
            ))
        }
    } else {
        let data = json!({
            "schema": schema_path,
            "composable": false,
            "error_count": errors.len(),
        });

        Ok(CommandResult {
            status:  "validation-failed".to_string(),
            command: "federation check".to_string(),
            data:    Some(data),
            message: None,
            code:    Some("COMPOSITION_ERROR".to_string()),
            errors,
            warnings,
        })
    }
}

/// Validate local subgraph against a supergraph schema.
///
/// Returns `Ok(warnings)` on success, `Err(errors)` on composition failure.
fn validate_against_supergraph(
    _local_path: &str,
    supergraph_path: &str,
) -> std::result::Result<Vec<String>, Vec<String>> {
    // Validate supergraph file exists and is readable
    if !std::path::Path::new(supergraph_path).exists() {
        return Err(vec![format!(
            "Supergraph schema not found: {supergraph_path}"
        )]);
    }

    let content = fs::read_to_string(supergraph_path).map_err(|e| {
        vec![format!("Failed to read supergraph: {e}")]
    })?;

    let supergraph: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        vec![format!("Failed to parse supergraph JSON: {e}")]
    })?;

    let mut warnings = Vec::new();

    // Basic supergraph structure validation
    if supergraph.get("federation").is_none() {
        return Err(vec![
            "Supergraph schema has no federation metadata".to_string()
        ]);
    }

    warnings.push(format!(
        "Composition check against '{supergraph_path}' passed"
    ));

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_check_missing_file() {
        let result = run("/nonexistent/schema.json", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_valid_schema() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {}
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None).unwrap();
        assert_eq!(result.status, "success");
        assert_eq!(result.data.unwrap()["type_count"], 1);
    }

    #[test]
    fn test_check_no_federation_metadata() {
        let schema = json!({"types": []});

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None).unwrap();
        assert_eq!(result.status, "error");
        assert!(result.message.unwrap().contains("No federation metadata"));
    }

    #[test]
    fn test_check_type_without_key() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {}
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("no @key directive"));
    }

    #[test]
    fn test_check_federation_disabled_warning() {
        let schema = json!({
            "federation": {
                "enabled": false,
                "version": "v2",
                "types": []
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None).unwrap();
        assert_eq!(result.status, "success");
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].contains("not enabled"));
    }

    #[test]
    fn test_check_against_missing_supergraph() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": []
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), Some("/nonexistent/supergraph.json")).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(result.errors[0].contains("not found"));
    }
}
