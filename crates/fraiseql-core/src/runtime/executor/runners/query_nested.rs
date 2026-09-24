//! A GraphQL selection into a nested type, classified as a read of that type.
//!
//! `{ users { orders { margin } } }` serves `Order` documents out of the `users` view's
//! materialised `data`. Field-level RBAC used to classify the root's projection against
//! the root type and nothing else, so what a read of `Order` would have masked or refused
//! reached the response through any type that embeds it. Every level is classified here,
//! against its own type, before the read — so a `Reject` at any depth never reaches the
//! database — and masked afterwards at the keys the response carries it under.
//!
//! # By field, not by response key
//!
//! A selection's alias is the client's choice of output key, not another field:
//! `{ orders { m: margin } }` reads `margin`. The planner lists the root's projection by
//! response key, which is what the projector needs, and the classifier used to be handed
//! that list — so an aliased field matched no declared field and passed through
//! unclassified. Classification here takes the field name; masking takes the key.

use std::collections::HashSet;

use crate::{
    error::Result,
    graphql::FieldSelection,
    runtime::{field_filter::FieldAccessResult, projection::effective_selections},
    schema::{CompiledSchema, FieldType},
    security::SecurityContext,
};

/// What field-level RBAC decided for a whole selection tree.
pub(super) struct SelectionAccess {
    /// The root level, in the shape the projector takes: the projection's response keys,
    /// and the masked ones among them.
    pub(super) root: FieldAccessResult,
    /// Every `(type, field)` the caller may not read and that masks, at any level.
    masked:          HashSet<(String, String)>,
}

impl SelectionAccess {
    /// Classify every level of `root_fields` against the type it selects from.
    ///
    /// `projection_keys` is the planner's projection — the root's response keys, in
    /// order — returned unchanged as [`Self::root`]'s `projected`.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` for a selected field, at any depth, that requires a
    /// scope the caller lacks and whose `on_deny` is `Reject`.
    pub(super) fn classify(
        schema: &CompiledSchema,
        root_type: &str,
        root_fields: &[FieldSelection],
        projection_keys: Vec<String>,
        security_context: Option<&SecurityContext>,
    ) -> Result<Self> {
        let mut masked = HashSet::new();
        classify_level(schema, root_type, root_fields, security_context, &mut masked)?;

        let masked_keys = effective_selections(root_fields, root_type, schema)
            .into_iter()
            .filter(|sel| masked.contains(&(root_type.to_string(), sel.name.clone())))
            .map(|sel| sel.response_key().to_string())
            .collect();
        Ok(Self {
            root: FieldAccessResult {
                projected: projection_keys,
                masked:    masked_keys,
            },
            masked,
        })
    }

    /// Null every masked field of a projected result, at every level, under the key the
    /// response carries it.
    pub(super) fn null_masked(
        &self,
        value: &mut serde_json::Value,
        root_type: &str,
        root_fields: &[FieldSelection],
        schema: &CompiledSchema,
    ) {
        if !self.masked.is_empty() {
            null_masked_at(value, root_type, root_fields, &self.masked, schema);
        }
    }
}

/// The object type a field holds — its element type for a list.
pub(super) fn object_type_of(field_type: &FieldType) -> Option<&str> {
    if field_type.is_scalar() {
        return None;
    }
    field_type.inner_type().unwrap_or(field_type).type_name()
}

/// Classify one level's selections against `type_name`, then every level beneath it.
fn classify_level(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
    security_context: Option<&SecurityContext>,
    masked: &mut HashSet<(String, String)>,
) -> Result<()> {
    let level: Vec<&FieldSelection> = effective_selections(selections, type_name, schema)
        .into_iter()
        .filter(|sel| sel.name != "__typename")
        .collect();
    let names = level.iter().map(|sel| sel.name.clone()).collect();
    let access = super::super::support::security::classify_fields_for_read(
        schema,
        type_name,
        names,
        security_context,
    )?;
    masked.extend(access.masked.into_iter().map(|field| (type_name.to_string(), field)));

    let Some(type_def) = schema.find_type(type_name) else {
        return Ok(());
    };
    for sel in level {
        let child = type_def
            .fields
            .iter()
            .find(|f| f.name == sel.name)
            .and_then(|f| object_type_of(&f.field_type));
        if let Some(child) = child {
            classify_level(schema, child, &sel.nested_fields, security_context, masked)?;
        }
    }
    Ok(())
}

fn null_masked_at(
    value: &mut serde_json::Value,
    type_name: &str,
    selections: &[FieldSelection],
    masked: &HashSet<(String, String)>,
    schema: &CompiledSchema,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                null_masked_at(item, type_name, selections, masked, schema);
            }
        },
        serde_json::Value::Object(object) => {
            let type_def = schema.find_type(type_name);
            for sel in effective_selections(selections, type_name, schema) {
                let key = sel.response_key();
                if masked.contains(&(type_name.to_string(), sel.name.clone())) {
                    if let Some(slot) = object.get_mut(key) {
                        *slot = serde_json::Value::Null;
                    }
                    continue;
                }
                let child = type_def
                    .and_then(|t| t.fields.iter().find(|f| f.name == sel.name))
                    .and_then(|f| object_type_of(&f.field_type));
                if let (Some(child), Some(nested)) = (child, object.get_mut(key)) {
                    null_masked_at(nested, child, &sel.nested_fields, masked, schema);
                }
            }
        },
        _ => {},
    }
}
