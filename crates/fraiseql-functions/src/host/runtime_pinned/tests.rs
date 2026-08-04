//! Tests for [`RuntimePinnedHost`] — async ops must execute on the owner
//! runtime, sync accessors must pass through.

#![allow(clippy::unwrap_used)] // Reason: test module

use std::sync::Arc;

use super::RuntimePinnedHost;
use crate::host::dyn_context::{BoxFuture, DynHostContext};

/// A probe host whose `sql_query` records the name of the thread it was polled
/// on — the observable difference between "ran on the owner runtime's worker"
/// and "ran wherever the caller polled".
struct ThreadProbeHost {
    payload: crate::types::EventPayload,
    seen:    std::sync::Mutex<Vec<String>>,
}

impl DynHostContext for ThreadProbeHost {
    fn query(
        &self,
        _graphql: &str,
        _variables: serde_json::Value,
    ) -> BoxFuture<'_, fraiseql_error::Result<serde_json::Value>> {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn sql_query(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> BoxFuture<'_, fraiseql_error::Result<Vec<serde_json::Value>>> {
        Box::pin(async move {
            let name = std::thread::current().name().unwrap_or("<unnamed>").to_string();
            self.seen.lock().unwrap().push(name);
            Ok(vec![])
        })
    }

    fn http_request(
        &self,
        _method: &str,
        _url: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> BoxFuture<'_, fraiseql_error::Result<crate::host::HttpResponse>> {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn storage_get(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> BoxFuture<'_, fraiseql_error::Result<Vec<u8>>> {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn storage_put(
        &self,
        _bucket: &str,
        _key: &str,
        _body: &[u8],
        _content_type: &str,
    ) -> BoxFuture<'_, fraiseql_error::Result<()>> {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn send_email<'a>(
        &'a self,
        _request: &'a crate::outbound::SendEmailRequest,
    ) -> BoxFuture<'a, fraiseql_error::Result<crate::outbound::SendEmailResponse>> {
        Box::pin(async { Err(fraiseql_error::FraiseQLError::validation("not wired")) })
    }

    fn auth_context(&self) -> fraiseql_error::Result<serde_json::Value> {
        Ok(serde_json::json!({ "probe": true }))
    }

    fn env_var(&self, _name: &str) -> fraiseql_error::Result<Option<String>> {
        Ok(Some("probe".to_string()))
    }

    fn event_payload(&self) -> &crate::types::EventPayload {
        &self.payload
    }

    fn log(&self, _level: crate::types::LogLevel, _message: &str) {}

    fn idempotency_token(&self) -> Option<String> {
        Some("probe-token".to_string())
    }
}

fn probe_payload() -> crate::types::EventPayload {
    crate::types::EventPayload {
        trigger_type: "test".to_string(),
        entity:       "Test".to_string(),
        event_kind:   "created".to_string(),
        data:         serde_json::json!({}),
        timestamp:    chrono::Utc::now(),
    }
}

/// An async op invoked from a foreign throwaway runtime must execute on the
/// owner runtime's (named) worker threads — the #970 guarantee.
#[test]
fn async_ops_execute_on_the_owner_runtime() {
    let owner = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_name("pinned-owner")
        .enable_all()
        .build()
        .unwrap();
    let probe = Arc::new(ThreadProbeHost {
        payload: probe_payload(),
        seen:    std::sync::Mutex::new(Vec::new()),
    });
    let pinned = RuntimePinnedHost::new(probe.clone(), owner.handle().clone());

    // Poll from a SEPARATE throwaway runtime, mimicking a guest invocation.
    let throwaway = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    throwaway.block_on(async { pinned.sql_query("probe", &[]).await }).unwrap();

    let seen = probe.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "the op ran exactly once");
    assert_eq!(
        seen[0], "pinned-owner",
        "the op must run on the owner runtime's worker, not the polling runtime"
    );
}

/// Sync accessors pass straight through to the inner host.
#[test]
fn sync_accessors_pass_through() {
    let owner = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let probe = Arc::new(ThreadProbeHost {
        payload: probe_payload(),
        seen:    std::sync::Mutex::new(Vec::new()),
    });
    let pinned = RuntimePinnedHost::new(probe, owner.handle().clone());

    assert_eq!(pinned.auth_context().unwrap(), serde_json::json!({ "probe": true }));
    assert_eq!(pinned.env_var("X").unwrap().as_deref(), Some("probe"));
    assert_eq!(pinned.idempotency_token().as_deref(), Some("probe-token"));
    assert_eq!(pinned.event_payload().entity, "Test");
}
