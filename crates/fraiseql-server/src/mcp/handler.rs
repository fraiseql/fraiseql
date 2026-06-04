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
    runtime::Executor,
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
/// Holds the compiled schema, executor, and pre-computed tool list.
/// One instance is created per MCP session via the service factory.
pub struct FraiseQLMcpService<A: DatabaseAdapter> {
    schema:         Arc<CompiledSchema>,
    executor:       Arc<Executor<A>>,
    tools:          Vec<Tool>,
    config:         McpConfig,
    oidc_validator: Option<Arc<OidcValidator>>,
}

impl<A: DatabaseAdapter> FraiseQLMcpService<A> {
    /// Create a new MCP service.
    ///
    /// The service starts without an OIDC validator; attach one with
    /// [`with_oidc_validator`](Self::with_oidc_validator) to enable per-request
    /// Bearer-token authentication over the HTTP transport.
    #[must_use]
    pub fn new(schema: Arc<CompiledSchema>, executor: Arc<Executor<A>>, config: McpConfig) -> Self {
        let tools = super::tools::schema_to_tools(&schema, &config);
        Self {
            schema,
            executor,
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
            Ok(user) => Ok(Some(SecurityContext::from_user(&user, request_id))),
            Err(e) => {
                tracing::warn!(error = %e, "MCP token validation failed");
                Err(error_result("Invalid or expired authentication token"))
            },
        }
    }
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
        let require_auth = self.config.require_auth;

        // Pre-extract credentials synchronously: the HTTP transport injects the
        // request parts into the context extensions (the stdio transport does
        // not). Extracting the token here avoids holding the non-`Sync` parts
        // across the token-validation await point.
        let request_id = context.id.to_string();
        let token = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| extract_bearer(&parts.headers));

        async move {
            MCP_TOOL_CALLS_TOTAL.fetch_add(1, Ordering::Relaxed);

            tracing::info!(tool = %tool_name, "MCP tool call");

            let security_context = match self.authenticate(token, request_id).await {
                Ok(ctx) => ctx,
                Err(err_result) => {
                    MCP_TOOL_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
                    return Ok(err_result);
                },
            };

            let result = super::executor::call_tool(
                &tool_name,
                arguments.as_ref(),
                &self.schema,
                &self.executor,
                security_context.as_ref(),
                require_auth,
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
