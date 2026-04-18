//! REST transport TOML configuration.

use serde::{Deserialize, Serialize};

/// REST transport configuration in `fraiseql.toml`.
///
/// Maps to `fraiseql_core::schema::config_types::RestConfig` at compile time.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RestTomlConfig {
    /// Whether the REST transport is enabled.
    pub enabled: bool,
    /// Base path for REST endpoints (e.g., `"/rest/v1"`).
    pub path: String,
    /// Maximum rows per page (clamps `?limit=`).
    pub max_page_size: u64,
    /// Default page size when no `?limit=` is specified.
    pub default_page_size: u64,
    /// Batch size for NDJSON streaming responses.
    pub ndjson_batch_size: u64,
    /// Maximum affected rows for bulk PATCH/DELETE.
    pub max_bulk_affected: u64,
    /// Maximum byte length for `?filter=` JSON values.
    pub max_filter_bytes: u64,
    /// How DELETE endpoints report success (`"no_content"` or `"returning"`).
    pub delete_response: String,
    /// Default result cache TTL in seconds (0 = no caching).
    pub default_cache_ttl: u64,
    /// CDN `s-maxage` value in seconds (`None` = omit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdn_max_age: Option<u64>,
    /// Whether REST endpoints require authentication by default.
    pub require_auth: bool,
    /// SSE heartbeat interval in seconds.
    pub sse_heartbeat_seconds: u64,
    /// Maximum depth for resource embedding.
    pub max_embedding_depth: u32,
    /// Whitelist of type names to expose as REST resources (empty = all).
    #[serde(default)]
    pub include: Vec<String>,
    /// Blacklist of type names to exclude from REST resources.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Whether to enable `ETag` / `If-None-Match` conditional response support.
    pub etag: bool,
    /// TTL in seconds for idempotency key deduplication.
    pub idempotency_ttl_seconds: u64,
}

impl Default for RestTomlConfig {
    fn default() -> Self {
        Self {
            enabled:                 false,
            path:                    "/rest/v1".to_string(),
            max_page_size:           1_000,
            default_page_size:       100,
            ndjson_batch_size:       500,
            max_bulk_affected:       10_000,
            max_filter_bytes:        4_096,
            delete_response:         "no_content".to_string(),
            default_cache_ttl:       0,
            cdn_max_age:             None,
            require_auth:            false,
            sse_heartbeat_seconds:   15,
            max_embedding_depth:     3,
            include:                 vec![],
            exclude:                 vec![],
            etag:                    false,
            idempotency_ttl_seconds: 86_400,
        }
    }
}
