//! Security configuration loading and initialization
//!
//! Loads security configuration from schema.compiled.json and initializes
//! all security subsystems (audit logging, rate limiting, error sanitization, etc.)

use std::env;

use serde_json::Value as JsonValue;

/// Security configuration loaded from schema.compiled.json
#[derive(Debug, Clone)]
pub struct SecurityConfigFromSchema {
    /// Audit logging configuration
    pub audit_logging:      AuditLoggingSettings,
    /// Error sanitization configuration
    pub error_sanitization: ErrorSanitizationSettings,
    /// Rate limiting configuration
    pub rate_limiting:      RateLimitingSettings,
    /// State encryption configuration
    pub state_encryption:   StateEncryptionSettings,
}

/// Audit logging subsystem settings loaded from the compiled schema.
#[derive(Debug, Clone)]
pub struct AuditLoggingSettings {
    /// Whether audit logging is active.
    pub enabled:                bool,
    /// Minimum tracing level for audit records (e.g., `"info"`, `"debug"`).
    pub log_level:              String,
    /// When `true`, raw credential values may appear in log records.
    /// Must be `false` in production deployments.
    pub include_sensitive_data: bool,
    /// When `true`, log records are written from a background task rather than
    /// the request thread, reducing latency at the cost of some delivery guarantees.
    pub async_logging:          bool,
    /// Number of audit records to buffer before flushing (async mode only).
    pub buffer_size:            u32,
    /// How frequently (in seconds) the async buffer is flushed.
    pub flush_interval_secs:    u32,
}

/// Error sanitization settings — controls how authentication errors are presented to clients.
#[derive(Debug, Clone)]
pub struct ErrorSanitizationSettings {
    /// Whether error sanitization is active.
    /// When `false`, internal error details may be forwarded to API clients.
    pub enabled:                bool,
    /// Replace specific internal error messages with generic user-safe strings.
    pub generic_messages:       bool,
    /// Log the full internal error message via `tracing` before sanitizing.
    pub internal_logging:       bool,
    /// When `true`, sensitive field values (tokens, keys, etc.) may appear in error messages.
    /// **Must be `false` in production** — setting this to `true` fails
    /// [`crate::security_init::validate_security_config`].
    pub leak_sensitive_details: bool,
    /// Format template for user-facing error messages (e.g., `"generic"`).
    pub user_facing_format:     String,
}

/// Rate-limiting thresholds for each authentication endpoint.
#[derive(Debug, Clone)]
pub struct RateLimitingSettings {
    /// Whether rate limiting is active across all auth endpoints.
    pub enabled:                    bool,
    /// Maximum requests to `/auth/start` per IP per window.
    pub auth_start_max_requests:    u32,
    /// Window duration (in seconds) for the `/auth/start` rate limit.
    pub auth_start_window_secs:     u64,
    /// Maximum requests to `/auth/callback` per IP per window.
    pub auth_callback_max_requests: u32,
    /// Window duration (in seconds) for the `/auth/callback` rate limit.
    pub auth_callback_window_secs:  u64,
    /// Maximum token refresh requests per user per window.
    pub auth_refresh_max_requests:  u32,
    /// Window duration (in seconds) for the `/auth/refresh` rate limit.
    pub auth_refresh_window_secs:   u64,
    /// Maximum logout requests per user per window.
    pub auth_logout_max_requests:   u32,
    /// Window duration (in seconds) for the `/auth/logout` rate limit.
    pub auth_logout_window_secs:    u64,
    /// Maximum failed login attempts per user per window (brute-force protection).
    pub failed_login_max_requests:  u32,
    /// Window duration (in seconds) for the failed login rate limit (typically 1 hour).
    pub failed_login_window_secs:   u64,
}

/// OAuth state encryption settings loaded from the compiled schema.
#[derive(Debug, Clone)]
pub struct StateEncryptionSettings {
    /// Whether OAuth PKCE state tokens are encrypted before being sent to the provider.
    pub enabled:              bool,
    /// AEAD algorithm to use (e.g., `"chacha20-poly1305"`, `"aes-256-gcm"`).
    pub algorithm:            String,
    /// When `true`, keys are rotated automatically.
    pub key_rotation_enabled: bool,
    /// Nonce size in bytes (must be 12 for both ChaCha20-Poly1305 and AES-256-GCM).
    pub nonce_size:           u32,
    /// Encryption key size in bytes (must be 32 for both supported algorithms).
    pub key_size:             u32,
}

impl Default for SecurityConfigFromSchema {
    fn default() -> Self {
        Self {
            audit_logging:      AuditLoggingSettings {
                enabled:                true,
                log_level:              "info".to_string(),
                include_sensitive_data: false,
                async_logging:          true,
                buffer_size:            1000,
                flush_interval_secs:    5,
            },
            error_sanitization: ErrorSanitizationSettings {
                enabled:                true,
                generic_messages:       true,
                internal_logging:       true,
                leak_sensitive_details: false,
                user_facing_format:     "generic".to_string(),
            },
            rate_limiting:      RateLimitingSettings {
                enabled:                    true,
                auth_start_max_requests:    100,
                auth_start_window_secs:     60,
                auth_callback_max_requests: 50,
                auth_callback_window_secs:  60,
                auth_refresh_max_requests:  10,
                auth_refresh_window_secs:   60,
                auth_logout_max_requests:   20,
                auth_logout_window_secs:    60,
                failed_login_max_requests:  5,
                failed_login_window_secs:   3600,
            },
            state_encryption:   StateEncryptionSettings {
                enabled:              true,
                algorithm:            "chacha20-poly1305".to_string(),
                key_rotation_enabled: false,
                nonce_size:           12,
                key_size:             32,
            },
        }
    }
}

impl SecurityConfigFromSchema {
    /// Parse security configuration from JSON (from schema.compiled.json)
    pub fn from_json(value: &JsonValue) -> anyhow::Result<Self> {
        let mut config = Self::default();

        if let Some(audit) = value.get("auditLogging").and_then(|v| v.as_object()) {
            config.audit_logging.enabled =
                audit.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            config.audit_logging.log_level =
                audit.get("logLevel").and_then(|v| v.as_str()).unwrap_or("info").to_string();
            config.audit_logging.include_sensitive_data =
                audit.get("includeSensitiveData").and_then(|v| v.as_bool()).unwrap_or(false);
            config.audit_logging.async_logging =
                audit.get("asyncLogging").and_then(|v| v.as_bool()).unwrap_or(true);
            config.audit_logging.buffer_size =
                audit.get("bufferSize").and_then(|v| v.as_u64()).unwrap_or(1000) as u32;
            config.audit_logging.flush_interval_secs =
                audit.get("flushIntervalSecs").and_then(|v| v.as_u64()).unwrap_or(5) as u32;
        }

        if let Some(error_san) = value.get("errorSanitization").and_then(|v| v.as_object()) {
            config.error_sanitization.enabled =
                error_san.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            config.error_sanitization.generic_messages =
                error_san.get("genericMessages").and_then(|v| v.as_bool()).unwrap_or(true);
            config.error_sanitization.internal_logging =
                error_san.get("internalLogging").and_then(|v| v.as_bool()).unwrap_or(true);
            config.error_sanitization.leak_sensitive_details =
                error_san.get("leakSensitiveDetails").and_then(|v| v.as_bool()).unwrap_or(false);
            config.error_sanitization.user_facing_format = error_san
                .get("userFacingFormat")
                .and_then(|v| v.as_str())
                .unwrap_or("generic")
                .to_string();
        }

        if let Some(rate_limit) = value.get("rateLimiting").and_then(|v| v.as_object()) {
            config.rate_limiting.enabled =
                rate_limit.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

            if let Some(auth_start) = rate_limit.get("authStart").and_then(|v| v.as_object()) {
                config.rate_limiting.auth_start_max_requests =
                    auth_start.get("maxRequests").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
                config.rate_limiting.auth_start_window_secs =
                    auth_start.get("windowSecs").and_then(|v| v.as_u64()).unwrap_or(60);
            }

            if let Some(auth_callback) = rate_limit.get("authCallback").and_then(|v| v.as_object())
            {
                config.rate_limiting.auth_callback_max_requests =
                    auth_callback.get("maxRequests").and_then(|v| v.as_u64()).unwrap_or(50) as u32;
                config.rate_limiting.auth_callback_window_secs =
                    auth_callback.get("windowSecs").and_then(|v| v.as_u64()).unwrap_or(60);
            }

            if let Some(auth_refresh) = rate_limit.get("authRefresh").and_then(|v| v.as_object()) {
                config.rate_limiting.auth_refresh_max_requests =
                    auth_refresh.get("maxRequests").and_then(|v| v.as_u64()).unwrap_or(10) as u32;
                config.rate_limiting.auth_refresh_window_secs =
                    auth_refresh.get("windowSecs").and_then(|v| v.as_u64()).unwrap_or(60);
            }

            if let Some(auth_logout) = rate_limit.get("authLogout").and_then(|v| v.as_object()) {
                config.rate_limiting.auth_logout_max_requests =
                    auth_logout.get("maxRequests").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
                config.rate_limiting.auth_logout_window_secs =
                    auth_logout.get("windowSecs").and_then(|v| v.as_u64()).unwrap_or(60);
            }

            if let Some(failed_login) = rate_limit.get("failedLogin").and_then(|v| v.as_object()) {
                config.rate_limiting.failed_login_max_requests =
                    failed_login.get("maxRequests").and_then(|v| v.as_u64()).unwrap_or(5) as u32;
                config.rate_limiting.failed_login_window_secs =
                    failed_login.get("windowSecs").and_then(|v| v.as_u64()).unwrap_or(3600);
            }
        }

        if let Some(state_enc) = value.get("stateEncryption").and_then(|v| v.as_object()) {
            config.state_encryption.enabled =
                state_enc.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            config.state_encryption.algorithm = state_enc
                .get("algorithm")
                .and_then(|v| v.as_str())
                .unwrap_or("chacha20-poly1305")
                .to_string();
            config.state_encryption.key_rotation_enabled =
                state_enc.get("keyRotationEnabled").and_then(|v| v.as_bool()).unwrap_or(false);
            config.state_encryption.nonce_size =
                state_enc.get("nonceSize").and_then(|v| v.as_u64()).unwrap_or(12) as u32;
            config.state_encryption.key_size =
                state_enc.get("keySize").and_then(|v| v.as_u64()).unwrap_or(32) as u32;
        }

        Ok(config)
    }

    /// Apply environment variable overrides
    pub fn apply_env_overrides(&mut self) {
        // Audit logging
        if let Ok(level) = env::var("AUDIT_LOG_LEVEL") {
            self.audit_logging.log_level = level;
        }

        // Rate limiting
        if let Ok(val) = env::var("RATE_LIMIT_AUTH_START") {
            if let Ok(n) = val.parse() {
                self.rate_limiting.auth_start_max_requests = n;
            }
        }
        if let Ok(val) = env::var("RATE_LIMIT_AUTH_CALLBACK") {
            if let Ok(n) = val.parse() {
                self.rate_limiting.auth_callback_max_requests = n;
            }
        }
        if let Ok(val) = env::var("RATE_LIMIT_AUTH_REFRESH") {
            if let Ok(n) = val.parse() {
                self.rate_limiting.auth_refresh_max_requests = n;
            }
        }
        if let Ok(val) = env::var("RATE_LIMIT_AUTH_LOGOUT") {
            if let Ok(n) = val.parse() {
                self.rate_limiting.auth_logout_max_requests = n;
            }
        }
        if let Ok(val) = env::var("RATE_LIMIT_FAILED_LOGIN") {
            if let Ok(n) = val.parse() {
                self.rate_limiting.failed_login_max_requests = n;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]
    // Reason: test modules use wildcard imports for conciseness
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SecurityConfigFromSchema::default();
        assert!(config.audit_logging.enabled);
        assert!(config.error_sanitization.enabled);
        assert!(config.rate_limiting.enabled);
        assert!(config.state_encryption.enabled);
    }

    #[test]
    fn test_parse_from_json() {
        let json = serde_json::json!({
            "auditLogging": {
                "enabled": true,
                "logLevel": "debug",
                "includeSensitiveData": false
            },
            "rateLimiting": {
                "enabled": true,
                "authStart": {
                    "maxRequests": 200,
                    "windowSecs": 60
                }
            }
        });

        let config = SecurityConfigFromSchema::from_json(&json).expect("Failed to parse");
        assert_eq!(config.audit_logging.log_level, "debug");
        assert_eq!(config.rate_limiting.auth_start_max_requests, 200);
    }

    #[test]
    fn test_apply_env_overrides() {
        // Note: This test would require setting env vars during test execution
        // For now, we just verify the method works with defaults
        let mut config = SecurityConfigFromSchema::default();
        config.apply_env_overrides();
        // No assertions needed, just verify it doesn't panic
    }
}
