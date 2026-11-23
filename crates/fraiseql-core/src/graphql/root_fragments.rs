//! Fragments at the root of an operation, expanded into the fields they select.
//!
//! A fragment may stand wherever a selection set is, the operation's own included
//! (GraphQL § 5.5), and `CollectFields` expands it before anything executes (§ 6.3.2).
//! Every stage after the parse reads the operation's root fields by name: which root it
//! is, which runner reads it, which rules hold for it. A spread at the root is expanded
//! here, once, so that each stage meets the fields the document selects, through every
//! entry a document arrives by.
//!
//! A spread's `@skip` and `@include` travel onto each field it contributes, still
//! unevaluated: they are decided per request, from its variables, as a field's own are.

use std::collections::HashSet;

use super::{FieldSelection, FragmentDefinition, parser::GraphQLParseError};

/// `selections`, the root of a `kind` operation (`query`, `mutation`, `subscription`), with
/// every fragment at it expanded in place.
///
/// # Errors
///
/// [`GraphQLParseError::RootFragment`] for a fragment typed other than the operation's root
/// type, a spread of a fragment the document does not define, or a cycle of spreads.
pub(super) fn expand(
    selections: &[FieldSelection],
    fragments: &[FragmentDefinition],
    kind: &str,
) -> Result<Vec<FieldSelection>, GraphQLParseError> {
    if !selections.iter().any(is_fragment) {
        return Ok(selections.to_vec());
    }
    let root = Root {
        fragments,
        kind,
        type_name: match kind {
            "mutation" => "Mutation",
            "subscription" => "Subscription",
            _ => "Query",
        },
    };
    let mut expanded = Vec::with_capacity(selections.len());
    root.expand_into(selections, &mut HashSet::new(), &mut expanded)?;
    Ok(expanded)
}

fn is_fragment(selection: &FieldSelection) -> bool {
    selection.name.starts_with("...")
}

struct Root<'a> {
    fragments: &'a [FragmentDefinition],
    kind:      &'a str,
    type_name: &'static str,
}

impl Root<'_> {
    fn expand_into(
        &self,
        selections: &[FieldSelection],
        spreading: &mut HashSet<String>,
        expanded: &mut Vec<FieldSelection>,
    ) -> Result<(), GraphQLParseError> {
        for selection in selections {
            let Some(spread) = selection.name.strip_prefix("...") else {
                expanded.push(selection.clone());
                continue;
            };
            let start = expanded.len();
            if let Some(condition) = spread.strip_prefix("on ") {
                if !condition.is_empty() {
                    self.check_type(condition, || format!("an inline fragment on `{condition}`"))?;
                }
                self.expand_into(&selection.nested_fields, spreading, expanded)?;
            } else {
                let fragment =
                    self.fragments.iter().find(|f| f.name == spread).ok_or_else(|| {
                        self.refusal(&format!("fragment `{spread}` is spread and not defined"))
                    })?;
                self.check_type(&fragment.type_condition, || {
                    format!("fragment `{spread}` is on `{}`", fragment.type_condition)
                })?;
                if !spreading.insert(spread.to_string()) {
                    return Err(self.refusal(&format!("fragment `{spread}` spreads itself")));
                }
                self.expand_into(&fragment.selections, spreading, expanded)?;
                spreading.remove(spread);
            }
            for field in &mut expanded[start..] {
                field.inherit_directives(&selection.directives);
            }
        }
        Ok(())
    }

    fn check_type(
        &self,
        condition: &str,
        what: impl FnOnce() -> String,
    ) -> Result<(), GraphQLParseError> {
        if condition == self.type_name {
            return Ok(());
        }
        Err(self.refusal(&format!("{}, whose fields are not `{}`'s", what(), self.type_name)))
    }

    fn refusal(&self, why: &str) -> GraphQLParseError {
        GraphQLParseError::RootFragment(format!(
            "{why}: at the root of a {kind}, a fragment selects fields of `{root}`",
            kind = self.kind,
            root = self.type_name,
        ))
    }
}

#[cfg(test)]
#[path = "root_fragments_tests.rs"]
mod tests;
