//! Input field defaults an update would write.
//!
//! An update leaves a field the request omits unchanged: its function reads the keys it was
//! sent. A field that declares a default is never omitted (GraphQL fills the default in), so on
//! an update the default overwrites the stored value on every request that leaves the field
//! out. [`update_input_defaults`] finds every such field, on any input object an `Update`
//! mutation receives, nested ones included, so the compiler can refuse it.

use std::collections::HashSet;

use fraiseql_core::schema::{CompiledSchema, FieldType, MutationOperation};

/// One defaulted input field an `Update` mutation receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInputDefault {
    /// The `Update` mutation.
    pub mutation:   String,
    /// The input type declaring the field.
    pub input_type: String,
    /// The field.
    pub field:      String,
    /// Its default, as JSON text.
    pub default:    String,
}

/// Every defaulted field of an input object an `Update` mutation of `schema` receives, at any
/// depth, in declaration order.
#[must_use]
pub fn update_input_defaults(schema: &CompiledSchema) -> Vec<UpdateInputDefault> {
    let mut found = Vec::new();
    for mutation in &schema.mutations {
        if !matches!(mutation.operation, MutationOperation::Update { .. }) {
            continue;
        }
        let mut pending: Vec<String> =
            mutation.arguments.iter().filter_map(|a| input_type_of(&a.arg_type)).collect();
        let mut seen = HashSet::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            let Some(input_type) = schema.find_input_type(&name) else {
                continue;
            };
            for field in &input_type.fields {
                if let Some(default) = &field.default_value {
                    found.push(UpdateInputDefault {
                        mutation:   mutation.name.clone(),
                        input_type: name.clone(),
                        field:      field.name.clone(),
                        default:    default.clone(),
                    });
                }
                pending.push(named_type(&field.field_type).to_string());
            }
        }
    }
    found
}

/// The type an argument of compiled type `declared` names, through lists.
fn input_type_of(declared: &FieldType) -> Option<String> {
    match declared {
        FieldType::List { item, .. } => input_type_of(item),
        // An input-type reference compiles as `Object`.
        FieldType::Object(name) | FieldType::Input(name) => Some(name.clone()),
        _ => None,
    }
}

/// The type a written type names: `"[LineInput!]!"` names `LineInput`.
fn named_type(written: &str) -> &str {
    written.trim().trim_matches(|c| matches!(c, '[' | ']' | '!' | ' '))
}

#[cfg(test)]
#[path = "update_input_defaults_tests.rs"]
mod tests;
