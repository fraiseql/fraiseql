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
    let version = federation.get("version").and_then(|v| v.as_str()).unwrap_or("unknown");
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
            let name = fed_type.get("name").and_then(|v| v.as_str()).unwrap_or("<unknown>");

            // Check @key presence
            let keys = fed_type.get("keys").and_then(|v| v.as_array());
            if keys.is_none_or(|k| k.is_empty()) {
                errors.push(format!("Type '{name}' has no @key directive"));
            }

            // Check for empty key fields and key field existence
            if let Some(keys) = keys {
                for key in keys {
                    let fields = key.get("fields").and_then(|v| v.as_array());
                    if fields.is_none_or(|f| f.is_empty()) {
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
            let name = fed_type.get("name").and_then(|v| v.as_str()).unwrap_or("<unknown>");

            errors.extend(check_requires_fields(name, fed_type));
            warnings.extend(check_provides_fields(name, fed_type));

            // Check resolvable: false keys
            if let Some(keys) = fed_type.get("keys").and_then(|v| v.as_array()) {
                for key in keys {
                    let resolvable =
                        key.get("resolvable").and_then(|v| v.as_bool()).unwrap_or(true);
                    if !resolvable {
                        let fields_str = key
                            .get("fields")
                            .and_then(|v| v.as_array())
                            .map(|f| {
                                f.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ")
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
            let name = fed_type.get("name").and_then(|v| v.as_str()).unwrap_or("<unknown>");

            if let Some(directives) = fed_type.get("field_directives").and_then(|v| v.as_object()) {
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
            CommandResult::success_with_warnings("federation check", data, warnings)
        }
    } else {
        let data = json!({
            "schema": schema_path,
            "composable": false,
            "error_count": errors.len(),
        });

        CommandResult {
            status: "validation-failed".to_string(),
            command: "federation check".to_string(),
            data: Some(data),
            message: None,
            code: Some("COMPOSITION_ERROR".to_string()),
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

/// Collect the supergraph's declared subgraph roster (`federation.subgraphs`,
/// an array of objects with a `name`).
///
/// Returns `None` when the supergraph carries no roster — which means
/// `@override(from:)` references CANNOT be validated and must be warned about,
/// never guessed at. (#820: names used to be harvested from the supergraph's
/// own `override_from` annotations — the empty set for any real supergraph —
/// so every legitimate override was reported as referencing an unknown
/// subgraph.)
fn supergraph_roster(schema: &serde_json::Value) -> Option<std::collections::HashSet<String>> {
    let subgraphs = schema.pointer("/federation/subgraphs")?.as_array()?;
    Some(
        subgraphs
            .iter()
            .filter_map(|sg| sg.get("name").and_then(|n| n.as_str()))
            .map(ToString::to_string)
            .collect(),
    )
}

/// Whether `field` is shareable on this federation type entry: field-level
/// `@shareable` (directive or `shareable_fields`), or type-level `@shareable`.
fn field_is_shareable(fed_type: &serde_json::Value, field: &str) -> bool {
    if fed_type.get("type_shareable").and_then(|v| v.as_bool()).unwrap_or(false) {
        return true;
    }
    if let Some(fields) = fed_type.get("shareable_fields").and_then(|v| v.as_array()) {
        if fields.iter().any(|f| f.as_str() == Some(field)) {
            return true;
        }
    }
    fed_type
        .pointer(&format!("/field_directives/{field}/shareable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Whether `field` is declared `@external` on this type entry (a reference to
/// another subgraph's field, not a second definition of it).
fn field_is_external(fed_type: &serde_json::Value, field: &str) -> bool {
    fed_type
        .get("external_fields")
        .and_then(|v| v.as_array())
        .is_some_and(|fields| fields.iter().any(|f| f.as_str() == Some(field)))
}

/// The `@key` field-sets declared on a type entry.
fn declared_keys(fed_type: &serde_json::Value) -> Vec<Vec<String>> {
    fed_type
        .get("keys")
        .and_then(|v| v.as_array())
        .map(|keys| {
            keys.iter()
                .filter_map(|k| k.get("fields").and_then(|f| f.as_array()))
                .map(|fields| {
                    fields.iter().filter_map(|f| f.as_str()).map(ToString::to_string).collect()
                })
                .collect()
        })
        .unwrap_or_default()
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

    let types = schema.pointer("/federation/types").and_then(|v| v.as_array());

    if let Some(types) = types {
        for fed_type in types {
            let name = fed_type.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name == "Query" || name == "Mutation" {
                if let Some(fields) = fed_type.get("inaccessible_fields").and_then(|v| v.as_array())
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

/// Validate local subgraph against a supergraph schema (#820).
///
/// Genuinely compares the two — it never reports a check it did not run:
/// - **`@key` agreement**: every `@key` the local subgraph declares on a type the supergraph also
///   carries must be one of the supergraph's declared keys for that type.
/// - **field sharing**: a field defined by both sides must be `@shareable` on both (or
///   `@external`/`@override` on the local side) — the `INVALID_FIELD_SHARING` class that shipped as
///   #698.
/// - **`@override(from:)` references**: validated against the supergraph's declared roster
///   (`federation.subgraphs`); when the supergraph carries no roster this is explicitly reported as
///   unchecked, never guessed from `override_from` annotations (which made every legitimate
///   override fail).
///
/// The success message states this scope and defers final authority to the
/// gateway composer — `composable: true` is never fabricated for a comparison
/// that did not run.
///
/// Returns `Ok(warnings)` on success, `Err(errors)` on composition failure.
fn validate_against_supergraph(
    local_path: &str,
    supergraph_path: &str,
) -> std::result::Result<Vec<String>, Vec<String>> {
    // Validate supergraph file exists and is readable
    if !std::path::Path::new(supergraph_path).exists() {
        return Err(vec![format!("Supergraph schema not found: {supergraph_path}")]);
    }

    let content = fs::read_to_string(supergraph_path)
        .map_err(|e| vec![format!("Failed to read supergraph: {e}")])?;

    let supergraph: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| vec![format!("Failed to parse supergraph JSON: {e}")])?;

    let mut warnings = Vec::new();
    let mut errs = Vec::new();

    // Basic supergraph structure validation
    if supergraph.get("federation").is_none() {
        return Err(vec!["Supergraph schema has no federation metadata".to_string()]);
    }

    let roster = supergraph_roster(&supergraph);

    // Index the supergraph's types by name for the structural comparison.
    let empty = Vec::new();
    let supergraph_types: std::collections::HashMap<&str, &serde_json::Value> = supergraph
        .pointer("/federation/types")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(|n| (n, t)))
        .collect();

    let local_content = fs::read_to_string(local_path)
        .map_err(|e| vec![format!("Failed to re-read local schema: {e}")])?;
    let local_schema: serde_json::Value = serde_json::from_str(&local_content)
        .map_err(|e| vec![format!("Failed to re-parse local schema: {e}")])?;

    if let Some(types) = local_schema.pointer("/federation/types").and_then(|v| v.as_array()) {
        for fed_type in types {
            let name = fed_type.get("name").and_then(|v| v.as_str()).unwrap_or("<unknown>");
            let super_type = supergraph_types.get(name);

            // @key agreement with the supergraph's declaration of the same type.
            if let Some(super_type) = super_type {
                let super_keys = declared_keys(super_type);
                if !super_keys.is_empty() {
                    for local_key in declared_keys(fed_type) {
                        if !super_keys.contains(&local_key) {
                            errs.push(format!(
                                "Type '{name}': local @key(fields: \"{}\") is not one of the \
                                 supergraph's declared keys for this type ({:?})",
                                local_key.join(" "),
                                super_keys.iter().map(|k| k.join(" ")).collect::<Vec<_>>()
                            ));
                        }
                    }
                }
            }

            if let Some(directives) = fed_type.get("field_directives").and_then(|v| v.as_object()) {
                for (field_name, directive) in directives {
                    // @override(from:) roster validation.
                    if let Some(override_from) =
                        directive.get("override_from").and_then(|v| v.as_str())
                    {
                        if !override_from.is_empty() {
                            match &roster {
                                Some(names) if !names.contains(override_from) => {
                                    errs.push(format!(
                                        "Type '{name}' field '{field_name}': \
                                         @override(from: \"{override_from}\") references \
                                         subgraph '{override_from}', which is not in the \
                                         supergraph's declared roster"
                                    ));
                                },
                                Some(_) => {},
                                None => {
                                    warnings.push(format!(
                                        "Type '{name}' field '{field_name}': \
                                         @override(from: \"{override_from}\") could not be \
                                         checked — the supergraph declares no subgraph roster \
                                         (federation.subgraphs)"
                                    ));
                                },
                            }
                        }
                        // An @override field is taken over, not shared — skip
                        // the field-sharing check for it.
                        continue;
                    }

                    // Field sharing: a field both sides define must be
                    // shareable on both, unless the local side merely
                    // references it (@external).
                    if let Some(super_type) = super_type {
                        let super_has_field = super_type
                            .pointer(&format!("/field_directives/{field_name}"))
                            .is_some();
                        if super_has_field
                            && !field_is_external(fed_type, field_name)
                            && !(field_is_shareable(fed_type, field_name)
                                && field_is_shareable(super_type, field_name))
                        {
                            errs.push(format!(
                                "Type '{name}' field '{field_name}': defined by both this \
                                 subgraph and the supergraph but not @shareable on both \
                                 sides (INVALID_FIELD_SHARING)"
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

    // State exactly what ran; authoritative composition is the gateway
    // composer's job and this check never claims otherwise (#820).
    warnings.push(format!(
        "Compared against '{supergraph_path}': @key agreement, field sharing, and \
         @override roster references. Authoritative composition (satisfiability, full \
         directive semantics) is performed by the gateway composer"
    ));

    Ok(warnings)
}
