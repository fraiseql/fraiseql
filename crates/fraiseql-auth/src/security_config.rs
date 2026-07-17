//! Security configuration loading and initialization
//!
//! Loads security configuration from schema.compiled.json and initializes
//! all security subsystems (audit logging, rate limiting, error sanitization, etc.)

use std::env;

use serde_json::Value as JsonValue;

/// Security configuration loaded from schema.compiled.json
///
/// Note: rate limiting is intentionally **not** represented here. The live
/// rate-limit configuration is read by the server middleware from the compiled
/// schema's flat `security.rate_limiting` snake_case key
/// (`fraiseql-server middleware/rate_limit/config.rs`, `RateLimitingSecurityConfig`).
/// A former nested-camelCase reader on this struct (`rateLimiting.authStart.maxRequests`)
/// never matched the merger's emitted flat shape, so it silently fed hardcoded
/// defaults; it was removed under #612 (item 5b) to eliminate that drift.
#[derive(Debug, Clone)]
pub struct SecurityConfigFromSchema {
    /// Audit logging configuration
    pub audit_logging:      AuditLoggingSettings,
    /// Error sanitization configuration
    pub error_sanitization: ErrorSanitizationSettings,
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
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON structure contains invalid or unparseable fields.
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
            #[allow(clippy::cast_possible_truncation)]
            // Reason: buffer_size is a config value bounded well within u32 range
            {
                config.audit_logging.buffer_size =
                    audit.get("bufferSize").and_then(|v| v.as_u64()).unwrap_or(1000) as u32;
                config.audit_logging.flush_interval_secs =
                    audit.get("flushIntervalSecs").and_then(|v| v.as_u64()).unwrap_or(5) as u32;
            }
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

        // Rate limiting is read from the compiled schema's flat `security.rate_limiting`
        // key by the server middleware (`RateLimitingSecurityConfig`), not here. The
        // former nested-camelCase reader was removed under #612 (item 5b).

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
            #[allow(clippy::cast_possible_truncation)]
            // Reason: nonce/key sizes are small constants (12, 32) well within u32 range
            {
                config.state_encryption.nonce_size =
                    state_enc.get("nonceSize").and_then(|v| v.as_u64()).unwrap_or(12) as u32;
                config.state_encryption.key_size =
                    state_enc.get("keySize").and_then(|v| v.as_u64()).unwrap_or(32) as u32;
            }
        }

        Ok(config)
    }

    /// Apply environment variable overrides
    pub fn apply_env_overrides(&mut self) {
        // Audit logging
        if let Ok(level) = env::var("AUDIT_LOG_LEVEL") {
            self.audit_logging.log_level = level;
        }

        // Rate limiting env overrides (RATE_LIMIT_*) apply to the live server-side
        // `RateLimitingSecurityConfig`, not to this struct — the nested-camelCase
        // reader that owned them here was removed under #612 (item 5b).
    }
}
