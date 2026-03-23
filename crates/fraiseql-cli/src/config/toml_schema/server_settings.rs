//! Server settings configuration for TOML schema (validation, debug, MCP, REST).

use fraiseql_core::schema::DeleteResponse;
use serde::{Deserialize, Serialize};

/// MCP (Model Context Protocol) server configuration.
///
/// Enables AI/LLM tools to interact with FraiseQL queries and mutations
/// through the standardized Model Context Protocol.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfig {
    /// Enable MCP server endpoint.
    pub enabled:      bool,
    /// Transport mode: "http", "stdio", or "both".
    pub transport:    String,
    /// HTTP path for MCP endpoint (e.g., "/mcp").
    pub path:         String,
    /// Require authentication for MCP requests.
    pub require_auth: bool,
    /// Whitelist of query/mutation names to expose (empty = all).
    #[serde(default)]
    pub include:      Vec<String>,
    /// Blacklist of query/mutation names to hide.
    #[serde(default)]
    pub exclude:      Vec<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled:      false,
            transport:    "http".to_string(),
            path:         "/mcp".to_string(),
            require_auth: true,
            include:      Vec::new(),
            exclude:      Vec::new(),
        }
    }
}

/// Query validation limits (depth and complexity).
///
/// ```toml
/// [validation]
/// max_query_depth = 10
/// max_query_complexity = 100
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ValidationConfig {
    /// Maximum allowed query nesting depth. `None` uses the server default (10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_query_depth: Option<u32>,

    /// Maximum allowed query complexity score. `None` uses the server default (100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_query_complexity: Option<u32>,
}

/// Debug/development configuration.
///
/// Controls features that should only be enabled during development or
/// in trusted environments. All flags default to off.
///
/// ```toml
/// [debug]
/// enabled = true
/// database_explain = true
/// expose_sql = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DebugConfig {
    /// Master switch — all debug features require this to be `true`.
    pub enabled: bool,

    /// When `true`, the explain endpoint will also run `EXPLAIN` against the
    /// database and include the query plan in the response.
    pub database_explain: bool,

    /// When `true`, the explain endpoint includes the generated SQL in the
    /// response. Defaults to `true` (SQL is shown even without
    /// `database_explain`).
    pub expose_sql: bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            enabled:          false,
            database_explain: false,
            expose_sql:       true,
        }
    }
}

/// Development mode configuration (TOML authoring struct).
///
/// When enabled, the server injects default JWT claims for unauthenticated
/// requests — removing the need for a real OIDC setup during local development.
///
/// ```toml
/// [dev]
/// enabled = true
/// default_claims = { sub = "dev-user", tenant_id = "dev-tenant" }
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevConfig {
    /// Enable dev mode. Default: false.
    pub enabled:        bool,
    /// Default claims injected when no `Authorization` header is present.
    #[serde(default)]
    pub default_claims: std::collections::HashMap<String, serde_json::Value>,
}

/// REST transport configuration (TOML authoring struct).
///
/// ```toml
/// [rest]
/// enabled = true
/// path = "/rest/v1"
/// require_auth = true
/// max_page_size = 100
/// default_page_size = 20
/// delete_response = "no_content"
/// etag = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RestConfig {
    /// Enable REST transport endpoints.
    pub enabled:                 bool,
    /// Base path for REST endpoints (must start with `/`).
    pub path:                    String,
    /// Require authentication for REST requests.
    pub require_auth:            bool,
    /// Whitelist of resource names to expose (empty = all).
    #[serde(default)]
    pub include:                 Vec<String>,
    /// Blacklist of resource names to hide.
    #[serde(default)]
    pub exclude:                 Vec<String>,
    /// Response behavior for DELETE operations.
    pub delete_response:         DeleteResponse,
    /// Maximum page size for list queries. Must be > 0.
    pub max_page_size:           u64,
    /// Default page size when not specified by client.
    pub default_page_size:       u64,
    /// Whether to generate ETag headers for responses.
    pub etag:                    bool,
    /// Maximum allowed size in bytes for filter query parameters.
    pub max_filter_bytes:        usize,
    /// Maximum nesting depth for resource embedding (default 3).
    pub max_embedding_depth:     usize,
    /// Maximum rows affected by a single bulk UPDATE or DELETE.
    /// Default: 1000. Set to 0 to disable the limit.
    pub max_bulk_affected:       u64,
    /// Default `Cache-Control: max-age` for GET responses (seconds).
    /// Individual queries can override via `cache_ttl_seconds`.
    /// Default: 60.
    pub default_cache_ttl:       u64,
    /// TTL for idempotency key deduplication (seconds).
    /// After this period, the same `Idempotency-Key` can be reused.
    /// Default: 86400 (24 hours).
    pub idempotency_ttl_seconds: u64,
    /// CDN/shared-cache TTL in seconds (`s-maxage`).
    /// Only applies to public (unauthenticated) GET responses.
    #[serde(default)]
    pub cdn_max_age:             Option<u64>,
    /// Batch size for NDJSON streaming. Rows are fetched from the database
    /// in batches of this size and streamed to the client incrementally.
    /// Default: 500.
    pub ndjson_batch_size:       u64,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            enabled:                 false,
            path:                    "/rest/v1".to_string(),
            require_auth:            true,
            include:                 Vec::new(),
            exclude:                 Vec::new(),
            delete_response:         DeleteResponse::NoContent,
            max_page_size:           100,
            default_page_size:       20,
            etag:                    true,
            max_filter_bytes:        4096,
            max_embedding_depth:     fraiseql_core::schema::DEFAULT_MAX_EMBEDDING_DEPTH,
            max_bulk_affected:       fraiseql_core::schema::DEFAULT_MAX_BULK_AFFECTED,
            default_cache_ttl:       60,
            idempotency_ttl_seconds: 86_400,
            cdn_max_age:             None,
            ndjson_batch_size:       500,
        }
    }
}

/// gRPC transport configuration (TOML authoring struct).
///
/// ```toml
/// [grpc]
/// enabled = true
/// port = 50052
/// reflection = true
/// max_message_size_bytes = 4194304
/// descriptor_path = "proto/descriptor.binpb"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GrpcConfig {
    /// Enable gRPC transport.
    pub enabled:                bool,
    /// Port for the gRPC server.
    pub port:                   u16,
    /// Enable gRPC server reflection.
    pub reflection:             bool,
    /// Maximum inbound message size in bytes.
    pub max_message_size_bytes: usize,
    /// Path to the compiled `FileDescriptorSet` binary.
    pub descriptor_path:        String,
    /// Whitelist of type names to expose (empty = all).
    #[serde(default)]
    pub include_types:          Vec<String>,
    /// Blacklist of type names to hide.
    #[serde(default)]
    pub exclude_types:          Vec<String>,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            enabled:                false,
            port:                   50052,
            reflection:             true,
            max_message_size_bytes: 4 * 1024 * 1024,
            descriptor_path:        "proto/descriptor.binpb".to_string(),
            include_types:          Vec::new(),
            exclude_types:          Vec::new(),
        }
    }
}
