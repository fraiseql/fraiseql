//! A page of a Relay query read by a transport that has no GraphQL document: a REST relay
//! route.

use super::Executor;
use crate::{
    error::Result,
    runtime::matcher::QueryMatch,
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
        let mut response = self
            .query_runner()
            .execute_relay_direct(query_match, security_context, request_budget)
            .await?;
        let key = query_match.response_key().to_string();
        let Some(edges) = response["data"][&key]["edges"].as_array_mut() else {
            return Ok(response);
        };
        let nodes: Vec<serde_json::Value> =
            edges.iter_mut().map(|edge| edge["node"].take()).collect();
        let completed = self.complete_direct(
            serde_json::json!({ "data": { key.as_str(): nodes } }),
            query_match,
            &[],
        )?;
        let nodes = completed["data"][&key].as_array().cloned().unwrap_or_default();
        for (edge, node) in edges.iter_mut().zip(nodes) {
            edge["node"] = node;
        }
        Ok(response)
    }
}
