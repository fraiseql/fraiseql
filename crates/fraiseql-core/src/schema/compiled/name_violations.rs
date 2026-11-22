//! Every declared name a compiled schema publishes is a GraphQL Name.
//!
//! A client types these names, introspection and the SDL publish them, and several reach
//! SQL: a field's key is a JSONB key, a type's name a `__typename` literal. Checked here,
//! where the compiler loads what it writes and a server loads any artifact, no name that
//! is not a Name gets past either one. The escaping at each SQL site stays.

use super::schema::CompiledSchema;
use crate::schema::{ArgumentDefinition, FieldDefinition, is_graphql_name};

/// Why `name` cannot be declared, phrased to follow the name, or `None` when it can.
fn fault(name: &str) -> Option<&'static str> {
    if !is_graphql_name(name) {
        Some("is not a GraphQL name")
    } else if name.starts_with("__") {
        Some("starts with `__`, which introspection reserves")
    } else {
        None
    }
}

/// The names found at fault, one line each.
#[derive(Default)]
struct Faults(Vec<String>);

impl Faults {
    fn name(&mut self, kind: &str, path: &str, name: &str) {
        if let Some(reason) = fault(name) {
            self.0.push(format!("{kind} `{path}` {reason}"));
        }
    }

    /// A declaration published under another name than the one it is stored as.
    fn published(&mut self, kind: &str, path: &str, published: &str) {
        if let Some(reason) = fault(published) {
            self.0
                .push(format!("{kind} `{path}` is published as `{published}`, which {reason}"));
        }
    }

    fn fields(&mut self, owner: &str, fields: &[FieldDefinition]) {
        for field in fields {
            let path = format!("{owner}.{}", field.name);
            self.name("field", &path, field.name.as_str());
            if let Some(alias) = &field.alias {
                self.published("field", &path, alias);
            }
        }
    }

    fn arguments(&mut self, owner: &str, arguments: &[ArgumentDefinition]) {
        for argument in arguments {
            self.name("argument", &format!("{owner}({}:)", argument.name), &argument.name);
        }
    }

    /// A root operation, published under the schema's naming convention.
    fn operation(
        &mut self,
        schema: &CompiledSchema,
        kind: &str,
        name: &str,
        arguments: &[ArgumentDefinition],
    ) {
        self.name(kind, name, name);
        let published = schema.display_name(name);
        if published != name {
            self.published(kind, name, &published);
        }
        self.arguments(name, arguments);
    }
}

impl CompiledSchema {
    /// Every declared name that is not a GraphQL Name (§ 2.1.9), starts with `__` (§ 3.1),
    /// or, for an enum value, is `true`, `false` or `null` (§ 3.9).
    pub(super) fn name_violations(&self) -> Vec<String> {
        let mut faults = Faults::default();
        for type_def in &self.types {
            faults.name("type", type_def.name.as_str(), type_def.name.as_str());
            faults.fields(type_def.name.as_str(), &type_def.fields);
        }
        for interface in &self.interfaces {
            faults.name("interface", &interface.name, &interface.name);
            faults.fields(&interface.name, &interface.fields);
        }
        for union in &self.unions {
            faults.name("union", &union.name, &union.name);
        }
        for enum_def in &self.enums {
            faults.name("enum", &enum_def.name, &enum_def.name);
            for value in &enum_def.values {
                let path = format!("{}.{}", enum_def.name, value.name);
                if matches!(value.name.as_str(), "true" | "false" | "null") {
                    faults.0.push(format!(
                        "enum value `{path}` is `true`, `false` or `null`, which a document \
                         reads as a literal"
                    ));
                } else {
                    faults.name("enum value", &path, &value.name);
                }
            }
        }
        for input in &self.input_types {
            faults.name("input type", &input.name, &input.name);
            for field in &input.fields {
                faults.name("input field", &format!("{}.{}", input.name, field.name), &field.name);
            }
        }
        for query in &self.queries {
            faults.operation(self, "query", &query.name, &query.arguments);
        }
        for mutation in &self.mutations {
            faults.operation(self, "mutation", &mutation.name, &mutation.arguments);
        }
        for subscription in &self.subscriptions {
            faults.operation(self, "subscription", &subscription.name, &subscription.arguments);
        }
        for directive in &self.directives {
            let path = format!("@{}", directive.name);
            faults.name("directive", &path, &directive.name);
            faults.arguments(&path, &directive.arguments);
        }
        faults.0
    }
}
