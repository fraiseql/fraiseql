//! MCP server handler implementation.
//!
//! Implements the rmcp `ServerHandler` trait to expose FraiseQL queries and
//! mutations as MCP tools.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor, schema::CompiledSchema};
use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, ListToolsResult, ServerCapabilities,
        ServerInfo, Tool,
    },
    service::RequestContext,
};

use super::McpConfig;

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
    schema:   Arc<CompiledSchema>,
    executor: Arc<Executor<A>>,
    tools:    Vec<Tool>,
    _config:  McpConfig,
}

impl<A: DatabaseAdapter> FraiseQLMcpService<A> {
    /// Create a new MCP service.
    pub fn new(
        schema: Arc<CompiledSchema>,
        executor: Arc<Executor<A>>,
        config: McpConfig,
    ) -> Self {
        let tools = super::tools::schema_to_tools(&schema, &config);
        Self {
            schema,
            executor,
            tools,
            _config: config,
        }
    }
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> ServerHandler for FraiseQLMcpService<A> {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("FraiseQL GraphQL database — query and mutate via MCP tools".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>>
           + Send
           + '_ {
        let result = ListToolsResult {
            tools: self.tools.clone(),
            next_cursor: None,
            meta: None,
        };
        std::future::ready(Ok(result))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>>
           + Send
           + '_ {
        let tool_name = request.name.to_string();
        let arguments = request.arguments;

        async move {
            MCP_TOOL_CALLS_TOTAL.fetch_add(1, Ordering::Relaxed);

            tracing::info!(tool = %tool_name, "MCP tool call");

            let result = super::executor::call_tool(
                &tool_name,
                arguments.as_ref(),
                &self.schema,
                &self.executor,
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
