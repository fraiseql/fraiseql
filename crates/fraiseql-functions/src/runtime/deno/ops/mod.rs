//! Deno host operation implementations for FraiseQL guest functions.
//!
//! These `#[op2]` ops bridge the guest's `Deno.core.ops.fraiseql_*` calls onto the
//! live [`DynHostContext`] that the
//! runtime stores in the V8 op-state. They are the async-I/O counterpart to the
//! sync `fraiseql_log` op (which lives in `executor.rs` next to the `LogCollector`
//! it shares).
//!
//! The host context is threaded in by
//! [`DenoRuntime::invoke_with_context`](super::DenoRuntime::invoke_with_context).
//! On the sync `invoke` path no host is provided, so these ops fail loud with a
//! clear message rather than silently returning empty data — mirroring the
//! fail-loud contract of the rest of the host surface.

// The `#[op2]` proc-macro emits `#[inline(always)]` on the generated `call` shim
// and requires owned args (`#[string] name: String`) even where we only borrow;
// the async ops hold `Rc<RefCell<OpState>>` and so are single-threaded by
// construction (deno drives them on one thread). None of these are actionable in
// our code — mirror the same allows the `fraiseql_log` op carries in `executor.rs`.
#![allow(
    clippy::inline_always,
    clippy::needless_pass_by_value,
    clippy::future_not_send
)]

#[cfg(test)]
mod tests;

use std::{cell::RefCell, rc::Rc, sync::Arc};

use deno_core::{JsBuffer, OpState, ToJsBuffer, error::AnyError, op2};

use crate::host::dyn_context::DynHostContext;

/// Wrapper stored in the V8 `OpState` carrying the live host context for the
/// async I/O ops. Present only on the `invoke_with_context` path.
pub(crate) struct DenoHostContext(pub Arc<dyn DynHostContext>);

/// Fetch the live host context out of the op-state, cloning the `Arc` so the
/// borrow is released before any `.await` (deno drives async ops on the same
/// single-threaded runtime, so we must never hold the `RefCell` borrow across a
/// suspension point).
fn require_host(state: &OpState) -> Result<Arc<dyn DynHostContext>, AnyError> {
    state.try_borrow::<DenoHostContext>().map(|h| Arc::clone(&h.0)).ok_or_else(|| {
        deno_core::anyhow::anyhow!(
            "host context unavailable: this function was invoked without an I/O host context \
                 (use invoke_with_context)"
        )
    })
}

/// Fetch the host from a shared, ref-counted op-state (async-op form).
fn require_host_rc(state: &Rc<RefCell<OpState>>) -> Result<Arc<dyn DynHostContext>, AnyError> {
    require_host(&state.borrow())
}

/// Convert a host-op error into an `AnyError`, tagging it **permanent** when it is
/// a client error (4xx) — a failure that will not succeed on retry (a denied
/// identity, a rejected recipient, a validation error). The marker travels in the
/// message so the runtime maps it to a 4xx `FraiseQLError`, which durable dispatch
/// dead-letters immediately rather than exhausting retries. A 5xx stays untagged
/// (transient, retried).
fn op_error(error: fraiseql_error::FraiseQLError) -> AnyError {
    if error.is_client_error() {
        deno_core::anyhow::anyhow!("{} {error}", crate::types::PERMANENT_ERROR_MARKER)
    } else {
        AnyError::new(error)
    }
}

/// Serialised HTTP response handed back to the guest.
///
/// `body` is a [`ToJsBuffer`] so it serialises to a `Uint8Array` in JS, matching
/// the `HttpResponse` shape declared in `FRAISEQL_HOST_TYPES`.
#[derive(serde::Serialize)]
struct HttpResponseJs {
    status:  u16,
    headers: Vec<(String, String)>,
    body:    ToJsBuffer,
}

/// Execute a GraphQL query on behalf of the guest.
///
/// `Deno.core.ops.fraiseql_query(graphql, variablesJson) -> resultJson`
#[op2(async)]
#[string]
pub(crate) async fn fraiseql_query(
    state: Rc<RefCell<OpState>>,
    #[string] graphql: String,
    #[string] variables: String,
) -> Result<String, AnyError> {
    let host = require_host_rc(&state)?;
    let vars = parse_json_arg(&variables, "variables")?;
    let result = host.query(&graphql, vars).await.map_err(op_error)?;
    serde_json::to_string(&result)
        .map_err(|e| deno_core::anyhow::anyhow!("failed to serialise query result: {e}"))
}

/// Execute a raw SQL query on behalf of the guest.
///
/// `Deno.core.ops.fraiseql_sql_query(sql, paramsJson) -> rowsJson`
#[op2(async)]
#[string]
pub(crate) async fn fraiseql_sql_query(
    state: Rc<RefCell<OpState>>,
    #[string] sql: String,
    #[string] params: String,
) -> Result<String, AnyError> {
    let host = require_host_rc(&state)?;
    let params_value = parse_json_arg(&params, "params")?;
    let params_vec = match params_value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Null => Vec::new(),
        other => vec![other],
    };
    let rows = host.sql_query(&sql, &params_vec).await.map_err(op_error)?;
    serde_json::to_string(&rows)
        .map_err(|e| deno_core::anyhow::anyhow!("failed to serialise sql result: {e}"))
}

/// Make an outbound HTTP request on behalf of the guest (SSRF-allowlisted by the host).
///
/// `Deno.core.ops.fraiseql_http_request(method, url, headers, body?) -> HttpResponse`
#[op2(async)]
#[serde]
pub(crate) async fn fraiseql_http_request(
    state: Rc<RefCell<OpState>>,
    #[string] method: String,
    #[string] url: String,
    #[serde] headers: Vec<(String, String)>,
    #[buffer] body: Option<JsBuffer>,
) -> Result<HttpResponseJs, AnyError> {
    let host = require_host_rc(&state)?;
    let body_vec: Option<Vec<u8>> = body.map(|b| b.to_vec());
    let resp = host
        .http_request(&method, &url, &headers, body_vec.as_deref())
        .await
        .map_err(op_error)?;
    Ok(HttpResponseJs {
        status:  resp.status,
        headers: resp.headers,
        body:    ToJsBuffer::from(resp.body),
    })
}

/// Retrieve an object from storage on behalf of the guest.
///
/// `Deno.core.ops.fraiseql_storage_get(bucket, key) -> Uint8Array`
#[op2(async)]
#[buffer]
pub(crate) async fn fraiseql_storage_get(
    state: Rc<RefCell<OpState>>,
    #[string] bucket: String,
    #[string] key: String,
) -> Result<Vec<u8>, AnyError> {
    let host = require_host_rc(&state)?;
    host.storage_get(&bucket, &key).await.map_err(op_error)
}

/// Store an object to storage on behalf of the guest.
///
/// `Deno.core.ops.fraiseql_storage_put(bucket, key, body, contentType) -> void`
#[op2(async)]
pub(crate) async fn fraiseql_storage_put(
    state: Rc<RefCell<OpState>>,
    #[string] bucket: String,
    #[string] key: String,
    #[buffer] body: JsBuffer,
    #[string] content_type: String,
) -> Result<(), AnyError> {
    let host = require_host_rc(&state)?;
    let body = body.to_vec();
    host.storage_put(&bucket, &key, &body, &content_type).await.map_err(op_error)
}

/// Send an email on behalf of the guest (the `from` is host-owned).
///
/// `Deno.core.ops.fraiseql_send_email(requestJson) -> responseJson`
#[op2(async)]
#[string]
pub(crate) async fn fraiseql_send_email(
    state: Rc<RefCell<OpState>>,
    #[string] request: String,
) -> Result<String, AnyError> {
    let host = require_host_rc(&state)?;
    let req: crate::outbound::SendEmailRequest = serde_json::from_str(&request)
        .map_err(|e| deno_core::anyhow::anyhow!("invalid send_email request JSON: {e}"))?;
    let response = host.send_email(&req).await.map_err(op_error)?;
    serde_json::to_string(&response)
        .map_err(|e| deno_core::anyhow::anyhow!("failed to serialise send_email response: {e}"))
}

/// Return the current auth context as a JSON string (sync).
///
/// `Deno.core.ops.fraiseql_auth_context() -> authJson`
#[op2]
#[string]
pub(crate) fn fraiseql_auth_context(state: &OpState) -> Result<String, AnyError> {
    let host = require_host(state)?;
    let ctx = host.auth_context().map_err(op_error)?;
    serde_json::to_string(&ctx)
        .map_err(|e| deno_core::anyhow::anyhow!("failed to serialise auth context: {e}"))
}

/// Return an environment variable value, or `null` if unset (sync).
///
/// `Deno.core.ops.fraiseql_env_var(name) -> string | null`
#[op2]
#[string]
pub(crate) fn fraiseql_env_var(
    state: &OpState,
    #[string] name: String,
) -> Result<Option<String>, AnyError> {
    let host = require_host(state)?;
    host.env_var(&name).map_err(op_error)
}

/// Parse a JSON-string op argument, treating an empty string as an empty object.
fn parse_json_arg(raw: &str, what: &str) -> Result<serde_json::Value, AnyError> {
    if raw.trim().is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(raw).map_err(|e| deno_core::anyhow::anyhow!("invalid {what} JSON: {e}"))
}
