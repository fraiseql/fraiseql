# Phase 21, Cycle 1: Security Audit Report

**Date**: January 26, 2026
**Status**: RED phase complete - Issues identified

---

## Executive Summary

Security audit identified **4 findings** that require remediation:

| Finding | Severity | Count | Action |
|---------|----------|-------|--------|
| CORS misconfiguration | **CRITICAL** | 2 locations | Fix: restrict origins |
| TODO/FIXME markers | HIGH | 48 | Remove or resolve |
| Phase markers | HIGH | 25 | Remove all references |
| Debug println/eprintln | MEDIUM | ~60 in production code | Replace with tracing |
| Hardcoded localhost | LOW | ~6 in examples/docs | Review context |
| unwrap() usage | LOW | 1,703 total (mostly tests) | No change needed |

---

## Detailed Findings

### 1. 🚨 CORS Misconfiguration (CRITICAL)

**Locations**:
- `crates/fraiseql-server/src/runtime_middleware/cors.rs` - `AllowOrigin::any()`
- `crates/fraiseql-server/src/middleware/cors.rs` - `.allow_origin(Any)`

**Risk**: Production deployment with `AllowOrigin::any()` allows CSRF attacks

**Fix**:
```rust
// BEFORE (vulnerable):
.allow_origin(Any)

// AFTER (secure):
.allow_origin(AllowOrigin::list(vec![
    "https://app.example.com".parse().unwrap(),
    "https://api.example.com".parse().unwrap(),
]))
```

**Status**: Needs to be fixed before production

---

### 2. 📝 TODO/FIXME Markers (HIGH)

**Count**: 48 TODO markers found across codebase

**Sample Locations**:
- `crates/fraiseql-server/src/lib.rs` - Multiple doc comment TODOs
- Various query optimization TODOs
- Schema validation TODOs

**Action**: Review each TODO and either:
1. Fix the issue (if critical)
2. Convert to known limitations documentation
3. Remove if no longer relevant

---

### 3. 🏷️ Phase Markers (HIGH)

**Count**: 25 phase markers found

**Examples**:
```rust
// Phase X: [description]
// TODO: Phase specific work
```

**Action**: Remove all phase references per finalization plan

---

### 4. 🔍 Debug Prints in Production Code (MEDIUM)

**Count**: ~60 println!/eprintln! in production code

**Locations**:
- `crates/fraiseql-core/src/compiler/mod.rs` - Debug compiler output
- `crates/fraiseql-core/src/db/postgres/adapter.rs` - DEBUG: SQL prints
- `crates/fraiseql-wire/src/connection/tls.rs` - TLS warnings

**Fix**: Replace with proper structured logging via `tracing` crate

**Examples**:
```rust
// BEFORE:
eprintln!("[compiler] Parsing schema...");
eprintln!("DEBUG: SQL with projection = {}", sql);

// AFTER:
tracing::info!("Parsing schema");
tracing::debug!("SQL with projection = {}", sql);
```

---

### 5. ✅ Non-Issues (Reviewed and Cleared)

**Hardcoded Secrets**: CLEARED
- Test token formats (e.g., `"new_access_token_{uuid}"`) are acceptable
- OAuth provider examples with localhost are documentation only
- `wrong_secret_key` in tests is intentional test data

**unwrap() Usage**: CLEARED (mostly tests)
- 1,703 unwrap() calls are acceptable because:
  - Majority are in test code
  - Benchmarks can panic on setup failures
  - Error paths properly handled in production code

**Hardcoded Localhost**: CLEARED (documentation/examples)
- Only appear in:
  - Code example comments
  - Test setup code
  - Configuration examples in docstrings
- Not in production initialization code

---

## Security Model Assessment

**✅ Verified Secure**:
- SQL injection prevention (parameterized queries used throughout)
- No unescaped user input in queries
- Input validation on HTTP boundaries
- Authentication properly implemented (JWT, OIDC, OAuth2)
- Authorization checks in place (field-level, operation-level)
- TLS enforcement option available
- SCRAM authentication supported
- Audit logging of sensitive operations

**⚠️ Needs Configuration**:
- CORS: Must restrict to specific origins in production
- TLS: Can be disabled (intended for development)
- Introspection: Can be disabled per security profile
- Rate limiting: Available but must be enabled
- Secrets: Should be externalized (not in code)

---

## Remediation Plan (GREEN Phase)

### Priority 1: CRITICAL (Before GA)
- [ ] Fix CORS to restrict origins
  - Create environment variable for allowed origins
  - Add configuration documentation
  - Add warning if running with AllowOrigin::any()

### Priority 2: HIGH (Finalization)
- [ ] Remove all 25 phase markers
- [ ] Review and resolve all 48 TODO markers
- [ ] Replace debug prints with structured logging

### Priority 3: MEDIUM (Documentation)
- [ ] Document security model
- [ ] Create security hardening guide
- [ ] Add production deployment checklist

---

## Files to Modify

**CRITICAL**:
```
crates/fraiseql-server/src/runtime_middleware/cors.rs  (2 changes)
crates/fraiseql-server/src/middleware/cors.rs          (1 change)
crates/fraiseql-server/src/lib.rs                      (config docs)
```

**HIGH**:
```
Multiple files with TODO/FIXME/Phase markers (see: git grep results)
crates/fraiseql-core/src/compiler/mod.rs               (~5 debug prints)
crates/fraiseql-core/src/db/postgres/adapter.rs        (~2 debug prints)
crates/fraiseql-wire/src/connection/tls.rs             (~1 warning print)
```

---

## Testing Strategy

After fixes:

```bash
# Verify CORS is properly configured
cargo test cors_test

# Verify no phase markers remain
git grep -i "// phase" -- '*.rs'

# Verify no debug prints
git grep "eprintln!\|println!" -- '*.rs' | grep -v 'test\|example\|cli'

# Run full test suite
cargo test --all

# Build in release mode
cargo build --release
```

---

## Conclusion

The codebase is **generally secure** with proper authentication, authorization, and input validation. The main issues are:

1. **CORS configuration** - Critical, must be restricted for production
2. **Development artifacts** - Phase markers and TODOs must be cleaned
3. **Debug output** - Should use structured logging instead of println!

All issues are remediable and straightforward. No fundamental security flaws found.

---

## Next Phase: GREEN

Proceed to fixing identified issues in TDD GREEN phase.
