//! A page of a Relay query read without a GraphQL document: a REST relay route.
//!
//! The read resolves through [`resolve_root_read`](QueryRunner::resolve_root_read) as
//! [`Door::Connection`], so the operation gate is asked as for any direct read, then pages
//! through the Relay runner: `first`/`after`/`last`/`before` bound and position the page by
//! keyset under the request's `orderBy`, and each row carries the cursor that resumes after it.

use std::sync::Arc;

use super::{
    query::QueryRunner,
    root_read::{Door, RootRoute},
};
use crate::{
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::{CountSelection, EmbedSelection, matcher::QueryMatch},
    security::{RequestBudget, SecurityContext},
};

/// The `pageInfo` a connection page answers.
const PAGE_INFO: [&str; 4] = ["hasNextPage", "hasPreviousPage", "startCursor", "endCursor"];

/// A leaf selection of `name`.
fn leaf(name: &str) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     Vec::new(),
        nested_fields: Vec::new(),
        directives:    Vec::new(),
    }
}

/// `name { fields }`.
fn object(name: &str, nested_fields: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        nested_fields,
        ..leaf(name)
    }
}

/// `query_match` with its root's selected fields as a connection selects them:
/// `edges { cursor node { <fields> } } pageInfo { … }`.
fn as_connection(query_match: &QueryMatch) -> QueryMatch {
    let mut connection = query_match.clone();
    let mut roots = query_match.selections.to_vec();
    if let Some(root) = roots.first_mut() {
        let node = std::mem::take(&mut root.nested_fields);
        root.nested_fields = vec![
            object("edges", vec![leaf("cursor"), object("node", node)]),
            object("pageInfo", PAGE_INFO.iter().map(|f| leaf(f)).collect()),
        ];
    }
    connection.selections = Arc::from(roots);
    connection
}

impl QueryRunner {
    /// The connection page a Relay query answers for `query_match`, whose root selects the
    /// fields of each row: `{"data": {<key>: {"edges": [{cursor, node}], "pageInfo": …}}}`.
    ///
    /// # Errors
    ///
    /// Everything [`resolve_root_read`](Self::resolve_root_read) refuses for a direct read,
    /// and everything the Relay runner refuses: a cursor issued under another ordering, a
    /// page over its ceiling, a policy-gated field. `CostExceeded` and `ResponseTooLarge`
    /// against `request_budget`.
    pub(in super::super) async fn execute_relay_direct(
        &self,
        query_match: &QueryMatch,
        security_context: Option<&SecurityContext>,
        request_budget: Option<&RequestBudget>,
    ) -> Result<serde_json::Value> {
        self.relay_page(query_match, &[], &[], security_context, request_budget).await
    }

    /// The connection page of [`execute_relay_direct`](Self::execute_relay_direct), with
    /// `embeds` and `counts` read into each node: none for a flat page.
    pub(super) async fn relay_page(
        &self,
        query_match: &QueryMatch,
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        security_context: Option<&SecurityContext>,
        request_budget: Option<&RequestBudget>,
    ) -> Result<serde_json::Value> {
        let query_def = &query_match.query_def;
        match self.resolve_root_read(query_match, None, security_context, Door::Connection)? {
            RootRoute::Relay => {},
            _ => {
                return Err(FraiseQLError::Internal {
                    message: format!(
                        "the page of '{}' was not routed to its connection; this is a bug",
                        query_def.name
                    ),
                    source:  None,
                });
            },
        }

        // Each embedded level resolved through every gate an embedded level passes, after the
        // root's own gates and before anything is scored or read.
        let planned = self.plan_relay_embeds(query_match, embeds, counts, security_context)?;

        // Scored before the database: the selected fields over the page the request asks for,
        // and each embedded level beneath it (flat when nothing is embedded).
        let fields = query_match.selections.first().map_or(0, |root| root.nested_fields.len());
        let page = ["first", "last"]
            .into_iter()
            .map(|name| {
                super::query_params::coerce_pagination_arg(name, query_match.arguments.get(name))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .max();
        self.charge_direct_read_cost(
            &query_def.name,
            &super::query_relay_embed::relay_page_projection(fields, page, &planned),
            request_budget,
        )?;

        let session_vars = self.resolve_session_vars(security_context)?;
        let session_pairs: Vec<(&str, &str)> =
            session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let connection = as_connection(query_match);
        let response = self
            .execute_relay_query(&connection, None, security_context, &session_pairs, &planned)
            .await?;

        // The response-bytes ceiling, charged on the rows the page serves.
        if let Some(budget) = request_budget.and_then(RequestBudget::bytes) {
            let edges = response["data"][query_match.response_key()]["edges"].as_array();
            for edge in edges.into_iter().flatten() {
                budget.charge(crate::security::response_budget::json_bytes(&edge["node"]))?;
            }
        }
        Ok(response)
    }
}
