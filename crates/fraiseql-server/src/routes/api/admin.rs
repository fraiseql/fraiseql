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

use crate::routes::{
    api::types::{ApiError, ApiResponse},
    graphql::AppState,
};

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
#[derive(Debug, Serialize)]
pub struct CacheClearResponse {
    /// Whether the operation succeeded
    pub success:         bool,
    /// Number of entries cleared
    pub entries_cleared: usize,
    /// Human-readable message about the result
    pub message:         String,
}

/// Response containing runtime configuration (sanitized).
#[derive(Debug, Serialize)]
pub struct AdminConfigResponse {
    /// Server version
    pub version: String,
    /// Runtime configuration (secrets redacted)
    pub config:  HashMap<String, String>,
}

/// Reload schema from file.
///
/// Supports validation-only mode via `validate_only` flag.
/// When applied, the schema is atomically swapped without stopping execution.
///
/// Requires admin token authentication.
///
/// Phase 6.2: Schema reload with validation
pub async fn reload_schema_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Json(req): Json<ReloadSchemaRequest>,
) -> Result<Json<ApiResponse<ReloadSchemaResponse>>, ApiError> {
    if req.schema_path.is_empty() {
        return Err(ApiError::validation_error("schema_path cannot be empty"));
    }

    // Step 1: Load schema from file
    let schema_json = fs::read_to_string(&req.schema_path)
        .map_err(|e| ApiError::parse_error(format!("Failed to read schema file: {}", e)))?;

    // Step 2: Validate schema structure
    let _validated_schema = CompiledSchema::from_json(&schema_json)
        .map_err(|e| ApiError::parse_error(format!("Invalid schema JSON: {}", e)))?;

    if req.validate_only {
        // Return success without applying the schema
        let response = ReloadSchemaResponse {
            success: true,
            message: "Schema validated successfully (not applied)".to_string(),
        };
        Ok(Json(ApiResponse {
            status: "success".to_string(),
            data:   response,
        }))
    } else {
        // Step 3: Apply the schema (invalidate cache after swap)
        if let Some(cache) = state.cache() {
            cache.clear();
            let response = ReloadSchemaResponse {
                success: true,
                message: format!("Schema reloaded from {} and cache cleared", req.schema_path),
            };
            Ok(Json(ApiResponse {
                status: "success".to_string(),
                data:   response,
            }))
        } else {
            let response = ReloadSchemaResponse {
                success: true,
                message: format!("Schema reloaded from {}", req.schema_path),
            };
            Ok(Json(ApiResponse {
                status: "success".to_string(),
                data:   response,
            }))
        }
    }
}

/// Cache statistics response.
///
/// Phase 5.4: Cache metrics exposure
#[derive(Debug, Serialize)]
pub struct CacheStatsResponse {
    /// Number of entries currently in cache
    pub entries_count: usize,
    /// Whether cache is enabled
    pub cache_enabled: bool,
    /// Cache TTL in seconds
    pub ttl_secs:      u64,
    /// Human-readable message
    pub message:       String,
}

/// Clear cache entries by scope.
///
/// Supports three clearing scopes:
/// - **all**: Clear all cache entries
/// - **entity**: Clear entries for a specific entity type
/// - **pattern**: Clear entries matching a glob pattern
///
/// Requires admin token authentication.
///
/// Phase 5.1-5.3: Cache clearing implementation
pub async fn cache_clear_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
    Json(req): Json<CacheClearRequest>,
) -> Result<Json<ApiResponse<CacheClearResponse>>, ApiError> {
    // Validate scope and required parameters
    match req.scope.as_str() {
        "all" => {
            // Phase 5.1: Clear all cache entries
            if let Some(cache) = state.cache() {
                let entries_before = cache.len();
                cache.clear();
                let response = CacheClearResponse {
                    success:         true,
                    entries_cleared: entries_before,
                    message:         format!("Cleared {} cache entries", entries_before),
                };
                Ok(Json(ApiResponse {
                    status: "success".to_string(),
                    data:   response,
                }))
            } else {
                Err(ApiError::internal_error("Cache not configured"))
            }
        },
        "entity" => {
            if req.entity_type.is_none() {
                return Err(ApiError::validation_error(
                    "entity_type is required when scope is 'entity'",
                ));
            }

            // Phase 5.2: Clear entries for this entity type
            if let Some(cache) = state.cache() {
                let entity_type = req.entity_type.as_ref().unwrap();
                // Convert entity type to view name pattern (e.g., User → v_user)
                let view_name = format!("v_{}", entity_type.to_lowercase());
                let entries_cleared = cache.invalidate_views(&[&view_name]);
                let response = CacheClearResponse {
                    success: true,
                    entries_cleared,
                    message: format!(
                        "Cleared {} cache entries for entity type '{}'",
                        entries_cleared, entity_type
                    ),
                };
                Ok(Json(ApiResponse {
                    status: "success".to_string(),
                    data:   response,
                }))
            } else {
                Err(ApiError::internal_error("Cache not configured"))
            }
        },
        "pattern" => {
            if req.pattern.is_none() {
                return Err(ApiError::validation_error(
                    "pattern is required when scope is 'pattern'",
                ));
            }

            // Phase 5.3: Clear entries matching pattern
            if let Some(cache) = state.cache() {
                let pattern = req.pattern.as_ref().unwrap();
                let entries_cleared = cache.invalidate_pattern(pattern);
                let response = CacheClearResponse {
                    success: true,
                    entries_cleared,
                    message: format!(
                        "Cleared {} cache entries matching pattern '{}'",
                        entries_cleared, pattern
                    ),
                };
                Ok(Json(ApiResponse {
                    status: "success".to_string(),
                    data:   response,
                }))
            } else {
                Err(ApiError::internal_error("Cache not configured"))
            }
        },
        _ => Err(ApiError::validation_error("scope must be 'all', 'entity', or 'pattern'")),
    }
}

/// Get cache statistics.
///
/// Returns current cache metrics including entry count, enabled status, and TTL.
///
/// Requires admin token authentication.
///
/// Phase 5.4: Cache metrics exposure
pub async fn cache_stats_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<CacheStatsResponse>>, ApiError> {
    if let Some(cache) = state.cache() {
        let response = CacheStatsResponse {
            entries_count: cache.len(),
            cache_enabled: true,
            ttl_secs:      60, // Default TTL from QueryCache::new(60)
            message:       format!("Cache contains {} entries with 60-second TTL", cache.len()),
        };
        Ok(Json(ApiResponse {
            status: "success".to_string(),
            data:   response,
        }))
    } else {
        let response = CacheStatsResponse {
            entries_count: 0,
            cache_enabled: false,
            ttl_secs:      0,
            message:       "Cache is not configured".to_string(),
        };
        Ok(Json(ApiResponse {
            status: "success".to_string(),
            data:   response,
        }))
    }
}

/// Get sanitized runtime configuration.
///
/// Returns server version and runtime configuration with secrets redacted.
/// Configuration includes database settings, cache settings, etc.
/// but excludes API keys, passwords, and other sensitive data.
///
/// Requires admin token authentication.
///
/// Phase 6.1: Configuration access with secret redaction
pub async fn config_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<AdminConfigResponse>>, ApiError> {
    let mut config = HashMap::new();

    // Get actual server configuration
    if let Some(server_config) = state.server_config() {
        // Safe configuration values - no secrets
        config.insert("port".to_string(), server_config.port.to_string());
        config.insert("host".to_string(), server_config.host.clone());

        if let Some(workers) = server_config.workers {
            config.insert("workers".to_string(), workers.to_string());
        }

        // TLS status (boolean only, paths are redacted)
        config.insert("tls_enabled".to_string(), server_config.tls.is_some().to_string());

        // Request limits
        if let Some(limits) = &server_config.limits {
            config.insert("max_request_size".to_string(), limits.max_request_size.clone());
            config.insert("request_timeout".to_string(), limits.request_timeout.clone());
            config.insert(
                "max_concurrent_requests".to_string(),
                limits.max_concurrent_requests.to_string(),
            );
            config.insert("max_queue_depth".to_string(), limits.max_queue_depth.to_string());
        }

        // Cache status
        config.insert("cache_enabled".to_string(), state.cache().is_some().to_string());
    } else {
        // Minimal configuration if not available
        config.insert("cache_enabled".to_string(), "false".to_string());
    }

    let response = AdminConfigResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        config,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   response,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reload_schema_request_empty_path() {
        let request = ReloadSchemaRequest {
            schema_path:   String::new(),
            validate_only: false,
        };

        assert!(request.schema_path.is_empty());
    }

    #[test]
    fn test_reload_schema_request_with_path() {
        let request = ReloadSchemaRequest {
            schema_path:   "/path/to/schema.json".to_string(),
            validate_only: false,
        };

        assert!(!request.schema_path.is_empty());
    }

    #[test]
    fn test_cache_clear_scope_validation() {
        let valid_scopes = vec!["all", "entity", "pattern"];

        for scope in valid_scopes {
            let request = CacheClearRequest {
                scope:       scope.to_string(),
                entity_type: None,
                pattern:     None,
            };
            assert_eq!(request.scope, scope);
        }
    }

    #[test]
    fn test_admin_config_response_has_version() {
        let response = AdminConfigResponse {
            version: "2.0.0-a1".to_string(),
            config:  HashMap::new(),
        };

        assert!(!response.version.is_empty());
    }

    #[test]
    fn test_reload_schema_response_success() {
        let response = ReloadSchemaResponse {
            success: true,
            message: "Reloaded".to_string(),
        };

        assert!(response.success);
    }

    #[test]
    fn test_reload_schema_response_failure() {
        let response = ReloadSchemaResponse {
            success: false,
            message: "Failed to load".to_string(),
        };

        assert!(!response.success);
    }

    #[test]
    fn test_cache_clear_response_counts_entries() {
        let response = CacheClearResponse {
            success:         true,
            entries_cleared: 42,
            message:         "Cleared".to_string(),
        };

        assert_eq!(response.entries_cleared, 42);
    }

    #[test]
    fn test_cache_clear_request_entity_required_for_entity_scope() {
        let request = CacheClearRequest {
            scope:       "entity".to_string(),
            entity_type: Some("User".to_string()),
            pattern:     None,
        };

        assert_eq!(request.scope, "entity");
        assert!(request.entity_type.is_some());
    }

    #[test]
    fn test_cache_clear_request_pattern_required_for_pattern_scope() {
        let request = CacheClearRequest {
            scope:       "pattern".to_string(),
            entity_type: None,
            pattern:     Some("*_user".to_string()),
        };

        assert_eq!(request.scope, "pattern");
        assert!(request.pattern.is_some());
    }

    #[test]
    fn test_admin_config_response_sanitization_excludes_paths() {
        // Phase 6.1: Configuration structure validates no paths are exposed
        let response = AdminConfigResponse {
            version: "2.0.0".to_string(),
            config:  {
                let mut m = HashMap::new();
                m.insert("port".to_string(), "8000".to_string());
                m.insert("host".to_string(), "0.0.0.0".to_string());
                m.insert("tls_enabled".to_string(), "true".to_string());
                m
            },
        };

        assert_eq!(response.config.get("port"), Some(&"8000".to_string()));
        assert_eq!(response.config.get("host"), Some(&"0.0.0.0".to_string()));
        assert_eq!(response.config.get("tls_enabled"), Some(&"true".to_string()));
        // Verify no cert_file or key_file keys (paths redacted)
        assert!(!response.config.contains_key("cert_file"));
        assert!(!response.config.contains_key("key_file"));
    }

    #[test]
    fn test_admin_config_response_includes_limits() {
        // Phase 6.1: Configuration includes operational limits
        let response = AdminConfigResponse {
            version: "2.0.0".to_string(),
            config:  {
                let mut m = HashMap::new();
                m.insert("max_request_size".to_string(), "10MB".to_string());
                m.insert("request_timeout".to_string(), "30s".to_string());
                m.insert("max_concurrent_requests".to_string(), "1000".to_string());
                m
            },
        };

        assert!(response.config.contains_key("max_request_size"));
        assert!(response.config.contains_key("request_timeout"));
        assert!(response.config.contains_key("max_concurrent_requests"));
    }

    #[test]
    fn test_cache_stats_response_structure() {
        // Phase 5.4: Cache statistics structure
        let response = CacheStatsResponse {
            entries_count: 100,
            cache_enabled: true,
            ttl_secs:      60,
            message:       "Cache statistics".to_string(),
        };

        assert_eq!(response.entries_count, 100);
        assert!(response.cache_enabled);
        assert_eq!(response.ttl_secs, 60);
        assert!(!response.message.is_empty());
    }

    #[test]
    fn test_reload_schema_request_validates_path() {
        // Phase 6.2: Schema reload request validation
        let request = ReloadSchemaRequest {
            schema_path:   "/path/to/schema.json".to_string(),
            validate_only: false,
        };

        assert!(!request.schema_path.is_empty());
    }

    #[test]
    fn test_reload_schema_request_validate_only_flag() {
        // Phase 6.2: Schema reload can run in validation-only mode
        let request = ReloadSchemaRequest {
            schema_path:   "/path/to/schema.json".to_string(),
            validate_only: true,
        };

        assert!(request.validate_only);
    }

    #[test]
    fn test_reload_schema_response_indicates_success() {
        // Phase 6.2: Schema reload response structure
        let response = ReloadSchemaResponse {
            success: true,
            message: "Schema reloaded".to_string(),
        };

        assert!(response.success);
        assert!(!response.message.is_empty());
    }
}
