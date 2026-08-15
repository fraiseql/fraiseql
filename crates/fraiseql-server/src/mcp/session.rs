//! Per-thread working state for MCP agents (#967).
//!
//! An agent that calls three tools in a row is doing one piece of work, and
//! `[mcp] session_state = true` lets the server remember that: each
//! authenticated tool call appends to a thread, and every result carries the
//! thread back so the agent can see what it has already done.
//!
//! # The whole security question is what the thread is keyed on
//!
//! rmcp's streamable-HTTP transport surfaces a session in the `mcp-session-id`
//! header. **That header is client-controlled.** Keying the store on it directly
//! would let any caller read and overwrite any other caller's thread by sending
//! their id — the store is durable and cross-request, so this is not a
//! same-session leak but a persistent one.
//!
//! The store is two-level — `(session_id: Uuid, thread_id: &str)` — and that
//! shape maps exactly onto the problem:
//!
//! * `session_id` is derived **only** from the authenticated principal, as a `UUIDv5` over the
//!   `user_id`. Nothing a client sends contributes to it.
//! * `thread_id` is the client's `mcp-session-id`, verbatim.
//!
//! So a client partitions its **own** threads freely and can address nothing
//! else: reaching another principal's thread would require producing their
//! `user_id`, which is the token's subject and not a header. A collision between
//! two principals is a `UUIDv5` collision, not a guessable string.
//!
//! There is no fallback. Without an authenticated principal there is nothing safe
//! to key on, so continuity is simply off for that call — an anonymous caller
//! sharing one unscoped thread with every other anonymous caller is worse than no
//! continuity at all.

use std::sync::Arc;

use fraiseql_auth::session_state::SessionState;
use fraiseql_core::security::SecurityContext;
use serde_json::json;
use uuid::Uuid;

/// The header rmcp's streamable-HTTP transport carries the client's session in.
const SESSION_HEADER: &str = "mcp-session-id";

/// The store key each thread's rolling call log lives under.
const CONTEXT_KEY: &str = "_context";

/// How many calls a thread remembers.
///
/// A bound rather than a growing list: the entry is re-serialized and re-written
/// on every call, and the store caps an entry's size — an unbounded log would
/// start failing writes partway through a long agent run, which is a worse
/// failure than forgetting the oldest step.
const MAX_REMEMBERED_CALLS: usize = 20;

/// The namespace for the principal → `session_id` derivation.
///
/// A fixed application namespace, so the same user always resolves to the same
/// session across restarts, and a `user_id` that happens to look like another
/// deployment's cannot address its threads.
const PRINCIPAL_NAMESPACE: Uuid = Uuid::from_bytes([
    0x96, 0x70, 0xf7, 0xa1, 0x4e, 0x2b, 0x5c, 0x8d, 0x9a, 0x03, 0x1f, 0x6e, 0x77, 0xb4, 0x22, 0x0e,
]);

/// A resolved place to keep one agent thread's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadKey {
    /// Derived from the authenticated principal alone.
    pub session_id: Uuid,
    /// The client's own `mcp-session-id`.
    pub thread_id:  String,
}

/// Derive the thread key for a request, or `None` when there is nothing safe to
/// key on.
///
/// `None` — meaning "no continuity for this call" — when the caller is
/// unauthenticated or sent no `mcp-session-id`. Both are refusals to guess: an
/// unauthenticated caller has no principal, and a caller with no session header
/// has not asked for a thread.
#[must_use]
pub fn thread_key(
    security_context: Option<&SecurityContext>,
    headers: &axum::http::HeaderMap,
) -> Option<ThreadKey> {
    let ctx = security_context?;
    let thread_id = headers.get(SESSION_HEADER)?.to_str().ok()?.trim();
    if thread_id.is_empty() {
        return None;
    }
    Some(ThreadKey {
        session_id: Uuid::new_v5(&PRINCIPAL_NAMESPACE, ctx.user_id.0.as_bytes()),
        thread_id:  thread_id.to_string(),
    })
}

/// The thread's remembered calls, oldest first.
///
/// A read failure yields an empty history rather than failing the tool call:
/// continuity is an aid, and a store hiccup must not turn a working query into
/// an error. The failure is logged, not swallowed silently.
pub async fn read_context(store: &Arc<SessionState>, key: &ThreadKey) -> Vec<serde_json::Value> {
    match store.get(key.session_id, &key.thread_id, CONTEXT_KEY).await {
        Ok(Some(entry)) => {
            entry.value.get("calls").and_then(|c| c.as_array().cloned()).unwrap_or_default()
        },
        Ok(None) => Vec::new(),
        Err(e) => {
            tracing::warn!(error = %e, "MCP session state unreadable; continuing without it");
            Vec::new()
        },
    }
}

/// Append this call to the thread.
///
/// Records the tool name and the argument **names** — never the values. An
/// argument is caller data that may be a customer identifier, a search term or an
/// embedding; the log exists so an agent can see the shape of what it has done,
/// and copying values into a second durable store is a data-retention decision
/// nobody made.
///
/// A write failure is logged and swallowed for the same reason a read failure is:
/// the tool call itself succeeded, and reporting an error for it would be a lie
/// about what happened to the database.
pub async fn record_call(
    store: &Arc<SessionState>,
    key: &ThreadKey,
    tool_name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    prior: Vec<serde_json::Value>,
) {
    let mut argument_names: Vec<&String> =
        arguments.map(|a| a.keys().collect()).unwrap_or_default();
    argument_names.sort();

    let mut calls = prior;
    calls.push(json!({
        "tool": tool_name,
        "arguments": argument_names,
        "at": chrono::Utc::now().to_rfc3339(),
    }));
    // Keep the most recent window.
    if calls.len() > MAX_REMEMBERED_CALLS {
        let drop = calls.len() - MAX_REMEMBERED_CALLS;
        calls.drain(..drop);
    }

    if let Err(e) = store
        .set(key.session_id, &key.thread_id, CONTEXT_KEY, json!({ "calls": calls }))
        .await
    {
        tracing::warn!(error = %e, "MCP session state not written; the tool call itself succeeded");
    }
}

/// Attach the thread's history to a tool result's `_meta`.
///
/// `_meta` rather than the content block, because the content is the operation's
/// answer: an agent parsing a query result must not have to strip the server's
/// bookkeeping out of it first.
pub fn attach_context(
    result: &mut rmcp::model::CallToolResult,
    key: &ThreadKey,
    calls: &[serde_json::Value],
) {
    let mut meta = result.meta.take().unwrap_or_default();
    meta.insert(
        "fraiseql/session".to_string(),
        json!({
            "threadId": key.thread_id,
            "calls": calls,
        }),
    );
    result.meta = Some(meta);
}

#[cfg(test)]
mod tests;
