# Security Runtime Initialization

This document describes how FraiseQL v2.0.0-alpha.1 loads and initializes security configuration at server runtime.

## Architecture Overview

Security configuration flows through the system in three stages:

```text
┌──────────────────┐
│ FraiseQL.toml    │  Developer specifies configuration
└────────┬─────────┘
         │ (FraiseQL-cli compile)
         ↓
┌──────────────────────────┐
│ schema.compiled.json     │  Config embedded in schema
│ "security": {...}        │
└────────┬─────────────────┘
         │ (Server startup)
         ↓
┌──────────────────────────┐
│ RuntimeSecurityConfig    │  Loaded and validated
│ + env overrides          │
└──────────────────────────┘
```text

## Loading Security Configuration

### At Server Startup

When the FraiseQL server starts, it performs the following steps:

1. **Load compiled schema** from `schema.compiled.json`
2. **Extract security section** from the compiled schema JSON
3. **Parse configuration** into `SecurityConfigFromSchema` struct
4. **Apply environment variable overrides** for production deployments
5. **Validate configuration** to catch dangerous settings
6. **Log configuration** for observability

### Code Location

- `crates/FraiseQL-server/src/auth/security_init.rs` - Initialization functions
- `crates/FraiseQL-server/src/auth/security_config.rs` - Configuration parsing
- `crates/FraiseQL-server/src/main.rs` - Integration into server startup

### Example Flow in main.rs

```rust
// Load compiled schema
let schema = schema_loader.load().await?;

// Initialize security configuration from schema
let security_config = init_security_config(&schema.to_json()?)
    .unwrap_or_else(|e| {
        warn!("Using default security configuration: {e}");
        init_default_security_config()
    });

// Validate configuration
validate_security_config(&security_config)?;

// Log configuration for observability
log_security_config(&security_config);
```text

## Environment Variable Overrides

The loaded configuration supports environment variable overrides for production use. This allows operators to customize settings without recompiling the schema.

### Supported Environment Variables

```bash
# Audit logging
AUDIT_LOG_LEVEL=debug                    # Override log level

# Rate limiting
RATE_LIMIT_AUTH_START=200                # Override auth/start max requests
RATE_LIMIT_AUTH_CALLBACK=100             # Override auth/callback max requests
RATE_LIMIT_AUTH_REFRESH=20               # Override auth/refresh max requests
RATE_LIMIT_AUTH_LOGOUT=50                # Override auth/logout max requests
RATE_LIMIT_FAILED_LOGIN=3                # Override failed login max requests

# State encryption
STATE_ENCRYPTION_KEY=<base64-encoded-32-byte-key>  # Override encryption key
```text

### Example

```bash
# Start server with custom rate limiting
RATE_LIMIT_FAILED_LOGIN=1 RATE_LIMIT_AUTH_START=50 \
  FraiseQL-server --schema schema.compiled.json
```text

## Configuration Validation

The server validates the loaded security configuration before startup:

```rust
pub fn validate_security_config(config: &SecurityConfigFromSchema) -> Result<()> {
    // Ensure sensitive data leaking is disabled
    if config.error_sanitization.leak_sensitive_details {
        return Err(AuthError::ConfigError {
            message: "leak_sensitive_details must be false in production".to_string(),
        });
    }

    // Ensure rate limit windows are valid
    if config.rate_limiting.auth_start_window_secs == 0 {
        return Err(AuthError::ConfigError {
            message: "auth_start_window_secs must be greater than 0".to_string(),
        });
    }

    Ok(())
}
```text

## Default Configuration

If the schema doesn't include a security section or loading fails, sensible defaults are used:

```rust
pub fn init_default_security_config() -> SecurityConfigFromSchema {
    SecurityConfigFromSchema {
        audit_logging: AuditLoggingSettings {
            enabled: true,
            log_level: "info",
            include_sensitive_data: false,
            async_logging: true,
            buffer_size: 1000,
            flush_interval_secs: 5,
        },
        error_sanitization: ErrorSanitizationSettings {
            enabled: true,
            generic_messages: true,
            internal_logging: true,
            leak_sensitive_details: false,
            user_facing_format: "generic",
        },
        rate_limiting: RateLimitingSettings {
            enabled: true,
            auth_start_max_requests: 100,
            auth_start_window_secs: 60,
            // ... other settings
        },
        state_encryption: StateEncryptionSettings {
            enabled: true,
            algorithm: "chacha20-poly1305",
            key_rotation_enabled: false,
            nonce_size: 12,
            key_size: 32,
        },
    }
}
```text

## Observability

The loaded configuration is logged at startup for audit and debugging purposes:

```text
INFO Audit logging configuration:
     audit_logging_enabled=true, audit_log_level=info,
     audit_async_logging=true, audit_buffer_size=1000

INFO Error sanitization configuration:
     error_sanitization_enabled=true, error_generic_messages=true,
     error_internal_logging=true, error_leak_sensitive=false

INFO Rate limiting configuration:
     rate_limiting_enabled=true, auth_start_max=100,
     auth_callback_max=50, auth_refresh_max=10, failed_login_max=5

INFO State encryption configuration:
     state_encryption_enabled=true, state_encryption_algorithm=chacha20-poly1305,
     state_encryption_nonce_size=12, state_encryption_key_size=32
```text

## Integration with Security Subsystems

The loaded `SecurityConfigFromSchema` provides values for initializing:

1. **Audit Logger** - Uses audit_logging settings (log_level, async_logging, buffer_size)
2. **Error Sanitizer** - Uses error_sanitization settings (generic_messages, leak_sensitive_details)
3. **Rate Limiters** - Uses rate_limiting settings (max requests, window duration)
4. **State Encryption** - Uses state_encryption settings (algorithm, key size, nonce size)

Example usage:

```rust
// Initialize rate limiter from config
let auth_start_limiter = KeyedRateLimiter::new(
    RateLimitConfig {
        max_requests: security_config.rate_limiting.auth_start_max_requests,
        window_secs: security_config.rate_limiting.auth_start_window_secs,
    }
);

// Initialize state encryption from config
let state_encryption = StateEncryption::new(&encryption_key)?;
```text

## Testing

Comprehensive tests verify the security configuration flow:

### Unit Tests (`crates/FraiseQL-server/src/auth/security_init.rs`)

- `test_init_default_security_config()` - Verify default values
- `test_validate_security_config_success()` - Valid configuration passes
- `test_validate_security_config_leak_sensitive_fails()` - Dangerous config rejected
- `test_init_security_config_from_json()` - Parse from JsonValue
- `test_init_security_config_from_string()` - Parse from string
- `test_init_security_config_missing_section()` - Missing security section handled

### Integration Tests (`crates/FraiseQL-server/tests/security_config_runtime_test.rs`)

- `test_security_config_parsing_from_schema()` - Full schema parsing
- `test_security_config_initialization_with_defaults()` - Custom + default values
- `test_security_config_validation()` - Configuration validation
- `test_security_config_default_values()` - Default values are sensible
- `test_security_config_complete_schema()` - Real-world schema structure
- `test_security_config_missing_optional_fields()` - Partial configuration
- `test_security_config_rate_limit_windows()` - Rate limit configurations

## Development Workflow

### To Add a New Configuration Field

1. Add field to `SecurityConfigFromSchema` struct in `security_config.rs`
2. Add parsing logic in `from_json()` method with default fallback
3. Add environment variable override in `apply_env_overrides()` if applicable
4. Update tests to verify parsing and defaults
5. Update `SECURITY_CONFIGURATION.md` with documentation

### To Add a New Security Subsystem

1. Initialize subsystem using values from `SecurityConfigFromSchema`
2. Apply configuration in main.rs after loading security config
3. Pass initialized subsystem to Server constructor
4. Add tests verifying initialization from configuration

## Future Enhancements

- Hot-reload configuration without server restart
- Configuration validation in CLI before compilation
- Per-tenant security configuration
- Dynamic rate limit adjustment based on metrics
- Encryption key rotation support

## See Also

- [SECURITY_CONFIGURATION.md](./SECURITY_CONFIGURATION.md) - Configuration reference
- [Authentication Design](../architecture/security/authentication-detailed.md) - Authentication system architecture
