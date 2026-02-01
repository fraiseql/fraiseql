# Phase 7: Enterprise Security & Features

**Status**: 📋 PLANNED (Next Phase)
**Objective**: Implement comprehensive security enhancements for enterprise deployments
**Expected Duration**: 2-3 days (17 hours estimated)
**Target Rating**: 8.2 → 9.2/10
**Release**: v2.1.0 (minor version)

---

## Overview

Phase 7 builds on the production-ready foundation of Phase 6 to add enterprise-grade security features. This phase implements five complementary security improvements that together raise FraiseQL from 8.2/10 to 9.2/10 security rating.

**Key Principle**: Defense-in-depth. Each improvement adds a layer of protection.

---

## Phase Objective

Transform FraiseQL v2 from production-ready to enterprise-grade by implementing:

1. **Audit Logging** — Track secret access for compliance
2. **Error Sanitization** — Hide internals from attackers
3. **Constant-Time Comparison** — Prevent timing attacks
4. **PKCE State Encryption** — Protect OAuth state
5. **Rate Limiting** — Prevent brute force attacks

Result: Security rating 9.2/10, enterprise-ready implementation.

---

## Success Criteria

- ✅ All 5 security improvements implemented
- ✅ Security tests passing (expansion of Phase 6 Cycle 3 suite)
- ✅ Zero performance degradation
- ✅ Backward compatible (no breaking changes)
- ✅ Can be released as v2.1.0 patch
- ✅ 9.2/10 security rating achieved
- ✅ Documentation updated
- ✅ All lints passing

---

## TDD Cycles

### Cycle 1: Audit Logging for Secret Access

**Objective**: Track all secret access for compliance and monitoring

**RED Phase**
- Write tests for audit logging:
  - JWT validation logging
  - OIDC credential access logging
  - Session token access logging
  - Failed secret access attempts
  - Timestamp and user context included
- Create audit log structure
- Verify no logs currently exist for secret access

**GREEN Phase**
- Add structured logging around all secret operations:
  - `auth/handlers.rs` — OIDC flow, JWT validation
  - `auth/jwt.rs` — Token validation, signature checks
  - `auth/oidc.rs` — Client secret usage
  - `auth/session.rs` — Session token access
- Use structured logging with context (user, timestamp, event type)
- Implement audit log level (separate from regular logs)

**REFACTOR Phase**
- Consolidate logging patterns
- Extract logging into reusable trait
- Ensure consistency across all auth modules
- No code duplication

**CLEANUP Phase**
- Verify all audit points covered
- Format and lint
- Document audit logging configuration
- Update README with audit logging guide

**Files to Modify**:
- `crates/fraiseql-server/src/auth/handlers.rs`
- `crates/fraiseql-server/src/auth/jwt.rs`
- `crates/fraiseql-server/src/auth/oidc.rs`
- `crates/fraiseql-server/src/auth/session.rs`
- `crates/fraiseql-server/src/observability/logging.rs` (audit extension)

**Dependencies**: None (use existing `tracing`)

**Tests**: 8-10 new audit logging tests

---

### Cycle 2: Error Message Sanitization

**Objective**: Hide implementation details in user-facing errors

**RED Phase**
- Write tests showing current error leakage:
  - JWT validation errors expose signature details
  - OIDC errors expose issuer URL details
  - Database errors expose schema info
- Define error sanitization rules
- Test public vs internal error messages

**GREEN Phase**
- Create sanitization layer in error handler:
  - User-facing: Generic messages ("Authentication failed")
  - Internal logs: Full details for debugging
- Update all auth error types to include both
- Add sanitization middleware for HTTP responses
- Implement for database errors as well

**REFACTOR Phase**
- Extract error sanitization into trait
- Use trait objects for different error types
- Consolidate sanitization logic
- Ensure no details leak in responses

**CLEANUP Phase**
- Verify all error paths sanitized
- Format and lint
- Document error handling patterns
- Add examples to docs

**Files to Modify**:
- `crates/fraiseql-server/src/auth/error.rs`
- `crates/fraiseql-server/src/error/mod.rs`
- `crates/fraiseql-server/src/handlers/middleware.rs` (error handler)
- `crates/fraiseql-core/src/error.rs`

**Dependencies**: None (use existing error infrastructure)

**Tests**: 12-15 error sanitization tests

---

### Cycle 3: Constant-Time Token Comparison

**Objective**: Prevent timing attacks on token validation

**RED Phase**
- Write tests demonstrating timing attack vulnerability:
  - Valid vs invalid token timing difference measurable
  - Same-length tokens with different prefixes
  - Comparison time varies with mismatch position
- Verify current implementation is vulnerable
- Plan constant-time comparison

**GREEN Phase**
- Add `subtle` crate for constant-time operations
- Replace all token comparisons:
  - JWT signature verification
  - Session token comparison
  - CSRF token validation
  - PKCE state verification
- Use `subtle::ConstantTimeComparison` trait

**REFACTOR Phase**
- Extract token comparison into helper function
- Use for all sensitive comparisons
- Ensure no shortcut comparisons remain
- Profile for performance (should be negligible)

**CLEANUP Phase**
- Verify all token comparisons constant-time
- Run security tests
- Performance benchmarks
- Format and lint

**Files to Modify**:
- `crates/fraiseql-server/src/auth/jwt.rs`
- `crates/fraiseql-server/src/auth/handlers.rs`
- `crates/fraiseql-server/src/auth/session.rs`
- `crates/fraiseql-server/src/auth/csrf.rs`

**Dependencies**: Add `subtle = "2.5"`

**Tests**: 10-12 timing attack prevention tests

---

### Cycle 4: PKCE State Encryption

**Objective**: Protect OAuth PKCE state from tampering

**RED Phase**
- Write tests for state encryption:
  - State is encrypted before storage
  - State is decrypted on verification
  - Tampered state fails verification
  - Expired state rejected even if valid
  - Wrong key decryption fails
- Verify current state storage is unencrypted
- Plan encryption strategy

**GREEN Phase**
- Add `chacha20poly1305` crate for AEAD encryption
- Create state encryption/decryption module
- Encrypt state before storing in session
- Decrypt and verify on PKCE callback
- Handle decryption failures gracefully

**REFACTOR Phase**
- Extract encryption into reusable trait
- Consolidate state handling
- Ensure key rotation not needed (single per deployment)
- Consider state versioning for future key rotation

**CLEANUP Phase**
- Verify all state encrypted
- Performance benchmarks (should be <1ms)
- Format and lint
- Document PKCE state encryption

**Files to Modify**:
- `crates/fraiseql-server/src/auth/handlers.rs`
- `crates/fraiseql-server/src/auth/state_store.rs` (new file)
- `crates/fraiseql-server/src/auth/pkce.rs`

**Dependencies**: Add `chacha20poly1305 = "0.10"`

**Tests**: 10-12 PKCE encryption tests

---

### Cycle 5: Rate Limiting on Auth Endpoints

**Objective**: Prevent brute force attacks

**RED Phase**
- Write tests for rate limiting:
  - 5 failed login attempts → lockout
  - Lockout duration: 15 minutes
  - Successful login resets counter
  - Different IP addresses have separate limits
  - Header-based rate limiting works
- Verify no rate limiting currently exists
- Plan rate limiting strategy

**GREEN Phase**
- Add `governor` crate for rate limiting
- Implement per-IP rate limiting:
  - Auth endpoints: 60 requests/minute
  - Failed login: 5 attempts before 15min lockout
  - Token refresh: 120 requests/minute
- Create rate limiter middleware
- Return proper HTTP 429 responses

**REFACTOR Phase**
- Extract rate limiting into pluggable trait
- Support different strategies (IP-based, token-based, etc.)
- Consolidate limit configuration
- Enable/disable via configuration

**CLEANUP Phase**
- Verify all auth endpoints rate limited
- Performance benchmarks (<1ms overhead)
- Format and lint
- Document rate limiting configuration

**Files to Modify**:
- `crates/fraiseql-server/src/auth/rate_limiter.rs` (new file)
- `crates/fraiseql-server/src/handlers/middleware.rs`
- `crates/fraiseql-server/src/config/mod.rs`

**Dependencies**: Add `governor = "0.10"`

**Tests**: 12-15 rate limiting tests

---

### Cycle 6: Integration Testing & Documentation

**Objective**: Verify all security improvements work together

**RED Phase**
- Write end-to-end security tests:
  - Full auth flow with all security features
  - Audit logs generated correctly
  - Errors sanitized throughout
  - Timing attacks prevented
  - State protected
  - Rate limiting enforced
- Write docs for security configuration
- Plan deployment guide updates

**GREEN Phase**
- Create E2E test harness
- Test all security features together
- Verify no conflicts or regressions
- Update configuration examples
- Document best practices

**REFACTOR Phase**
- Consolidate security test suite
- Extract common test patterns
- Improve test readability
- Better test organization

**CLEANUP Phase**
- All tests passing
- Security documentation complete
- Deployment guide updated
- Migration guide (if needed)
- Format and lint

**Files to Modify**:
- `crates/fraiseql-server/src/auth/security_tests.rs` (expand)
- `docs/enterprise/security-hardening.md` (new)
- `docs/deployment/rate-limiting.md` (new)
- `docs/deployment/audit-logging.md` (new)
- `SECURITY.md` (update)

**Tests**: 20-25 integration tests

---

## Dependencies to Add

Update `crates/fraiseql-server/Cargo.toml`:

```toml
[dependencies]
subtle = "2.5"                    # Constant-time operations
governor = "0.10"                # Rate limiting
chacha20poly1305 = "0.10"       # AEAD encryption

[dev-dependencies]
criterion = "0.5"                # Benchmarking for cycle 3/4/5
```

All are well-maintained, audited, widely-used crates.

---

## Implementation Plan

### Week 1 (Day 1-2): Core Security Features
```
Cycle 1: Audit Logging            (4-5 hours)
Cycle 2: Error Sanitization        (3-4 hours)
Cycle 3: Constant-Time Comparison (2-3 hours)
```

**Checkpoint**: Security rating 8.8/10

### Week 1 (Day 3): Advanced Features
```
Cycle 4: PKCE Encryption           (3-4 hours)
Cycle 5: Rate Limiting             (4-5 hours)
```

**Checkpoint**: Security rating 9.15/10

### Week 2 (Day 1): Testing & Docs
```
Cycle 6: Integration & Documentation (4-5 hours)
```

**Final**: Security rating 9.2/10 ✅

---

## Risk Assessment

### Low Risk Items
- ✅ Audit logging (no breaking changes)
- ✅ Error sanitization (backward compatible)
- ✅ Constant-time comparison (drop-in replacement)

### Medium Risk Items
- ⚠️ PKCE state encryption (requires encryption key management)
- ⚠️ Rate limiting (may affect legitimate traffic)

### Risk Mitigation
- **Encryption**: Use environment-based key (same as JWT key)
- **Rate Limiting**: Configurable limits, whitelist support
- **Testing**: Comprehensive E2E tests before release
- **Documentation**: Clear configuration guides
- **Rollback**: Can disable features via config if needed

---

## Quality Metrics

### Code Coverage Targets
- Audit logging: 95%+ coverage
- Error sanitization: 100% coverage (critical path)
- Constant-time comparison: 100% coverage
- PKCE encryption: 95%+ coverage
- Rate limiting: 95%+ coverage
- Integration: 85%+ coverage

### Performance Targets
- Audit logging: <1ms overhead
- Constant-time comparison: <0.1ms (crypto-grade)
- PKCE encryption: <1ms
- Rate limiting: <0.5ms
- Total auth overhead: <5ms (acceptable)

### Security Targets
- Zero timing attacks: ✅
- Zero information leakage: ✅
- No bypass of rate limiting: ✅
- State tampering prevented: ✅
- Audit trail complete: ✅

---

## Deployment Notes

### Before Release
1. Load test with rate limiting enabled
2. Verify audit logs don't impact performance
3. Test encryption key rotation strategy
4. Verify error messages in production
5. Performance benchmarks

### Backward Compatibility
- ✅ All changes backward compatible
- ✅ No API changes
- ✅ No database migrations
- ✅ No configuration breaking changes
- ✅ Can release as minor version (v2.1.0)

### Configuration Changes
New optional config section:
```toml
[security]
audit_logging_enabled = true
error_sanitization = true
constant_time_comparison = true
pkce_state_encryption = true
rate_limiting_enabled = true

[rate_limiting]
auth_endpoints_per_minute = 60
failed_login_attempts = 5
lockout_duration_minutes = 15
```

---

## Success Criteria Summary

### Phase 7: COMPLETE when:

1. **Security Improvements**:
   - ✅ Audit logging implemented and tested
   - ✅ Error sanitization complete
   - ✅ Constant-time comparison for all tokens
   - ✅ PKCE state encryption working
   - ✅ Rate limiting operational

2. **Quality**:
   - ✅ All tests passing (100+ new tests)
   - ✅ Security tests 95%+ coverage
   - ✅ No performance regression
   - ✅ Zero clippy warnings
   - ✅ Code formatted

3. **Documentation**:
   - ✅ Enterprise security guide added
   - ✅ Rate limiting configuration documented
   - ✅ Audit logging setup guide added
   - ✅ SECURITY.md updated
   - ✅ Deployment guide updated

4. **Release Ready**:
   - ✅ Version bumped to 2.1.0
   - ✅ CHANGELOG updated
   - ✅ Release notes written
   - ✅ Backward compatibility verified

---

## Deliverables

### Code
- `crates/fraiseql-server/src/auth/audit_logger.rs` (new)
- `crates/fraiseql-server/src/auth/state_store.rs` (new)
- `crates/fraiseql-server/src/auth/rate_limiter.rs` (new)
- Updated auth modules with security features
- 100+ new security tests

### Documentation
- `docs/enterprise/security-hardening.md` (new)
- `docs/enterprise/audit-logging.md` (new)
- `docs/enterprise/rate-limiting.md` (new)
- SECURITY.md (updated)
- CHANGELOG.md (updated)

### Artifacts
- Release notes (v2.1.0)
- Migration guide (if needed)
- Configuration examples

---

## Dependencies

| Crate | Version | Purpose | Maturity |
|-------|---------|---------|----------|
| subtle | 2.5 | Constant-time ops | ✅ Audited |
| governor | 0.10 | Rate limiting | ✅ Production |
| chacha20poly1305 | 0.10 | AEAD encryption | ✅ Audited |

All are:
- ✅ Well-maintained
- ✅ Used in production systems
- ✅ Have security audits
- ✅ Zero unsafe code option

---

## Related Phases

- **Phase 6**: Provided foundation (tests, docs, security audit)
- **Phase 8** (Future): Additional enterprise features (vault integration, HSM support)

---

## Next Steps

1. **Approval**: Review and approve this plan
2. **Kick-off**: Begin Cycle 1 (Audit Logging)
3. **Daily**: Follow TDD cycle (RED → GREEN → REFACTOR → CLEANUP)
4. **Reviews**: Security team review each cycle
5. **Release**: v2.1.0 once all cycles complete

---

**Phase 7 Status**: 📋 PLANNED - Ready to begin

