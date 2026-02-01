# Constant-Time Comparison Integration Guide
# Phase 7, Cycle 3: REFACTOR phase - Integration points

## Overview

The `ConstantTimeOps` utility provides constant-time comparison functions for preventing timing attacks on token validation. This document identifies key integration points and provides guidance for applying constant-time comparison throughout the authentication system.

## Critical Integration Points

### 1. JWT Token Comparison (jwt.rs)

**Current Status**: JWT signature verification is handled by the `jsonwebtoken` crate, which uses the `subtle` crate internally for constant-time comparison.

**Integration**: No changes needed - already protected by underlying library.

**Reference**: `JwtValidator::validate()` in jwt.rs line 106

### 2. Session Token Hash Comparison (session.rs / session_postgres.rs)

**Current Status**: Tokens are hashed using SHA256 before storage. Hash comparison happens in session store implementation.

**Recommended Approach**:
```rust
// In SessionStore::validate_session_token() or similar
use crate::auth::constant_time::ConstantTimeOps;

let provided_token_hash = hash_token(&provided_token);
let stored_token_hash = db_session.refresh_token_hash;

// Use constant-time comparison instead of:
// if provided_token_hash == stored_token_hash { ... }

let tokens_match = ConstantTimeOps::compare_hmac(
    provided_token_hash.as_bytes(),
    stored_token_hash.as_bytes()
);
if tokens_match { ... }
```

**Files to Modify**: `session_postgres.rs`

### 3. CSRF State Token Comparison (state_store.rs)

**Current Status**: State lookup in `InMemoryStateStore::retrieve()` uses HashMap key lookup, which is constant-time for the hash lookup but the key comparison might leak timing.

**Recommended Approach**:
```rust
// When implementing StateStore::retrieve() with custom comparison:
use crate::auth::constant_time::ConstantTimeOps;

// Instead of direct HashMap lookup with potential timing leak:
// if let Some(value) = self.states.remove(&query_state) { ... }

// Use constant-time comparison:
for entry in self.states.iter() {
    if ConstantTimeOps::compare_str(entry.key(), &query_state) {
        let value = entry.remove();
        return Ok(value);
    }
}
```

**Files to Modify**: `state_store.rs` (both InMemoryStateStore and RedisStateStore)

### 4. PKCE Code Verifier Comparison (handlers.rs)

**Current Status**: PKCE code verifier is validated against the stored verifier in the OAuth flow.

**Recommended Approach**:
```rust
// In auth_callback() when validating PKCE:
use crate::auth::constant_time::ConstantTimeOps;

let provided_verifier = extract_pkce_verifier(&request);
let stored_verifier = session.pkce_verifier;

let verifier_valid = ConstantTimeOps::compare_pkce_verifier(
    &stored_verifier,
    &provided_verifier
);
if !verifier_valid { return Err(...); }
```

**Files to Modify**: `handlers.rs` (in auth_callback function)

### 5. Session Revocation Token Comparison

**Current Status**: Session tokens are revoked using hash lookup.

**Recommended Approach**:
```rust
// In auth_logout():
use crate::auth::constant_time::ConstantTimeOps;

let token_hash = hash_token(&refresh_token);

// Use constant-time comparison when validating before revocation
let session_exists = state.session_store.verify_session_exists_constant_time(
    &token_hash
).await?;

if session_exists {
    state.session_store.revoke_session(&token_hash).await?;
}
```

**Files to Modify**: `handlers.rs` (in auth_logout function)

## Implementation Priority

1. **HIGH**: Session token hash comparison (session_postgres.rs)
   - Used on every refresh token validation
   - Most frequent operation

2. **HIGH**: CSRF state token comparison (state_store.rs)
   - Used once per OAuth flow
   - Critical for CSRF protection

3. **MEDIUM**: PKCE code verifier comparison (handlers.rs)
   - Used once per OAuth flow
   - Important for PKCE compliance

4. **MEDIUM**: Session revocation token comparison (handlers.rs)
   - Used once per logout
   - Less frequent but security-critical

## API Reference

All functions in `ConstantTimeOps` are located in `constant_time.rs`:

```rust
// General byte comparison
pub fn compare(expected: &[u8], actual: &[u8]) -> bool

// String comparison
pub fn compare_str(expected: &str, actual: &str) -> bool

// Length-safe comparison (handles different lengths)
pub fn compare_len_safe(expected: &[u8], actual: &[u8]) -> bool

// Token-specific helpers
pub fn compare_jwt(expected: &str, actual: &str) -> bool
pub fn compare_session_token(expected: &str, actual: &str) -> bool
pub fn compare_csrf_token(expected: &str, actual: &str) -> bool
pub fn compare_hmac(expected: &[u8], actual: &[u8]) -> bool
pub fn compare_refresh_token(expected: &str, actual: &str) -> bool
pub fn compare_auth_code(expected: &str, actual: &str) -> bool
pub fn compare_pkce_verifier(expected: &str, actual: &str) -> bool
pub fn compare_state_token(expected: &str, actual: &str) -> bool
```

## Testing Constant-Time Comparison

Unit tests are included in `constant_time.rs` covering:
- Basic equality/inequality
- Different lengths
- Null bytes and all byte values
- Very long tokens (10KB+)
- Token-specific comparison functions

Run tests with:
```bash
cargo test constant_time::tests
```

## Security Guarantees

- Comparisons take constant time regardless of mismatch position
- Uses `subtle` crate's `ConstantTimeEq` trait
- Prevents attackers from inferring token validity through response timing
- Effective against both attackers measuring direct response times and statistical timing analysis

## Performance Impact

- Minimal: typically < 1 microsecond per comparison
- Constant-time guarantees don't add measurable latency for typical token lengths (< 2KB)
- Negligible compared to database operations (typically 1-10ms)

## Notes

- The `subtle` crate uses CPU-level constant-time primitives where available
- On some platforms/CPUs, true constant-time may not be achievable; `subtle` provides best-effort implementation
- Always compare the provided (untrusted) value against the known (trusted) value
- `compare_len_safe()` should be used when comparing values of potentially different lengths
