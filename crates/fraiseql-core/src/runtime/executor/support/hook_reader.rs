//! The engine's `before:mutation` read bridge (#1328).
//!
//! [`CallerScopedReader`] is the one production implementation of
//! [`GuestQueryBridge`]: it answers a hook's `fraiseql_query` from the executor
//! that is running the write, as the principal that issued it.
//!
//! # Why the engine builds it, and not the server
//!
//! Both halves of the contract are structural here and could only be *conventions*
//! anywhere else:
//!
//! - **The executor is the current one.** The bridge is constructed inside `execute_mutation_impl`
//!   from that call's own `ExecutorContext`, so it reads the schema the write is being planned
//!   against. A server-side bridge would have to hold a late-bound handle to the executor —
//!   installed after the gate, refreshed on hot reload, and silently unwired if either step were
//!   missed.
//! - **The identity is the caller's.** It is the same `Option<&SecurityContext>` the write is being
//!   adjudicated for, not a `run_as` ceiling resolved from a function definition. There is no
//!   widening step to get wrong: a hook cannot read what its caller could not.
//!
//! Read-only enforcement lives one layer down, in
//! [`Executor::execute_read_only`](crate::runtime::Executor), so it is decided on
//! the same classification the dispatch would have routed on.

use std::{future::Future, pin::Pin, sync::Arc};

use super::super::{Executor, context::ExecutorContext};
use crate::{
    db::traits::DatabaseAdapter,
    error::Result,
    security::{GuestQueryBridge, SecurityContext},
};

/// A read-only GraphQL bridge bound to one executor and one principal.
pub(in super::super) struct CallerScopedReader<A: DatabaseAdapter> {
    /// The context of the executor adjudicating the write.
    ctx:       Arc<ExecutorContext<A>>,
    /// The principal that issued the write, or `None` for an anonymous one — in
    /// which case the hook reads anonymously, which is exactly what its caller
    /// could do. Owned because the bridge outlives this call: the gate hands it to
    /// guest code that the Deno runtime runs on its own OS thread.
    principal: Option<SecurityContext>,
}

impl<A: DatabaseAdapter> CallerScopedReader<A> {
    /// Bind a read bridge to `ctx`, running as `principal`.
    pub(in super::super) fn new(
        ctx: Arc<ExecutorContext<A>>,
        principal: Option<&SecurityContext>,
    ) -> Self {
        Self {
            ctx,
            principal: principal.cloned(),
        }
    }
}

impl<A: DatabaseAdapter> GuestQueryBridge for CallerScopedReader<A> {
    fn query<'a>(
        &'a self,
        graphql: &'a str,
        variables: Option<&'a serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + 'a>> {
        let executor = Executor::from_ctx(Arc::clone(&self.ctx));
        Box::pin(async move {
            executor.execute_read_only(graphql, variables, self.principal.as_ref()).await
        })
    }
}
