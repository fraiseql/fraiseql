//! Federation check command — validate a subgraph's federation declarations.
//!
//! Usage: fraiseql federation check <schema.compiled.json> [--against <other.compiled.json>]
//!
//! Reads the `federation` block `fraiseql compile` writes — `entities`, `shareable_types` —
//! and judges every entity's `@key` with
//! [`CompiledSchema::federation_key_problems`], the function the compiler and the schema
//! loader run. The three cannot disagree about a key.
//!
//! This command used to walk a `federation.types[].keys[]` shape that no producer writes.
//! On every compiled schema it found no types, validated nothing and reported success
//! (#1395). That shape is now refused: the compiled `federation` block rejects unknown
//! keys.
//!
//! With `--against`, the subgraph is also compared with another subgraph's compiled
//! schema: an entity both declare must be keyed identically, and a field both define on
//! the same type must be resolvable by both (`@shareable` on both sides, `@external` here,
//! or part of the entity's key). Authoritative composition remains the gateway
//! composer's job, and the result says so.

use std::{collections::HashSet, fs};

use anyhow::Result;
use fraiseql_core::schema::{CompiledSchema, FederationConfig};
use serde_json::json;

use crate::output::CommandResult;

const COMMAND: &str = "federation check";

/// Run federation check command.
///
/// When `json` is `true`, the result is serialized and written to stdout before returning.
///
/// # Errors
///
/// Returns an error if a schema file cannot be read, or the result cannot be serialized.
/// A schema that is not a compiled schema, or whose federation declarations cannot be
/// served, is a `validation-failed` result, not an error.
pub fn run(schema_path: &str, against: Option<&str>, json: bool) -> Result<CommandResult> {
    let result = check(schema_path, against)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|e| anyhow::anyhow!("Failed to serialize result: {e}"))?
        );
    }
    Ok(result)
}

fn check(schema_path: &str, against: Option<&str>) -> Result<CommandResult> {
    let schema = match load(schema_path)? {
        Ok(schema) => schema,
        Err(problem) => {
            return Ok(CommandResult::validation_failed(COMMAND, vec![problem], "INVALID_SCHEMA"));
        },
    };

    let Some(federation) = &schema.federation else {
        return Ok(CommandResult::error(
            COMMAND,
            "No federation metadata found in schema",
            "NO_FEDERATION_METADATA",
        ));
    };

    let mut errors = schema.federation_key_violations();
    let mut warnings = Vec::new();

    if !federation.enabled {
        warnings.push("Federation is present but not enabled".to_string());
    }
    let version = federation.version.as_deref().unwrap_or("unknown");
    if version != "v2" {
        warnings.push(format!("Federation version '{version}' is not v2"));
    }
    if federation.entities.is_empty() && federation.enabled {
        warnings.push("Federation enabled but no entities declared".to_string());
    }

    if let Some(other_path) = against {
        match load(other_path)? {
            Err(problem) => errors.push(problem),
            Ok(other) => {
                errors.extend(compare(&schema, &other));
                warnings.push(format!(
                    "Compared against '{other_path}': @key agreement and field sharing. \
                     Authoritative composition (satisfiability, full directive semantics) is \
                     performed by the gateway composer"
                ));
            },
        }
    }

    if !errors.is_empty() {
        let mut result = CommandResult::validation_failed(COMMAND, errors, "COMPOSITION_ERROR");
        result.warnings = warnings;
        return Ok(result);
    }

    let data = json!({
        "schema": schema_path,
        "federation_version": version,
        "entity_count": federation.entities.len(),
    });
    Ok(if warnings.is_empty() {
        CommandResult::success(COMMAND, data)
    } else {
        CommandResult::success_with_warnings(COMMAND, data, warnings)
    })
}

/// Read a compiled schema. The inner `Err` is a refusal of the document's shape, worded
/// for the report; the outer one is a file that could not be read.
pub(super) fn load(path: &str) -> Result<std::result::Result<CompiledSchema, String>> {
    let content =
        fs::read_to_string(path).map_err(|e| anyhow::anyhow!("Failed to read {path}: {e}"))?;
    Ok(serde_json::from_str::<CompiledSchema>(&content).map_err(|e| {
        format!(
            "{path} is not a compiled schema ({e}); run `fraiseql compile` and check the \
             schema.compiled.json it writes"
        )
    }))
}

/// The field names a key selects, at any depth.
fn key_names(key_fields: &[String]) -> HashSet<&str> {
    key_fields
        .iter()
        .flat_map(|k| k.split(|c: char| c.is_whitespace() || matches!(c, ',' | '{' | '}')))
        .filter(|name| !name.is_empty())
        .collect()
}

/// A key as a token sequence, so `["a b"]` and `["a", "b"]` compare equal.
fn normalized(key_fields: &[String]) -> Vec<String> {
    key_fields
        .iter()
        .flat_map(|k| {
            k.replace('{', " { ")
                .replace('}', " } ")
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|t| !t.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Can a subgraph described by `federation` resolve `field` of `type_name` beside another?
fn shareable(federation: Option<&FederationConfig>, type_name: &str, field: &str) -> bool {
    federation.is_some_and(|fed| {
        fed.shareable_types.iter().any(|t| t == type_name)
            || fed.entities.iter().any(|e| {
                e.name == type_name
                    && (e.shareable_fields.iter().any(|f| f == field)
                        || key_names(&e.key_fields).contains(field))
            })
    })
}

/// What composing `local` beside `other` would refuse.
fn compare(local: &CompiledSchema, other: &CompiledSchema) -> Vec<String> {
    let mut errors = Vec::new();
    let local_fed = local.federation.as_ref();
    let other_fed = other.federation.as_ref();

    for entity in local_fed.map(|f| f.entities.as_slice()).unwrap_or_default() {
        let theirs = other_fed.and_then(|f| f.entities.iter().find(|e| e.name == entity.name));
        if let Some(theirs) = theirs {
            if normalized(&entity.key_fields) != normalized(&theirs.key_fields) {
                errors.push(format!(
                    "Type '{}': @key(fields: \"{}\") here, @key(fields: \"{}\") in the other \
                     subgraph; an entity must be keyed identically everywhere it is declared",
                    entity.name,
                    entity.key_fields.join(" "),
                    theirs.key_fields.join(" ")
                ));
            }
        }
    }

    for type_def in &local.types {
        let name = type_def.name.as_str();
        let Some(theirs) = other.find_type(name) else {
            continue;
        };
        let external: HashSet<&str> = local_fed
            .and_then(|f| f.entities.iter().find(|e| e.name == name))
            .map(|e| e.external_fields.iter().map(String::as_str).collect())
            .unwrap_or_default();
        for field in &type_def.fields {
            let field = field.name.as_str();
            if !theirs.fields.iter().any(|f| f.name == field) || external.contains(field) {
                continue;
            }
            if !(shareable(local_fed, name, field) && shareable(other_fed, name, field)) {
                errors.push(format!(
                    "Type '{name}' field '{field}': defined by both subgraphs but not \
                     @shareable on both (INVALID_FIELD_SHARING)"
                ));
            }
        }
    }

    errors
}
