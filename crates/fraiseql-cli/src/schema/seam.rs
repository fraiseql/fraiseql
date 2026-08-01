//! The authoring → compile seam: one definition of what an SDK can author.
//!
//! `#755` was eight authorable categories dropped at once, and the reason was structural:
//! **four** independent sites each hand-maintained its own private list of "the sections
//! that matter", and all four lists said `types` / `queries` / `mutations`:
//!
//! * [`MultiFileLoader::load_from_directory_with_tracking`](super::MultiFileLoader)
//! * [`MultiFileLoader::load_from_paths`](super::MultiFileLoader)
//! * `SchemaMerger::extend_from_json_file`
//! * `SchemaMerger::merge_values`, which rebuilt the merged JSON from scratch
//!
//! An SDK could therefore author an enum, have it validated, and watch it vanish — with
//! `✓ Schema compiled successfully` on stdout. Adding a ninth category to
//! `IntermediateSchema` would have had to be remembered at all four sites to work on any
//! TOML-workflow path, and at none of them to work on the legacy JSON path. That is not a
//! bug to patch; it is a shape to remove.
//!
//! This module is that one definition. Every loader and the merger consume it, so a new
//! section is carried by all of them or by none, and
//! `seam_coverage_manifest_test.rs` fails the build if a field is added to
//! `IntermediateSchema` without being classified here.

use anyhow::{Result, bail};
use serde_json::Value;

/// Top-level sections that are **arrays of named items**, concatenated when several files
/// contribute to one schema.
///
/// Ordering mirrors `IntermediateSchema`'s field order so the two read as one list.
pub const AUTHORABLE_ARRAY_SECTIONS: &[&str] = &[
    "types",
    "enums",
    "input_types",
    "interfaces",
    "unions",
    "queries",
    "mutations",
    "subscriptions",
    "fragments",
    "directives",
    "fact_tables",
    "aggregate_queries",
    "observers",
    "sources",
    "custom_scalars",
];

/// Top-level sections that are **single values**: a config block or a scalar setting.
///
/// Two files may not each define one of these — that is an authoring conflict with no
/// defensible resolution, so it is an error rather than a last-write-wins race.
pub const AUTHORABLE_SINGLETON_SECTIONS: &[&str] = &[
    "version",
    "naming_convention",
    "session_variables",
    "federation",
    "federation_config",
    "inject_defaults",
    "security",
    "auth",
    "observers_config",
    "subscriptions_config",
    "validation_config",
    "debug_config",
    "mcp_config",
    "rest_config",
    "grpc_config",
    "hierarchies_config",
    "changelog_config",
    "query_defaults",
];

/// Merge one loaded schema file's sections into an accumulating target.
///
/// Array sections concatenate; singleton sections are carried, and a second file declaring
/// the same singleton differently is an error naming both sources.
///
/// Unknown keys are deliberately **carried through untouched** rather than filtered: the
/// consumer (`IntermediateSchema`, which denies unknown fields) is the single place that
/// decides what is authorable. A filter here would restore the silent drop this module
/// exists to remove — the key would disappear before anything could object to it.
///
/// # Errors
///
/// Returns an error if `source` is not a JSON object, or if two sources declare the same
/// singleton section with different values.
pub fn absorb_sections(target: &mut Value, source: &Value, source_label: &str) -> Result<()> {
    let Some(source_map) = source.as_object() else {
        bail!("{source_label}: expected a JSON object at the top level");
    };

    let Some(target_map) = target.as_object_mut() else {
        bail!("internal error: the seam accumulator is not a JSON object");
    };

    for (key, value) in source_map {
        if AUTHORABLE_ARRAY_SECTIONS.contains(&key.as_str()) {
            let Some(items) = value.as_array() else {
                bail!(
                    "{source_label}: section `{key}` must be an array of definitions, found {}",
                    json_type_name(value)
                );
            };
            let slot = target_map.entry(key.clone()).or_insert_with(|| Value::Array(Vec::new()));
            let Some(accumulated) = slot.as_array_mut() else {
                bail!(
                    "{source_label}: section `{key}` was already accumulated as a non-array, \
                     which means an earlier source declared it with a conflicting shape"
                );
            };
            accumulated.extend(items.iter().cloned());
        } else if let Some(existing) = target_map.get(key) {
            if existing != value {
                bail!(
                    "{source_label}: section `{key}` is already defined with a different value by \
                     an earlier schema file. A single-valued section must be declared in exactly \
                     one place — move it to one file, or reconcile the two declarations."
                );
            }
        } else {
            target_map.insert(key.clone(), value.clone());
        }
    }

    Ok(())
}

/// A JSON value's type, for error messages.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// The singular noun for a section, for "Duplicate {noun} '{name}'" diagnostics.
///
/// Spelled out rather than derived: stripping a trailing `s` turns `queries` into `querie`
/// and `enums` into `enum`, and a diagnostic is the one place a reader is already confused.
pub fn section_noun(section: &str) -> &str {
    match section {
        "types" => "type",
        "enums" => "enum",
        "input_types" => "input type",
        "interfaces" => "interface",
        "unions" => "union",
        "queries" => "query",
        "mutations" => "mutation",
        "subscriptions" => "subscription",
        "fragments" => "fragment",
        "directives" => "directive",
        "fact_tables" => "fact table",
        "aggregate_queries" => "aggregate query",
        "observers" => "observer",
        "sources" => "source",
        "custom_scalars" => "custom scalar",
        other => other,
    }
}

/// An empty accumulator with every array section present, so a schema that authors none of
/// them still deserializes into empty vectors rather than missing keys.
pub fn empty_accumulator() -> Value {
    let mut map = serde_json::Map::new();
    for key in AUTHORABLE_ARRAY_SECTIONS {
        map.insert((*key).to_string(), Value::Array(Vec::new()));
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests;
