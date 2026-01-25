# Phase 11.1: Critical - TLS Certificate Validation Security

**Priority**: 🔴 CRITICAL
**CVSS Score**: 9.8
**Effort**: 2 hours
**Duration**: 1 day
**Status**: [ ] Not Started

---

## Objective

Secure TLS certificate validation against man-in-the-middle (MITM) attacks by preventing accidental production use of dangerous certificate bypass modes.

---

## Success Criteria

- [ ] TLS danger mode panics in release builds
- [ ] Runtime warning logged in debug builds when danger mode enabled
- [ ] Default behavior uses system certificate store
- [ ] Certificate validation can't be accidentally disabled
- [ ] Environment variable controls are explicit
- [ ] Tests verify panic behavior in production
- [ ] All existing tests still pass
- [ ] Zero clippy warnings

---

## Security Context

**Vulnerability**: NoVerifier struct in `tls.rs` accepts ANY certificate

**Risk**: If `danger_accept_invalid_certs=true` enabled in production:
- Man-in-the-middle attacks succeed silently
- Database credentials transmitted to attacker
- All query results intercepted
- Authentication tokens captured
- No warning to operators

**Current Mitigation**: Documented as development-only (weak)

**Required Mitigation**: Technical enforcement (strong)

---

## Implementation Plan

### TDD Cycle 1: Panic in Release Builds

#### RED: Write failing test
```rust
#[test]
#[should_panic(expected = "Certificate validation bypass")]
fn test_danger_mode_panics_in_release() {
    #[cfg(not(debug_assertions))]
    {
        // This should panic
        enable_danger_certificate_bypass();
    }
}
```

#### GREEN: Implement minimal panic
```rust
pub fn enable_danger_certificate_bypass() {
    #[cfg(not(debug_assertions))]
    {
        panic!("Certificate validation bypass not allowed in release builds");
    }

    #[cfg(debug_assertions)]
    {
        eprintln!("⚠️  DANGER: TLS CERTIFICATE VALIDATION DISABLED");
    }
}
```

#### REFACTOR: Extract into TLS initialization
```rust
pub fn initialize_tls_config(config: &TlsConfig) -> Result<ServerConfig> {
    // Check danger mode early
    if config.danger_accept_invalid_certs {
        #[cfg(not(debug_assertions))]
        {
            panic!(
                "🚨 CRITICAL: TLS certificate validation bypass not allowed in release builds"
            );
        }

        #[cfg(debug_assertions)]
        {
            eprintln!("🚨 WARNING: Certificate validation is DISABLED");
            eprintln!("🚨 This is only for development/testing with self-signed certificates");
            eprintln!("🚨 NEVER use in production");
        }
    }

    // Continue with normal initialization
    build_rustls_config(config)
}
```

#### CLEANUP
- [ ] Remove debug prints if condition not met
- [ ] Add comment explaining why panic is needed
- [ ] Verify clippy passes

---

### TDD Cycle 2: Environment Variable Validation

#### RED: Write test for env var safety
```rust
#[test]
fn test_env_var_production_check() {
    // Set production env
    std::env::set_var("ENVIRONMENT", "production");

    // Try to enable danger mode
    let result = TlsConfig::from_env();

    // Should fail or refuse to enable danger mode
    assert!(!result.danger_accept_invalid_certs);
}
```

#### GREEN: Add env validation
```rust
impl TlsConfig {
    pub fn from_env() -> Self {
        let danger = std::env::var("FRAISEQL_TLS_DANGER_ACCEPT_INVALID")
            .map(|v| v == "true")
            .unwrap_or(false);

        let is_production = std::env::var("ENVIRONMENT")
            .map(|v| v == "production")
            .unwrap_or(false);

        if danger && is_production {
            eprintln!("🚨 ERROR: Cannot enable TLS danger mode in production");
            eprintln!("🚨 Removing environment variable and using secure mode");
            std::env::remove_var("FRAISEQL_TLS_DANGER_ACCEPT_INVALID");
        }

        Self {
            danger_accept_invalid_certs: danger && !is_production,
            // ... other fields
        }
    }
}
```

#### REFACTOR: Create dedicated function
```rust
pub fn validate_tls_safety(config: &TlsConfig) -> Result<()> {
    // Check 1: Release build
    #[cfg(not(debug_assertions))]
    {
        if config.danger_accept_invalid_certs {
            return Err(FraiseQLError::Security(
                "Certificate validation bypass disabled in release builds".to_string()
            ));
        }
    }

    // Check 2: Production environment
    let is_production = std::env::var("ENVIRONMENT")
        .map(|v| v == "production")
        .unwrap_or(false);

    if config.danger_accept_invalid_certs && is_production {
        return Err(FraiseQLError::Security(
            "Certificate validation bypass cannot be enabled in production".to_string()
        ));
    }

    Ok(())
}
```

#### CLEANUP
- [ ] Verify all tests pass
- [ ] Check error messages are clear
- [ ] Remove any temporary logging

---

### TDD Cycle 3: Logging & Documentation

#### RED: Test that danger mode is logged
```rust
#[test]
fn test_danger_mode_logged_on_startup() {
    let config = TlsConfig {
        danger_accept_invalid_certs: true,
        // ...
    };

    // Should generate a warning log entry
    let _result = initialize_tls_config(&config);

    // Note: Actual log checking depends on logging framework
}
```

#### GREEN: Add startup logging
```rust
pub fn log_tls_configuration(config: &ServerConfig) {
    info!("TLS Configuration:");
    info!("  Certificates: {:?}", config.cert_paths);
    info!("  Validation: {}", if config.danger_accept_invalid_certs {
        "DISABLED (development only)"
    } else {
        "ENABLED (system certificates)"
    });

    if config.danger_accept_invalid_certs {
        warn!("⚠️  TLS Certificate validation is DISABLED");
        warn!("⚠️  This is ONLY for development with self-signed certificates");
        warn!("⚠️  NEVER use in production");
    }
}
```

#### REFACTOR: Integrate with server startup
```rust
pub async fn run_server(config: AppConfig) -> Result<()> {
    // Validate TLS safety
    validate_tls_safety(&config.tls)?;

    // Log configuration
    log_tls_configuration(&config.tls);

    // Continue startup
    // ...
}
```

#### CLEANUP
- [ ] Verify logging at appropriate levels (warn/info)
- [ ] Check no debug prints remain
- [ ] Ensure error messages are operator-friendly

---

## Files to Modify

1. **`crates/fraiseql-wire/src/connection/tls.rs`**
   - Add safety validation function
   - Add logging on initialization
   - Add panic in release builds

2. **`crates/fraiseql-server/src/config.rs`**
   - Validate TLS config on load
   - Check environment variables

3. **`crates/fraiseql-server/src/main.rs`**
   - Log TLS configuration on startup
   - Catch TLS errors early

---

## Tests to Create

```rust
#[cfg(test)]
mod tls_security_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "release builds")]
    #[cfg(not(debug_assertions))]
    fn test_danger_mode_panics_in_release() { }

    #[test]
    fn test_danger_mode_warning_in_debug() { }

    #[test]
    fn test_danger_mode_disabled_in_production() { }

    #[test]
    fn test_default_uses_system_certificates() { }

    #[test]
    fn test_explicit_cert_loading_works() { }

    #[test]
    fn test_invalid_cert_paths_error() { }

    #[test]
    fn test_server_startup_validates_tls() { }
}
```

---

## Configuration Changes

### Before
```toml
[tls]
danger_accept_invalid_certs = true  # Weak enforcement
```

### After
```toml
[tls]
# danger_accept_invalid_certs is ONLY for development
# NEVER enable in production - will be rejected at startup

# Recommended: Use system certificates
server_cert = "/etc/ssl/certs/ca-bundle.crt"
server_key = "/etc/ssl/private/ca-key.pem"

# Or use custom CA
custom_ca = "/path/to/custom/ca.pem"
```

### Environment Variables
```bash
# NEVER set in production:
FRAISEQL_TLS_DANGER_ACCEPT_INVALID=false  # Default: false

# Always set in production:
ENVIRONMENT=production  # Triggers additional validation
```

---

## Verification Checklist

- [ ] Debug builds: warning logged, works with self-signed certs
- [ ] Release builds: panic if danger flag set
- [ ] Production env: panic if danger flag set
- [ ] Default: uses system certificate store
- [ ] Custom certs: can load from paths
- [ ] All tests: passing
- [ ] Clippy: no warnings
- [ ] Startup logging: clear messages

---

## Rollback Plan

If issues arise:
```bash
# Revert this phase only
git revert <commit-hash>

# Verify servers still start
cargo run --release

# Check logs for TLS messages
```

---

## Performance Impact

**Expected**: Negligible
- Configuration validation: <1ms on startup
- Logging: depends on logging framework (typically <1ms)
- Certificate checking: already part of connection setup

---

## Documentation Updates

### SECURITY.md
Add section:
```markdown
## TLS Configuration

### Development (Self-Signed Certificates)
To use self-signed certificates in development:

1. Set `FRAISEQL_TLS_DANGER_ACCEPT_INVALID=true` (debug builds only)
2. The server will log a warning on startup
3. Certificate validation is disabled

### Production (System Certificates)
1. DO NOT set `FRAISEQL_TLS_DANGER_ACCEPT_INVALID`
2. Provide valid certificates via config
3. Certificate validation is enforced
```

### INSTALLATION.md
Add note:
```markdown
For development with self-signed certs, set:
```bash
export FRAISEQL_TLS_DANGER_ACCEPT_INVALID=true
```
Only available in debug builds.
```

---

## Commit Message Template

```
fix(security-11.1): Enforce TLS certificate validation in production

## Changes
- Add runtime panic if danger mode enabled in release builds
- Add environment variable validation (reject danger mode in production)
- Add startup logging for TLS configuration
- Add tests for panic behavior

## Verification
✅ Tests pass (including panic test in release mode)
✅ Clippy clean
✅ Debug builds: warning logged, self-signed certs work
✅ Release builds: danger mode causes panic
✅ Production env: danger mode rejected at startup
```

---

## Dependencies Added

```toml
# No new dependencies needed
# Uses existing standard library and logging framework
```

---

## Phase Status

**Ready**: ✅ Implementation plan complete
**Next**: BEGIN TDD CYCLE 1 - Write failing test for release panic

---

**Review**: [Pending approval]
**Reviewed By**: [Awaiting]
**Approved**: [Awaiting]
