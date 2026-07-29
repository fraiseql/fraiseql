//! MCP server handler implementation.
//!
//! Implements the rmcp `ServerHandler` trait to expose FraiseQL queries and
//! mutations as MCP tools.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    schema::CompiledSchema,
    security::{OidcValidator, SecurityContext},
};
use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, ListToolsResult, ServerCapabilities, ServerInfo,
        Tool,
    },
    service::RequestContext,
};

use super::{McpConfig, executor::error_result};
use crate::routes::graphql::AppState;

/// Extract a Bearer token from an HTTP `Authorization` header.
///
/// Returns the credential following `"Bearer "` for a well-formed
/// `Authorization: Bearer <token>` header, or `None` when the header is absent,
/// non-UTF-8, not a Bearer credential, or empty.
pub(crate) fn extract_bearer(headers: &http::HeaderMap) -> Option<String> {
    let value = headers.get(http::header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Global counter for MCP tool calls (used by metrics endpoint).
pub static MCP_TOOL_CALLS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Global counter for MCP tool call errors.
pub static MCP_TOOL_ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Returns the total MCP tool call count for metrics.
pub fn mcp_tool_calls_total() -> u64 {
    MCP_TOOL_CALLS_TOTAL.load(Ordering::Relaxed)
}

/// Returns the total MCP tool error count for metrics.
pub fn mcp_tool_errors_total() -> u64 {
    MCP_TOOL_ERRORS_TOTAL.load(Ordering::Relaxed)
}

/// FraiseQL MCP service handler.
///
/// Holds the server state, the compiled schema and the pre-computed tool list.
/// One instance is created per MCP session via the service factory.
///
/// The service holds the whole [`AppState`], not a bare executor: MCP is a
/// transport onto the same runtime as `/graphql`, so it must reach the same tenant
/// registry, domain registry and error sanitizer. Capturing one executor at
/// session construction is precisely what made every MCP call run on the default
/// tenant's database and let a suspended tenant keep reading (#858).
pub struct FraiseQLMcpService<A: DatabaseAdapter> {
    state:          AppState<A>,
    schema:         Arc<CompiledSchema>,
    tools:          Vec<Tool>,
    config:         McpConfig,
    oidc_validator: Option<Arc<OidcValidator>>,
}

impl<A: DatabaseAdapter> FraiseQLMcpService<A> {
    /// Create a new MCP service over the server's state.
    ///
    /// The advertised tool list is computed from the state's current (default)
    /// schema. That same schema resolves every incoming tool call, so an operation
    /// can only be reached over MCP if it was advertised — even when a per-tenant
    /// executor carries a different compiled schema.
    ///
    /// The service starts without an OIDC validator; attach one with
    /// [`with_oidc_validator`](Self::with_oidc_validator) to enable per-request
    /// Bearer-token authentication over the HTTP transport.
    #[must_use]
    pub fn new(state: AppState<A>, config: McpConfig) -> Self {
        let schema = Arc::new(state.executor().schema().clone());
        let tools = super::tools::schema_to_tools(&schema, &config);
        Self {
            state,
            schema,
            tools,
            config,
            oidc_validator: None,
        }
    }

    /// Attach an OIDC validator used to authenticate MCP tool calls.
    ///
    /// When present, a `Bearer` token carried by the HTTP transport is validated
    /// and turned into a [`SecurityContext`] so RLS and `@inject` parameters are
    /// applied. The stdio transport carries no per-request credentials, so it is
    /// always governed by the fail-closed policy in
    /// [`executor::call_tool`](super::executor::call_tool).
    #[must_use]
    pub fn with_oidc_validator(mut self, validator: Option<Arc<OidcValidator>>) -> Self {
        self.oidc_validator = validator;
        self
    }

    /// Validate a Bearer token (if any) into an optional [`SecurityContext`].
    ///
    /// The caller pre-extracts `token` and `request_id` from the transport
    /// request *before* any `.await`, so the non-`Sync` HTTP request parts need
    /// not be held across the validation await point.
    ///
    /// The context is built by the same function the `/graphql` extractor uses, so
    /// the JWT's `org_id` becomes `tenant_id` and its extra claims become
    /// `attributes` on this transport too (#858).
    ///
    /// - `Ok(None)` — no validator configured, or no Bearer token present (anonymous). The
    ///   fail-closed gate in `executor::call_tool` still refuses the call when RLS or
    ///   `require_auth` demand a context.
    /// - `Ok(Some(ctx))` — the token validated successfully.
    /// - `Err(result)` — a token was present but invalid or expired.
    async fn authenticate(
        &self,
        token: Option<String>,
        request_id: String,
    ) -> Result<Option<SecurityContext>, CallToolResult> {
        let Some(validator) = self.oidc_validator.as_ref() else {
            return Ok(None); // Auth not configured — anonymous; gate decides.
        };
        let Some(token) = token else {
            return Ok(None); // No Bearer credential — anonymous; gate decides.
        };

        match validator.validate_token(&token).await {
            Ok(user) => Ok(Some(crate::extractors::build_security_context(&user, request_id))),
            Err(e) => {
                tracing::warn!(error = %e, "MCP token validation failed");
                Err(error_result("Invalid or expired authentication token"))
            },
        }
    }

    /// Authenticate, resolve the tenant, and run the tool call on that tenant's
    /// executor.
    ///
    /// This is everything [`ServerHandler::call_tool`] does once the transport's
    /// credentials and headers are in hand. It is exposed because
    /// [`RequestContext`] needs a live `Peer` and cannot be constructed in a test,
    /// and the dispatch this function performs is exactly what must be tested
    /// against a real two-tenant deployment.
    ///
    /// Tenant resolution and dispatch go through the same seam as the `/graphql`
    /// handler, so an unregistered tenant key is refused, a suspended tenant is
    /// refused, and the tenant's concurrency and per-second quotas are charged —
    /// none of which happened when this transport ran everything on the executor
    /// it captured at session construction (#858).
    #[doc(hidden)] // Internal-pub: the testable seam under `ServerHandler::call_tool`.
    pub async fn call_tool_authenticated(
        &self,
        tool_name: &str,
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
        token: Option<String>,
        request_id: String,
        headers: &axum::http::HeaderMap,
    ) -> CallToolResult
    where
        A: Clone + Send + Sync + 'static,
    {
        use crate::routes::graphql::tenant_dispatch;

        let security_context = match self.authenticate(token, request_id).await {
            Ok(ctx) => ctx,
            Err(err_result) => return err_result,
        };
        let security_context = security_context.as_ref();

        let sanitizer = &self.state.error_sanitizer;

        let tenant_key =
            match tenant_dispatch::resolve_tenant_key(&self.state, security_context, headers) {
                Ok(key) => key,
                Err(e) => return error_result(&sanitize(sanitizer, &e)),
            };

        let dispatch = match tenant_dispatch::dispatch_to_tenant(&self.state, tenant_key.as_deref())
        {
            Ok(d) => d,
            Err(e) => return error_result(&sanitize(sanitizer, &e)),
        };

        super::executor::call_tool(
            tool_name,
            arguments,
            &super::executor::McpCallContext {
                schema: &self.schema,
                executor: &dispatch.executor,
                config: &self.config,
                security_context,
                error_sanitizer: sanitizer,
            },
        )
        .await
    }
}

/// Render an error for the MCP client through the configured sanitizer.
///
/// `/graphql` runs every execution error through
/// [`ErrorSanitizer`](crate::config::error_sanitization::ErrorSanitizer) — the
/// documented "hide implementation details in error messages" control. The MCP
/// path returned `e.to_string()` raw, so a `FraiseQLError::Database` handed an AI
/// agent the driver message and SQLSTATE verbatim, internal view names included
/// (#875, item 1).
pub(crate) fn sanitize(
    sanitizer: &crate::config::error_sanitization::ErrorSanitizer,
    error: &fraiseql_error::FraiseQLError,
) -> String {
    sanitizer
        .sanitize(crate::error::GraphQLError::from_fraiseql_error(error))
        .message
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> ServerHandler for FraiseQLMcpService<A> {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("FraiseQL GraphQL database — query and mutate via MCP tools")
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_
    {
        let result = ListToolsResult {
            tools:       self.tools.clone(),
            next_cursor: None,
            meta:        None,
        };
        std::future::ready(Ok(result))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_
    {
        let tool_name = request.name.to_string();
        let arguments = request.arguments;

        // Pre-extract credentials and headers synchronously: the HTTP transport
        // injects the request parts into the context extensions (the stdio
        // transport does not). Extracting them here avoids holding the non-`Sync`
        // parts across the token-validation await point. The stdio transport
        // carries no headers, so tenant resolution there falls back to the JWT
        // claim alone — which is the only trustworthy source anyway.
        let request_id = context.id.to_string();
        let parts = context.extensions.get::<http::request::Parts>();
        let token = parts.and_then(|parts| extract_bearer(&parts.headers));
        let headers = parts.map(|parts| parts.headers.clone()).unwrap_or_default();

        async move {
            MCP_TOOL_CALLS_TOTAL.fetch_add(1, Ordering::Relaxed);

            tracing::info!(tool = %tool_name, "MCP tool call");

            let result = self
                .call_tool_authenticated(
                    &tool_name,
                    arguments.as_ref(),
                    token,
                    request_id,
                    &headers,
                )
                .await;

            if result.is_error == Some(true) {
                MCP_TOOL_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
            }

            Ok(result)
        }
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|t| t.name == name).cloned()
    }
}
