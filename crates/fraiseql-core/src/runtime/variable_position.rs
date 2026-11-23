//! § 5.8.5 (All Variable Usages Are Allowed), its nullability half: a variable is used only
//! where a value of its declared type fits. `$id: ID` at an `ID!` argument, or `$ids: [ID]`
//! at a `[ID!]` one, is refused when the document is validated, whatever values the request
//! supplies: the document itself says a null may arrive where none is accepted.
//!
//! A position's type is the one introspection publishes for it, so a client that declares
//! each variable at the type introspection gives is accepted. Following the specification's
//! `IsVariableUsageAllowed`, a nullable variable fits a non-null position when the variable
//! declares a non-null default or the position declares a default of its own; the exemption
//! is the position's, so it does not reach a list's items.
//!
//! Every usage in an argument is judged: the argument itself, a field of an input object
//! written inline (`where: {status: {in: [PENDING, $s]}}`) and an item of a list.
//!
//! The named-type half (`$n: String` at an `Int` argument) is
//! [`validate_argument_values`](super::validate_argument_values)'s. A list variable at a
//! position that is not a list, or the reverse, is a shape question this module leaves
//! alone, as that one does.

use serde_json::Value;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{
        types::{GraphQLArgument, VariableDefinition},
        value_json,
    },
    schema::{
        ArgumentDefinition, CompiledSchema, FieldType, IntrospectionType, TypeKind, arg_type_ref,
        input_field_type_ref,
    },
};

/// How deep an inline value is followed; the document parser bounds nesting well below it.
const MAX_DEPTH: usize = 64;

/// Refuse a variable used in `provided`, the arguments a root field writes, at a position
/// whose type does not accept the variable's declared nullability.
///
/// `field_label` names the field as a client reads the schema (`Query.orders`), and
/// `declared` is the field's argument list as introspection publishes it.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] naming the variable, its declared type, where it is used and
/// the type published there.
pub fn validate_variable_positions(
    schema: &CompiledSchema,
    field_label: &str,
    declared: &[ArgumentDefinition],
    provided: &[GraphQLArgument],
    variable_defs: &[VariableDefinition],
) -> Result<()> {
    if variable_defs.is_empty() {
        return Ok(());
    }
    for arg in provided {
        let Some(definition) = declared.iter().find(|d| d.name == arg.name) else {
            continue;
        };
        let value = value_json::decode(&arg.value_json)?;
        if !mentions_variable(&value) {
            continue;
        }
        let usage = Usage {
            schema,
            field_label,
            argument: &arg.name,
            variable_defs,
        };
        let position = Position {
            published: &arg_type_ref(definition),
            defaulted: definition.default_value.is_some(),
            path:      &arg.name,
        };
        usage.walk(&value, &position, 0)?;
    }
    Ok(())
}

/// The type an argument publishes, in the notation a document declares a variable in: the
/// type a variable bound to it should be declared at.
#[must_use]
pub fn published_argument_type(argument: &ArgumentDefinition) -> String {
    render(&arg_type_ref(argument))
}

/// The `name: String!` argument of the `__type` meta-field (§ 4.1), which no schema declares.
#[must_use]
pub fn type_name_argument() -> ArgumentDefinition {
    ArgumentDefinition::new("name", FieldType::String)
}

fn mentions_variable(value: &Value) -> bool {
    value_json::variable_name(value).is_some()
        || match value {
            Value::Array(items) => items.iter().any(mentions_variable),
            Value::Object(fields) => fields.values().any(mentions_variable),
            _ => false,
        }
}

/// One argument's value, and what it is judged against.
struct Usage<'a> {
    schema:        &'a CompiledSchema,
    field_label:   &'a str,
    argument:      &'a str,
    variable_defs: &'a [VariableDefinition],
}

/// A place a value is written: the type introspection publishes for it, whether it declares
/// a default, and where it is, for the message.
struct Position<'a> {
    published: &'a IntrospectionType,
    defaulted: bool,
    path:      &'a str,
}

impl Usage<'_> {
    fn walk(&self, value: &Value, position: &Position<'_>, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        if let Some(name) = value_json::variable_name(value) {
            return self.check(name, position);
        }
        let nullable = without_non_null(position.published);
        match nullable.kind {
            TypeKind::List => {
                let Some(item) = nullable.of_type.as_deref() else {
                    return Ok(());
                };
                let items = value.as_array().map_or_else(|| vec![value], |a| a.iter().collect());
                for (index, item_value) in items.into_iter().enumerate() {
                    let path = if value.is_array() {
                        format!("{}[{index}]", position.path)
                    } else {
                        position.path.to_string()
                    };
                    let at = Position {
                        published: item,
                        defaulted: false,
                        path:      &path,
                    };
                    self.walk(item_value, &at, depth + 1)?;
                }
                Ok(())
            },
            TypeKind::InputObject => self.walk_input_object(value, nullable, position, depth),
            _ => Ok(()),
        }
    }

    fn walk_input_object(
        &self,
        value: &Value,
        published: &IntrospectionType,
        position: &Position<'_>,
        depth: usize,
    ) -> Result<()> {
        let (Some(name), Some(object)) = (published.name.as_deref(), value.as_object()) else {
            return Ok(());
        };
        let Some(input) = self.schema.find_input_type(name) else {
            return Ok(());
        };
        for field in &input.fields {
            // The surface name is what a client writes; the stored one is accepted too.
            let surface = self.schema.display_name(&field.name);
            let Some((key, field_value)) = object
                .get_key_value(surface.as_str())
                .or_else(|| object.get_key_value(field.name.as_str()))
            else {
                continue;
            };
            let path = format!("{}.{key}", position.path);
            let at = Position {
                published: &input_field_type_ref(field, self.schema),
                defaulted: field.default_value.is_some(),
                path:      &path,
            };
            self.walk(field_value, &at, depth + 1)?;
        }
        Ok(())
    }

    fn check(&self, name: &str, position: &Position<'_>) -> Result<()> {
        // An undeclared reference is § 5.8.3's error, raised before this runs.
        let Some(definition) = self.variable_defs.iter().find(|v| v.name == name) else {
            return Ok(());
        };
        let declared = declared_type(definition);
        let default_non_null =
            definition.default_value.as_deref().is_some_and(|d| d.trim() != "null");
        if fits(&declared, position.published, default_non_null || position.defaulted) {
            return Ok(());
        }
        let place = if position.path == self.argument {
            format!("for argument `{}`", self.argument)
        } else {
            format!("at `{}`", position.path)
        };
        Err(FraiseQLError::Validation {
            message: format!(
                "Variable `${name}` is declared as `{declared}` but is used {place} on {}, \
                 which has type `{}`",
                self.field_label,
                render(position.published),
            ),
            path:    Some(position.path.to_string()),
        })
    }
}

/// The variable's type as the document wrote it.
fn declared_type(definition: &VariableDefinition) -> String {
    let bang = if definition.var_type.nullable {
        ""
    } else {
        "!"
    };
    format!("{}{bang}", definition.var_type.name)
}

/// `AreTypesCompatible`, for nullability: whether a value of `declared` (a type as written)
/// may be used where `published` is expected. `exempt` lifts the outermost non-null
/// requirement, for a default that stands in for a null.
fn fits(declared: &str, published: &IntrospectionType, exempt: bool) -> bool {
    let (declared_non_null, declared) =
        declared.strip_suffix('!').map_or((false, declared), |inner| (true, inner));
    let published_non_null = matches!(published.kind, TypeKind::NonNull);
    if published_non_null && !declared_non_null && !exempt {
        return false;
    }
    let published = without_non_null(published);
    match (list_item(declared), &published.kind, published.of_type.as_deref()) {
        (Some(declared_item), TypeKind::List, Some(published_item)) => {
            fits(declared_item, published_item, false)
        },
        _ => true,
    }
}

fn list_item(declared: &str) -> Option<&str> {
    declared.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
}

fn without_non_null(published: &IntrospectionType) -> &IntrospectionType {
    match (&published.kind, published.of_type.as_deref()) {
        (TypeKind::NonNull, Some(inner)) => inner,
        _ => published,
    }
}

/// A published type reference in the notation a document writes it in.
fn render(published: &IntrospectionType) -> String {
    match (&published.kind, published.of_type.as_deref()) {
        (TypeKind::NonNull, Some(inner)) => format!("{}!", render(inner)),
        (TypeKind::List, Some(inner)) => format!("[{}]", render(inner)),
        _ => published.name.clone().unwrap_or_default(),
    }
}

#[cfg(test)]
#[path = "variable_position_tests.rs"]
mod tests;
