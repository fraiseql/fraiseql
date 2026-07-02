//! Cross-subgraph `@requires` pre-fetch for saga forward execution.
//!
//! Before a saga step's mutation runs, any [`crate::saga_store::RequiredField`] it
//! declares is fetched from its owning subgraph's `_entities` endpoint and merged
//! into the step's mutation variables. This lets a step whose input depends on data
//! owned by another subgraph (a `chargeCard` step that `@requires product.price`)
//! run in a distributed saga.
//!
//! The pure helpers (building the representation, extracting the field, merging the
//! value, shaping the failure) have no I/O and are unit-tested on every push. The
//! async [`resolve_required_fields`] drives the HTTP entity resolver over them. All
//! are consumed only by the feature-gated wired executor, so without `unstable-saga`
//! they are dead in a non-test build — hence the module-level `allow(dead_code)` for
//! that configuration (the established `#428` pattern shared with `forward.rs`).
#![cfg_attr(not(feature = "unstable-saga"), allow(dead_code))]

use std::collections::HashMap;

use reqwest::Url;
use serde_json::Value;

use super::StepExecutionResult;
use crate::{
    http_resolver::HttpEntityResolver,
    saga_store::{RequiredField, StepState},
    selection_parser::FieldSelection,
    types::EntityRepresentation,
};

/// Build the federation `_entities` representation for a required field's owning
/// entity from its `typename` + `key` object.
///
/// The `key` must be a JSON object (e.g. `{"id": "product-1"}`); its members become
/// both the representation's key fields and its all-fields, so the resolver sends
/// them alongside the synthesised `__typename`.
fn build_representation(typename: &str, key: &Value) -> Result<EntityRepresentation, String> {
    let obj = key.as_object().ok_or_else(|| {
        format!("@requires key for '{typename}' must be a JSON object, got: {key}")
    })?;
    let fields: HashMap<String, Value> = obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    Ok(EntityRepresentation {
        typename:   typename.to_string(),
        key_fields: fields.clone(),
        all_fields: fields,
    })
}

/// Extract a (possibly nested, dotted) field from a resolved entity.
///
/// A missing path — or a path that resolves to JSON `null` — returns `None`: a
/// required field the owning subgraph has no value for is treated as unresolved so
/// the step fails before dispatch rather than running with a null input.
fn extract_field(entity: &Value, field_path: &str) -> Option<Value> {
    let mut current = entity;
    for segment in field_path.split('.') {
        current = current.get(segment)?;
    }
    if current.is_null() {
        None
    } else {
        Some(current.clone())
    }
}

/// Merge a fetched value into the mutation variables object under `target_var`.
///
/// Fails if `variables` is not a JSON object (mutation variables always are); the
/// error is surfaced so the step fails before dispatch rather than silently
/// dropping the fetched input.
fn merge_variable(variables: &mut Value, target_var: &str, value: Value) -> Result<(), String> {
    let object = variables.as_object_mut().ok_or_else(|| {
        format!("cannot merge @requires field '{target_var}' into non-object mutation variables")
    })?;
    object.insert(target_var.to_string(), value);
    Ok(())
}

/// Build the pre-dispatch failure result for a step whose `@requires` could not be
/// resolved: a real [`StepState::Failed`] step (never a fabricated success, audit
/// H32), carrying the reason. `duration_ms` is zero — the mutation never ran.
pub(super) fn prefetch_failure(step_number: u32, error: &str) -> (StepExecutionResult, StepState) {
    (
        StepExecutionResult {
            step_number,
            success: false,
            data: None,
            error: Some(error.to_string()),
            duration_ms: 0,
        },
        StepState::Failed,
    )
}

/// Resolve every `@requires` field for a step and return the mutation variables
/// with the fetched values merged in.
///
/// Each field is fetched from its owning subgraph's `_entities` endpoint via the
/// HTTP entity resolver, the required scalar is extracted, and it is merged into a
/// clone of `base_variables` under the field's `target_var`. Any failure — no
/// resolver configured, an unregistered subgraph, a transport error, or a missing
/// entity/field — is returned as an `Err(String)` so the caller fails the step
/// **before** dispatch; the mutation never runs with missing inputs (#429). On
/// success the fully-merged variables object is returned.
pub(super) async fn resolve_required_fields(
    required_fields: &[RequiredField],
    base_variables: &Value,
    entity_resolver: Option<&HttpEntityResolver>,
    subgraph_urls: &HashMap<String, Url>,
) -> Result<Value, String> {
    let resolver = entity_resolver.ok_or_else(|| {
        "step declares @requires fields but no entity resolver is configured (call \
         with_entity_resolver)"
            .to_string()
    })?;

    let mut variables = base_variables.clone();
    for required in required_fields {
        let url = subgraph_urls.get(&required.subgraph).ok_or_else(|| {
            format!(
                "@requires field '{}' names unregistered subgraph '{}'",
                required.field_path, required.subgraph
            )
        })?;

        let representation = build_representation(&required.typename, &required.key)?;
        // The `_entities` selection requests the first path segment; the full dotted
        // path is traversed on the returned entity.
        let selected = required.field_path.split('.').next().unwrap_or(&required.field_path);
        let selection = FieldSelection::new(vec![selected.to_string()]);

        let resolved = resolver
            .resolve_entities(url.as_str(), std::slice::from_ref(&representation), &selection)
            .await
            .map_err(|e| {
                format!(
                    "@requires fetch for '{}' from subgraph '{}' failed: {e}",
                    required.field_path, required.subgraph
                )
            })?;

        // A representation the owning subgraph cannot resolve comes back as a
        // missing or JSON-`null` entity — both mean "not found", so the step fails
        // before dispatch rather than running with a missing input.
        let entity = resolved
            .into_iter()
            .next()
            .flatten()
            .filter(|entity| !entity.is_null())
            .ok_or_else(|| {
                format!(
                    "@requires entity '{}' not found in subgraph '{}'",
                    required.typename, required.subgraph
                )
            })?;

        let value = extract_field(&entity, &required.field_path).ok_or_else(|| {
            format!(
                "@requires field '{}' missing from resolved '{}' entity",
                required.field_path, required.typename
            )
        })?;

        merge_variable(&mut variables, &required.target_var, value)?;
    }

    Ok(variables)
}

#[cfg(test)]
mod tests;
