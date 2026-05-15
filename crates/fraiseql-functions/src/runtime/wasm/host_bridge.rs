//! Object-safe host context bridge for WASM store data.
//!
//! `HostContext` uses `impl Future` return types which are not dyn-compatible.
//! This module provides `DynHostContext`, an object-safe wrapper that erases
//! the future types via `Pin<Box<dyn Future>>`.

use crate::host::HttpResponse;
use fraiseql_error::Result;
use std::future::Future;
use std::pin::Pin;

/// Boxed future type alias for readability.
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Object-safe version of `HostContext` for dynamic dispatch in WASM stores.
///
/// Each async method returns a boxed future instead of `impl Future`, allowing
/// storage as `Arc<dyn DynHostContext>` in `StoreData`.
pub trait DynHostContext: Send + Sync {
    /// Execute a GraphQL query.
    fn query(&self, graphql: &str, variables: serde_json::Value) -> BoxFuture<'_, Result<serde_json::Value>>;

    /// Execute a raw SQL query.
    fn sql_query(&self, sql: &str, params: &[serde_json::Value]) -> BoxFuture<'_, Result<Vec<serde_json::Value>>>;

    /// Make an HTTP request.
    fn http_request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> BoxFuture<'_, Result<HttpResponse>>;

    /// Retrieve an object from storage.
    fn storage_get(&self, bucket: &str, key: &str) -> BoxFuture<'_, Result<Vec<u8>>>;

    /// Store an object to storage.
    fn storage_put(
        &self,
        bucket: &str,
        key: &str,
        body: &[u8],
        content_type: &str,
    ) -> BoxFuture<'_, Result<()>>;

    /// Get the current auth context.
    ///
    /// # Errors
    ///
    /// Returns `Err` if auth context is unavailable.
    fn auth_context(&self) -> Result<serde_json::Value>;

    /// Get an environment variable.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the variable is blocked from access.
    fn env_var(&self, name: &str) -> Result<Option<String>>;

    /// Get the event payload.
    fn event_payload(&self) -> &crate::types::EventPayload;

    /// Log a message.
    fn log(&self, level: crate::types::LogLevel, message: &str);
}

/// Blanket implementation: any `T: HostContext + Send + Sync` can be used as `DynHostContext`.
impl<T: crate::HostContext + Send + Sync> DynHostContext for T {
    fn query(&self, graphql: &str, variables: serde_json::Value) -> BoxFuture<'_, Result<serde_json::Value>> {
        // Own the graphql string so the future doesn't borrow a local reference
        let graphql = graphql.to_string();
        Box::pin(async move { crate::HostContext::query(self, &graphql, variables).await })
    }

    fn sql_query(&self, sql: &str, params: &[serde_json::Value]) -> BoxFuture<'_, Result<Vec<serde_json::Value>>> {
        let sql = sql.to_string();
        let params = params.to_vec();
        Box::pin(async move { crate::HostContext::sql_query(self, &sql, &params).await })
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
        let body = body.map(|b| b.to_vec());
        Box::pin(async move {
            crate::HostContext::http_request(self, &method, &url, &headers, body.as_deref()).await
        })
    }

    fn storage_get(&self, bucket: &str, key: &str) -> BoxFuture<'_, Result<Vec<u8>>> {
        let bucket = bucket.to_string();
        let key = key.to_string();
        Box::pin(async move { crate::HostContext::storage_get(self, &bucket, &key).await })
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
        Box::pin(async move {
            crate::HostContext::storage_put(self, &bucket, &key, &body, &content_type).await
        })
    }

    fn auth_context(&self) -> Result<serde_json::Value> {
        crate::HostContext::auth_context(self)
    }

    fn env_var(&self, name: &str) -> Result<Option<String>> {
        crate::HostContext::env_var(self, name)
    }

    fn event_payload(&self) -> &crate::types::EventPayload {
        crate::HostContext::event_payload(self)
    }

    fn log(&self, level: crate::types::LogLevel, message: &str) {
        crate::HostContext::log(self, level, message);
    }
}
