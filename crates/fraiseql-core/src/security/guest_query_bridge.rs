//! The read-only GraphQL bridge a sandboxed guest reads through (#1328, #1329).
//!
//! One bridge, two consumers, and it belongs to neither: a `before:mutation` hook
//! adjudicating a write reads through it, and since #1329 so does a `request:query`
//! function answering a root query field. It lived in
//! [`mutation_gate`](super::mutation_gate) while there was only one, and the name
//! said so — `MutationHookReader`, handed to a function that is not a mutation hook.
//!
//! The contract is the same for both, and it is the whole reason the bridge exists
//! rather than a `fraiseql_sql_query`: a guest reads **what its caller could have
//! read**, through the engine, and cannot write.

use std::{future::Future, pin::Pin};

use crate::error::Result;

/// A **read-only** GraphQL bridge scoped to the principal that issued the write
/// (#1328).
///
/// A `before:mutation` rule that depends on data — a credit limit, a price, a
/// quota, the target row's current state — needs to read. This is the only way it
/// can, and the two words in the name are the whole contract:
///
/// - **read-only**: a document whose operation the engine would execute as a *write* is refused by
///   name. The hook cannot become a second write path, so an abort cannot leave a half-applied
///   change behind.
/// - **scoped to the caller**: the read runs as the requesting principal, not under a `run_as`
///   ceiling, so a hook can never surface a row the caller could not have read itself. An anonymous
///   write reads anonymously.
///
/// # The read is outside the mutation's transaction
///
/// Deliberately: holding a Postgres transaction (and its row locks, and a pooled
/// connection) open across a V8 isolate running user-supplied `JavaScript` would make
/// function latency into database lock time, reachable by anyone who can author a
/// function. The consequence is stated in `docs/architecture/functions.md` and is
/// load-bearing for anyone writing a rule:
///
/// > For anything derivable from its **input**, `before:mutation` is authoritative. For
/// > anything requiring a **read**, it is a fast, friendly rejection — the read is not in
/// > the mutation's transaction, so the authoritative rule must still be a constraint or
/// > the SQL function.
///
/// A hook author who believes a read-backed check is authoritative has written a
/// check-then-act race and does not know it.
pub trait GuestQueryBridge: Send + Sync {
    /// Execute a read-only GraphQL document as the requesting principal.
    ///
    /// # Errors
    ///
    /// - [`FraiseQLError::Authorization`](crate::error::FraiseQLError::Authorization) when the
    ///   document's operation is one the engine would execute as a write. This is the read-only
    ///   refusal and names itself.
    /// - Anything the read itself returns — an unknown field, a validation failure, a database
    ///   error, the executor's query timeout.
    fn query<'a>(
        &'a self,
        graphql: &'a str,
        variables: Option<&'a serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + 'a>>;
}
