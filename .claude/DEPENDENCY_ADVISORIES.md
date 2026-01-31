# Dependency Advisories & Remediation Plan

**Date**: January 31, 2026
**Status**: Documented for Phase 5 (Hardening)
**Review Cycle**: Before GA Release

---

## Known Advisories

### 1. RSA 0.9.10 - Timing Sidechannel (RUSTSEC-2023-0071)

**Severity**: Medium (5.9)
**Status**: No fix available
**Dependency Path**: `fraiseql-server` → `sqlx` → `sqlx-mysql` → `rsa`

**Details**:
- Title: Marvin Attack - potential key recovery through timing sidechannels
- Root cause: `rsa` crate does not implement constant-time operations
- **Why we accept this**: RSA is used transitively by MySQL's TLS implementation only. All FraiseQL cryptographic operations use other libraries (sha2, hmac, aes-gcm) which are hardened.

**Action**:
- Monitor for updates to `rsa` crate
- Consider alternative MySQL driver if available in future
- Not blocking for GA release

---

### 2. Instant 0.1.13 - Unmaintained (RUSTSEC-2024-0384)

**Severity**: Warning (unmaintained)
**Status**: Stable, no known issues
**Dependency Path**: `fraiseql-cli` → `notify` → `notify-types` → `instant`

**Details**:
- The `instant` crate is no longer maintained
- However, it provides only a simple monotonic clock wrapper
- No security vulnerabilities reported
- **Why we accept this**: Used only for file watching in CLI during development, not in server runtime

**Action**:
- Monitor for performance-critical issues
- Consider migrating to `std::time::Instant` in future Rust versions
- Not blocking for GA release

---

### 3. Paste 1.0.15 - Unmaintained (RUSTSEC-2024-0436)

**Severity**: Warning (unmaintained)
**Status**: Stable, widely used
**Dependency Path**: `fraiseql-arrow` → `clickhouse` → `polonius-the-crab` → `macro_rules_attribute` → `paste`

**Details**:
- The `paste` crate is no longer maintained
- However, it's a well-established macro utility with no known vulnerabilities
- Widely used in Rust ecosystem
- **Why we accept this**: Transitive dependency through ClickHouse driver. Core FraiseQL doesn't depend on it directly.

**Action**:
- Monitor for macro expansion issues
- Consider eliminating ClickHouse driver in future if not actively used
- Not blocking for GA release

---

### 4. Additional Warnings (Monitored but Not Blocking)

**Dependency**: `rustls-pemfile`, `lru`
**Status**: Flagged by audit but stable
**Action**: Monitor during regular dependency updates

---

## Phase 5 Hardening Plan

### Week 1: Dependency Audit
- [ ] Run `cargo audit` before each release
- [ ] Document any new advisories
- [ ] Assess risk vs. migration cost

### Week 2: Evaluation & Remediation
- [ ] Evaluate alternatives to unmaintained crates
- [ ] Test alternative implementations
- [ ] Plan migration if beneficial

### Week 3: Implementation (if needed)
- [ ] Update crates that have fixes available
- [ ] Test thoroughly
- [ ] Document changes

### Week 4: Verification
- [ ] Run full audit suite
- [ ] Verify no new warnings introduced
- [ ] Document final status

---

## Summary

**Production Readiness**: ✅ APPROVED
- No critical vulnerabilities in code paths
- All high-risk advisories either transitive or unmaintained but stable
- Risk is acceptable for GA release
- Formal audit plan in place for Phase 5

---

## Reference

- [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071) - RSA timing sidechannel
- [RUSTSEC-2024-0384](https://rustsec.org/advisories/RUSTSEC-2024-0384) - instant unmaintained
- [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436) - paste unmaintained

**Next Review**: Phase 5 (Production Hardening)
