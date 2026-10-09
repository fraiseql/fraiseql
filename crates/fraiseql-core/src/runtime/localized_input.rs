//! A localized field's write input, coerced to the locale map its SQL function receives (#1513).
//!
//! A mutation argument (or input-object field) declared localized accepts four shapes, from
//! every transport:
//!
//! | written | the function receives |
//! |---|---|
//! | `"Pomme"` (REST, MCP) or `{value: "Pomme"}` (GraphQL `LocalizedInput`) | `{"<request locale>": "Pomme"}` |
//! | `{translations: [{locale: "fr-FR", value: "Pomme"}, …]}` (GraphQL) | `{"fr-FR": "Pomme", …}` |
//! | `{"fr-FR": "Pomme", …}` (REST, MCP) | the map |
//! | `null` | `null` |
//!
//! Every locale must be one of `allowed` (matched case-insensitively, written in its
//! configured spelling), each at most once, and every label a string or `null`; a `null`
//! label is passed through, so the function's merge can remove that key. Anything else is a
//! validation error, raised before any statement. Merging into the stored map is the
//! function's job.

use serde_json::{Map, Value};

use crate::{
    error::{FraiseQLError, Result},
    schema::{ArgumentDefinition, CompiledSchema, FieldType, LocaleConfig},
};

/// The mutation's arguments with every localized value coerced to its locale map, or `None`
/// when the mutation takes no localized value (the arguments are then used as given).
///
/// `arguments` is the request's argument object. Argument and input-field names are matched
/// as declared and as published (`display_name`), as the other argument checks do.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] for a value that is none of the accepted shapes, a locale
/// outside `allowed`, a locale given twice, or a label that is not a string or `null`.
pub fn coerce_localized_arguments(
    schema: &CompiledSchema,
    label: &str,
    declared: &[ArgumentDefinition],
    arguments: Option<&Value>,
) -> Result<Option<Value>> {
    let Some(config) = schema.locale.as_ref() else {
        return Ok(None);
    };
    let Some(Value::Object(given)) = arguments else {
        return Ok(None);
    };
    let request_locale =
        crate::runtime::request_locale(schema).unwrap_or_else(|| config.default.clone());
    let coercer = Coercer {
        schema,
        config,
        request_locale: &request_locale,
    };
    let mut coerced = given.clone();
    let mut changed = false;
    for argument in declared {
        for key in [argument.name.clone(), schema.display_name(&argument.name)] {
            let Some(value) = coerced.get_mut(&key) else {
                continue;
            };
            let at = format!("{label}({})", argument.name);
            if argument.localized {
                *value = coercer.localize(&at, value)?;
                changed = true;
            } else if let Some(input) = input_type_of(&argument.arg_type) {
                changed |= coercer.walk(&at, input, value)?;
            }
            break;
        }
    }
    Ok(changed.then_some(Value::Object(coerced)))
}

/// The input object a value of `field_type` is (through list wrappers), by name.
fn input_type_of(field_type: &FieldType) -> Option<&str> {
    match field_type {
        FieldType::List(inner) => input_type_of(inner),
        FieldType::Input(name) | FieldType::Object(name) => Some(name),
        _ => None,
    }
}

struct Coercer<'a> {
    schema:         &'a CompiledSchema,
    config:         &'a LocaleConfig,
    request_locale: &'a str,
}

impl Coercer<'_> {
    /// Coerce the localized fields of `value`, an object of input type `type_name` (or a list
    /// of them). Whether anything was localized.
    fn walk(&self, at: &str, type_name: &str, value: &mut Value) -> Result<bool> {
        let Some(input) = self.schema.find_input_type(type_name) else {
            return Ok(false);
        };
        match value {
            Value::Array(items) => {
                let mut changed = false;
                for item in items {
                    changed |= self.walk(at, type_name, item)?;
                }
                Ok(changed)
            },
            Value::Object(object) => {
                let mut changed = false;
                for field in &input.fields {
                    for key in [field.name.clone(), self.schema.display_name(&field.name)] {
                        let Some(inner) = object.get_mut(&key) else {
                            continue;
                        };
                        let at = format!("{at}.{}", field.name);
                        if field.localized {
                            *inner = self.localize(&at, inner)?;
                            changed = true;
                        } else {
                            let nested = field.field_type.trim_matches(['[', ']', '!']);
                            changed |= self.walk(&at, nested, inner)?;
                        }
                        break;
                    }
                }
                Ok(changed)
            },
            _ => Ok(false),
        }
    }

    /// One localized value, as its locale map.
    fn localize(&self, at: &str, value: &Value) -> Result<Value> {
        let map = match value {
            Value::Null => return Ok(Value::Null),
            Value::String(label) => {
                Map::from_iter([(self.request_locale.to_string(), Value::String(label.clone()))])
            },
            Value::Object(object) if object.len() == 1 && object.contains_key("value") => {
                let label = Self::label(at, self.request_locale, &object["value"])?;
                Map::from_iter([(self.request_locale.to_string(), label)])
            },
            Value::Object(object) if object.len() == 1 && object.contains_key("translations") => {
                self.translations(at, &object["translations"])?
            },
            Value::Object(object)
                if !object.contains_key("value") && !object.contains_key("translations") =>
            {
                let mut map = Map::new();
                for (locale, label) in object {
                    let tag = self.allowed(at, locale)?;
                    map.insert(tag.clone(), Self::label(at, &tag, label)?);
                }
                map
            },
            other => {
                return Err(refusal(format!(
                    "{at} is localized: write a string, {{value: String}}, {{translations: \
                     [{{locale, value}}]}} or a map of locale to label, not {other}"
                )));
            },
        };
        Ok(Value::Object(map))
    }

    /// `{translations: [{locale, value}, …]}`, as a map.
    fn translations(&self, at: &str, list: &Value) -> Result<Map<String, Value>> {
        let Value::Array(entries) = list else {
            return Err(refusal(format!("{at}: `translations` must be a list, not {list}")));
        };
        let mut map = Map::new();
        for entry in entries {
            let (Some(Value::String(locale)), Some(label), 2) =
                (entry.get("locale"), entry.get("value"), entry.as_object().map_or(0, Map::len))
            else {
                return Err(refusal(format!(
                    "{at}: each translation is {{locale: String!, value: String}}, not {entry}"
                )));
            };
            let tag = self.allowed(at, locale)?;
            if map.contains_key(&tag) {
                return Err(refusal(format!("{at}: locale `{tag}` is given more than once")));
            }
            map.insert(tag.clone(), Self::label(at, &tag, label)?);
        }
        Ok(map)
    }

    /// `locale`, as the allowed tag it names (in its configured spelling).
    fn allowed(&self, at: &str, locale: &str) -> Result<String> {
        self.config
            .allowed
            .iter()
            .find(|tag| tag.eq_ignore_ascii_case(locale))
            .cloned()
            .ok_or_else(|| {
                refusal(format!(
                    "{at}: locale `{locale}` is not allowed (allowed: {})",
                    self.config.allowed.join(", ")
                ))
            })
    }

    /// A label: a string, or `null` to remove the locale's label.
    fn label(at: &str, locale: &str, label: &Value) -> Result<Value> {
        match label {
            Value::String(_) | Value::Null => Ok(label.clone()),
            other => Err(refusal(format!(
                "{at}: the `{locale}` label must be a string or null, not {other}"
            ))),
        }
    }
}

const fn refusal(message: String) -> FraiseQLError {
    FraiseQLError::Validation {
        message,
        path: None,
    }
}
