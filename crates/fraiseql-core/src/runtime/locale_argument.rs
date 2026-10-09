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

use fraiseql_db::TranslationPart;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{FieldSelection, GraphQLArgument},
    schema::{CompiledSchema, FieldType, LOCALIZED_STRING_TYPE},
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
        let label = format!("{type_name}.{}", sel.name);
        if type_def.translations_of(&sel.name).is_some() {
            check_translations(&label, sel)?;
            continue;
        }
        let Some(field) = type_def.find_field(&sel.name) else {
            continue;
        };
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

/// A translations sibling (`nameTranslations { locale value }`): it takes no argument, and
/// its sub-selection names only `LocalizedString`'s fields. A gated base field gates it too,
/// under its own name, at every classifier (`schema::gated_field`, #1523).
fn check_translations(label: &str, sel: &FieldSelection) -> Result<()> {
    if let Some(argument) = sel.arguments.first() {
        return Err(refusal(format!("Unknown argument '{}' on {label}", argument.name)));
    }
    for sub in &sel.nested_fields {
        let name = sub.name.strip_prefix("...on ").map_or(sub.name.as_str(), str::trim);
        let known =
            matches!(name, "locale" | "value" | LOCALIZED_STRING_TYPE) || name.starts_with("__");
        if !known {
            return Err(refusal(format!(
                "Cannot query field '{name}' on type '{LOCALIZED_STRING_TYPE}'."
            )));
        }
        if name == LOCALIZED_STRING_TYPE {
            check_translations(label, sub)?;
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
        .and_then(|a| crate::graphql::value_json::decode(&a.value_json).ok())
        .and_then(|value| value.as_str().map(str::to_string))
        .and_then(|tag| schema.locale.as_ref()?.chain(&tag).map(<[String]>::to_vec));
    argued.or_else(|| crate::runtime::localization_chain(schema))
}

/// How a translations sibling selection reads its base field's map (#1513): every allowed
/// locale, with the keys its sub-selection names. `None` when the schema declares no locale.
#[must_use]
pub fn translations_read(
    schema: &CompiledSchema,
    selection: &FieldSelection,
) -> Option<fraiseql_db::LocalizedRead> {
    Some(fraiseql_db::LocalizedRead::Translations {
        allowed: schema.locale.as_ref()?.allowed.clone(),
        keys:    translation_keys(&selection.nested_fields),
    })
}

/// The keys of a translations element, by response key, as `selections` name them (an
/// inline fragment on `LocalizedString` flattened). Anything else was refused before.
fn translation_keys(selections: &[FieldSelection]) -> Vec<(String, TranslationPart)> {
    let mut keys = Vec::new();
    for sel in selections {
        if sel.name.strip_prefix("...on ").is_some() {
            keys.extend(translation_keys(&sel.nested_fields));
            continue;
        }
        let part = match sel.name.as_str() {
            "locale" => TranslationPart::Locale,
            "value" => TranslationPart::Value,
            "__typename" => TranslationPart::Typename,
            _ => continue,
        };
        keys.push((sel.response_key().to_string(), part));
    }
    keys
}

/// The translations sibling of a stored locale map, in process.
///
/// The twin of `fraiseql_db::projection_generator::localized_translations_expr`, for a
/// document projected in Rust. A value that is not a map lists nothing.
#[must_use]
pub fn translations(
    value: &serde_json::Value,
    read: &fraiseql_db::LocalizedRead,
) -> serde_json::Value {
    let fraiseql_db::LocalizedRead::Translations { allowed, keys } = read else {
        return serde_json::Value::Array(Vec::new());
    };
    let map = value.as_object();
    let elements = allowed
        .iter()
        .filter_map(|tag| {
            let label = map?.get(tag)?.as_str()?;
            let element = keys
                .iter()
                .map(|(key, part)| {
                    let v = match part {
                        TranslationPart::Locale => tag.as_str(),
                        TranslationPart::Value => label,
                        TranslationPart::Typename => LOCALIZED_STRING_TYPE,
                    };
                    (key.clone(), serde_json::Value::String(v.to_string()))
                })
                .collect::<serde_json::Map<_, _>>();
            Some(serde_json::Value::Object(element))
        })
        .collect();
    serde_json::Value::Array(elements)
}
