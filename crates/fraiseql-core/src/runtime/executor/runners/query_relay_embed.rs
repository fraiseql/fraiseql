//! A REST relay route's embedded relationships, read into its keyset page.
//!
//! `?select=id,author(name)` on a relay route is the Relay page of the route's query with
//! each embedded relationship read as a level of one composed statement whose root is that
//! page (`fraiseql_db::ComposedKeyset`): the page is cut — past its cursor, in its order,
//! `first`/`last` + 1 rows — before any level is joined to it, so an embed cannot change
//! which rows a page holds, their order or their cursors.
//!
//! Each level is planned by [`QueryRunner::plan_embeds`], the planner of the offset route's
//! composed read, so it passes every gate an embedded level passes there: the target
//! query's operation gate asked as a nested read, its role and actor gates, a selected
//! policy-gated field refused, field-level RBAC against the target type (a masked key
//! written `NULL` in the statement), and the target's row scope as the level's `WHERE`.
//! The statement runs with the page's session settings, on the page's connection.

use super::{query::QueryRunner, query_composed::PlannedEmbed, query_nested::merge_composed_row};
use crate::{
    backend::{COMPOSED_EMBEDS_KEY, ComposedLevel, JsonbValue, types::ReadRouting},
    error::Result,
    graphql::DirectReadProjection,
    runtime::{CountSelection, EmbedSelection, matcher::QueryMatch},
    security::{RequestBudget, SecurityContext},
};

/// The projection a relay page is scored as: `fields` over the requested `page`, and each
/// embedded level beneath it. With nothing embedded it is the flat projection a page
/// without embeds is scored as, so an embed only ever adds to the charge.
pub(super) fn relay_page_projection(
    fields: usize,
    page: Option<u32>,
    embeds: &[PlannedEmbed],
) -> DirectReadProjection {
    DirectReadProjection {
        nested: embeds.iter().filter_map(PlannedEmbed::projection).collect(),
        ..DirectReadProjection::flat(fields, page)
    }
}

impl QueryRunner {
    /// The connection page a REST relay route answers for `query_match`, with `embeds` and
    /// `counts` read into each node: [`execute_relay_direct`](Self::execute_relay_direct)'s
    /// page, cursors and `pageInfo`, each node carrying its embedded values.
    ///
    /// # Errors
    ///
    /// Everything `execute_relay_direct` refuses; everything an embedded level of the offset
    /// route's composed read is refused for; `FraiseQLError::Unsupported` from an adapter
    /// that cannot compose, and for a selected nested object whose type scopes its rows
    /// beside an embed.
    pub(in super::super) async fn execute_relay_composed(
        &self,
        query_match: &QueryMatch,
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        security_context: Option<&SecurityContext>,
        request_budget: Option<&RequestBudget>,
    ) -> Result<serde_json::Value> {
        self.relay_page(query_match, embeds, counts, security_context, request_budget)
            .await
    }

    /// Each embedded relationship of a relay page's nodes, resolved through every gate an
    /// embedded level passes. Empty when nothing is embedded.
    pub(super) fn plan_relay_embeds(
        &self,
        query_match: &QueryMatch,
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        security_context: Option<&SecurityContext>,
    ) -> Result<Vec<PlannedEmbed>> {
        if embeds.is_empty() && counts.is_empty() {
            return Ok(Vec::new());
        }
        let planned = self.plan_embeds(
            &query_match.query_def.return_type,
            "",
            embeds,
            counts,
            None,
            security_context,
        )?;
        for embed in &planned {
            embed.refuse_gated_leaf_objects(self, security_context)?;
        }
        Ok(planned)
    }

    /// Read `root`, a relay page with its gated nested levels as `root.embeds`, as one
    /// composed statement with `embeds` joined to it as well. Each row is the stored
    /// document, its gated nested levels merged in as a flat page's are, and the values of
    /// `embeds` for that row, which are attached to its node once it is projected.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Unsupported` from an adapter that cannot compose, refused from its
    /// capability flag; the adapter's own errors.
    pub(super) async fn execute_relay_composed_page(
        &self,
        mut root: ComposedLevel,
        embeds: &[PlannedEmbed],
        root_type: &str,
        session_pairs: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Vec<(JsonbValue, Option<serde_json::Value>)>> {
        if !self.ctx.adapter.supports_composed_reads() {
            return Err(crate::backend::composed_read_unsupported(&root.view));
        }
        let nested = root.embeds.len();
        let view = root.view.clone();
        root.embeds.extend(
            embeds
                .iter()
                .filter_map(|embed| embed.lower(&self.ctx.schema, root_type, &view)),
        );
        let rows = self
            .ctx
            .adapter
            .execute_composed_with_session(&root, session_pairs, routing)
            .await?;
        Ok(rows
            .iter()
            .map(|row| {
                let document = merge_composed_row(&row.data, &root.embeds[..nested]);
                let embedded = if embeds.is_empty() {
                    None
                } else {
                    row.data.get(COMPOSED_EMBEDS_KEY).cloned()
                };
                (JsonbValue::new(document), embedded)
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "query_relay_embed_tests.rs"]
mod query_relay_embed_tests;
