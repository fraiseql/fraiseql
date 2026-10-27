//! The `@tag`s a federated subgraph's service SDL carries for what roles gate.
//!
//! A root query or mutation with a `requires_role` is tagged with that role's tag. So is
//! a type gated by the role, and a type nothing reaches once that role's declarations are
//! gone: a contract excluding the role then leaves out what only they expose. A type two
//! roles reach is tagged with neither, since excluding one leaves it reached by the other.
//!
//! Only an object type of this subgraph's own is tagged. A tag on a supergraph type is a
//! tag on every subgraph's declaration of it, so tagging a type another subgraph declares
//! would take it out from under that subgraph's fields too. An entity or `@shareable` type
//! is declared elsewhere by design; an input or enum may be by name alone (two subgraphs'
//! `StringFilter`s merge). A contract that leaves out unreachable types leaves those out
//! once nothing it keeps reaches them.

use std::collections::{BTreeSet, HashSet};

use super::{
    IntrospectionResponses,
    caller_view::{reachable_types, schema_of},
};
use crate::{
    error::Result,
    federation::{ContractTags, role_tag},
    schema::CompiledSchema,
    security::role_gate::visible_schema_where,
};

/// The tags `schema`'s service SDL carries.
///
/// # Errors
///
/// A [`Validation`](crate::error::FraiseQLError::Validation) error when a gating role
/// cannot be named by a tag a contract accepts.
pub fn contract_tags(schema: &CompiledSchema) -> Result<ContractTags> {
    let mut tags = ContractTags::none();
    let roots = [
        (
            "Query",
            schema.queries.iter().map(|q| (&q.name, &q.requires_role)).collect::<Vec<_>>(),
        ),
        (
            "Mutation",
            schema.mutations.iter().map(|m| (&m.name, &m.requires_role)).collect(),
        ),
    ];
    for (root, fields) in roots {
        for (field, role) in fields {
            if let Some(role) = role {
                tags.fields
                    .entry(root.to_string())
                    .or_default()
                    .insert(field.clone(), role_tag(role)?);
            }
        }
    }

    let gated_types = schema.types.iter().filter_map(|t| t.requires_role.as_deref());
    let roles: BTreeSet<&str> = schema
        .queries
        .iter()
        .filter_map(|q| q.requires_role.as_deref())
        .chain(schema.mutations.iter().filter_map(|m| m.requires_role.as_deref()))
        .chain(gated_types)
        .collect();
    if roles.is_empty() {
        return Ok(tags);
    }

    // What this subgraph alone declares: the object types it may tag.
    let shared: HashSet<&str> = schema
        .federation
        .iter()
        .flat_map(|fed| {
            fed.entities
                .iter()
                .map(|e| e.name.as_str())
                .chain(fed.shareable_types.iter().map(String::as_str))
        })
        .collect();
    let own: HashSet<&str> = schema
        .types
        .iter()
        .map(|t| t.name.as_str())
        .filter(|name| !shared.contains(name))
        .collect();

    let reached =
        |s: &CompiledSchema| reachable_types(schema_of(&IntrospectionResponses::build(s)));
    let all = reached(schema);
    for role in roles {
        let tag = role_tag(role)?;
        let without = reached(&visible_schema_where(schema, |required| required != Some(role)));
        let only_through_role =
            all.iter().filter(|name| !without.contains(*name)).map(String::as_str);
        let gated_by_role = schema
            .types
            .iter()
            .filter(|t| t.requires_role.as_deref() == Some(role))
            .map(|t| t.name.as_str());
        for name in only_through_role.chain(gated_by_role).filter(|name| own.contains(name)) {
            tags.types.entry(name.to_string()).or_default().insert(tag.clone());
        }
    }
    Ok(tags)
}

#[cfg(test)]
#[path = "contract_tags_tests.rs"]
mod tests;
