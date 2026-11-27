//! A subscriber's own read of the row an event names.
//!
//! The change log carries the document as the write returned it, evaluated in the writing
//! session. A view or a policy that reads the session (a mask or a row predicate keyed on
//! `current_setting(..)`) answered for the writer there. A subscriber is served what its
//! own read of the row returns: the type's view, read by the row's `id` under the
//! subscriber's row scope and session variables. A [`SubscriberRow`] is that read's
//! result and nothing else: it has no other constructor, and [`SubscriptionPlan::deliver`]
//! serves nothing but one.

use std::{collections::VecDeque, sync::Arc};

use super::{Served, SubscriptionPlan};
use crate::{
    db::{WhereClause, WhereOperator},
    error::Result,
    runtime::executor::{context::ExecutorContext, support::security::read_session_variables},
};

/// How many rows one subscription remembers having been delivered.
///
/// A delete reaches only a subscriber that was delivered the row, carrying the row as that
/// subscriber read it. Past the bound the oldest is forgotten, and a later delete of it is
/// not delivered: a delete is never sent to a subscriber not known to have been delivered
/// the row.
pub const DELIVERED_ROWS_BOUND: usize = 256;

/// How many bytes of rows one subscription remembers.
///
/// Each remembered row is held whole, so a subscription to large rows is bounded by their
/// size as well as by [`DELIVERED_ROWS_BOUND`]. The oldest are forgotten first, with the
/// same consequence.
pub const DELIVERED_ROWS_BYTES: usize = 1 << 20;

/// A row as one subscriber's own read returned it.
#[derive(Debug, Clone)]
pub struct SubscriberRow {
    document: Arc<serde_json::Value>,
}

impl SubscriberRow {
    /// The document the subscriber's read returned.
    #[must_use]
    pub fn document(&self) -> &serde_json::Value {
        &self.document
    }

    /// A row standing in for a read, in this crate's unit tests only.
    #[cfg(test)]
    pub(crate) fn read_as(document: serde_json::Value) -> Self {
        Self {
            document: Arc::new(document),
        }
    }
}

/// The read one subscriber makes of one row. Two equal reads return the same row: the same
/// executor, view, predicate and session settings.
struct SubscriberRead {
    ctx:          Arc<ExecutorContext>,
    type_name:    String,
    source:       String,
    where_clause: WhereClause,
    session_vars: Vec<(String, String)>,
}

impl PartialEq for SubscriberRead {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.ctx, &other.ctx)
            && self.type_name == other.type_name
            && self.source == other.source
            && self.where_clause == other.where_clause
            && self.session_vars == other.session_vars
    }
}

impl SubscriberRead {
    async fn run(&self) -> Result<Option<SubscriberRow>> {
        let pairs: Vec<(&str, &str)> =
            self.session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let request = fraiseql_db::ProjectionRequest {
            view:          &self.source,
            source_args:   None,
            projection:    None,
            where_clause:  Some(&self.where_clause),
            order_by:      None,
            limit:         Some(1),
            offset:        None,
            matched_up_to: None,
            decimals:      self.ctx.decimals.shape(&self.type_name).map(|shape| &**shape),
            scalar_keys:   None,
        };
        // The primary: the row was just written, and a replica may not hold it yet.
        let rows = self
            .ctx
            .adapter
            .execute_with_projection_arc_with_session(
                &request,
                &pairs,
                crate::backend::types::ReadRouting::Primary,
            )
            .await?;
        Ok(rows.first().map(|row| SubscriberRow {
            document: Arc::new(row.as_value().clone()),
        }))
    }
}

impl SubscriptionPlan {
    /// The read this subscriber makes of row `entity_id`.
    fn subscriber_read(&self, entity_id: &str) -> Result<SubscriberRead> {
        let id = WhereClause::Field {
            path:     vec!["id".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::Value::String(entity_id.to_string()),
        };
        let where_clause = match &self.row_scope {
            Some(scope) => WhereClause::And(vec![scope.clone(), id]),
            None => id,
        };
        let session_vars = crate::runtime::with_request_locale_sync(self.locale.clone(), || {
            read_session_variables(&self.ctx.schema, self.principal.as_ref())
        })?;
        Ok(SubscriberRead {
            ctx: Arc::clone(&self.ctx),
            type_name: self.type_name.clone(),
            source: self.source.clone(),
            where_clause,
            session_vars,
        })
    }

    /// Row `entity_id` as this subscriber's own read returns it, or `None` when its read
    /// returns no such row.
    ///
    /// # Errors
    ///
    /// As the session variables' resolution, and `Database` when the read fails.
    pub async fn read(&self, entity_id: &str) -> Result<Option<SubscriberRow>> {
        self.subscriber_read(entity_id)?.run().await
    }

    /// Serve one change to this subscriber from its own read, or `None` when it is not
    /// delivered.
    ///
    /// A row its read does not return, or that `admits` refuses, is not delivered, and is
    /// forgotten as delivered. A delete is delivered only when this subscription was
    /// delivered the row, as the subscriber last read it.
    ///
    /// # Errors
    ///
    /// As [`read`](Self::read). The caller ends the subscription: an event is never served
    /// from anything but the subscriber's read.
    pub async fn serve_change(
        &self,
        change: Change<'_>,
        delivered: &mut DeliveredRows,
        reads: &mut SubscriberReads,
        admits: impl FnOnce(&serde_json::Value) -> bool,
    ) -> Result<Option<Served>> {
        if change.deleted {
            return Ok(delivered
                .forget(change.entity_id)
                .and_then(|row| self.deliver(&row, change.captured)));
        }
        let row = reads.read(self, change.event_id, change.entity_id).await?;
        let Some(row) = row.filter(|row| admits(row.document())) else {
            delivered.forget(change.entity_id);
            return Ok(None);
        };
        let served = self.deliver(&row, change.captured);
        if served.is_some() {
            delivered.remember(change.entity_id, row);
        } else {
            delivered.forget(change.entity_id);
        }
        Ok(served)
    }
}

/// One change, as a subscriber is served it.
#[derive(Debug, Clone, Copy)]
pub struct Change<'a> {
    /// The event, which names the change across every subscriber it reaches.
    pub event_id:  &'a str,
    /// The changed row's `id`.
    pub entity_id: &'a str,
    /// The row was deleted.
    pub deleted:   bool,
    /// The change was captured outside a mutation.
    pub captured:  bool,
}

/// The reads of one event, shared by the subscriptions of one reader that read it alike: two
/// subscriptions share a read only when the reads are equal, so no session's row is served to
/// another session.
#[derive(Default)]
pub struct SubscriberReads {
    event: Option<String>,
    done:  Vec<(SubscriberRead, Option<SubscriberRow>)>,
}

impl SubscriberReads {
    async fn read(
        &mut self,
        plan: &SubscriptionPlan,
        event_id: &str,
        entity_id: &str,
    ) -> Result<Option<SubscriberRow>> {
        if self.event.as_deref() != Some(event_id) {
            self.event = Some(event_id.to_string());
            self.done.clear();
        }
        let read = plan.subscriber_read(entity_id)?;
        if let Some((_, row)) = self.done.iter().find(|(done, _)| *done == read) {
            return Ok(row.clone());
        }
        let row = read.run().await?;
        self.done.push((read, row.clone()));
        Ok(row)
    }
}

/// The rows one subscription was delivered, newest last, at most [`DELIVERED_ROWS_BOUND`]
/// of them and [`DELIVERED_ROWS_BYTES`] of their documents.
#[derive(Debug, Default)]
pub struct DeliveredRows {
    rows:  VecDeque<(String, SubscriberRow, usize)>,
    bytes: usize,
}

impl DeliveredRows {
    fn remember(&mut self, id: &str, row: SubscriberRow) {
        self.forget(id);
        let size = id.len() + serde_json::to_vec(row.document()).map_or(usize::MAX, |v| v.len());
        self.bytes = self.bytes.saturating_add(size);
        self.rows.push_back((id.to_string(), row, size));
        while self.rows.len() > DELIVERED_ROWS_BOUND || self.bytes > DELIVERED_ROWS_BYTES {
            let Some((_, _, size)) = self.rows.pop_front() else {
                break;
            };
            self.bytes -= size;
        }
    }

    fn forget(&mut self, id: &str) -> Option<SubscriberRow> {
        let at = self.rows.iter().position(|(held, ..)| held == id)?;
        let (_, row, size) = self.rows.remove(at)?;
        self.bytes -= size;
        Some(row)
    }
}
