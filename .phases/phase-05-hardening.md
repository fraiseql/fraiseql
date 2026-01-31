# Phase 5: Production Hardening

**Status**: 📋 PLANNED (After Phase 4)
**Objective**: Security, dependencies, and operational readiness
**Expected Duration**: 2-3 days

---

## Success Criteria

- [ ] Security audit completed and issues remediated
- [ ] Critical dependencies updated (protobuf)
- [ ] All known CVEs addressed
- [ ] OpenTelemetry observability fully integrated
- [ ] Structured logging configured
- [ ] Prometheus metrics exposed
- [ ] Distributed tracing working
- [ ] No clippy warnings
- [ ] All tests passing
- [ ] Documentation updated

---

## Objective

Phases 1-4 built the features. Phase 5 hardens for production:

1. Security audit and remediation
2. Dependency management
3. Observability infrastructure
4. Operational tooling

---

## TDD Cycles

### Cycle 1: Security Audit & Fixes

**Objective**: Identify and fix security vulnerabilities

**RED Phase** ✓
- Security audit:
  - Input validation on all boundaries
  - SQL injection protection
  - XSS prevention
  - CSRF protection
  - Authentication/authorization
  - Secrets handling
  - TLS/mTLS configuration
  - Rate limiting
  - CORS configuration
- Write tests that verify security:
  - SQL injection attempts blocked
  - XSS payloads escaped
  - Invalid tokens rejected
  - Unauthorized access denied
  - Secrets not logged

**GREEN Phase**
- Fix security issues found:
  - Add input validation
  - Escape user input
  - Verify token validation
  - Check CORS rules
  - Verify TLS setup
- Add missing protections

**REFACTOR Phase**
- Consolidate validation logic
- Improve error messages (without leaking info)
- Better secrets handling
- Cleaner authorization checks

**CLEANUP Phase**
- Fix warnings
- Format code
- Commit with security summary

### Cycle 2: Dependency Management

**Objective**: Update critical dependencies and manage vulnerabilities

**RED Phase** ✓
- Audit dependencies:
  - Check for known CVEs (cargo audit)
  - Identify unmaintained dependencies
  - Find version mismatches
  - Check for security updates
- Document findings:
  - Critical issues (must fix)
  - High issues (should fix soon)
  - Medium issues (monitor)
  - Low issues (note)

**GREEN Phase**
- Update critical dependencies:
  - Protobuf: 2.28.0 → 3.7.2 (critical)
  - Review changelog for breaking changes
  - Update dependent code if needed
  - Run full test suite
- Plan for other updates:
  - Schedule medium-priority updates
  - Monitor low-priority items

**REFACTOR Phase**
- Clean up dependency usage
- Reduce unnecessary dependencies
- Consolidate transitive dependency versions

**CLEANUP Phase**
- Verify all tests pass
- Format code
- Document dependencies in README
- Commit with update details

### Cycle 3: Observability Integration

**Objective**: Add OpenTelemetry observability

**RED Phase** ✓
- Write failing tests for:
  - Trace context propagation
  - Span creation and attributes
  - Log correlation with traces
  - Metrics collection
  - OTLP export
  - Jaeger/Zipkin integration

**GREEN Phase**
- Integrate OpenTelemetry:
  - Initialize tracer provider
  - Add spans to request handlers
  - Propagate trace context
  - Export to OTLP collector
- Add structured logging:
  - JSON formatted logs
  - Trace ID in all logs
  - Severity levels
  - Context propagation

**REFACTOR Phase**
- Improve span naming (conventions)
- Better cardinality management
- Optimize sampling strategy
- Add custom metrics

**CLEANUP Phase**
- Fix warnings
- Format code
- Document observability setup
- Commit with tracing integration

### Cycle 4: Operational Tools

**Objective**: Add tooling for operations and debugging

**RED Phase** ✓
- Write tests for:
  - Health check endpoint
  - Readiness probe
  - Liveness probe
  - Metrics endpoint (/metrics)
  - Config validation at startup
  - Graceful shutdown
  - Signal handling

**GREEN Phase**
- Implement operational endpoints:
  - `/health` - simple health check
  - `/ready` - readiness probe (database connected)
  - `/live` - liveness probe (process running)
  - `/metrics` - Prometheus metrics
- Add lifecycle handlers:
  - Signal handling (SIGTERM)
  - Graceful shutdown
  - Connection draining
  - In-flight request completion

**REFACTOR Phase**
- Improve probe logic
- Better metrics naming
- Cleaner lifecycle handling

**CLEANUP Phase**
- Fix warnings
- Format code
- Document operations
- Commit with ops tooling

### Cycle 5: Documentation & Release Prep

**Objective**: Complete documentation and prepare for release

**RED Phase** ✓
- Verify documentation:
  - Security guide complete
  - Operations guide complete
  - Troubleshooting guide complete
  - API documentation current
  - Examples working
  - Release notes drafted

**GREEN Phase**
- Update all documentation:
  - Add security best practices
  - Document operational endpoints
  - Add troubleshooting section
  - Update examples
- Create release notes

**REFACTOR Phase**
- Improve documentation clarity
- Better organization
- Cross-references updated
- Code examples verified

**CLEANUP Phase**
- Proofread all documentation
- Check for broken links
- Verify code examples work
- Commit with documentation updates

---

## Security Checklist

### Input Validation
- [ ] All user input validated
- [ ] SQL injection protected (parameterized queries)
- [ ] XSS protection (output encoding)
- [ ] Path traversal protection
- [ ] Rate limiting on endpoints
- [ ] File upload validation

### Authentication & Authorization
- [ ] JWT validation works
- [ ] OIDC provider integration tested
- [ ] Role-based access control (RBAC) enforced
- [ ] Multi-tenancy isolation verified
- [ ] Token refresh working
- [ ] Token revocation implemented

### Secrets & Credentials
- [ ] No hardcoded secrets in code
- [ ] Environment variable configuration
- [ ] KMS integration for key management
- [ ] Secrets not logged
- [ ] Connection strings secured
- [ ] API keys protected

### TLS/Encryption
- [ ] TLS 1.2+ enforced
- [ ] mTLS supported
- [ ] Certificate validation
- [ ] Cipher suite hardened
- [ ] HTTPS enforced
- [ ] HSTS configured

### Monitoring & Auditing
- [ ] All operations logged
- [ ] Security events traced
- [ ] Audit trail available
- [ ] Anomaly detection possible
- [ ] Alerts configured

---

## Dependency Status

### Critical (From Phase 16 GA Audit)
- ⚠️ **Protobuf 2.28.0 → 3.7.2** (HIGH severity CVE)
  - Action: Update immediately
  - Impact: gRPC and serialization
  - Testing: Run full suite after update

### High Priority (Monitor)
- ⚠️ **RSA 0.9.10** (no fix available, transitive dependency)
  - Action: Monitor for updates
  - Impact: Cryptographic operations
  - Testing: Watch for security advisories

### Medium Priority (Stable but Outdated)
- ⚠️ **instant** - Used for timers
- ⚠️ **paste** - Used for macros
- ⚠️ **rustls-pemfile** - PEM parsing
- ⚠️ **lru** - Caching implementation

### Maintenance
- Create dependency monitoring process
- Monthly audit cycle
- Automated update checks
- Performance regression testing on updates

---

## Files to Update

### Security
- `docs/security-guide.md` ✨
- `crates/fraiseql-server/src/security/` (hardening)
- Tests for security scenarios

### Dependencies
- `Cargo.lock` (after updates)
- `Cargo.toml` (version updates)
- `docs/dependencies.md` (documentation)

### Observability
- `crates/fraiseql-server/src/observability/` ✨
- `docs/observability-guide.md` (configuration)
- `docs/distributed-tracing.md` (tracing setup)

### Operations
- `docs/operations-guide.md` (probes, metrics)
- `scripts/healthcheck.sh` ✨
- Kubernetes manifests with probes

### Documentation
- `SECURITY.md` (at repo root)
- `CHANGELOG.md` (release notes)
- `docs/README.md` (updated structure)

---

## Definition of Done

Phase 5 is complete when:

1. ✅ Security audit completed
2. ✅ All critical CVEs fixed
3. ✅ OpenTelemetry working end-to-end
4. ✅ Operational endpoints implemented
5. ✅ Security documentation complete
6. ✅ Code clean with no warnings
7. ✅ All tests passing
8. ✅ Ready for release

---

## Next Phase

**Phase 6: Finalization** focuses on:
- Code archaeology removal
- Final quality review
- Documentation polish
- Release preparation

See `.phases/phase-06-finalization.md` for details.

---

## Notes

- Security is non-negotiable
- Every CVE must be understood and addressed
- Observability is operational insurance
- Good operations prevent incidents
- Documentation helps future maintainers

---

**Phase 5 will be started after Phase 4 completion.**
