//! Synchronous, decision-returning enforcement immediately before a mutation writes.
//!
//! This is the engine half of the `before:mutation` trigger. Where
//! [`Authorizer`](crate::security::Authorizer) answers "may this principal run
//! this operation?", a [`BeforeMutationGate`] answers the broader question a
//! business rule asks: "given the arguments this write is about to run with, may
//! it run at all, and are these the arguments it should run with?" The engine
//! *enforces* the answer; the *decision* is delegated to an app-supplied trait
//! object — in `fraiseql-server` that object runs the compiled schema's
//! `before:mutation` function chain.
//!
//! # Why it lives at the chokepoint
//!
//! `before:mutation` is enforcement, so it is worth nothing unless it is
//! unbypassable. It used to run once per HTTP request in the GraphQL handler,
//! against the request's `variables`, keyed on `parse_query(…).root_field` — and
//! that shape left three ways to execute a mutation without running its chain
//! (#1327):
//!
//! 1. a **second root field**: the handler keyed on the *first* root, and since #759 the executor
//!    runs every root serially, so the second one executed having run only the first one's chain;
//! 2. **inline arguments**: the chain was handed `request.variables`, so `guarded(input: { … })`
//!    was invisible to it and a rewrite could only reach variables;
//! 3. the **REST write route**, which dispatched `after:mutation` only.
//!
//! The gate is therefore enforced from `execute_mutation_impl` — the single
//! point every mutation entry path converges on, the same place `requires_role`,
//! `requires_actor` (#966) and the `Authorizer` (#422) are enforced, and for the
//! same reason. Per root, in document order, immediately before that root's
//! write, with the arguments the write will actually run with.
//!
//! # Semantics
//!
//! - **Fail-closed**: any `Err` from [`BeforeMutationGate::before_mutation`] refuses the write. It
//!   never falls through to "proceed with the original input", which would run a mutation the chain
//!   declined to approve.
//! - **Keyed on the field name, never the alias**: two roots calling the same mutation differ only
//!   by response key, so keying on the alias would run one chain twice and skip the other.
//!   [`BeforeMutationRequest::mutation`] is the name; `response_key` is carried for diagnostics
//!   only.
//! - **Resolved arguments**: [`BeforeMutationRequest::arguments`] is the merged view — request
//!   variables plus inline literals, nested `$var` references already substituted — so a rule can
//!   read what the write will use regardless of how the client spelled it.
//! - **Rewrites reach the write**: [`BeforeMutationOutcome::ProceedWith`] replaces the arguments
//!   the engine binds to the SQL function, not just the request variables.
//!
//! # Wiring
//!
//! Register an implementation on [`RuntimeConfig`](crate::runtime::RuntimeConfig)
//! via
//! [`with_before_mutation_gate`](crate::runtime::RuntimeConfig::with_before_mutation_gate),
//! parallel to [`with_authorizer`](crate::runtime::RuntimeConfig::with_authorizer).

use std::{future::Future, pin::Pin, sync::Arc};

use crate::{
    error::{FraiseQLError, Result},
    security::SecurityContext,
};

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
pub trait MutationHookReader: Send + Sync {
    /// Execute a read-only GraphQL document as the requesting principal.
    ///
    /// # Errors
    ///
    /// - [`FraiseQLError::Authorization`] when the document's operation is one the engine would
    ///   execute as a write. This is the read-only refusal and names itself.
    /// - Anything the read itself returns — an unknown field, a validation failure, a database
    ///   error, the executor's query timeout.
    fn query<'a>(
        &'a self,
        graphql: &'a str,
        variables: Option<&'a serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + 'a>>;
}

/// The write a [`BeforeMutationGate`] is being asked to adjudicate.
#[non_exhaustive]
pub struct BeforeMutationRequest<'a> {
    /// The authenticated principal, or `None` for an unauthenticated (anonymous) request.
    pub principal:    Option<&'a SecurityContext>,
    /// The mutation field name — the key the chain is looked up under. **Never** the
    /// response alias.
    pub mutation:     &'a str,
    /// The key this root's result appears under in `data` (the alias when the document
    /// supplies one, otherwise the same as `mutation`). Diagnostics only.
    pub response_key: &'a str,
    /// The arguments this write will run with: request variables merged with the root
    /// field's inline literals, nested `$var` references resolved. `Null` when the write
    /// has no arguments at all.
    pub arguments:    &'a serde_json::Value,
    /// The caller-scoped, read-only GraphQL bridge this hook may read through
    /// (#1328), or `None` on a request built by hand.
    ///
    /// Owned rather than borrowed because the gate hands it to guest code that runs
    /// on another thread — the Deno runtime spawns an OS thread per invocation — so
    /// it has to outlive this call. The engine builds one per adjudicated write; a
    /// gate that never reads simply drops it.
    pub reader:       Option<Arc<dyn MutationHookReader>>,
}

impl<'a> BeforeMutationRequest<'a> {
    /// Build a request with no read bridge. The engine is the only production
    /// constructor; this exists so an implementor outside this crate can exercise
    /// its own gate, which `#[non_exhaustive]` would otherwise make impossible.
    #[must_use]
    pub const fn new(
        principal: Option<&'a SecurityContext>,
        mutation: &'a str,
        response_key: &'a str,
        arguments: &'a serde_json::Value,
    ) -> Self {
        Self {
            principal,
            mutation,
            response_key,
            arguments,
            reader: None,
        }
    }

    /// Attach the caller-scoped read bridge (#1328).
    #[must_use]
    pub fn with_reader(mut self, reader: Arc<dyn MutationHookReader>) -> Self {
        self.reader = Some(reader);
        self
    }
}

/// What the engine does next with the write.
#[derive(Debug)]
#[non_exhaustive]
pub enum BeforeMutationOutcome {
    /// Run the write with the arguments it already had.
    Proceed,
    /// Run the write with these arguments instead. They replace the whole argument
    /// view the engine binds from, so an implementation that rewrites one key must
    /// return the other keys too.
    ProceedWith {
        /// The argument object the write will run with.
        arguments: serde_json::Value,
    },
    /// Do not run the write. `reason` is folded into a
    /// [`FraiseQLError::Validation`] and reaches the client verbatim — it is the
    /// rule's own message, not engine text.
    Abort {
        /// The client-facing reason the write was refused.
        reason: String,
    },
}

/// A pluggable, decision-returning enforcement point that runs immediately before
/// a mutation writes.
///
/// Implementations must be `Send + Sync` to be shared across the async execution
/// path. Implementations run on the hot path of every matching write, so a gate
/// with nothing to say for a given mutation should return
/// [`BeforeMutationOutcome::Proceed`] as cheaply as it can.
///
/// # Example
///
/// ```
/// use fraiseql_core::error::Result;
/// use fraiseql_core::security::{
///     BeforeMutationGate, BeforeMutationOutcome, BeforeMutationRequest,
/// };
///
/// /// Refuse any write whose `input.amount` exceeds a ceiling.
/// struct CapAmount {
///     ceiling: i64,
/// }
///
/// #[async_trait::async_trait]
/// impl BeforeMutationGate for CapAmount {
///     async fn before_mutation(
///         &self,
///         req: &BeforeMutationRequest<'_>,
///     ) -> Result<BeforeMutationOutcome> {
///         let amount = req.arguments.pointer("/input/amount").and_then(|v| v.as_i64());
///         match amount {
///             Some(value) if value > self.ceiling => Ok(BeforeMutationOutcome::Abort {
///                 reason: format!("amount {value} exceeds the {} ceiling", self.ceiling),
///             }),
///             _ => Ok(BeforeMutationOutcome::Proceed),
///         }
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait BeforeMutationGate: Send + Sync {
    /// Decide whether this write may run, and with which arguments.
    ///
    /// # Errors
    ///
    /// Any `Err` refuses the write (fail-closed). Return
    /// [`BeforeMutationOutcome::Abort`] for a *rule* that declined — an `Err` means
    /// the gate itself could not reach a decision.
    async fn before_mutation(
        &self,
        request: &BeforeMutationRequest<'_>,
    ) -> Result<BeforeMutationOutcome>;
}

/// The engine-side enforcement of a configured [`BeforeMutationGate`].
///
/// Returns the replacement argument view when the gate rewrote it, `None` when the
/// arguments stand (including the zero-overhead case of no gate configured).
///
/// Called from `execute_mutation_impl`, after every static gate and after the
/// inline-argument merge, so `arguments` is what the write will bind from and a
/// refusal happens before anything is written.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] carrying the rule's own message when the
/// gate aborts, or the gate's own error unchanged when it could not decide.
pub(crate) async fn enforce_before_mutation(
    gate: Option<&dyn BeforeMutationGate>,
    principal: Option<&SecurityContext>,
    mutation: &str,
    response_key: &str,
    arguments: Option<&serde_json::Value>,
    reader: impl FnOnce() -> Arc<dyn MutationHookReader>,
) -> Result<Option<serde_json::Value>> {
    // A write with no arguments is `Null`, not `{}`: that is the payload shape the
    // `before:mutation` chain has always been handed for an argument-less mutation,
    // and a rule written against it would break if it changed.
    static NO_ARGUMENTS: serde_json::Value = serde_json::Value::Null;

    let Some(gate) = gate else {
        return Ok(None);
    };

    // Built *after* the no-gate return, so a build with no gate installed still
    // costs one `Option` check: `reader` is a closure precisely so constructing it
    // (an `Arc` allocation plus a `SecurityContext` clone) happens only on a write
    // that is actually adjudicated.
    let request = BeforeMutationRequest::new(
        principal,
        mutation,
        response_key,
        arguments.unwrap_or(&NO_ARGUMENTS),
    )
    .with_reader(reader());
    match gate.before_mutation(&request).await? {
        BeforeMutationOutcome::Proceed => Ok(None),
        BeforeMutationOutcome::ProceedWith { arguments } => Ok(Some(arguments)),
        BeforeMutationOutcome::Abort { reason } => Err(FraiseQLError::Validation {
            message: reason,
            path:    None,
        }),
    }
}

#[cfg(test)]
mod tests;
