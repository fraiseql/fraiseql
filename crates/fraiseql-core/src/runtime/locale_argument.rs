//! The `locale:` argument of a localized field (#1513).
//!
//! `name(locale: "de-DE")` reads one field selection in another allowed locale than the
//! request's. The argument is adjudicated once, when the document is matched and before any
//! statement: its value (a literal or a variable) must name a locale of `allowed`, which
//! [`LocaleConfig::match_tag`] decides as it does for the request locale, and it is rewritten
//! in place to that allowed tag as a string literal. Every projector then reads the chain of
//! a selection with [`selection_chain`], which trusts only what this pass wrote.
//!
//! The `_entities` resolver runs the same pass over the router's selection, at each
//! representation's type.
//!
//! `locale:` is defined on localized fields only, so on any other field it is refused rather
//! than ignored, as an undeclared root argument is (GraphQL § 5.4.1).
//!
//! [`LocaleConfig::match_tag`]: crate::schema::LocaleConfig::match_tag

use std::collections::HashMap;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{FieldSelection, GraphQLArgument},
    schema::{CompiledSchema, FieldType},
};

/// The argument's name.
pub const LOCALE_ARGUMENT: &str = "locale";

/// Validate every `locale:` argument in `selections`, scoped to `type_name`.
///
/// Each is rewritten to the allowed tag it stands for. A variable that is omitted or `null`
/// drops the argument: the selection reads the request locale.
///
/// `relay` scopes `selections` to the connection of `type_name` (`edges { node { … } }`).
///
/// # Errors
///
/// [`FraiseQLError::Validation`] when a value is not a string, names no allowed locale, or is
/// written on a field that is not localized.
pub fn resolve_locale_arguments(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &mut [FieldSelection],
    variables: &HashMap<String, serde_json::Value>,
    relay: bool,
) -> Result<()> {
    if !relay {
        return resolve_at(schema, type_name, selections, variables);
    }
    for edges in selections.iter_mut().filter(|s| s.name == "edges") {
        for node in edges.nested_fields.iter_mut().filter(|s| s.name == "node") {
            resolve_at(schema, type_name, &mut node.nested_fields, variables)?;
        }
    }
    Ok(())
}

fn resolve_at(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &mut [FieldSelection],
    variables: &HashMap<String, serde_json::Value>,
) -> Result<()> {
    let Some(type_def) = schema.find_type(type_name) else {
        return Ok(());
    };
    for sel in selections {
        // An inline fragment is read at its own type condition.
        if let Some(condition) = sel.name.strip_prefix("...on ") {
            resolve_at(schema, condition.trim(), &mut sel.nested_fields, variables)?;
            continue;
        }
        let Some(field) = type_def.find_field(&sel.name) else {
            continue;
        };
        let label = format!("{type_name}.{}", sel.name);
        if let Some(at) = sel.arguments.iter().position(|a| a.name == LOCALE_ARGUMENT) {
            if !field.localized {
                return Err(refusal(format!(
                    "Unknown argument '{LOCALE_ARGUMENT}' on {label}: it is defined on localized \
                     fields only"
                )));
            }
            match canonical_tag(schema, &label, &sel.arguments[at], variables)? {
                Some(tag) => {
                    sel.arguments[at] = GraphQLArgument {
                        name:       LOCALE_ARGUMENT.to_string(),
                        value_type: "string".to_string(),
                        value_json: serde_json::Value::String(tag).to_string(),
                    };
                },
                None => {
                    sel.arguments.remove(at);
                },
            }
        }
        let child = match &field.field_type {
            FieldType::List(inner) => inner.type_name(),
            other => other.type_name(),
        };
        if let Some(child) = child.filter(|_| !sel.nested_fields.is_empty()) {
            resolve_at(schema, child, &mut sel.nested_fields, variables)?;
        }
    }
    Ok(())
}

/// The allowed tag `argument` stands for; `None` for an omitted or `null` value.
fn canonical_tag(
    schema: &CompiledSchema,
    label: &str,
    argument: &GraphQLArgument,
    variables: &HashMap<String, serde_json::Value>,
) -> Result<Option<String>> {
    let value = crate::runtime::QueryMatcher::resolve_inline_arg(argument, variables)?;
    let requested = match value {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(serde_json::Value::String(requested)) => requested,
        Some(other) => {
            return Err(refusal(format!(
                "Argument '{LOCALE_ARGUMENT}' on {label} must be a String, got {other}"
            )));
        },
    };
    // A localized field is refused at load without `[locale]`, so this is always `Some`.
    let Some(config) = schema.locale.as_ref() else {
        return Ok(None);
    };
    config.match_tag(&requested).map(|tag| Some(tag.to_string())).ok_or_else(|| {
        refusal(format!(
            "Argument '{LOCALE_ARGUMENT}' on {label}: \"{requested}\" is not an allowed locale \
             (allowed: {})",
            config.allowed.join(", ")
        ))
    })
}

const fn refusal(message: String) -> FraiseQLError {
    FraiseQLError::Validation {
        message,
        path: None,
    }
}

/// The chain a localized field selection reads through: its `locale:` argument's when it has
/// one, otherwise the request locale's. `None` when the schema declares no locale.
///
/// The argument is read as [`resolve_locale_arguments`] left it, an allowed tag; anything
/// else names no chain and falls back to the request locale's.
#[must_use]
pub fn selection_chain(schema: &CompiledSchema, selection: &FieldSelection) -> Option<Vec<String>> {
    let argued = selection
        .arguments
        .iter()
        .find(|a| a.name == LOCALE_ARGUMENT)
        .and_then(|a| serde_json::from_str::<String>(&a.value_json).ok())
        .and_then(|tag| schema.locale.as_ref()?.chain(&tag).map(<[String]>::to_vec));
    argued.or_else(|| crate::runtime::localization_chain(schema))
}
