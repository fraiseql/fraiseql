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
/// When `json` is `true`, the result is serialized and written to stdout before returning.
///
/// # Errors
///
/// Returns an error if the schema file cannot be read or parsed.
pub fn run(schema_path: &str, supergraph_path: Option<&str>, json: bool) -> Result<CommandResult> {
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

            // Check for empty key fields and key field existence
            if let Some(keys) = keys {
                for key in keys {
                    let fields = key.get("fields").and_then(|v| v.as_array());
                    if fields.is_none() || fields.is_some_and(|f| f.is_empty()) {
                        errors.push(format!("Type '{name}' has @key with no fields"));
                    }

                    // Validate @key field names exist on the type (when
                    // enough metadata is available to check)
                    if let Some(fields) = fields {
                        let known_fields = collect_known_fields(fed_type);
                        if !known_fields.is_empty() {
                            for field in fields {
                                if let Some(field_name) = field.as_str() {
                                    if !known_fields.contains(field_name) {
                                        errors.push(format!(
                                            "Type '{name}' has @key(fields: \"{field_name}\") \
                                             but no field named '{field_name}' exists on the type"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Validate @requires fields exist on type
    if let Some(types) = types {
        for fed_type in types {
            let name = fed_type
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");

            errors.extend(check_requires_fields(name, fed_type));
            warnings.extend(check_provides_fields(name, fed_type));

            // Check resolvable: false keys
            if let Some(keys) = fed_type.get("keys").and_then(|v| v.as_array()) {
                for key in keys {
                    let resolvable = key.get("resolvable").and_then(|v| v.as_bool()).unwrap_or(true);
                    if !resolvable {
                        let fields_str = key
                            .get("fields")
                            .and_then(|v| v.as_array())
                            .map(|f| {
                                f.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();
                        warnings.push(format!(
                            "Type '{name}' @key(fields: \"{fields_str}\") has resolvable: false \
                             — this key cannot be used for entity resolution"
                        ));
                    }
                }
            }
        }
    }

    // Check @inaccessible on root Query/Mutation fields
    warnings.extend(check_root_field_inaccessibility(&schema));

    // Validate @override directives
    if let Some(types) = types {
        for fed_type in types {
            let name = fed_type
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");

            if let Some(directives) = fed_type.get("field_directives").and_then(|v| v.as_object())
            {
                for (field_name, directive) in directives {
                    if let Some(override_from) =
                        directive.get("override_from").and_then(|v| v.as_str())
                    {
                        // Empty string is always an error
                        if override_from.is_empty() {
                            errors.push(format!(
                                "Type '{name}' field '{field_name}': \
                                 @override(from: \"\") — empty string is invalid"
                            ));
                        }
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

    let result = if errors.is_empty() {
        let data = json!({
            "schema": schema_path,
            "federation_version": version,
            "type_count": type_count,
            "composable": true,
        });

        if warnings.is_empty() {
            CommandResult::success("federation check", data)
        } else {
            CommandResult::success_with_warnings(
                "federation check",
                data,
                warnings,
            )
        }
    } else {
        let data = json!({
            "schema": schema_path,
            "composable": false,
            "error_count": errors.len(),
        });

        CommandResult {
            status:  "validation-failed".to_string(),
            command: "federation check".to_string(),
            data:    Some(data),
            message: None,
            code:    Some("COMPOSITION_ERROR".to_string()),
            errors,
            warnings,
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|e| anyhow::anyhow!("Failed to serialize result: {e}"))?
        );
    }

    Ok(result)
}

/// Collect known field names for a federated type from its JSON metadata.
///
/// Checks `field_directives` keys, `external_fields`, and `shareable_fields` —
/// any field that appears in these lists is definitely declared on the type.
fn collect_known_fields(fed_type: &serde_json::Value) -> std::collections::HashSet<String> {
    let mut known = std::collections::HashSet::new();

    // Fields from field_directives keys
    if let Some(directives) = fed_type.get("field_directives").and_then(|v| v.as_object()) {
        for key in directives.keys() {
            known.insert(key.clone());
        }
    }

    // Fields from external_fields
    if let Some(fields) = fed_type.get("external_fields").and_then(|v| v.as_array()) {
        for f in fields {
            if let Some(name) = f.as_str() {
                known.insert(name.to_string());
            }
        }
    }

    // Fields from shareable_fields
    if let Some(fields) = fed_type.get("shareable_fields").and_then(|v| v.as_array()) {
        for f in fields {
            if let Some(name) = f.as_str() {
                known.insert(name.to_string());
            }
        }
    }

    // Fields from inaccessible_fields
    if let Some(fields) = fed_type.get("inaccessible_fields").and_then(|v| v.as_array()) {
        for f in fields {
            if let Some(name) = f.as_str() {
                known.insert(name.to_string());
            }
        }
    }

    known
}

/// Collect known subgraph names from `@override(from:)` annotations in the schema.
fn known_subgraph_names_from_metadata(schema: &serde_json::Value) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    if let Some(types) = schema
        .pointer("/federation/types")
        .and_then(|v| v.as_array())
    {
        for fed_type in types {
            if let Some(directives) = fed_type.get("field_directives").and_then(|v| v.as_object())
            {
                for directive in directives.values() {
                    if let Some(from) = directive.get("override_from").and_then(|v| v.as_str()) {
                        if !from.is_empty() {
                            names.insert(from.to_string());
                        }
                    }
                }
            }
        }
    }
    names
}

/// Validate that `@requires` field references exist on the type.
fn check_requires_fields(type_name: &str, fed_type: &serde_json::Value) -> Vec<String> {
    let mut errs = Vec::new();
    let known = collect_known_fields(fed_type);
    if known.is_empty() {
        return errs;
    }

    if let Some(directives) = fed_type.get("field_directives").and_then(|v| v.as_object()) {
        for (field_name, directive) in directives {
            if let Some(requires) = directive.get("requires").and_then(|v| v.as_array()) {
                for req in requires {
                    // Extract the top-level field name from the path
                    let top_field = req
                        .get("path")
                        .and_then(|p| p.as_array())
                        .and_then(|p| p.first())
                        .and_then(|v| v.as_str());
                    if let Some(top) = top_field {
                        if !known.contains(top) {
                            errs.push(format!(
                                "Type '{type_name}' field '{field_name}': \
                                 @requires references field '{top}' which does not exist on the type"
                            ));
                        }
                    }
                }
            }
        }
    }
    errs
}

/// Emit warnings for `@provides` fields that cannot be verified locally.
fn check_provides_fields(type_name: &str, fed_type: &serde_json::Value) -> Vec<String> {
    let mut warns = Vec::new();
    if let Some(directives) = fed_type.get("field_directives").and_then(|v| v.as_object()) {
        for (field_name, directive) in directives {
            if let Some(provides) = directive.get("provides").and_then(|v| v.as_array()) {
                if !provides.is_empty() {
                    warns.push(format!(
                        "Type '{type_name}' field '{field_name}': \
                         @provides cannot be fully validated locally \
                         (return type fields may be in another subgraph)"
                    ));
                }
            }
        }
    }
    warns
}

/// Warn if any `@inaccessible` field appears on a root Query or Mutation type.
fn check_root_field_inaccessibility(schema: &serde_json::Value) -> Vec<String> {
    let mut warns = Vec::new();

    let types = schema
        .pointer("/federation/types")
        .and_then(|v| v.as_array());

    if let Some(types) = types {
        for fed_type in types {
            let name = fed_type
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if name == "Query" || name == "Mutation" {
                if let Some(fields) =
                    fed_type.get("inaccessible_fields").and_then(|v| v.as_array())
                {
                    for f in fields {
                        if let Some(field_name) = f.as_str() {
                            warns.push(format!(
                                "Type '{name}' field '{field_name}' is @inaccessible — \
                                 this hides a root {name} field from the public API, \
                                 which is unusual and likely unintentional"
                            ));
                        }
                    }
                }
            }
        }
    }
    warns
}

/// Validate local subgraph against a supergraph schema.
///
/// Returns `Ok(warnings)` on success, `Err(errors)` on composition failure.
fn validate_against_supergraph(
    local_path: &str,
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
    let mut errs = Vec::new();

    // Basic supergraph structure validation
    if supergraph.get("federation").is_none() {
        return Err(vec![
            "Supergraph schema has no federation metadata".to_string()
        ]);
    }

    // Collect known subgraph names from the supergraph
    let supergraph_subgraph_names = known_subgraph_names_from_metadata(&supergraph);

    // Validate @override(from:) references in local schema
    let local_content = fs::read_to_string(local_path).map_err(|e| {
        vec![format!("Failed to re-read local schema: {e}")]
    })?;
    let local_schema: serde_json::Value = serde_json::from_str(&local_content).map_err(|e| {
        vec![format!("Failed to re-parse local schema: {e}")]
    })?;

    if let Some(types) = local_schema
        .pointer("/federation/types")
        .and_then(|v| v.as_array())
    {
        for fed_type in types {
            let name = fed_type.get("name").and_then(|v| v.as_str()).unwrap_or("<unknown>");
            if let Some(directives) = fed_type.get("field_directives").and_then(|v| v.as_object())
            {
                for (field_name, directive) in directives {
                    if let Some(override_from) =
                        directive.get("override_from").and_then(|v| v.as_str())
                    {
                        if !override_from.is_empty()
                            && !supergraph_subgraph_names.contains(override_from)
                        {
                            errs.push(format!(
                                "Type '{name}' field '{field_name}': \
                                 @override(from: \"{override_from}\") references unknown \
                                 subgraph '{override_from}'"
                            ));
                        }
                    }
                }
            }
        }
    }

    if !errs.is_empty() {
        return Err(errs);
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
        let result = run("/nonexistent/schema.json", None, false);
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

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success");
        assert_eq!(result.data.unwrap()["type_count"], 1);
    }

    #[test]
    fn test_check_no_federation_metadata() {
        let schema = json!({"types": []});

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
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

        let result = run(path.to_str().unwrap(), None, false).unwrap();
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

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success");
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].contains("not enabled"));
    }

    #[test]
    fn test_check_key_field_not_on_type_errors() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["userId"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "name": {"external": false, "shareable": false, "inaccessible": false, "requires": [], "provides": []}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(
            result.errors.iter().any(|e| e.contains("userId") && e.contains("no field named")),
            "Expected error about missing key field 'userId': {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_key_field_exists_passes() {
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
                        "field_directives": {
                            "id": {"external": false, "shareable": false, "inaccessible": false, "requires": [], "provides": []}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_check_key_field_in_external_fields_passes() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": true,
                        "external_fields": ["id"],
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

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_check_override_empty_string_errors() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Product",
                        "keys": [{"fields": ["sku"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "sku": {},
                            "price": {"override_from": ""}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed", "Result: {result:?}");
        assert!(
            result.errors.iter().any(|e| e.contains("empty string")),
            "Expected empty override error: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_override_unknown_subgraph_with_against_errors() {
        // Local schema with @override referencing "old-pricing"
        let local = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Product",
                        "keys": [{"fields": ["sku"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "sku": {},
                            "price": {"override_from": "nonexistent-service"}
                        }
                    }
                ]
            }
        });

        // Supergraph with no subgraph named "nonexistent-service"
        let supergraph = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": []
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let local_path = dir.path().join("local.compiled.json");
        let super_path = dir.path().join("supergraph.json");
        fs::write(&local_path, serde_json::to_string_pretty(&local).unwrap()).unwrap();
        fs::write(&super_path, serde_json::to_string_pretty(&supergraph).unwrap()).unwrap();

        let result = run(
            local_path.to_str().unwrap(),
            Some(super_path.to_str().unwrap()),
            false,
        )
        .unwrap();
        assert_eq!(result.status, "validation-failed", "Result: {result:?}");
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("nonexistent-service") && e.contains("unknown")),
            "Expected unknown subgraph error: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_override_local_no_against_passes() {
        // @override(from: "old") without --against should pass (can't verify)
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Product",
                        "keys": [{"fields": ["sku"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "sku": {},
                            "price": {"override_from": "old-pricing"}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Result: {result:?}");
    }

    #[test]
    fn test_check_requires_nonexistent_field_errors() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": true,
                        "external_fields": ["id"],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {"external": true},
                            "profile": {
                                "requires": [{"path": ["nonexistent"]}]
                            }
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed", "Result: {result:?}");
        assert!(
            result.errors.iter().any(|e| e.contains("nonexistent") && e.contains("@requires")),
            "Expected @requires error about nonexistent field: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_requires_existing_field_passes() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "User",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": true,
                        "external_fields": ["id", "email"],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {"external": true},
                            "email": {"external": true},
                            "profile": {
                                "requires": [{"path": ["email"]}]
                            }
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_check_provides_emits_warning() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Order",
                        "keys": [{"fields": ["id"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {},
                            "user": {
                                "provides": [{"path": ["name"]}]
                            }
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
        assert!(
            result.warnings.iter().any(|w| w.contains("@provides") && w.contains("cannot be fully validated")),
            "Expected @provides warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_inaccessible_on_query_root_warns() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Query",
                        "keys": [{"fields": ["_service"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": ["secretField"],
                        "field_directives": {
                            "_service": {}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("@inaccessible") && w.contains("Query") && w.contains("secretField")),
            "Expected warning about @inaccessible on Query root field: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_inaccessible_on_mutation_root_warns() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Mutation",
                        "keys": [{"fields": ["_service"], "resolvable": true}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": ["dangerousAction"],
                        "field_directives": {
                            "_service": {}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("@inaccessible") && w.contains("Mutation") && w.contains("dangerousAction")),
            "Expected warning about @inaccessible on Mutation root field: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_resolvable_false_key_warns() {
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Product",
                        "keys": [{"fields": ["sku"], "resolvable": false}],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "sku": {}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
        assert!(
            result.warnings.iter().any(|w| w.contains("resolvable: false") && w.contains("Product")),
            "Expected resolvable: false warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_check_multiple_keys_validated_independently() {
        // One key field exists, the other doesn't
        let schema = json!({
            "federation": {
                "enabled": true,
                "version": "v2",
                "types": [
                    {
                        "name": "Account",
                        "keys": [
                            {"fields": ["id"], "resolvable": true},
                            {"fields": ["missingField"], "resolvable": true}
                        ],
                        "is_extends": false,
                        "external_fields": [],
                        "shareable_fields": [],
                        "inaccessible_fields": [],
                        "field_directives": {
                            "id": {},
                            "name": {}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "validation-failed", "Result: {result:?}");
        // Should error on missingField but not on id
        assert!(
            result.errors.iter().any(|e| e.contains("missingField")),
            "Expected error about missingField: {:?}",
            result.errors
        );
        assert!(
            !result.errors.iter().any(|e| e.contains("'id'")),
            "Should NOT error about 'id' which exists: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_check_inaccessible_on_regular_type_no_warning() {
        // @inaccessible on a non-root type should NOT produce the root-field warning
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
                        "inaccessible_fields": ["ssn"],
                        "field_directives": {
                            "id": {},
                            "ssn": {"inaccessible": true}
                        }
                    }
                ]
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.compiled.json");
        fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success", "Errors: {:?}", result.errors);
        assert!(
            !result.warnings.iter().any(|w| w.contains("@inaccessible")),
            "Should NOT warn about @inaccessible on non-root type: {:?}",
            result.warnings
        );
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

        let result = run(path.to_str().unwrap(), Some("/nonexistent/supergraph.json"), false).unwrap();
        assert_eq!(result.status, "validation-failed");
        assert!(result.errors[0].contains("not found"));
    }

    #[test]
    fn test_check_json_false_does_not_print() {
        // json=false: run() should return Ok without panicking.
        // Printing is suppressed; only the returned CommandResult matters.
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

        let result = run(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!(result.status, "success");
    }

    #[test]
    fn test_check_json_output_is_valid_json() {
        // When json=true, run() prints to stdout. We verify the returned
        // CommandResult is serialisable so the print cannot fail.
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

        // json=true: run() prints JSON to stdout and still returns the result
        let result = run(path.to_str().unwrap(), None, true).unwrap();
        assert_eq!(result.status, "success");
        // Verify the result itself is JSON-serialisable (the print path is exercised above)
        let serialized = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["data"]["type_count"], 1);
    }
}
