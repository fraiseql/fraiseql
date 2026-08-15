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
    security::{AuthMiddleware, AuthRequest, AuthenticatedUser, OidcValidator, SecurityContext},
};
use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
        ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
        ServerCapabilities, ServerInfo, Tool,
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

/// The token validator MCP authenticates Bearer credentials against — the same
/// two auth modes `/graphql` accepts (#376 auth parity).
///
/// MCP used to accept only an [`OidcValidator`], so an HS256-only deployment
/// (`[auth_hs256]`, the integration-testing / service-to-service mode) could
/// never authenticate an MCP call and `require_auth = true` refused to mount
/// the endpoint at all.
pub enum McpTokenValidator {
    /// OIDC discovery + JWKS validation (`[auth]`), as on `/graphql`.
    Oidc(Arc<OidcValidator>),
    /// Local HS256 shared-secret validation (`[auth_hs256]`), as on `/graphql`.
    Hs256(Arc<AuthMiddleware>),
}

impl std::fmt::Debug for McpTokenValidator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Oidc(_) => f.write_str("McpTokenValidator::Oidc"),
            Self::Hs256(_) => f.write_str("McpTokenValidator::Hs256"),
        }
    }
}

impl McpTokenValidator {
    /// Validate a bare Bearer credential into an [`AuthenticatedUser`].
    ///
    /// # Errors
    ///
    /// Returns the validator's error rendered as a string (logged, never sent
    /// to the client verbatim).
    async fn validate(&self, token: &str) -> Result<AuthenticatedUser, String> {
        match self {
            Self::Oidc(v) => v.validate_token(token).await.map_err(|e| e.to_string()),
            Self::Hs256(m) => {
                // AuthMiddleware validates a whole Authorization header, which
                // is where this token came from.
                let req = AuthRequest::new(Some(format!("Bearer {token}")));
                m.validate_request(&req).map_err(|e| e.to_string())
            },
        }
    }
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
    state:         AppState<A>,
    schema:        Arc<CompiledSchema>,
    tools:         Vec<Tool>,
    config:        McpConfig,
    validator:     Option<McpTokenValidator>,
    /// The `[session_state]` store, when the deployment configured one (#967).
    session_state: Option<Arc<fraiseql_auth::session_state::SessionState>>,
}

impl<A: DatabaseAdapter> FraiseQLMcpService<A> {
    /// Create a new MCP service over the server's state.
    ///
    /// The advertised tool list is computed from the state's current (default)
    /// schema. That same schema resolves every incoming tool call, so an operation
    /// can only be reached over MCP if it was advertised — even when a per-tenant
    /// executor carries a different compiled schema.
    ///
    /// The service starts without a token validator; attach one with
    /// [`with_token_validator`](Self::with_token_validator) to enable
    /// per-request Bearer-token authentication over the HTTP transport.
    #[must_use]
    pub fn new(state: AppState<A>, config: McpConfig) -> Self {
        let schema = Arc::new(state.executor().schema().clone());
        let tools = super::tools::schema_to_tools(&schema, &config);
        Self {
            state,
            schema,
            tools,
            config,
            validator: None,
            session_state: None,
        }
    }

    /// Bind the `[session_state]` store, enabling per-thread continuity (#967).
    ///
    /// Continuity is active only when this is `Some` **and** `[mcp]
    /// session_state = true`: the store is a deployment's, the flag is the
    /// operator's decision to use it for MCP, and neither implies the other. A
    /// server with the store configured but the flag unset behaves exactly as
    /// before.
    #[must_use]
    pub fn with_session_state(
        mut self,
        store: Option<Arc<fraiseql_auth::session_state::SessionState>>,
    ) -> Self {
        self.session_state = store;
        self
    }

    /// Attach the token validator used to authenticate MCP tool calls — OIDC or
    /// HS256, whichever `/graphql` uses (#376 auth parity).
    ///
    /// When present, a `Bearer` token carried by the HTTP transport is validated
    /// and turned into a [`SecurityContext`] so RLS and `@inject` parameters are
    /// applied. The stdio transport carries no per-request credentials, so it is
    /// always governed by the fail-closed policy in
    /// [`executor::call_tool`](super::executor::call_tool).
    #[must_use]
    pub fn with_token_validator(mut self, validator: Option<McpTokenValidator>) -> Self {
        self.validator = validator;
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
        let Some(validator) = self.validator.as_ref() else {
            return Ok(None); // Auth not configured — anonymous; gate decides.
        };
        let Some(token) = token else {
            return Ok(None); // No Bearer credential — anonymous; gate decides.
        };

        // The transport stamp (#376) rides the framework-reserved `fraiseql.`
        // attribute namespace, which the shared builder strips from token
        // claims — so it is set here, by the transport itself, and cannot be
        // forged by a caller. The mutation runner records it into the
        // change-log row's `extra_metadata.transport`, making MCP-originated
        // writes queryable in the audit trail.
        match validator.validate(&token).await {
            Ok(user) => Ok(Some(
                crate::extractors::build_security_context(&user, request_id).with_transport("mcp"),
            )),
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

        // Per-thread continuity (#967), when the operator asked for it and the
        // deployment configured a store. Resolved *after* authentication, because
        // the thread is keyed on the authenticated principal and on nothing the
        // client sent — see `session::thread_key`.
        let thread = self
            .config
            .session_state
            .then_some(self.session_state.as_ref())
            .flatten()
            .and_then(|store| {
                super::session::thread_key(security_context, headers).map(|key| (store, key))
            });

        let prior = match thread {
            Some((store, ref key)) => super::session::read_context(store, key).await,
            None => Vec::new(),
        };

        let mut result = super::executor::call_tool(
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
        .await;

        if let Some((store, key)) = thread {
            // Only a call that happened is remembered. A refused or failed call
            // did nothing to the database, and a thread that recorded it would
            // tell the agent it had already done work it has not done.
            if result.is_error != Some(true) {
                super::session::record_call(store, &key, tool_name, arguments, prior.clone()).await;
                let mut calls = prior;
                calls.push(serde_json::json!({ "tool": tool_name }));
                super::session::attach_context(&mut result, &key, &calls);
            }
        }

        result
    }
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> FraiseQLMcpService<A> {
    /// Read a Resource, given credentials already in hand (#967).
    ///
    /// The seam under [`ServerHandler::read_resource`], for the same reason
    /// [`call_tool_authenticated`](Self::call_tool_authenticated) is one:
    /// [`RequestContext`] needs a live `Peer` and cannot be constructed in a
    /// test, and what must be tested is precisely that this path and the tool
    /// path see the same identity, the same tenant and the same rows.
    ///
    /// It delegates to `call_tool_authenticated` rather than reimplementing it,
    /// so RLS parity is **structural**: there is one execution path, and this
    /// function contributes only a URI parse and a result shape.
    ///
    /// # Errors
    ///
    /// [`rmcp::ErrorData::resource_not_found`] for a URI that is not
    /// `fraiseql://query/{name}`, and `invalid_request` when the underlying tool
    /// call was refused — a refusal must not come back as a successful read whose
    /// body says "access denied" (#749).
    #[doc(hidden)] // Internal-pub: the testable seam under `ServerHandler::read_resource`.
    pub async fn read_resource_authenticated(
        &self,
        uri: &str,
        token: Option<String>,
        request_id: String,
        headers: &axum::http::HeaderMap,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        let Some(name) = super::resources::query_name_from_uri(uri) else {
            return Err(rmcp::ErrorData::resource_not_found(
                format!("Unknown resource URI: {uri}"),
                None,
            ));
        };

        let result = self.call_tool_authenticated(name, None, token, request_id, headers).await;

        if result.is_error == Some(true) {
            let detail = first_text(&result).unwrap_or_else(|| "resource read refused".to_string());
            return Err(rmcp::ErrorData::invalid_request(detail, None));
        }

        Ok(ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
            uri:       uri.to_string(),
            mime_type: Some("application/json".to_string()),
            text:      first_text(&result).unwrap_or_default(),
            meta:      None,
        }]))
    }
}

/// The text of a tool result's first content block, if it has one.
///
/// `CallToolResult` carries `Option<Vec<Content>>`, and both the refusal path and
/// the success path of `read_resource` need the same string out of it — a second
/// extraction is a second place for "no content" to mean something different.
fn first_text(result: &CallToolResult) -> Option<String> {
    result.content.first()?.as_text().map(|t| t.text.clone())
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
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                // #967: Resources and Prompts are advertised only because they are
                // implemented below. A capability announced without a handler
                // answers `method_not_found` to a client that took it at its word.
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_instructions(
            "FraiseQL GraphQL database — query and mutate via MCP tools. Each readable query is \
             also a Resource at fraiseql://query/{name}; reading one runs the same operation \
             under the same authentication and tenant scoping as calling its tool.",
        )
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

    /// Every exposed query, advertised as a readable Resource (#967).
    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, rmcp::ErrorData>> + Send + '_
    {
        let result = ListResourcesResult::with_all_items(super::resources::schema_to_resources(
            &self.schema,
            &self.config,
        ));
        std::future::ready(Ok(result))
    }

    /// The parameterised reads — today, similarity search over a vector-backed
    /// query (#386/#967).
    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, rmcp::ErrorData>>
    + Send
    + '_ {
        let result = ListResourceTemplatesResult::with_all_items(
            super::resources::schema_to_resource_templates(&self.schema, &self.config),
        );
        std::future::ready(Ok(result))
    }

    /// Read a Resource — i.e. run the query it names.
    ///
    /// **This routes through `call_tool_authenticated`, and that is the whole
    /// security design.** A Resource read is an execution against the database
    /// with a caller's identity attached; giving it its own path would mean a
    /// second implementation of authentication, tenant resolution, quota
    /// charging, the allowlist and every executor gate — and the one that drifted
    /// would be the one nobody was testing (#808, and #1030 for the shape where a
    /// second door around a gate is the bug). Routing through the tool seam makes
    /// RLS parity structural rather than asserted: there is only one path.
    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send + '_
    {
        // Same synchronous pre-extraction as `call_tool`: the HTTP request parts
        // are not `Sync` and must not be held across the validation await.
        let request_id = context.id.to_string();
        let parts = context.extensions.get::<http::request::Parts>();
        let token = parts.and_then(|parts| extract_bearer(&parts.headers));
        let headers = parts.map(|parts| parts.headers.clone()).unwrap_or_default();
        let uri = request.uri;

        async move { self.read_resource_authenticated(&uri, token, request_id, &headers).await }
    }

    /// Every exposed operation, advertised as a Prompt (#967).
    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, rmcp::ErrorData>> + Send + '_
    {
        let result = ListPromptsResult::with_all_items(super::resources::schema_to_prompts(
            &self.schema,
            &self.config,
        ));
        std::future::ready(Ok(result))
    }

    /// Render one Prompt.
    ///
    /// No database access and no identity: a prompt is a sentence describing an
    /// operation, and getting it changes nothing. What the agent may then *do*
    /// with that sentence is decided when it calls the tool, by the gate that
    /// already exists. The allowlist is still consulted, so an operation an
    /// operator withheld is not described either.
    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, rmcp::ErrorData>> + Send + '_
    {
        let rendered = super::resources::render_prompt(
            &request.name,
            request.arguments.as_ref(),
            &self.schema,
            &self.config,
        );
        std::future::ready(match rendered {
            Some((description, messages)) => {
                Ok(GetPromptResult::new(messages).with_description(description))
            },
            None => Err(rmcp::ErrorData::invalid_params(
                format!("Unknown prompt: {}", request.name),
                None,
            )),
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|t| t.name == name).cloned()
    }
}
