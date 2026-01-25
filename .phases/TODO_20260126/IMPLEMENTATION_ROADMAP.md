# Implementation Roadmap: Phase 11 Security Remediation
**Total Effort**: 40 hours over 7 phases
**Timeline**: 3-4 weeks with testing
**Status**: Ready to begin

---

## 🗺️ Overview

```
├─ CRITICAL (6h)
│  ├─ Phase 11.1: TLS Security (2h)
│  └─ Phase 11.2: SQL Injection (4h)
│
├─ HIGH (13h)
│  ├─ Phase 11.3: Password Security (3h)
│  ├─ Phase 11.4: OIDC Cache (4h)
│  └─ Phase 11.5: CSRF Distributed (6h)
│
├─ MEDIUM (9h)
│  └─ Phase 11.6: Data Protection (9h - 4 issues)
│
└─ LOW (12h)
   └─ Phase 11.7: Enhancements (12h - 5 items)

Total: 40 hours
```

---

## Week 1: CRITICAL Path (12 hours)

### Day 1: Phase 11.1 - TLS Certificate Validation (2 hours)

**Objective**: Prevent man-in-the-middle attacks via certificate bypass

**Files**: `.phases/phase-11.1-tls-security.md`

**TDD Cycles**:
1. **Cycle 1: Release Panic Check** (45 min)
   - RED: Write test for release panic when danger mode enabled
   - GREEN: Implement panic in release builds
   - REFACTOR: Extract to function
   - CLEANUP: Fix lints, format

2. **Cycle 2: Environment Validation** (45 min)
   - RED: Write test for env var validation
   - GREEN: Check FRAISEQL_DANGER_SKIP_TLS_VERIFICATION env var
   - REFACTOR: Improve error messages
   - CLEANUP: Verify all paths covered

3. **Cycle 3: Logging & Documentation** (30 min)
   - RED: Write test for logging
   - GREEN: Add warning logs when danger mode active
   - REFACTOR: Improve log messages
   - CLEANUP: Update docs

**Verification**:
```bash
cargo nextest run test_tls
cargo clippy --all-targets --all-features -- -D warnings
```

**Commit Message**:
```
fix(security-11.1): Fix TLS certificate validation bypass

## Changes
- Panic in release builds when danger_accept_invalid_certs enabled
- Validate environment variables at startup
- Add warning logs for debug builds

## Verification
✅ TLS tests pass
✅ Clippy clean
✅ No warnings
```

---

### Day 2: Phase 11.2 - SQL Injection Prevention (4 hours + 4h testing)

**Objective**: Prevent SQL injection via unescaped field names

**Files**: `.phases/phase-11.2-sql-injection-fix.md`

**TDD Cycles**:
1. **Cycle 1: Field Name Escaping** (90 min)
   - RED: Write test showing injection vulnerability
   - GREEN: Implement SQL escaping (replace ' → '')
   - REFACTOR: Extract escape function
   - CLEANUP: Fix lints, add comments

2. **Cycle 2: Schema Validation** (90 min)
   - RED: Write test for schema validation
   - GREEN: Validate field names exist in schema
   - REFACTOR: Create validation utility
   - CLEANUP: Handle edge cases

3. **Cycle 3: Integration Testing** (90 min)
   - RED: Write end-to-end injection test
   - GREEN: Ensure all code paths covered
   - REFACTOR: Consolidate test logic
   - CLEANUP: Performance check

**Extended Testing** (4 hours):
- Run full test suite: `cargo nextest run`
- Check performance impact: `cargo bench`
- Verify all backends: PostgreSQL, MySQL, SQLite
- Integration test with actual queries

**Verification**:
```bash
cargo nextest run test_sql
cargo clippy --all-targets --all-features -- -D warnings
cargo test --release
```

**Commit Message**:
```
fix(security-11.2): Fix SQL injection in JSON path construction

## Changes
- Escape field names before SQL interpolation
- Add schema validation for field names
- Extend coverage to all database backends

## Verification
✅ SQL injection tests pass
✅ All 40+ integration tests pass
✅ Clippy clean
✅ No performance regression
```

---

## Week 2: HIGH Priority (13 hours)

### Day 1: Phase 11.3 - Password Memory Security (3 hours)

**Objective**: Prevent plaintext password exposure in memory

**Files**: `.phases/phase-11.3-password-security.md`

**Preparation** (15 min):
```bash
# Add dependency
cargo add zeroize --features std,derive
cargo add hex
cargo build  # Verify no errors
```

**TDD Cycles**:
1. **Cycle 1: Zeroize Wrapper Type** (60 min)
   - RED: Write test showing String not zeroed
   - GREEN: Create Password type with Zeroizing wrapper
   - REFACTOR: Implement proper Drop trait
   - CLEANUP: Add Debug impl that hides value

2. **Cycle 2: DbCredentials Update** (60 min)
   - RED: Write test for Password type usage
   - GREEN: Update DbCredentials struct
   - REFACTOR: Update all password handling
   - CLEANUP: Remove old String-based password fields

3. **Cycle 3: Error Message Sanitization** (60 min)
   - RED: Write test showing passwords in errors
   - GREEN: Sanitize error messages
   - REFACTOR: Create reusable sanitization
   - CLEANUP: Verify all error paths covered

**Verification**:
```bash
cargo nextest run test_password
cargo clippy --all-targets --all-features -- -D warnings
```

**Commit Message**:
```
fix(security-11.3): Secure password memory management

## Changes
- Add zeroize crate for secure memory handling
- Create Password wrapper type with auto-zeroing
- Replace plaintext String with Zeroizing<String>
- Sanitize error messages to hide passwords

## Verification
✅ Password memory tests pass
✅ Error message sanitization verified
✅ Clippy clean
```

---

### Day 2: Phase 11.4 - OIDC Token Cache Protection (4 hours)

**Objective**: Prevent OIDC token cache poisoning

**Files**: `.phases/phase-11.4-oidc-security.md`

**TDD Cycles**:
1. **Cycle 1: Reduce Cache TTL** (60 min)
   - RED: Write test for 300s TTL
   - GREEN: Update JWKS_CACHE_TTL constant
   - REFACTOR: Make configurable
   - CLEANUP: Update defaults

2. **Cycle 2: Key Rotation Detection** (90 min)
   - RED: Write test for rotation detection
   - GREEN: Implement key comparison
   - REFACTOR: Extract to method
   - CLEANUP: Add logging

3. **Cycle 3: Cache Invalidation** (90 min)
   - RED: Write test for cache invalidation on miss
   - GREEN: Implement invalidation logic
   - REFACTOR: Improve cache handling
   - CLEANUP: Remove dead code

**Verification**:
```bash
cargo nextest run test_oidc
cargo clippy --all-targets --all-features -- -D warnings
```

**Commit Message**:
```
fix(security-11.4): Prevent OIDC token cache poisoning

## Changes
- Reduce JWKS cache TTL from 3600s to 300s
- Implement key rotation detection
- Add cache invalidation on key miss
- Start background key rotation monitor

## Verification
✅ Cache TTL tests pass
✅ Key rotation tests pass
✅ Cache invalidation verified
✅ Clippy clean
```

---

### Day 3: Phase 11.5 - CSRF in Distributed Systems (6 hours)

**Objective**: Fix CSRF token validation for load-balanced deployments

**Files**: `.phases/phase-11.5-csrf-security.md`

**Preparation** (15 min):
```bash
# Add dependencies
cargo add redis --features aio,tokio-comp
cargo add async-trait
cargo build  # Verify no errors
```

**TDD Cycles**:
1. **Cycle 1: Redis State Store** (120 min)
   - RED: Write test for persistent state
   - GREEN: Implement RedisStateStore
   - REFACTOR: Create StateStore trait
   - CLEANUP: Add error handling

2. **Cycle 2: In-Memory Fallback** (120 min)
   - RED: Write test for single-instance fallback
   - GREEN: Implement InMemoryStateStore
   - REFACTOR: Implement cleanup task
   - CLEANUP: Handle expiration correctly

3. **Cycle 3: OAuth Integration** (120 min)
   - RED: Write test for multi-instance OAuth
   - GREEN: Inject store dependency
   - REFACTOR: Update handlers
   - CLEANUP: Add configuration support

**Verification**:
```bash
# Start Redis for tests
redis-server &

cargo nextest run test_csrf
cargo clippy --all-targets --all-features -- -D warnings

# Stop Redis
redis-cli shutdown
```

**Commit Message**:
```
fix(security-11.5): Fix CSRF in distributed deployments

## Changes
- Replace in-memory CSRF state store with persistent backend
- Add RedisStateStore for multi-instance deployments
- Keep InMemoryStateStore for single-instance fallback
- Implement automatic state expiration (10 minutes)
- Add background cleanup for in-memory store

## Verification
✅ Multi-instance OAuth tests pass
✅ State persistence verified
✅ Expiration working
✅ Clippy clean
```

---

## Week 3: MEDIUM Priority (9 hours)

### Days 1-5: Phase 11.6 - Data Protection Enhancements (9 hours)

**Objective**: Address 4 medium-severity data protection issues

**Files**: `.phases/phase-11.6-data-protection.md`

**Issue 1: Error Message Redaction** (2 hours)

TDD Cycles (2):
- Cycle 1: Profile-based error redaction (90 min)
- Cycle 2: Integration with response handlers (30 min)

```bash
# Tests
cargo nextest run test_error_redaction
```

**Issue 2: Extended Field Masking** (1 hour)

TDD Cycles (1):
- Cycle 1: Extend pattern list to 30+ items (60 min)

```bash
# Tests
cargo nextest run test_field_masking
```

**Issue 3: JSON Key Ordering** (2 hours)

TDD Cycles (2):
- Cycle 1: Implement sorting (90 min)
- Cycle 2: Verify determinism (30 min)

```bash
# Tests
cargo nextest run test_json_ordering
```

**Issue 4: Constant-Time Comparison** (1 hour)

TDD Cycles (1):
- Cycle 1: Add subtle crate integration (60 min)

```bash
# Preparation
cargo add subtle

# Tests
cargo nextest run test_timing_attack
```

**Integration Testing** (3 hours):
```bash
# Run all Phase 11.6 tests together
cargo nextest run test_data_protection

# Run full suite
cargo nextest run

# Check for performance
cargo bench --bench data_protection

# Verify no timing attacks
cargo test --release timing_attack
```

**Verification**:
```bash
cargo nextest run test_data_protection
cargo clippy --all-targets --all-features -- -D warnings
```

**Commit Message**:
```
fix(security-11.6): Address medium-severity data protection issues

## Changes
- Implement error message redaction in REGULATED profile
- Extend field masking patterns to 30+ sensitive field types
- Fix JSON variable ordering for deterministic cache keys
- Use constant-time comparison for bearer tokens

## Vulnerabilities Addressed
- CVSS 4.3 - Error message information leakage
- CVSS 5.2 - Field masking incomplete coverage
- CVSS 5.5 - JSON variable ordering cache evasion
- CVSS 4.7 - Bearer token timing attack

## Verification
✅ Error redaction tests pass
✅ Field masking tests pass
✅ JSON ordering deterministic
✅ Token comparison timing constant
✅ Clippy clean
```

---

## Week 4+: LOW Priority (12 hours)

### Phase 11.7 - Security Enhancements (12 hours)

**Objective**: Implement 5 low-severity security enhancements

**Files**: `.phases/phase-11.7-enhancements.md`

**Item 1: Query Depth/Complexity Limits** (3 hours)

TDD Cycles (3):
- Cycle 1: Implement complexity analysis (60 min)
- Cycle 2: Add depth validation (60 min)
- Cycle 3: Add complexity budget (60 min)

```bash
cargo nextest run test_query_complexity
```

**Item 2: Rate Limiting Verification** (1 hour)

Documentation task:
- Verify rate limiting key extraction strategy
- Document IP-based vs user-based limiting

```bash
# Create RATE_LIMITING.md
vi docs/RATE_LIMITING.md
```

**Item 3: SCRAM Documentation** (1 hour)

Documentation task:
- Document PostgreSQL version requirements
- Document SCRAM-SHA-256 vs SCRAM-SHA-256-PLUS

```bash
# Update INSTALLATION.md
vi docs/INSTALLATION.md
```

**Item 4: Audit Log Integrity** (4 hours)

TDD Cycles (3):
- Cycle 1: Implement hash chain (90 min)
- Cycle 2: Add tampering detection (90 min)
- Cycle 3: Integration with database (60 min)

```bash
cargo add sha2 hex

cargo nextest run test_audit_log_integrity
```

**Item 5: ID Enumeration Prevention** (3 hours)

TDD Cycles (3):
- Cycle 1: Implement opaque ID generation (60 min)
- Cycle 2: Add configuration options (60 min)
- Cycle 3: Integration with entity creation (60 min)

```bash
cargo add rand

cargo nextest run test_opaque_ids
```

**Verification**:
```bash
cargo nextest run test_enhancements
cargo clippy --all-targets --all-features -- -D warnings
```

**Commit Message**:
```
feat(security-11.7): Add security enhancements

## Changes
- Add GraphQL query depth/complexity validation
- Document rate limiting key extraction strategy
- Document PostgreSQL SCRAM requirements
- Implement immutable audit log with hash chains
- Add opaque ID generation option

## Enhancements (Low-severity items)
- Prevent DoS via deeply nested queries
- Verify rate limiting uses correct keys
- Ensure PostgreSQL version compatibility
- Detect audit log tampering
- Prevent ID enumeration attacks

## Verification
✅ Query complexity tests pass
✅ Audit log integrity verified
✅ Opaque IDs generated correctly
✅ Clippy clean
```

---

## 🧪 Testing Strategy by Phase

### Unit Tests
- RED: Write failing test first
- GREEN: Minimal code to pass
- REFACTOR: Improve design
- CLEANUP: Fix lints

### Integration Tests
- Test across crate boundaries
- Test with actual databases
- Test configuration options

### Performance Tests
- `cargo bench` for timing-critical code
- Compare before/after for each phase
- Verify no regression

### Final Verification Before Each Commit
```bash
# After each TDD cycle
cargo nextest run
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check

# Before committing
cargo check
cargo test --release
git status  # Verify only intended files changed
```

---

## 📅 Detailed Timeline

```
Week 1 (12 hours)
├─ Mon   (2h): Phase 11.1 - TLS Security
├─ Tue   (4h): Phase 11.2 - SQL Injection Fix
└─ Wed   (6h): Phase 11.2 - Testing & Integration

Week 2 (13 hours)
├─ Thu   (3h): Phase 11.3 - Password Security
├─ Fri   (4h): Phase 11.4 - OIDC Cache Poisoning
└─ Sat   (6h): Phase 11.5 - CSRF Distributed Systems

Week 3 (9 hours)
└─ All Week: Phase 11.6 - Data Protection (4 issues)

Week 4+ (12 hours)
└─ Ongoing: Phase 11.7 - Enhancements (5 items)
```

**Parallel Work**:
- Documentation can be written while tests are running
- Code reviews can happen during testing phases
- Deploy preparation can start after CRITICAL issues

---

## 🔄 Git Workflow for Each Phase

### Starting a Phase
```bash
git checkout -b feature/security-11-x-description
```

### During TDD Cycles
```bash
# After each CLEANUP
cargo nextest run
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
```

### Before Committing
```bash
cargo check
cargo clippy --all-targets --all-features
cargo nextest run
git diff  # Review changes
```

### Committing
```bash
git commit -m "$(cat <<'EOF'
fix(security-11.X): Clear description

## Changes
- Change 1
- Change 2

## Verification
✅ Tests pass
✅ Clippy clean

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>
EOF
)"
```

### Pushing
```bash
git push -u origin feature/security-11-x-description
```

---

## ✅ Success Criteria

### For Each Phase
- [ ] All TDD cycles completed
- [ ] All tests passing: `cargo nextest run`
- [ ] No clippy warnings: `cargo clippy -- -D warnings`
- [ ] Code formatted: `cargo fmt`
- [ ] Commit message complete
- [ ] No dead code remaining

### For All Phases Complete
- [ ] All 14 vulnerabilities addressed
- [ ] 2 CRITICAL issues fixed
- [ ] 3 HIGH issues fixed
- [ ] 4 MEDIUM issues fixed
- [ ] 5 LOW items enhanced
- [ ] 1,900+ tests passing
- [ ] Zero clippy warnings
- [ ] Documentation updated
- [ ] GA release ready

---

## 📊 Effort Tracking

| Phase | Estimate | Actual | Status |
|-------|----------|--------|--------|
| 11.1 | 2h | - | [ ] |
| 11.2 | 4h | - | [ ] |
| 11.3 | 3h | - | [ ] |
| 11.4 | 4h | - | [ ] |
| 11.5 | 6h | - | [ ] |
| 11.6 | 9h | - | [ ] |
| 11.7 | 12h | - | [ ] |
| **TOTAL** | **40h** | **-** | [ ] |

---

## 🚀 Quick Start

1. Read `.phases/phase-11.1-tls-security.md`
2. Follow TDD Cycle 1: RED step
3. Write failing test
4. Run `cargo nextest run` to confirm failure
5. Implement fix (GREEN step)
6. Continue with REFACTOR and CLEANUP
7. Commit and move to next cycle

---

## 📚 Reference Documents

- `INDEX.md` - Master tracking
- `CONVERSATION_SUMMARY.md` - Session recap
- `VULNERABILITY_SUMMARY.md` - Quick reference
- `phase-11.X-*.md` - Individual phase details

---

**Status**: Ready to begin Phase 11.1 ✅

**Next Action**: Read `.phases/phase-11.1-tls-security.md` and start TDD Cycle 1: RED
