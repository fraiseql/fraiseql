//! Admin API endpoints.
//!
//! Provides endpoints for:
//! - Hot-reloading schema without restart
//! - Invalidating cache by scope (all, entity type, or pattern)
//! - Inspecting runtime configuration (sanitized)

use std::{collections::HashMap, fs};

use axum::{Json, extract::State};
use fraiseql_core::{db::traits::DatabaseAdapter, schema::CompiledSchema};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::routes::{
    api::types::{ApiError, ApiResponse},
    graphql::AppState,
};

/// The cache that serves GraphQL queries — `ServerConfig.cache_enabled`.
const QUERY_RESULT_CACHE: &str = "query_result";

/// The Arrow Flight query cache — present only with the `arrow` feature and a
/// configured Flight service.
#[cfg(feature = "arrow")]
const ARROW_FLIGHT_CACHE: &str = "arrow_flight";

/// The TTL `create_flight_service` builds its `QueryCache` with.
#[cfg(feature = "arrow")]
const ARROW_FLIGHT_CACHE_TTL_SECS: u64 = 60;

/// Current status of the query result cache as understood by the server.
///
/// Used in the admin config endpoint and startup logs to give operators
/// an accurate picture of what `cache_enabled` actually activates.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CacheStatus {
    /// `cache_enabled = false` — no cache guard or caching active.
    Disabled,
    /// `cache_enabled = true` — RLS safety guard is active, but full
    /// query result caching (`CachedDatabaseAdapter`) is not yet wired.
    #[deprecated(
        since = "2.2.0",
        note = "CachedDatabaseAdapter is now always wired when cache_enabled = true. \
                Use `Active` or `Disabled` instead."
    )]
    RlsGuardOnly,
    /// Full query result caching is active.
    ///
    /// `CachedDatabaseAdapter` is wired into the server when `cache_enabled = true`.
    Active,
}

impl CacheStatus {
    /// Derive cache status from the `cache_enabled` flag.
    ///
    /// # Deprecated
    ///
    /// Use `AppState::adapter_cache_enabled` to determine the true cache state.
    #[must_use]
    #[deprecated(
        since = "2.2.0",
        note = "Use `AppState::adapter_cache_enabled` to determine the true cache state. \
                This function returns `RlsGuardOnly` which is no longer accurate."
    )]
    pub const fn from_cache_enabled(cache_enabled: bool) -> Self {
        #[allow(deprecated)] // Reason: function itself is deprecated; returns deprecated variant
        if cache_enabled {
            Self::RlsGuardOnly
        } else {
            Self::Disabled
        }
    }
}

/// Request to reload schema from file.
#[derive(Debug, Deserialize, Serialize)]
pub struct ReloadSchemaRequest {
    /// Path to compiled schema file
    pub schema_path:   String,
    /// If true, only validate the schema without applying changes
    pub validate_only: bool,
}

/// Response after schema reload attempt.
#[derive(Debug, Serialize)]
pub struct ReloadSchemaResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message about the result
    pub message: String,
}

/// Request to clear cache entries.
#[derive(Debug, Deserialize, Serialize)]
pub struct CacheClearRequest {
    /// Scope for clearing: "all", "entity", or "pattern"
    pub scope:       String,
    /// Entity type (required if scope is "entity")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// Pattern (required if scope is "pattern")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern:     Option<String>,
}

/// Response after cache clear operation.
///
/// Reports each cache separately (#941). `entries_cleared` is the total, kept so a
/// runbook one-liner has a single number to read; `caches` is what actually happened.
#[derive(Debug, Serialize)]
pub struct CacheClearResponse {
    /// Whether the operation succeeded
    pub success:         bool,
    /// Number of entries cleared, summed across every cache that served the scope
    pub entries_cleared: usize,
    /// Per-cache outcome, in a stable order
    pub caches:          Vec<CacheOperationResult>,
    /// Human-readable message about the result
    pub message:         String,
}

/// What one cache did for one admin request.
///
/// `configured = false` means the cache is not present on this server at all;
/// `entries_cleared = null` on a configured cache means it cannot serve this scope,
/// and `note` says so. The two used to be indistinguishable: every non-Arrow
/// deployment got `500 Cache not configured` while its query result cache was
/// serving traffic (#941).
#[derive(Debug, Serialize)]
pub struct CacheOperationResult {
    /// Which cache: `query_result` (serves GraphQL) or `arrow_flight` (Flight plans).
    pub cache:           &'static str,
    /// Whether this cache exists on this server.
    pub configured:      bool,
    /// Entries dropped, or `null` when the scope does not apply to this cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries_cleared: Option<usize>,
    /// Why nothing happened, when nothing happened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note:            Option<String>,
}

/// Response containing runtime configuration (sanitized).
#[derive(Debug, Serialize)]
pub struct AdminConfigResponse {
    /// Server version
    pub version: String,
    /// Runtime configuration (secrets redacted)
    pub config:  HashMap<String, String>,
}

/// Validate that a caller-supplied schema path is safe to open.
///
/// Two threats are guarded against:
///
/// 1. **Path traversal** — any component equal to `..` would let an attacker escape the intended
///    directory, so such paths are rejected.
/// 2. **Absolute-path escape** — when an `allowed_base` is given, the resolved path must start with
///    that prefix.  An absolute path like `/etc/passwd` is rejected when the allowed base is
///    `/var/fraiseql`.
///
/// # Errors
///
/// Returns `ApiError` with a validation error when the path is unsafe.
pub fn validate_schema_path(
    path: &str,
    allowed_base: Option<&std::path::Path>,
) -> Result<(), ApiError> {
    use std::path::{Component, Path};

    let p = Path::new(path);

    // Reject any `..` component regardless of position.
    if p.components().any(|c| c == Component::ParentDir) {
        return Err(ApiError::validation_error(
            "schema_path must not contain '..' (path traversal rejected)",
        ));
    }

    // When an allowed base is configured, verify the path stays within it.
    if let Some(base) = allowed_base {
        // Build the candidate path: if relative, join onto base; if absolute keep as-is.
        let candidate = if p.is_absolute() {
            p.to_path_buf()
        } else {
            base.join(p)
        };

        // Use `starts_with` on the lexically resolved path.  We intentionally
        // avoid `canonicalize` here to keep the function pure (no I/O), accepting
        // that symlink escapes are a separate, deployment-level concern.
        if !candidate.starts_with(base) {
            return Err(ApiError::validation_error(
                "schema_path is outside the allowed base directory",
            ));
        }
    }

    Ok(())
}

/// Reload schema from file.
///
/// Supports validation-only mode via `validate_only` flag.
/// When applied, the schema is atomically swapped without stopping execution.
///
/// # Errors
///
/// Returns `ApiError` with a validation error if `schema_path` is empty.
/// Returns `ApiError` with a validation error if `schema_path` contains path traversal.
/// Returns `ApiError` with a parse error if the schema file cannot be read or parsed.
///
/// Requires admin token authentication.
pub async fn reload_schema_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Json(req): Json<ReloadSchemaRequest>,
) -> Result<Json<ApiResponse<ReloadSchemaResponse>>, ApiError> {
    let _ = &state; // used conditionally by #[cfg(feature = "arrow")]
    if req.schema_path.is_empty() {
        return Err(ApiError::validation_error("schema_path cannot be empty"));
    }

    // SECURITY: Reject path traversal and out-of-base absolute paths.
    validate_schema_path(&req.schema_path, None)?;

    // Step 1: Load schema from file
    let schema_json = fs::read_to_string(&req.schema_path)
        .map_err(|e| ApiError::parse_error(format!("Failed to read schema file: {}", e)))?;

    // Step 2: Validate schema structure
    let _validated_schema = CompiledSchema::from_json(&schema_json, false)
        .map_err(|e| ApiError::parse_error(format!("Invalid schema JSON: {}", e)))?;

    if req.validate_only {
        info!(
            operation = "admin.reload_schema",
            schema_path = %req.schema_path,
            validate_only = true,
            success = true,
            "Admin: schema validation requested"
        );
        let response = ReloadSchemaResponse {
            success: true,
            message: "Schema validated successfully (not applied)".to_string(),
        };
        Ok(Json(ApiResponse {
            status: "success".to_string(),
            data:   response,
        }))
    } else {
        // Step 3: Atomically swap the executor with the validated schema.
        // We pass the already-validated JSON bytes to avoid re-reading from disk
        // (prevents TOCTOU: the file could change between validation and reload).
        let start = std::time::Instant::now();

        match state.reload_schema_from_json(&schema_json).await {
            Ok(()) => {
                let duration_ms = start.elapsed().as_millis();
                state
                    .metrics
                    .schema_reloads_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                info!(
                    operation = "admin.reload_schema",
                    schema_path = %req.schema_path,
                    duration_ms,
                    "Schema reloaded successfully"
                );

                let response = ReloadSchemaResponse {
                    success: true,
                    message: format!("Schema reloaded from {} in {duration_ms}ms", req.schema_path),
                };
                Ok(Json(ApiResponse {
                    status: "success".to_string(),
                    data:   response,
                }))
            },
            Err(e) => {
                state
                    .metrics
                    .schema_reload_errors_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                error!(
                    operation = "admin.reload_schema",
                    schema_path = %req.schema_path,
                    error = %e,
                    "Schema reload failed"
                );
                Err(ApiError::internal_error(format!("Schema reload failed: {e}")))
            },
        }
    }
}

/// Cache statistics response.
///
/// One entry per cache the server can hold (#941). The flat shape this replaced
/// described only the Arrow Flight cache, so a server whose query result cache was
/// serving traffic reported `cache_enabled: false, "Cache is not configured"` — while
/// `/api/v1/admin/config` on the same server reported the cache active.
#[derive(Debug, Serialize)]
pub struct CacheStatsResponse {
    /// Per-cache statistics, in a stable order.
    pub caches:  Vec<CacheStatsEntry>,
    /// Human-readable summary
    pub message: String,
}

/// Statistics for one cache.
#[derive(Debug, Serialize)]
pub struct CacheStatsEntry {
    /// Which cache: `query_result` (serves GraphQL) or `arrow_flight` (Flight plans).
    pub cache:         &'static str,
    /// Whether this cache exists on this server.
    pub configured:    bool,
    /// Entries currently held.
    pub entries_count: usize,
    /// Lookups served from cache (`null` when the cache does not track it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hits:          Option<u64>,
    /// Lookups that reached the database (`null` when the cache does not track it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub misses:        Option<u64>,
    /// Configured default entry TTL, in seconds.
    pub ttl_secs:      u64,
    /// Configured entry ceiling (`null` when unbounded or untracked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_entries:   Option<usize>,
}

/// Clear cache entries by scope.
///
/// Supports three clearing scopes:
/// - **all**: Clear all cache entries
/// - **entity**: Clear entries for a specific entity type
/// - **pattern**: Clear entries matching a glob pattern
///
/// # Errors
///
/// Returns `ApiError` with an internal error if the cache feature is not enabled.
/// Returns `ApiError` with a validation error if required parameters are missing or scope is
/// invalid.
///
/// Requires admin token authentication.
pub async fn cache_clear_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Json(req): Json<CacheClearRequest>,
) -> Result<Json<ApiResponse<CacheClearResponse>>, ApiError> {
    // Argument validation first, and identically for every cache: a missing
    // `entity_type` is the operator's mistake regardless of what is configured.
    match req.scope.as_str() {
        "entity" if req.entity_type.is_none() => {
            return Err(ApiError::validation_error(
                "entity_type is required when scope is 'entity'",
            ));
        },
        "pattern" if req.pattern.is_none() => {
            return Err(ApiError::validation_error("pattern is required when scope is 'pattern'"));
        },
        "all" | "entity" | "pattern" => {},
        _ => {
            return Err(ApiError::validation_error("scope must be 'all', 'entity', or 'pattern'"));
        },
    }

    #[cfg_attr(not(feature = "arrow"), allow(unused_mut))]
    // Reason: the arrow push below is the only mutation, and it is feature-gated.
    let mut caches = vec![clear_query_result_cache(&state, &req).await?];
    #[cfg(feature = "arrow")]
    caches.push(clear_arrow_flight_cache(&state, &req));

    let entries_cleared: usize = caches.iter().filter_map(|c| c.entries_cleared).sum();
    let served: Vec<&str> =
        caches.iter().filter(|c| c.entries_cleared.is_some()).map(|c| c.cache).collect();

    info!(
        operation = "admin.cache_clear",
        scope = %req.scope,
        entries_cleared,
        caches = ?served,
        success = true,
        "Admin: cache cleared"
    );

    let message = if served.is_empty() {
        format!("No cache on this server can serve scope '{}'", req.scope)
    } else {
        format!("Cleared {entries_cleared} entries from {}", served.join(", "))
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   CacheClearResponse {
            success: true,
            entries_cleared,
            caches,
            message,
        },
    }))
}

/// Apply an admin clear request to the cache that serves GraphQL queries.
///
/// `pattern` has no counterpart here — the query result cache is keyed by a hash, not
/// by a string an operator could glob — so the scope is reported unsupported rather
/// than silently doing nothing, which is the shape #941 is about.
async fn clear_query_result_cache<A: DatabaseAdapter>(
    state: &AppState<A>,
    req: &CacheClearRequest,
) -> Result<CacheOperationResult, ApiError> {
    let adapter = state.executor().adapter().clone();
    if adapter.result_cache_stats().is_none() {
        return Ok(CacheOperationResult {
            cache:           QUERY_RESULT_CACHE,
            configured:      false,
            entries_cleared: None,
            note:            Some(
                "the query result cache is not active (cache_enabled = false)".to_string(),
            ),
        });
    }

    let cleared = match req.scope.as_str() {
        "all" => adapter
            .clear_result_cache()
            .await
            .map_err(|e| ApiError::internal_error(format!("Cache clear failed: {e}")))?,
        "entity" => {
            let entity_type = req.entity_type.as_deref().unwrap_or_default();
            // Resolve the view from the schema rather than guessing `v_{lowercase}`:
            // that guess maps `OrderItem` to `v_orderitem` and evicts nothing.
            let Some(view) = view_of_entity_type(state, entity_type) else {
                return Ok(CacheOperationResult {
                    cache:           QUERY_RESULT_CACHE,
                    configured:      true,
                    entries_cleared: None,
                    note:            Some(format!(
                        "no type named '{entity_type}' in the compiled schema, or it declares \
                         no sql_source"
                    )),
                });
            };
            let evicted = adapter
                .invalidate_views(&[fraiseql_core::cache::ViewName::from(view.as_str())])
                .await
                .map_err(|e| ApiError::internal_error(format!("Cache clear failed: {e}")))?;
            Some(usize::try_from(evicted).unwrap_or(usize::MAX))
        },
        _ => {
            return Ok(CacheOperationResult {
                cache:           QUERY_RESULT_CACHE,
                configured:      true,
                entries_cleared: None,
                note:            Some(
                    "the query result cache is keyed by hash and cannot be globbed; use \
                     scope 'all' or 'entity'"
                        .to_string(),
                ),
            });
        },
    };

    Ok(CacheOperationResult {
        cache:           QUERY_RESULT_CACHE,
        configured:      true,
        entries_cleared: cleared,
        note:            None,
    })
}

/// Apply an admin clear request to the Arrow Flight query cache.
#[cfg(feature = "arrow")]
fn clear_arrow_flight_cache<A: DatabaseAdapter>(
    state: &AppState<A>,
    req: &CacheClearRequest,
) -> CacheOperationResult {
    let Some(cache) = state.cache() else {
        return CacheOperationResult {
            cache:           ARROW_FLIGHT_CACHE,
            configured:      false,
            entries_cleared: None,
            note:            Some("no Arrow Flight service is configured".to_string()),
        };
    };

    let cleared = match req.scope.as_str() {
        "all" => {
            let before = cache.len();
            cache.clear();
            before
        },
        "entity" => {
            let entity_type = req.entity_type.as_deref().unwrap_or_default();
            let view = view_of_entity_type(state, entity_type)
                .unwrap_or_else(|| format!("v_{}", entity_type.to_lowercase()));
            cache.invalidate_views(&[&view])
        },
        _ => cache.invalidate_pattern(req.pattern.as_deref().unwrap_or_default()),
    };

    CacheOperationResult {
        cache:           ARROW_FLIGHT_CACHE,
        configured:      true,
        entries_cleared: Some(cleared),
        note:            None,
    }
}

/// The view a GraphQL type reads from, per the compiled schema the server is serving.
fn view_of_entity_type<A: DatabaseAdapter>(
    state: &AppState<A>,
    entity_type: &str,
) -> Option<String> {
    state
        .executor()
        .schema()
        .types
        .iter()
        .find(|t| t.name == entity_type)
        .map(|t| t.sql_source.as_str().to_string())
        .filter(|source| !source.is_empty())
}

/// Get cache statistics.
///
/// Returns current cache metrics including entry count, enabled status, and TTL.
///
/// # Errors
///
/// This handler currently always succeeds; it is infallible.
///
/// Requires admin token authentication.
pub async fn cache_stats_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<CacheStatsResponse>>, ApiError> {
    // The cache that serves GraphQL queries — the one `cache_enabled` toggles and the
    // one `/admin/config` reports. It was invisible here until #941.
    #[cfg_attr(not(feature = "arrow"), allow(unused_mut))]
    // Reason: the arrow push below is the only mutation, and it is feature-gated.
    let mut caches = vec![state.executor().adapter().result_cache_stats().map_or(
        CacheStatsEntry {
            cache:         QUERY_RESULT_CACHE,
            configured:    false,
            entries_count: 0,
            hits:          None,
            misses:        None,
            ttl_secs:      0,
            max_entries:   None,
        },
        |s| CacheStatsEntry {
            cache:         QUERY_RESULT_CACHE,
            configured:    true,
            entries_count: s.entries,
            hits:          Some(s.hits),
            misses:        Some(s.misses),
            ttl_secs:      s.ttl_seconds,
            max_entries:   Some(s.max_entries),
        },
    )];

    #[cfg(feature = "arrow")]
    caches.push(state.cache().map_or(
        CacheStatsEntry {
            cache:         ARROW_FLIGHT_CACHE,
            configured:    false,
            entries_count: 0,
            hits:          None,
            misses:        None,
            ttl_secs:      0,
            max_entries:   None,
        },
        |cache| CacheStatsEntry {
            cache:         ARROW_FLIGHT_CACHE,
            configured:    true,
            entries_count: cache.len(),
            hits:          None,
            misses:        None,
            ttl_secs:      ARROW_FLIGHT_CACHE_TTL_SECS,
            max_entries:   None,
        },
    ));

    let configured: Vec<&str> = caches.iter().filter(|c| c.configured).map(|c| c.cache).collect();
    let message = if configured.is_empty() {
        "No cache is configured on this server".to_string()
    } else {
        format!("Configured cache(s): {}", configured.join(", "))
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   CacheStatsResponse { caches, message },
    }))
}

/// Get sanitized runtime configuration.
///
/// Returns the server version and the cache state, with secrets redacted.
///
/// This used to *promise* port/host/workers/limits too, read from an
/// `AppState` config slot that no production constructor ever filled — so the
/// branch never ran and the endpoint always reported `cache_enabled = false`
/// regardless of the actual adapter cache (#839's dead `RuntimeConfig` layer).
/// It now reports only what it actually knows, truthfully.
///
/// # Errors
///
/// This handler currently always succeeds; it is infallible.
///
/// Requires admin token authentication.
pub async fn config_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<AdminConfigResponse>>, ApiError> {
    let mut config = HashMap::new();

    // Cache status: read from adapter_cache_enabled (set at startup by
    // ServerBuilder). This reflects the CachedDatabaseAdapter state,
    // independent of the Arrow cache.
    let cache_active = state.adapter_cache_enabled;
    config.insert("cache_enabled".to_string(), cache_active.to_string());
    let cache_status = if cache_active {
        CacheStatus::Active
    } else {
        CacheStatus::Disabled
    };
    config.insert(
        "cache_status".to_string(),
        serde_json::to_string(&cache_status)
            .unwrap_or_else(|_| "\"disabled\"".to_string())
            .trim_matches('"')
            .to_string(),
    );

    let response = AdminConfigResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        config,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

/// Request body for `POST /api/v1/admin/explain`.
#[derive(Debug, Deserialize, Serialize)]
pub struct ExplainRequest {
    /// Name of the regular query to explain (e.g., `"users"`).
    pub query: String,

    /// GraphQL-style variable filters passed as a JSON object.
    ///
    /// Each key-value pair becomes an equality condition in the WHERE clause.
    /// Example: `{"status": "active"}` → `WHERE data->>'status' = 'active'`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<serde_json::Value>,

    /// Optional row limit to pass to the query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,

    /// Optional row offset to pass to the query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// Return the pre-built Grafana dashboard JSON for FraiseQL metrics.
///
/// The dashboard JSON is embedded at compile time from
/// `deploy/grafana/fraiseql-dashboard.json`.  Operators can import it into
/// Grafana with a single `curl` command (see `deploy/grafana/README.md`).
///
/// # Errors
///
/// This handler is infallible — the embedded JSON is validated at compile time
/// by the `test_grafana_dashboard_is_valid_json` unit test.
///
/// Requires admin token authentication.
pub async fn grafana_dashboard_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
) -> impl axum::response::IntoResponse {
    const DASHBOARD_JSON: &str = include_str!("../../../resources/fraiseql-dashboard.json");

    (
        axum::http::StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        DASHBOARD_JSON,
    )
}

/// Run `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)` for a named query.
///
/// Accepts a query name and optional variable filters, then executes
/// `EXPLAIN ANALYZE` against the backing PostgreSQL view using the exact
/// same parameterized SQL that a live query would use.
///
/// # Errors
///
/// * `400 Bad Request` — empty query name, unknown query, or mutation given
/// * `500 Internal Server Error` — database execution failure
///
/// Requires admin token authentication.
pub async fn explain_handler<A: DatabaseAdapter + 'static>(
    State(state): State<AppState<A>>,
    Json(req): Json<ExplainRequest>,
) -> Result<Json<ApiResponse<fraiseql_core::runtime::ExplainResult>>, ApiError> {
    if req.query.is_empty() {
        return Err(ApiError::validation_error("query cannot be empty"));
    }

    state
        .executor()
        .explain(&req.query, req.variables.as_ref(), req.limit, req.offset)
        .await
        .map(ApiResponse::success)
        .map_err(|e| match e {
            fraiseql_core::error::FraiseQLError::Validation { message, .. } => {
                ApiError::validation_error(message)
            },
            fraiseql_core::error::FraiseQLError::Unsupported { message } => {
                ApiError::validation_error(format!("Unsupported: {message}"))
            },
            other => ApiError::internal_error(other.to_string()),
        })
}
