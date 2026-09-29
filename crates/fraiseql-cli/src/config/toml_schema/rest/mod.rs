//! REST transport TOML configuration.

use fraiseql_core::schema::{DeleteResponse, RestConfig};
use serde::{Deserialize, Serialize};

/// REST transport configuration parsed from the `[rest]` TOML section.
///
/// All fields have defaults matching `RestConfig::default()` in `fraiseql-core`.
/// When `enabled` is `false` (the default), the REST transport is not mounted.
///
/// Unknown keys are refused. `[rest]` used to accept anything, so a retired key — or a
/// misspelled one — compiled and was silently the default.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RestTomlConfig {
    /// Whether the REST transport is enabled.
    pub enabled:                 bool,
    /// Base URL path for REST endpoints.
    pub path:                    String,
    /// Maximum rows per page (clamps `?limit=`).
    pub max_page_size:           u64,
    /// Default page size when no `?limit=` is specified.
    pub default_page_size:       u64,
    /// Page each parent row gets of an embedded relationship when the request names none
    /// (`?rel.limit=`). Applied at most at `max_page_size`.
    pub default_embed_page_size: u64,
    /// Batch size for NDJSON streaming responses.
    pub ndjson_batch_size:       u64,
    /// Maximum affected rows for bulk PATCH/DELETE.
    pub max_bulk_affected:       u64,
    /// Maximum byte length for `?filter=` JSON values.
    pub max_filter_bytes:        u64,
    /// How DELETE endpoints report success: `"no_content"` (default) or `"entity"`.
    pub delete_response:         DeleteResponseToml,
    /// Default result cache TTL in seconds (0 = no caching): the `max-age` of a GET's
    /// `Cache-Control`. An authenticated read (any credential) is `private`; only an
    /// anonymous read is `public`, and only a public one carries `s-maxage`.
    pub default_cache_ttl:       u64,
    /// CDN `s-maxage` value in seconds (`None` = omit).
    pub cdn_max_age:             Option<u64>,
    /// Whether REST endpoints require authentication by default.
    pub require_auth:            bool,
    /// SSE heartbeat interval in seconds.
    pub sse_heartbeat_seconds:   u64,
    /// How many delivered events a `Last-Event-ID` resume may reach back over
    /// (0 = no bound; see `RestConfig::sse_max_replay_events`).
    pub sse_max_replay_events:   u64,
    /// Maximum depth for resource embedding (`?select=posts(comments)`).
    pub max_embedding_depth:     u32,
    /// Allowlist of type names to expose as REST resources (empty = all).
    pub include:                 Vec<String>,
    /// Denylist of type names to exclude from REST resources.
    pub exclude:                 Vec<String>,
    /// Whether to enable `ETag` / `If-None-Match` conditional response support.
    pub etag:                    bool,
    /// TTL in seconds for idempotency key deduplication.
    pub idempotency_ttl_seconds: u64,
    /// Declarable **only to be refused**, with its replacement named.
    ///
    /// The key bounded `?select=`'s fan-out of sub-reads, and there are no sub-reads left:
    /// an embed is one composed statement, bounded by the request's cost ceiling. Denied as
    /// an unknown field it would read `unknown field max_embedded_reads`, which tells an
    /// operator what is wrong but not what to write instead.
    #[serde(skip_serializing)]
    pub max_embedded_reads:      Option<serde::de::IgnoredAny>,
}

impl RestTomlConfig {
    /// Refuse a `[rest]` key that was removed, naming what replaced it.
    ///
    /// # Errors
    ///
    /// When `max_embedded_reads` is set.
    pub fn reject_retired_keys(&self) -> anyhow::Result<()> {
        if self.max_embedded_reads.is_some() {
            anyhow::bail!(
                "[rest] max_embedded_reads was removed: a `?select=` embed is no longer a \
                 sub-read per parent row but one composed statement, and there are no sub-reads \
                 left to count. It is bounded by the request's cost ceiling — set \
                 [security.cost_budget] per_request_max (and [validation] max_response_bytes \
                 for its bytes) — and each embedded level's page by [rest] \
                 default_embed_page_size. Remove the key."
            );
        }
        Ok(())
    }
}

/// DELETE response mode for TOML configuration.
///
/// Uses lowercase serde names for TOML ergonomics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteResponseToml {
    /// Return `204 No Content`.
    #[default]
    NoContent,
    /// Return `200` with the deleted entity in the body.
    Entity,
}

impl Default for RestTomlConfig {
    fn default() -> Self {
        Self {
            enabled:                 false,
            path:                    "/rest/v1".to_string(),
            max_page_size:           1_000,
            default_page_size:       100,
            default_embed_page_size: 50,
            ndjson_batch_size:       500,
            max_bulk_affected:       10_000,
            max_filter_bytes:        4_096,
            delete_response:         DeleteResponseToml::NoContent,
            default_cache_ttl:       0,
            cdn_max_age:             None,
            require_auth:            false,
            sse_heartbeat_seconds:   30,
            sse_max_replay_events:   10_000,
            max_embedding_depth:     3,
            include:                 Vec::new(),
            exclude:                 Vec::new(),
            etag:                    true,
            idempotency_ttl_seconds: 300,
            max_embedded_reads:      None,
        }
    }
}

impl From<DeleteResponseToml> for DeleteResponse {
    fn from(toml: DeleteResponseToml) -> Self {
        match toml {
            DeleteResponseToml::NoContent => Self::NoContent,
            DeleteResponseToml::Entity => Self::Entity,
        }
    }
}

impl From<RestTomlConfig> for RestConfig {
    fn from(toml: RestTomlConfig) -> Self {
        Self {
            enabled:                 toml.enabled,
            path:                    toml.path,
            max_page_size:           toml.max_page_size,
            default_page_size:       toml.default_page_size,
            default_embed_page_size: toml.default_embed_page_size,
            ndjson_batch_size:       toml.ndjson_batch_size,
            max_bulk_affected:       toml.max_bulk_affected,
            max_filter_bytes:        toml.max_filter_bytes,
            delete_response:         toml.delete_response.into(),
            default_cache_ttl:       toml.default_cache_ttl,
            cdn_max_age:             toml.cdn_max_age,
            require_auth:            toml.require_auth,
            sse_heartbeat_seconds:   toml.sse_heartbeat_seconds,
            sse_max_replay_events:   toml.sse_max_replay_events,
            max_embedding_depth:     toml.max_embedding_depth,
            include:                 toml.include,
            exclude:                 toml.exclude,
            etag:                    toml.etag,
            idempotency_ttl_seconds: toml.idempotency_ttl_seconds,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests;
