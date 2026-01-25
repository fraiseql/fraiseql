# Security Remediation Plan - Phase 11

**Status**: 🔴 Planning
**Priority**: CRITICAL
**Phases**: 11.1 - 11.6
**Total Effort**: 3-4 weeks

---

## Overview

This remediation plan addresses all 14 security vulnerabilities identified in the professional security audit (January 25, 2026). Phases are organized by severity and dependency, with TDD cycles for each fix.

---

## Phase Structure

### Phase 11.1: Critical - TLS Certificate Validation (Day 1)
- Fix TLS danger mode exposure
- Add runtime safeguards
- File: `phase-11.1-tls-security.md`

### Phase 11.2: Critical - SQL Injection Fix (Day 1-2)
- Escape JSON field names
- Add schema validation
- File: `phase-11.2-sql-injection-fix.md`

### Phase 11.3: High - Password Zeroing (Day 2-3)
- Implement zeroize crate
- Secure password memory
- File: `phase-11.3-password-security.md`

### Phase 11.4: High - OIDC Cache Security (Day 3-4)
- Reduce cache TTL
- Monitor key rotation
- File: `phase-11.4-oidc-security.md`

### Phase 11.5: High - CSRF Distributed Fix (Day 4-5)
- Replace in-memory store
- Use persistent backend
- File: `phase-11.5-csrf-security.md`

### Phase 11.6: Medium - Data Protection (Day 5-7)
- Fix field masking gaps
- Implement error redaction
- Fix JSON ordering
- Fix timing attacks
- File: `phase-11.6-data-protection.md`

### Phase 11.7: Low - Enhancement Items (Week 2)
- Add query complexity limits
- Document rate limiting
- Improve audit logging
- File: `phase-11.7-enhancements.md`

---

## Implementation Timeline

```
Week 1 (Critical/High Severity):
  Mon (Day 1):   Phase 11.1 (TLS) + Phase 11.2 (SQL injection)
  Tue-Wed (D2-3): Phase 11.3 (Passwords)
  Thu (D4):      Phase 11.4 (OIDC)
  Fri (D5):      Phase 11.5 (CSRF)

Week 2 (Medium/Low Severity):
  Mon-Wed (D6-8): Phase 11.6 (Data protection)
  Thu-Fri (D9-10): Phase 11.7 (Enhancements)

Week 3 (Testing & Validation):
  Full integration testing
  Security regression tests
  Performance validation
```

---

## Risk Prioritization Matrix

```
Priority | Severity | Issue                          | Effort | Days
---------|----------|--------------------------------|--------|------
1        | CRITICAL | TLS validation bypass         | 2h     | 1
2        | CRITICAL | SQL injection (JSON paths)    | 4h     | 1
3        | HIGH     | Plaintext password storage    | 3h     | 1
4        | HIGH     | OIDC cache poisoning          | 4h     | 1
5        | HIGH     | CSRF in distributed systems   | 6h     | 2
6        | MEDIUM   | Error message leakage         | 2h     | 1
7        | MEDIUM   | Field masking gaps            | 3h     | 1
8        | MEDIUM   | JSON variable ordering        | 2h     | 1
9        | MEDIUM   | Bearer token timing attack    | 2h     | 1
10       | LOW      | Query depth/complexity        | 3h     | 1
11       | LOW      | Rate limiting verification    | 2h     | 1
12       | LOW      | Audit log integrity           | 4h     | 2
13       | LOW      | ID enumeration prevention     | 2h     | 1
14       | LOW      | SCRAM version support         | 1h     | 1
---------|----------|--------------------------------|--------|------
         |          | TOTAL                         | 40h    | 10
```

---

## Success Criteria (Overall)

### Security
- [ ] All CRITICAL vulnerabilities fixed and tested
- [ ] All HIGH severity vulnerabilities fixed and tested
- [ ] All MEDIUM severity vulnerabilities fixed
- [ ] All LOW severity enhancements completed

### Testing
- [ ] 100% of vulnerability fixes covered by tests
- [ ] Security regression tests pass
- [ ] Existing functional tests still pass
- [ ] Performance tests don't regress

### Code Quality
- [ ] Zero clippy warnings
- [ ] All code formatted
- [ ] Documentation updated
- [ ] Security comments added

### Validation
- [ ] Internal security review pass
- [ ] Threat model verification
- [ ] Penetration test ready

---

## Phase Dependencies

```
Phase 11.1 (TLS)       → Can run in parallel
Phase 11.2 (SQL)       → Can run in parallel
Phase 11.3 (Passwords) → Depends on: none
Phase 11.4 (OIDC)      → Depends on: none
Phase 11.5 (CSRF)      → Depends on: none
Phase 11.6 (Data)      → Depends on: 11.1, 11.2, 11.3
Phase 11.7 (Enhance)   → Depends on: 11.1-11.6 (after all fixes)
```

---

## Git Workflow

```bash
# Create feature branch
git checkout -b feature/security-remediation

# Work on phases
git checkout -b phase/11.1-tls
# ... make changes ...
git commit -m "fix(security-11.1): ..."

git checkout feature/security-remediation
git merge phase/11.1-tls

# Repeat for each phase

# Final PR to dev
git push origin feature/security-remediation
# Create PR for review
```

---

## Acceptance Criteria Per Phase

### Phase 11.1 (TLS)
- [ ] TLS danger mode panics in production builds
- [ ] Runtime warning logged if danger mode enabled
- [ ] Default behavior uses system certificate store
- [ ] Tests verify danger mode behavior
- [ ] All tests passing

### Phase 11.2 (SQL Injection)
- [ ] JSON field names escaped in SQL generation
- [ ] Schema validation of field names added
- [ ] Unit tests for SQL generation pass
- [ ] SQL injection tests fail (properly defended)
- [ ] Integration tests pass

### Phase 11.3 (Passwords)
- [ ] zeroize crate added to dependencies
- [ ] Password memory zeroed on drop
- [ ] Error messages don't leak passwords
- [ ] Memory tests verify zeroing
- [ ] All tests passing

### Phase 11.4 (OIDC)
- [ ] Cache TTL reduced to 300 seconds
- [ ] Key rotation monitoring implemented
- [ ] Cache invalidation on key miss added
- [ ] Tests verify key rotation detection
- [ ] All tests passing

### Phase 11.5 (CSRF)
- [ ] Persistent state store (Redis) implemented
- [ ] In-memory fallback for single-instance
- [ ] State validation works across instances
- [ ] Integration tests with multi-instance setup
- [ ] All tests passing

### Phase 11.6 (Data Protection)
- [ ] Error messages redacted in REGULATED profile
- [ ] Field masking patterns extended
- [ ] JSON variable ordering deterministic
- [ ] Bearer token comparison constant-time
- [ ] All tests passing

### Phase 11.7 (Enhancements)
- [ ] Query depth limits enforced
- [ ] Query complexity tracking added
- [ ] Rate limiting documented
- [ ] Audit log integrity checks added
- [ ] ID enumeration prevention added
- [ ] All tests passing

---

## Testing Strategy

### Unit Tests
```
Each phase includes:
- Positive test cases (vulnerability fixed)
- Negative test cases (exploit attempts fail)
- Edge cases
- Security-specific test utilities
```

### Integration Tests
```
Multi-phase integration:
- TLS + Database connection
- SQL injection + field validation
- Password + credential storage
- CSRF + session management
- OIDC + cache + key rotation
```

### Security Tests
```
Explicit vulnerability tests:
- Attempt each known exploit
- Verify it fails
- Document expected failure mode
```

### Performance Tests
```
Regression testing:
- Query latency (should not increase)
- Memory usage (should not increase)
- Cache hit rate (should not decrease)
```

---

## Code Review Checklist

For each phase:

- [ ] Code follows project standards
- [ ] Security comments explain why
- [ ] Tests are comprehensive
- [ ] No new security issues introduced
- [ ] Performance impact analyzed
- [ ] Documentation updated
- [ ] Clippy warnings addressed

---

## Rollout Strategy

### Before GA Release
1. Complete Phase 11.1 and 11.2 (CRITICAL)
2. Code review by security team
3. Merge to dev branch
4. Run full test suite

### Week 1 Post-GA (if released)
1. Complete Phase 11.3, 11.4, 11.5 (HIGH)
2. Urgent patch release prepared
3. Customers notified

### Week 2-3 Post-GA
1. Complete Phase 11.6 (MEDIUM)
2. Patch release with all security fixes
3. Release notes document fixes

### Following Month
1. Complete Phase 11.7 (LOW)
2. Regular release cycle

---

## Documentation Updates

### Security Documentation
- [ ] Update SECURITY.md with remediation status
- [ ] Add security guidelines
- [ ] Document TLS configuration
- [ ] Document password requirements
- [ ] Document OIDC setup

### API Documentation
- [ ] Document error message behavior (REGULATED profile)
- [ ] Document field masking
- [ ] Document rate limiting
- [ ] Document authentication requirements

### Operational Documentation
- [ ] Deployment requirements (Redis for CSRF)
- [ ] Configuration options
- [ ] Monitoring recommendations
- [ ] Troubleshooting guide

---

## Monitoring & Metrics

### Phase Completion
- Tracked in commit messages
- PR reviews for each phase
- Automated test results

### Security Metrics
- Vulnerability count (starts at 14, goes to 0)
- Test coverage (aim for 95%+)
- Code quality (zero warnings)

### Performance Metrics
- Query latency (baseline vs after fixes)
- Memory usage (baseline vs after fixes)
- Cache hit rate (baseline vs after fixes)

---

## Known Risks & Mitigations

### Risk: Breaking Changes
**Mitigation**: All fixes are internal, no API changes

### Risk: Performance Impact
**Mitigation**: Benchmark each phase, optimize if needed

### Risk: Regression
**Mitigation**: Full test suite runs after each phase

### Risk: Incomplete Fix
**Mitigation**: Multiple reviewers, security tests explicit

---

## Next Steps

1. ✅ Security audit complete (DONE)
2. ⏭️  Review and approve remediation plan (THIS STEP)
3. ⏭️  Begin Phase 11.1 (TLS security)
4. ⏭️  Work through phases sequentially
5. ⏭️  Final security validation
6. ⏭️  GA release with security patches

---

## Questions for Planning

1. Should GA be delayed for CRITICAL fixes, or released with urgent patch?
2. Should customers be notified pre-release of vulnerabilities?
3. Should penetration test be scheduled after all fixes?
4. Should external security firm review fixes?
5. What's timeline for deployment post-release?

---

**Status**: Ready for approval and Phase 11.1 start
**Prepared**: January 25, 2026
**Reviewer**: [Awaiting approval]
