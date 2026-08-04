//! Pins a host context's async I/O to a long-lived owner runtime (#970).
//!
//! Every Deno invocation executes on a dedicated thread with its own throwaway
//! `current_thread` Tokio runtime. A host-op future polled there registers any
//! network resource it *creates* — a fresh sqlx pool connection, a reqwest
//! keep-alive socket — with that runtime's I/O reactor, which dies with the
//! invocation thread. The resource itself outlives the invocation (it returns
//! to a shared pool), so the next user awaits a wakeup from a dead reactor:
//! every second scheduled firing timed out, and guest-created connections
//! poisoned the server's main DB pool.
//!
//! [`RuntimePinnedHost`] wraps an [`Arc<dyn DynHostContext>`] together with the
//! **owner** runtime's [`Handle`](tokio::runtime::Handle) (captured at dispatch
//! time, on the long-lived server runtime) and executes every async host op via
//! `handle.spawn(..)`. The guest's throwaway runtime then only ever awaits a
//! [`JoinHandle`](tokio::task::JoinHandle) — cross-runtime-safe — while all
//! real I/O lives on the reactor that outlives every invocation.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use fraiseql_error::Result;

use super::{
    HttpResponse,
    dyn_context::{BoxFuture, DynHostContext},
};

/// A [`DynHostContext`] decorator that executes every async op on the owner
/// runtime it was constructed with. See the module docs for why (#970).
pub struct RuntimePinnedHost {
    inner:  Arc<dyn DynHostContext>,
    handle: tokio::runtime::Handle,
}

impl RuntimePinnedHost {
    /// Wrap `inner` so all its async ops run on the runtime behind `handle`.
    ///
    /// Capture `handle` on the long-lived runtime that owns the process's
    /// shared pools (in the server: the main runtime the dispatch paths run
    /// on), never inside a guest invocation.
    #[must_use]
    pub fn new(inner: Arc<dyn DynHostContext>, handle: tokio::runtime::Handle) -> Self {
        Self { inner, handle }
    }

    /// Run one host op on the owner runtime and await its result from wherever
    /// the caller is polling. A join failure (the op panicked, or the owner
    /// runtime is shutting down) is a loud internal error, never a silent drop.
    fn pinned<T: Send + 'static>(
        &self,
        run: impl FnOnce(Arc<dyn DynHostContext>) -> BoxFuture<'static, Result<T>> + Send + 'static,
    ) -> BoxFuture<'_, Result<T>> {
        let inner = Arc::clone(&self.inner);
        let handle = self.handle.clone();
        Box::pin(async move {
            handle
                .spawn(run(inner))
                .await
                .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                    message: format!("host op failed on the owner runtime: {e}"),
                    source:  None,
                })?
        })
    }
}

impl DynHostContext for RuntimePinnedHost {
    fn query(
        &self,
        graphql: &str,
        variables: serde_json::Value,
    ) -> BoxFuture<'_, Result<serde_json::Value>> {
        let graphql = graphql.to_string();
        self.pinned(move |inner| Box::pin(async move { inner.query(&graphql, variables).await }))
    }

    fn sql_query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> BoxFuture<'_, Result<Vec<serde_json::Value>>> {
        let sql = sql.to_string();
        let params = params.to_vec();
        self.pinned(move |inner| Box::pin(async move { inner.sql_query(&sql, &params).await }))
    }

    fn http_request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> BoxFuture<'_, Result<HttpResponse>> {
        let method = method.to_string();
        let url = url.to_string();
        let headers = headers.to_vec();
        let body = body.map(<[u8]>::to_vec);
        self.pinned(move |inner| {
            Box::pin(
                async move { inner.http_request(&method, &url, &headers, body.as_deref()).await },
            )
        })
    }

    fn storage_get(&self, bucket: &str, key: &str) -> BoxFuture<'_, Result<Vec<u8>>> {
        let bucket = bucket.to_string();
        let key = key.to_string();
        self.pinned(move |inner| Box::pin(async move { inner.storage_get(&bucket, &key).await }))
    }

    fn storage_put(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        content_type: &str,
    ) -> BoxFuture<'_, Result<()>> {
        let bucket = bucket.to_string();
        let key = key.to_string();
        let body = body.to_vec();
        let content_type = content_type.to_string();
        self.pinned(move |inner| {
            Box::pin(async move { inner.storage_put(&bucket, &key, &body, &content_type).await })
        })
    }

    fn send_email<'a>(
        &'a self,
        request: &'a crate::outbound::SendEmailRequest,
    ) -> BoxFuture<'a, Result<crate::outbound::SendEmailResponse>> {
        let request = request.clone();
        self.pinned(move |inner| Box::pin(async move { inner.send_email(&request).await }))
    }

    fn auth_context(&self) -> Result<serde_json::Value> {
        self.inner.auth_context()
    }

    fn env_var(&self, name: &str) -> Result<Option<String>> {
        self.inner.env_var(name)
    }

    fn event_payload(&self) -> &crate::types::EventPayload {
        self.inner.event_payload()
    }

    fn log(&self, level: crate::types::LogLevel, message: &str) {
        self.inner.log(level, message);
    }

    fn idempotency_token(&self) -> Option<String> {
        self.inner.idempotency_token()
    }

    fn cursor(&self) -> BoxFuture<'_, Result<Option<serde_json::Value>>> {
        self.pinned(move |inner| Box::pin(async move { inner.cursor().await }))
    }

    fn advance_cursor(&self, value: serde_json::Value) -> BoxFuture<'_, Result<()>> {
        self.pinned(move |inner| Box::pin(async move { inner.advance_cursor(value).await }))
    }
}
