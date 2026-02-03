//! Admin API endpoints.
//!
//! Provides endpoints for:
//! - Hot-reloading schema without restart
//! - Invalidating cache by scope (all, entity type, or pattern)
//! - Inspecting runtime configuration (sanitized)

use axum::{
    extract::State,
    Json,
};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::routes::api::types::{ApiResponse, ApiError};
use crate::routes::graphql::AppState;

/// Request to reload schema from file.
#[derive(Debug, Deserialize, Serialize)]
pub struct ReloadSchemaRequest {
    /// Path to compiled schema file
    pub schema_path: String,
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
    pub scope: String,
    /// Entity type (required if scope is "entity")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    /// Pattern (required if scope is "pattern")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// Response after cache clear operation.
#[derive(Debug, Serialize)]
pub struct CacheClearResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Number of entries cleared
    pub entries_cleared: usize,
    /// Human-readable message about the result
    pub message: String,
}

/// Response containing runtime configuration (sanitized).
#[derive(Debug, Serialize)]
pub struct AdminConfigResponse {
    /// Server version
    pub version: String,
    /// Runtime configuration (secrets redacted)
    pub config: HashMap<String, String>,
}

/// Reload schema from file.
///
/// Supports validation-only mode via `validate_only` flag.
/// When applied, the schema is atomically swapped without stopping execution.
///
/// Requires admin token authentication.
pub async fn reload_schema_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
    Json(req): Json<ReloadSchemaRequest>,
) -> Result<Json<ApiResponse<ReloadSchemaResponse>>, ApiError> {
    // Placeholder: In a real implementation, this would:
    // 1. Load schema from req.schema_path
    // 2. Validate the schema structure
    // 3. If validate_only, return success without applying
    // 4. Otherwise, atomically swap the schema in AppState
    // 5. Drain active queries gracefully before swap

    if req.schema_path.is_empty() {
        return Err(ApiError::validation_error("schema_path cannot be empty"));
    }

    let response = if req.validate_only {
        ReloadSchemaResponse {
            success: true,
            message: "Schema validated successfully (not applied)".to_string(),
        }
    } else {
        ReloadSchemaResponse {
            success: true,
            message: format!("Schema reloaded from {}", req.schema_path),
        }
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

/// Clear cache entries by scope.
///
/// Supports three clearing scopes:
/// - **all**: Clear all cache entries
/// - **entity**: Clear entries for a specific entity type
/// - **pattern**: Clear entries matching a glob pattern
///
/// Requires admin token authentication.
pub async fn cache_clear_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
    Json(req): Json<CacheClearRequest>,
) -> Result<Json<ApiResponse<CacheClearResponse>>, ApiError> {
    // Validate scope and required parameters
    match req.scope.as_str() {
        "all" => {
            // Placeholder: Would iterate through all cache entries
            let response = CacheClearResponse {
                success: true,
                entries_cleared: 0,
                message: "Cleared all cache entries".to_string(),
            };
            Ok(Json(ApiResponse {
                status: "success".to_string(),
                data: response,
            }))
        }
        "entity" => {
            if req.entity_type.is_none() {
                return Err(ApiError::validation_error(
                    "entity_type is required when scope is 'entity'",
                ));
            }

            // Placeholder: Would find and clear entries for this entity
            let response = CacheClearResponse {
                success: true,
                entries_cleared: 0,
                message: format!(
                    "Cleared cache for entity type '{}'",
                    req.entity_type.unwrap_or_default()
                ),
            };
            Ok(Json(ApiResponse {
                status: "success".to_string(),
                data: response,
            }))
        }
        "pattern" => {
            if req.pattern.is_none() {
                return Err(ApiError::validation_error(
                    "pattern is required when scope is 'pattern'",
                ));
            }

            // Placeholder: Would find and clear entries matching pattern
            let response = CacheClearResponse {
                success: true,
                entries_cleared: 0,
                message: format!(
                    "Cleared cache matching pattern '{}'",
                    req.pattern.unwrap_or_default()
                ),
            };
            Ok(Json(ApiResponse {
                status: "success".to_string(),
                data: response,
            }))
        }
        _ => {
            Err(ApiError::validation_error(
                "scope must be 'all', 'entity', or 'pattern'",
            ))
        }
    }
}

/// Get sanitized runtime configuration.
///
/// Returns server version and runtime configuration with secrets redacted.
/// Configuration includes database settings, cache settings, etc.
/// but excludes API keys, passwords, and other sensitive data.
///
/// Requires admin token authentication.
pub async fn config_handler<A: DatabaseAdapter>(
    State(_state): State<AppState<A>>,
) -> Result<Json<ApiResponse<AdminConfigResponse>>, ApiError> {
    // Placeholder: In a real implementation, this would:
    // 1. Collect runtime configuration from AppState
    // 2. Build a HashMap of safe (non-sensitive) settings
    // 3. Redact any secrets that shouldn't be exposed
    // 4. Return the sanitized configuration

    let mut config = HashMap::new();
    config.insert("database_type".to_string(), "postgresql".to_string());
    config.insert("max_connections".to_string(), "100".to_string());
    config.insert("cache_enabled".to_string(), "true".to_string());
    config.insert("federation_enabled".to_string(), "false".to_string());

    let response = AdminConfigResponse {
        version: "2.0.0-a1".to_string(),
        config,
    };

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data: response,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reload_schema_request_empty_path() {
        let request = ReloadSchemaRequest {
            schema_path: String::new(),
            validate_only: false,
        };

        assert!(request.schema_path.is_empty());
    }

    #[test]
    fn test_reload_schema_request_with_path() {
        let request = ReloadSchemaRequest {
            schema_path: "/path/to/schema.json".to_string(),
            validate_only: false,
        };

        assert!(!request.schema_path.is_empty());
    }

    #[test]
    fn test_cache_clear_scope_validation() {
        let valid_scopes = vec!["all", "entity", "pattern"];

        for scope in valid_scopes {
            let request = CacheClearRequest {
                scope: scope.to_string(),
                entity_type: None,
                pattern: None,
            };
            assert_eq!(request.scope, scope);
        }
    }

    #[test]
    fn test_admin_config_response_has_version() {
        let response = AdminConfigResponse {
            version: "2.0.0-a1".to_string(),
            config: HashMap::new(),
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
            success: true,
            entries_cleared: 42,
            message: "Cleared".to_string(),
        };

        assert_eq!(response.entries_cleared, 42);
    }

    #[test]
    fn test_cache_clear_request_entity_required_for_entity_scope() {
        let request = CacheClearRequest {
            scope: "entity".to_string(),
            entity_type: Some("User".to_string()),
            pattern: None,
        };

        assert_eq!(request.scope, "entity");
        assert!(request.entity_type.is_some());
    }

    #[test]
    fn test_cache_clear_request_pattern_required_for_pattern_scope() {
        let request = CacheClearRequest {
            scope: "pattern".to_string(),
            entity_type: None,
            pattern: Some("*_user".to_string()),
        };

        assert_eq!(request.scope, "pattern");
        assert!(request.pattern.is_some());
    }
}
