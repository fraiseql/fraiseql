//! Operation-authorization op-list extraction.
//!
//! Maps a classified [`QueryType`] (plus the parsed AST for `Regular` queries) to the
//! list of root operations to hand the configured
//! [`Authorizer`](crate::security::Authorizer). Lives in the `runtime::executor`
//! module because [`QueryType`] is private to it; the trait, request/decision types,
//! and the [`enforce_authz`](crate::security::authorizer::enforce_authz) helper live
//! in [`crate::security::authorizer`].

use super::super::QueryType;
use crate::{
    graphql::ParsedQuery,
    schema::CompiledSchema,
    security::{AuthzOperation, OperationKind},
};

/// The type a root field of `kind` named `name` returns — `None` where the schema declares
/// no such field (introspection, `node`, `_entities`, an unknown name the matcher refuses).
pub(in crate::runtime::executor) fn root_target<'a>(
    schema: &'a CompiledSchema,
    kind: OperationKind,
    name: &str,
) -> Option<&'a str> {
    match kind {
        OperationKind::Query => schema.find_query(name).map(|q| q.return_type.as_str()),
        OperationKind::Mutation => schema.find_mutation(name).map(|m| m.return_type.as_str()),
        OperationKind::Subscription => {
            schema.find_subscription(name).map(|s| s.return_type.as_str())
        },
    }
}

/// A root operation, with the type it returns.
pub(in crate::runtime::executor) fn root_operation(
    schema: &CompiledSchema,
    kind: OperationKind,
    name: &str,
) -> AuthzOperation {
    AuthzOperation::root(kind, name, root_target(schema, kind, name))
}

/// Collect every root operation in a classified request, with the type it returns.
///
/// Uses the GraphQL **field name** (not the alias / response key), so the authorizer
/// keys on the real operation name. A multi-root `Regular` query yields one entry per
/// root selection.
///
/// Returns an **empty** vec for the `Mutation` variant: mutations are gated downstream
/// at `execute_mutation_impl`, the single point *every* mutation entry path converges
/// (including the anonymous-REST `execute_mutation` direct API that bypasses both
/// `*_internal` chokepoints). Gating `Mutation` here too would double-evaluate the
/// chokepoint paths and still miss the bypass, so it is centralized there.
pub(in crate::runtime::executor) fn collect_authz_ops(
    query_type: &QueryType,
    parsed_for_regular: Option<&ParsedQuery>,
    schema: &CompiledSchema,
) -> Vec<AuthzOperation> {
    let query = |name: &str| root_operation(schema, OperationKind::Query, name);
    match query_type {
        QueryType::Regular => parsed_for_regular.map_or_else(Vec::new, |parsed| {
            parsed.selections.iter().map(|sel| query(&sel.name)).collect()
        }),
        QueryType::Aggregate(name) | QueryType::Window(name) | QueryType::Federation(name) => {
            vec![query(name)]
        },
        QueryType::IntrospectionSchema => vec![query("__schema")],
        QueryType::IntrospectionType(_) => vec![query("__type")],
        QueryType::NodeQuery { .. } => vec![query("node")],
        // Both yield an empty op-list:
        // - `Mutation` is gated downstream at `execute_mutation_impl` (see fn-level docs).
        // - `TypeName` (`__typename`) is a GraphQL spec meta-field, always allowed.
        QueryType::Mutation { .. } | QueryType::TypeName { .. } => Vec::new(),
    }
}
