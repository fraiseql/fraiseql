//! The host surface a `request:query` function runs on (#1329).
//!
//! [`RequestQueryHost`] is written out the way [`BeforeMutationHost`] is, and for
//! the same reason: a host whose surface is "whatever the wiring happened to attach"
//! answers "what may a request-serving function do?" with a call-site audit, and
//! widens silently the day someone attaches one more backend. Every op is listed,
//! and the ones this kind does not get refuse **by name**, saying which trigger kind
//! is asking.
//!
//! # The surface, and why it is this one
//!
//! | op | `request:query` | why |
//! |---|---|---|
//! | `fraiseql_query` | **yes**, read-only, as the caller | it is a read: the field exists to compute an answer, and almost every answer depends on data |
//! | `fraiseql_http_request` | **yes**, SSRF-allowlisted | #1329's own examples — an LLM-backed answer, a BFF aggregation — are outbound calls. The guard is `outbound_http::perform`, the same code the live host runs, so "the SSRF allowlist, unchanged" is a fact about the binary rather than a promise |
//! | `fraiseql_auth_context` | yes, the caller's; refused when anonymous | a field keyed on who is asking |
//! | `fraiseql_log` | yes | diagnostics never leave the process |
//! | the event payload | the field's resolved **arguments**, under `data` | what the client asked for |
//! | `fraiseql_sql_query` | no | it has no execution backend anywhere (#871) |
//! | `fraiseql_storage_*`, `fraiseql_send_email` | **no** | a read that writes a file or sends mail is a side effect on a path that is cached, retried by clients, and reachable by anyone who can issue the query. Those belong in `after:mutation`, which is durable and dead-lettered |
//! | `fraiseql_env_var` | no | nothing on this path needs one, and an allowlisted read is a widening to make deliberately |
//! | the idempotency token, the source cursor | no | a request-serving invocation is not a durable dispatch and is not bound to a source |
//!
//! # Why this is narrower than `after:mutation` and wider than `before:mutation`
//!
//! `before:mutation` runs **inside** a write and its side effects are not rolled
//! back when a later hook aborts, so it gets no outbound anything. This one decides
//! nothing and rolls nothing back; its constraint is different — it answers a read,
//! so it must not *cause* anything. Hence the reads and the outbound call, and no
//! store, no mail.
//!
//! [`BeforeMutationHost`]: crate::host::before_mutation::BeforeMutationHost

use std::sync::Arc;

use fraiseql_core::security::{GuestQueryBridge, SecurityContext};
use fraiseql_error::{FraiseQLError, Result};

use crate::{
    HostContext,
    host::{HttpResponse, live::HostContextConfig},
    types::{EventPayload, LogLevel},
};

/// The host a `request:query` function runs on: reads, one outbound call, and
/// refusals that say why.
pub struct RequestQueryHost {
    /// The field's resolved arguments, in the shape the runner hands the guest.
    event_payload: EventPayload,
    /// The caller-scoped, read-only GraphQL bridge (#1328) — the same object a
    /// `before:mutation` hook reads through. `None` leaves `query` refusing loudly
    /// rather than answering from nowhere; the engine always supplies one.
    reader:        Option<Arc<dyn GuestQueryBridge>>,
    /// The principal that issued the request, or `None` when it was anonymous.
    principal:     Option<SecurityContext>,
    /// Outbound-HTTP configuration: the SSRF allowlist, the timeouts and the
    /// response ceiling. Carried rather than defaulted so the allowlist an operator
    /// configured is the one this surface enforces.
    http:          HostContextConfig,
    /// The shared outbound client, when the server built one.
    http_client:   Option<Arc<reqwest::Client>>,
}

impl RequestQueryHost {
    /// Build the host for one field invocation.
    #[must_use]
    pub fn new(
        event_payload: EventPayload,
        reader: Option<Arc<dyn GuestQueryBridge>>,
        principal: Option<&SecurityContext>,
        http: HostContextConfig,
    ) -> Self {
        Self {
            event_payload,
            reader,
            principal: principal.cloned(),
            http,
            http_client: None,
        }
    }

    /// Share an existing outbound HTTP client rather than building one per call.
    #[must_use]
    pub fn with_http_client(mut self, client: Arc<reqwest::Client>) -> Self {
        self.http_client = Some(client);
        self
    }

    /// The refusal every op outside the surface answers with.
    ///
    /// One constructor so the diagnosis is uniform and every refusal names the op
    /// and the trigger kind — a guest author reading a stack trace needs to know
    /// *which* surface said no, not just that something did.
    fn outside_the_surface(op: &str) -> FraiseQLError {
        FraiseQLError::Authorization {
            message:  format!(
                "`{op}` is not available to a request:query function: it answers a read, on a \
                 path that is cached and that any caller who can issue the query can reach, so \
                 it must not cause anything. Move the effect to an after:mutation function, \
                 which is durable, retried and dead-lettered."
            ),
            action:   Some(op.to_string()),
            resource: Some("request:query".to_string()),
        }
    }
}

// Reason: `HostContext` is async because other implementations perform I/O. The
// ops this host refuses answer from memory but still have to present the awaited
// signature the trait defines and every guest call site uses.
#[allow(unknown_lints, clippy::unused_async_trait_impl)]
impl HostContext for RequestQueryHost {
    async fn query(
        &self,
        graphql: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let reader = self.reader.as_ref().ok_or_else(|| FraiseQLError::Unsupported {
            message: "no read bridge is wired on this request:query host — the engine supplies \
                      one for every invocation"
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
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<HttpResponse> {
        crate::host::outbound_http::perform(
            &self.http,
            self.http_client.as_ref(),
            method,
            url,
            headers,
            body,
        )
        .await
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
        // An anonymous request has no authenticated identity, and saying so is not
        // the same as `LiveHostContext`'s "nothing was wired" (#803) — here the
        // absence is a fact about the request. Refused rather than answered with a
        // null-filled object, so a function cannot read `sub: null` as a user.
        let context = self.principal.as_ref().ok_or_else(|| FraiseQLError::Unsupported {
            message: "this query was issued anonymously: a request:query function has no \
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

/// Build the event payload a request-serving invocation is handed (#1329).
///
/// Shared so the server and the authoring harness (`fraiseql functions invoke`)
/// hand a guest the same shape. A harness that built its own would be a second copy
/// of the convention, free to drift from the one the server uses — which is the
/// defect `interpret_guest_decision` was extracted to prevent on the write side.
#[must_use]
pub fn request_query_payload(field: &str, arguments: serde_json::Value) -> EventPayload {
    EventPayload {
        trigger_type: "request:query".to_string(),
        // The root query field being answered. One function may back several, and
        // this is how the guest tells which.
        entity:       field.to_string(),
        event_kind:   "request".to_string(),
        data:         arguments,
        timestamp:    chrono::Utc::now(),
    }
}

/// Interpret what a request-serving guest returned as the field's data (#1329).
///
/// The convention is deliberately thin — the return value **is** the data — and
/// extracted anyway, because the one case that needs a decision is the one a second
/// implementation would get wrong: a guest that returns nothing. `None` is `null`,
/// which a nullable field renders as `null` and a non-null field is refused for by
/// the engine, naming the field. The alternative (treating "returned nothing" as an
/// empty object) would render every selected field as null and say nothing.
#[must_use]
pub fn interpret_query_answer(value: Option<serde_json::Value>) -> serde_json::Value {
    value.unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
#[path = "request_query/tests.rs"]
mod tests;
