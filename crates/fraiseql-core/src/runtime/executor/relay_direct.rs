//! A page of a Relay query read by a transport that has no GraphQL document: a REST relay
//! route.

use super::Executor;
use crate::{
    error::Result,
    runtime::{CountSelection, EmbedSelection, matcher::QueryMatch},
    security::{RequestBudget, SecurityContext},
};

impl Executor {
    /// The keyset page a Relay query answers for `query_match`, whose root selects the fields
    /// of each row and whose arguments carry `first`/`after`/`last`/`before`, `where` and
    /// `orderBy`: `{"data": {<key>: {"edges": [{cursor, node}], "pageInfo": {…}}}}`.
    ///
    /// Gated as every direct read is (the operation gate, the row scope, field-level RBAC)
    /// and paged as the GraphQL connection is, so a cursor resumes only the ordering it was
    /// issued under. Each node is completed as the same read through
    /// [`execute_query_direct`](Self::execute_query_direct) completes its rows.
    ///
    /// # Errors
    ///
    /// Everything `execute_query_direct` refuses, and a cursor that does not decode or was
    /// issued under another ordering (`FraiseQLError::Validation`).
    pub async fn execute_relay_direct(
        &self,
        query_match: &QueryMatch,
        security_context: Option<&SecurityContext>,
        request_budget: Option<&RequestBudget>,
    ) -> Result<serde_json::Value> {
        let response = self
            .query_runner()
            .execute_relay_direct(query_match, security_context, request_budget)
            .await?;
        self.complete_connection(response, query_match, &[])
    }

    /// [`execute_relay_direct`](Self::execute_relay_direct)'s page with `embeds` and
    /// `counts` read into each node: `?select=id,author(name)` on a REST relay route.
    ///
    /// The page — its rows, their order, cursors and `pageInfo` — is the one the route
    /// answers without the embeds. Each embedded relationship is read as a level of one
    /// statement rooted at that page, gated as the same embed of an offset route's
    /// [`execute_query_composed`](Self::execute_query_composed) is: the target query's
    /// operation, role and actor gates, its field-level RBAC and its row scope, under the
    /// page's session settings.
    ///
    /// # Errors
    ///
    /// Everything `execute_relay_direct` refuses, and everything `execute_query_composed`
    /// refuses for an embedded level.
    pub async fn execute_relay_composed(
        &self,
        query_match: &QueryMatch,
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        security_context: Option<&SecurityContext>,
        request_budget: Option<&RequestBudget>,
    ) -> Result<serde_json::Value> {
        // The enrichment backstop, as on every composed-read entry.
        crate::runtime::executor::support::security::enforce_enrichment_resolved(
            &self.ctx.schema,
            security_context,
        )?;

        let response = self
            .query_runner()
            .execute_relay_composed(query_match, embeds, counts, security_context, request_budget)
            .await?;
        self.complete_connection(response, query_match, embeds)
    }

    /// Complete each edge's node as the same read through
    /// [`execute_query_direct`](Self::execute_query_direct) completes its rows, with the
    /// levels `embeds` wrote into it.
    fn complete_connection(
        &self,
        mut response: serde_json::Value,
        query_match: &QueryMatch,
        embeds: &[EmbedSelection],
    ) -> Result<serde_json::Value> {
        let key = query_match.response_key().to_string();
        let Some(edges) = response["data"][&key]["edges"].as_array_mut() else {
            return Ok(response);
        };
        let nodes: Vec<serde_json::Value> =
            edges.iter_mut().map(|edge| edge["node"].take()).collect();
        let completed = self.complete_direct(
            serde_json::json!({ "data": { key.as_str(): nodes } }),
            query_match,
            embeds,
        )?;
        let nodes = completed["data"][&key].as_array().cloned().unwrap_or_default();
        for (edge, node) in edges.iter_mut().zip(nodes) {
            edge["node"] = node;
        }
        Ok(response)
    }
}
