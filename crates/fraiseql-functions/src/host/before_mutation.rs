//! The host surface a `before:mutation` hook runs on (#1328).
//!
//! [`BeforeMutationHost`] is deliberately **not** [`LiveHostContext`] with most of
//! its builders left unset. A host whose surface is "whatever the wiring happened
//! to attach" answers the question "what may a before-hook do?" with a call-site
//! audit, and widens silently the day someone attaches one more backend to it. This
//! type answers it by construction: every op is written out, and the ones a
//! before-hook does not get refuse **by name**, saying which trigger kind is
//! asking.
//!
//! # The surface, and why it is this one
//!
//! | op | before:mutation | why |
//! |---|---|---|
//! | `fraiseql_query` | **yes**, read-only, as the caller | #1328's whole point: a rule that depends on data — a credit limit, a price, a quota, the target row's state — has to read something |
//! | `fraiseql_log` | yes | diagnostics never leave the process |
//! | the event payload | yes | it *is* the mutation's resolved arguments |
//! | `fraiseql_auth_context` | yes, the caller's | a rule keyed on who is asking; refused on an anonymous write, because there is no authenticated identity to hand it and inventing one is #803 |
//! | `fraiseql_sql_query` | no | it has no execution backend anywhere (#871) |
//! | `fraiseql_http_request` | no | it runs **synchronously on the write path**, inside a budget measured in hundreds of milliseconds, and its side effects are not rolled back when a later hook aborts. An outbound call belongs in `after:mutation`, which is durable, retried and dead-lettered |
//! | `fraiseql_storage_*`, `fraiseql_send_email` | no | same reason: a side effect that survives the abort of the write it was deciding on |
//! | the idempotency token, the source cursor | no | a before-hook is not a durable dispatch and is not bound to a source; the trait's own defaults already say so |
//!
//! Nothing here is a *reduction*: before #1328 a before-hook ran on a
//! `NoopHostContext` **and** through the sync `invoke` path, so every one of these
//! ops failed. The read is what is new; the rest are refusals with a reason
//! attached instead of a generic "not available in snapshot context".
//!
//! [`LiveHostContext`]: crate::host::live::LiveHostContext

use std::sync::Arc;

use fraiseql_core::security::{GuestQueryBridge, SecurityContext};
use fraiseql_error::{FraiseQLError, Result};

use crate::{
    HostContext,
    host::HttpResponse,
    types::{EventPayload, LogLevel},
};

/// The host a `before:mutation` chain runs on: one read, and refusals that say why.
pub struct BeforeMutationHost {
    /// The mutation's resolved arguments, in the shape the chain hands the guest.
    event_payload: EventPayload,
    /// The caller-scoped, read-only GraphQL bridge (#1328). `None` leaves `query`
    /// refusing loudly rather than answering from nowhere — the engine always
    /// supplies one, so `None` means a caller built this host by hand.
    reader:        Option<Arc<dyn GuestQueryBridge>>,
    /// The principal that issued the write, or `None` when it was anonymous.
    principal:     Option<SecurityContext>,
}

impl BeforeMutationHost {
    /// Build the host for one adjudicated write.
    #[must_use]
    pub fn new(
        event_payload: EventPayload,
        reader: Option<Arc<dyn GuestQueryBridge>>,
        principal: Option<&SecurityContext>,
    ) -> Self {
        Self {
            event_payload,
            reader,
            principal: principal.cloned(),
        }
    }

    /// The refusal every op outside the surface answers with.
    ///
    /// One constructor so the diagnosis is uniform and every refusal names the op
    /// and the trigger kind — a guest author reading a stack trace needs to know
    /// *which* surface said no, not just that something did.
    fn outside_the_surface(op: &str) -> FraiseQLError {
        FraiseQLError::Authorization {
            message:  format!(
                "`{op}` is not available to a before:mutation hook: the hook runs \
                 synchronously on the write path, within a latency budget, and its side \
                 effects are not rolled back if the write is refused. Move the effect to an \
                 after:mutation function, which is durable, retried and dead-lettered."
            ),
            action:   Some(op.to_string()),
            resource: Some("before:mutation".to_string()),
        }
    }
}

// Reason: `HostContext` is async because other implementations perform I/O. The
// ops this host refuses answer from memory but still have to present the awaited
// signature the trait defines and every guest call site uses.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl HostContext for BeforeMutationHost {
    async fn query(
        &self,
        graphql: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let reader = self.reader.as_ref().ok_or_else(|| FraiseQLError::Unsupported {
            message: "no read bridge is wired on this before:mutation host — the engine \
                      supplies one for every adjudicated write"
                .to_string(),
        })?;
        // `Null` is what the guest sends when it passes no variables; forwarding it
        // as `Some(Null)` would make the executor's "were variables supplied?"
        // question answer yes with nothing in it.
        let variables = (!variables.is_null()).then_some(variables);
        reader.query(graphql, variables.as_ref()).await
    }

    async fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<serde_json::Value>> {
        Err(Self::outside_the_surface("sql_query"))
    }

    async fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> Result<HttpResponse> {
        Err(Self::outside_the_surface("http_request"))
    }

    async fn storage_get(&self, _bucket: &str, _key: &str) -> Result<Vec<u8>> {
        Err(Self::outside_the_surface("storage_get"))
    }

    async fn storage_put(
        &self,
        _bucket: &str,
        _key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> Result<()> {
        Err(Self::outside_the_surface("storage_put"))
    }

    async fn send_email(
        &self,
        _request: &crate::outbound::SendEmailRequest,
    ) -> Result<crate::outbound::SendEmailResponse> {
        Err(Self::outside_the_surface("send_email"))
    }

    fn auth_context(&self) -> Result<serde_json::Value> {
        // An anonymous write has no authenticated identity, and saying so is not
        // the same as `LiveHostContext`'s "nothing was wired" (#803) — here the
        // absence is a fact about the request, not about the plumbing. Refused
        // rather than answered with a null-filled object so a rule cannot read
        // `sub: null` as a user.
        let context = self.principal.as_ref().ok_or_else(|| FraiseQLError::Unsupported {
            message: "this mutation was issued anonymously: a before:mutation hook has no \
                      authenticated context to read"
                .to_string(),
        })?;
        Ok(crate::host::auth_context_json(context))
    }

    fn env_var(&self, _name: &str) -> Result<Option<String>> {
        Err(Self::outside_the_surface("env_var"))
    }

    fn event_payload(&self) -> &EventPayload {
        &self.event_payload
    }

    fn log(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Debug => tracing::debug!(target: "fraiseql::functions::guest", "{message}"),
            LogLevel::Info => tracing::info!(target: "fraiseql::functions::guest", "{message}"),
            LogLevel::Warn => tracing::warn!(target: "fraiseql::functions::guest", "{message}"),
            LogLevel::Error => tracing::error!(target: "fraiseql::functions::guest", "{message}"),
        }
    }
}

#[cfg(test)]
mod tests;
