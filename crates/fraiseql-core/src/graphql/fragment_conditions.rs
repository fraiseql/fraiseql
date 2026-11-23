//! Every fragment's type condition names a type the schema has, which can apply where the
//! fragment stands.
//!
//! § 5.5.1.2 (Fragment Spread Type Existence) refuses a fragment on a type the schema does
//! not define, and § 5.5.2.3 (Fragment Spread Is Possible) one whose type shares no object
//! type with the position it stands at: no value there could ever be of its type. Without
//! these, such a fragment was dropped where its type did not match, and its fields went
//! unchecked against any type.
//!
//! Checked on the document as written, before its named spreads are expanded: expansion
//! keeps a spread's fields and loses its type condition. Only at a position whose type the
//! schema knows, as an object, an interface or a union: elsewhere there is nothing to hold
//! a fragment against, which is the absence of evidence and not a defect.

use std::collections::HashSet;

use super::{FieldSelection, FragmentDefinition};
use crate::{
    error::{FraiseQLError, Result},
    schema::{CompiledSchema, FieldType},
};

/// Refuse a fragment whose type condition the schema does not know or that cannot apply.
///
/// `roots` are the root fields of an operation of `kind` (`query`, `mutation`,
/// `subscription`); the first fragment among them whose type condition names no type of
/// the schema, or cannot apply where it stands, is refused.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] at the fragment's path, naming it and its type.
pub fn check(
    roots: &[FieldSelection],
    fragments: &[FragmentDefinition],
    kind: &str,
    schema: &CompiledSchema,
) -> Result<()> {
    let walk = Walk { schema, fragments };
    for root in roots {
        let path = root.response_key().to_string();
        let mut spreading = HashSet::new();
        match kind {
            "mutation" => {
                if let Some(mutation) = schema.find_mutation(&root.name) {
                    walk.set_at(
                        &root.nested_fields,
                        &mutation.return_type,
                        &path,
                        &mut spreading,
                        false,
                    )?;
                }
            },
            "subscription" => {
                if let Some(subscription) = schema.find_subscription(&root.name) {
                    walk.set(
                        &root.nested_fields,
                        &subscription.return_type,
                        &path,
                        &mut spreading,
                    )?;
                }
            },
            _ if root.name == "node" && schema.publishes_node() => {
                walk.set(&root.nested_fields, "Node", &path, &mut spreading)?;
            },
            _ => {
                let Some(query) = schema.find_query(&root.name) else {
                    continue;
                };
                if query.returns_count {
                    continue;
                }
                if query.relay {
                    walk.connection(root, &query.return_type, &path, &mut spreading)?;
                } else {
                    walk.set(&root.nested_fields, &query.return_type, &path, &mut spreading)?;
                }
            },
        }
    }
    Ok(())
}

struct Walk<'a> {
    schema:    &'a CompiledSchema,
    fragments: &'a [FragmentDefinition],
}

impl<'a> Walk<'a> {
    /// A Relay connection's nodes, under `edges { node }` and `nodes`, hold `node_type`.
    fn connection(
        &self,
        root: &'a FieldSelection,
        node_type: &str,
        path: &str,
        spreading: &mut HashSet<&'a str>,
    ) -> Result<()> {
        for field in &root.nested_fields {
            let below = format!("{path}.{}", field.response_key());
            match field.name.as_str() {
                "nodes" => self.set(&field.nested_fields, node_type, &below, spreading)?,
                "edges" => {
                    for edge in field.nested_fields.iter().filter(|e| e.name == "node") {
                        let node = format!("{below}.{}", edge.response_key());
                        self.set(&edge.nested_fields, node_type, &node, spreading)?;
                    }
                },
                _ => {},
            }
        }
        Ok(())
    }

    /// The selection set `selections`, at `path`, a position holding `parent`.
    fn set(
        &self,
        selections: &'a [FieldSelection],
        parent: &str,
        path: &str,
        spreading: &mut HashSet<&'a str>,
    ) -> Result<()> {
        self.set_at(selections, parent, path, spreading, true)
    }

    /// [`set`](Self::set), where `possible` says whether `parent` names every type a value
    /// at this position can be. A mutation's result is its declared type or one of the
    /// error types its function returns, so at its own level a fragment is held to the
    /// types the schema has, not to the declared one.
    fn set_at(
        &self,
        selections: &'a [FieldSelection],
        parent: &str,
        path: &str,
        spreading: &mut HashSet<&'a str>,
        possible: bool,
    ) -> Result<()> {
        let Some(parent_types) = self.possible_types(parent) else {
            return Ok(());
        };
        for selection in selections {
            if let Some(condition) = selection.name.strip_prefix("...on ") {
                if condition.is_empty() {
                    self.set_at(&selection.nested_fields, parent, path, spreading, possible)?;
                    continue;
                }
                let what = format!("the inline fragment on `{condition}`");
                self.applies(condition, parent, &parent_types, &what, path, possible)?;
                self.set(&selection.nested_fields, condition, path, spreading)?;
            } else if let Some(name) = selection.name.strip_prefix("...") {
                // A spread of a fragment the document does not define is refused where
                // spreads are expanded; a cycle, there too.
                let Some(fragment) = self.fragments.iter().find(|f| f.name == name) else {
                    continue;
                };
                let condition = fragment.type_condition.as_str();
                let what = format!("fragment `{name}`, on `{condition}`,");
                self.applies(condition, parent, &parent_types, &what, path, possible)?;
                if spreading.insert(fragment.name.as_str()) {
                    self.set(&fragment.selections, condition, path, spreading)?;
                    spreading.remove(fragment.name.as_str());
                }
            } else if let Some(child) = self.field_type(parent, &selection.name) {
                let below = format!("{path}.{}", selection.response_key());
                self.set(&selection.nested_fields, child, &below, spreading)?;
            }
        }
        Ok(())
    }

    /// Refuse a fragment, `what`, on `condition` at `path`, a position holding `parent`
    /// (whose object types are `parent_types`), unless it names a type that can apply there.
    fn applies(
        &self,
        condition: &str,
        parent: &str,
        parent_types: &[&str],
        what: &str,
        path: &str,
        possible: bool,
    ) -> Result<()> {
        let refusal = |message: String| FraiseQLError::Validation {
            message,
            path: Some(path.to_string()),
        };
        let Some(condition_types) = self.possible_types(condition) else {
            return Err(refusal(format!(
                "Unknown type `{condition}`: {what} names no object, interface or union \
                 type of the schema"
            )));
        };
        // A type no object implements or belongs to is no evidence either way.
        if !possible
            || condition_types.is_empty()
            || parent_types.is_empty()
            || condition_types.iter().any(|t| parent_types.contains(t))
        {
            return Ok(());
        }
        Err(refusal(format!(
            "{what} can never apply here: `{condition}` and `{parent}` share no object type"
        )))
    }

    /// The object types a value of `name` can be, or `None` when `name` is no object,
    /// interface or union of the schema.
    fn possible_types(&self, name: &str) -> Option<Vec<&'a str>> {
        let schema = self.schema;
        if schema.find_type(name).is_some() {
            return schema.find_type(name).map(|t| vec![t.name.as_str()]);
        }
        if schema.find_interface(name).is_some() {
            return Some(
                schema
                    .types
                    .iter()
                    .filter(|t| t.implements.iter().any(|i| i == name))
                    .map(|t| t.name.as_str())
                    .collect(),
            );
        }
        schema
            .find_union(name)
            .map(|u| u.member_types.iter().map(String::as_str).collect())
    }

    /// The composite type the field `field` of `parent` holds, itself or as a list's
    /// elements; `None` for a leaf, or a field `parent` does not declare.
    fn field_type(&self, parent: &str, field: &str) -> Option<&'a str> {
        let declared =
            self.schema.find_type(parent).and_then(|t| t.find_field(field)).or_else(|| {
                self.schema
                    .find_interface(parent)
                    .and_then(|i| i.fields.iter().find(|f| f.name == field))
            })?;
        let held = declared.field_type.inner_type().unwrap_or(&declared.field_type);
        match held {
            FieldType::Object(name) | FieldType::Interface(name) | FieldType::Union(name) => {
                Some(name.as_str())
            },
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "fragment_conditions_tests.rs"]
mod tests;
