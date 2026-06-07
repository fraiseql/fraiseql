//! Operation-authorization op-list extraction.
//!
//! Maps a classified [`QueryType`] (plus the parsed AST for `Regular` queries) to the
//! list of root operations to hand the configured
//! [`Authorizer`](crate::security::Authorizer). Lives in the `runtime::executor`
//! module because [`QueryType`] is private to it; the trait, request/decision types,
//! and the [`enforce_authz`](crate::security::authorizer::enforce_authz) helper live
//! in [`crate::security::authorizer`].

use super::super::QueryType;
use crate::{graphql::ParsedQuery, security::OperationKind};

/// Collect the `(kind, name)` of every root operation in a classified request.
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
) -> Vec<(OperationKind, String)> {
    match query_type {
        QueryType::Regular => parsed_for_regular.map_or_else(Vec::new, |parsed| {
            parsed
                .selections
                .iter()
                .map(|sel| (OperationKind::Query, sel.name.clone()))
                .collect()
        }),
        QueryType::Aggregate(name) | QueryType::Window(name) | QueryType::Federation(name) => {
            vec![(OperationKind::Query, name.clone())]
        },
        QueryType::IntrospectionSchema => vec![(OperationKind::Query, "__schema".to_string())],
        QueryType::IntrospectionType(_) => vec![(OperationKind::Query, "__type".to_string())],
        QueryType::NodeQuery { .. } => vec![(OperationKind::Query, "node".to_string())],
        // Mutations are gated at `execute_mutation_impl` (see fn-level docs).
        QueryType::Mutation { .. } => Vec::new(),
    }
}
