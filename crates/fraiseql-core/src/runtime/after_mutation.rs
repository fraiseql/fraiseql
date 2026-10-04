//! The engine's notice that a mutation committed (#1340, #1440).
//!
//! `after:mutation` functions run once a write has committed. Deciding *that* a write
//! committed, and *what* it wrote, is the engine's knowledge: it holds the parsed outcome
//! of the mutation function. A transport holds only the response it is about to serve.
//!
//! Dispatch used to live in the transports, reading the response, and that placement was
//! wrong three ways:
//!
//! - **Only two transports dispatched.** GraphQL and REST did; MCP, gRPC and the functions bridge
//!   commit through the same executor and fired nothing (#1440).
//! - **A failure looked like a success.** Under `auto_error_union` a failed write is *data*: an
//!   error member of the union, not a GraphQL error. A dispatcher reading the response could not
//!   tell the two apart unless the client happened to select `__typename` (#1340).
//! - **The row was the client's selection.** The event's row image was whatever fields the client
//!   selected, so a `when` predicate on an unselected field never matched.
//!
//! The executor therefore calls an [`AfterMutationObserver`] from the mutation runner, the
//! single chokepoint every transport converges on. It does so once per committed success,
//! with the type the write produced and the full entity the function returned. A failed
//! write, a dry run and a refused write never reach it. Like
//! [`BeforeMutationGate`](crate::security::BeforeMutationGate), the observer is a trait
//! object the application supplies, because `fraiseql-core` knows nothing about function
//! runtimes. `fraiseql-server` installs one that dispatches into the compiled schema's
//! `functions` section.

use std::future::Future;

use crate::{schema::MutationOperation, security::SecurityContext};

/// A mutation that committed successfully, as the engine saw it.
#[derive(Debug, Clone, Copy)]
pub struct CommittedMutation<'a> {
    /// The mutation's name, as declared.
    pub mutation_name:  &'a str,
    /// The type the write produced: the function's `entity_type` stamp, or else the one type
    /// a success of this mutation can be.
    ///
    /// Never the name of a synthesized error union or a cascade payload: the produced
    /// entity, which is what an `after:mutation:<Type>` trigger names.
    pub entity_type:    &'a str,
    /// The declared operation (insert, update, delete, or a custom function).
    pub operation:      &'a MutationOperation,
    /// The full entity the mutation function returned, independent of what any client
    /// selected. For a delete it is the removed row.
    pub entity:         &'a serde_json::Value,
    /// The mutated entity's id, when the function reported one.
    pub entity_id:      Option<&'a str>,
    /// The principal the write ran as, or `None` for an unauthenticated write.
    pub security_ctx:   Option<&'a SecurityContext>,
    /// How deep in a dispatch chain the write was made: `0` for a write a request or a
    /// scheduled source made, `n` for one made by a function that a depth-`n - 1` event
    /// dispatched. See [`dispatched_at`].
    pub dispatch_depth: u8,
}

/// Receives every committed mutation, from every transport.
///
/// Called on the request path, after the commit and before the response is returned, so an
/// implementation must not block: hand the work to a task and return. Whatever it does
/// cannot change the outcome of a write that has already committed, which is why the method
/// returns nothing.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
///
/// use fraiseql_core::runtime::{AfterMutationObserver, CommittedMutation, RuntimeConfig};
///
/// /// Log every committed write.
/// struct AuditLog;
///
/// impl AfterMutationObserver for AuditLog {
///     fn on_committed(&self, mutation: &CommittedMutation<'_>) {
///         tracing::info!(mutation = mutation.mutation_name, entity = mutation.entity_type);
///     }
/// }
///
/// let config = RuntimeConfig::default().with_after_mutation_observer(Arc::new(AuditLog));
/// ```
pub trait AfterMutationObserver: Send + Sync {
    /// A mutation committed successfully.
    fn on_committed(&self, mutation: &CommittedMutation<'_>);
}

tokio::task_local! {
    static DISPATCH_DEPTH: u8;
}

/// Run `future` as the work of a function dispatched `depth` levels deep, so every write it
/// makes is observed with that [`CommittedMutation::dispatch_depth`].
///
/// A dispatched function's writes pass the same chokepoint as a request's, so an
/// `after:mutation` function that writes the entity it is triggered by would trigger itself
/// without end. The depth is what lets the observer stop such a chain. It is carried in a
/// task-local rather than on the [`SecurityContext`] because a client controls neither, and
/// cannot set this one: only code that wraps its own executor calls in this scope can.
pub async fn dispatched_at<F: Future>(depth: u8, future: F) -> F::Output {
    DISPATCH_DEPTH.scope(depth, future).await
}

/// The dispatch depth of the current task: `0` outside any [`dispatched_at`] scope.
#[must_use]
pub fn current_dispatch_depth() -> u8 {
    DISPATCH_DEPTH.try_with(|depth| *depth).unwrap_or(0)
}
