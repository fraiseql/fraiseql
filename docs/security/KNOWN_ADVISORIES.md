# Known Security Advisories

This document tracks known security advisories in FraiseQL v2's dependency tree and mitigation strategies.

**Generated**: 2026-02-16
**Audit Tool**: cargo-audit v0.19.0

## Critical Vulnerabilities

### RUSTSEC-2023-0071: RSA Marvin Attack Timing Sidechannel

- **Status**: ACCEPTED RISK
- **Severity**: Medium (5.9)
- **Affected Crate**: `rsa` v0.9.10
- **Source**: Transitive via `sqlx-mysql` → `sqlx` (0.8.6)
- **Impact**: Only affects MySQL SSL connection negotiation during handshake
- **Workaround**: Use PostgreSQL (primary recommended database) instead of MySQL

**Rationale**:

- FraiseQL prioritizes PostgreSQL as primary database backend
- MySQL support is secondary and timing attacks during initial SSL negotiation are low-risk in most deployments
- No fixed version available upstream; would require sqlx major version update

**Tracking**: https://github.com/RustCrypto/RSA/issues/456

---

## Warnings (Unmaintained/Unsound)

### RUSTSEC-2024-0384: `instant` crate unmaintained

- **Status**: MONITORING
- **Affected Crate**: `instant` v0.1.13
- **Source**: `notify` 7.0.0 → `notify-types` 1.0.1
- **Impact**: No recent updates; used only for time measurement in file watcher
- **Mitigation**: Monitor for `notify` crate updates that replace `instant`

### RUSTSEC-2025-0134: `rustls-pemfile` unmaintained

- **Status**: MONITORING
- **Affected Versions**: 1.0.4 (via `tiberius`), 2.2.0 (via `rustls-native-certs`)
- **Sources**:
  - v1.0.4: `tiberius` 0.12.3 → SQL Server support
  - v2.2.0: `rustls-native-certs` 0.7.3 → async-nats
- **Impact**: TLS certificate parsing; low risk for internal operations
- **Mitigation**: Monitor upstream rustls ecosystem for maintained alternatives

### RUSTSEC-2026-0002: `lru` crate unsound

- **Status**: MONITORING
- **Affected Crate**: `lru` v0.12.5
- **Source**: `aws-sdk-s3` 1.119.0 (optional S3 support)
- **Impact**: Iterator mutation could violate Stacked Borrows; used for AWS SDK caching
- **Mitigation**: Monitor `aws-sdk-s3` updates; consider reducing LRU cache reliance

---

## Yanked Versions

### testcontainers v0.26.4

- **Status**: UPDATING
- **Usage**: Development/testing only (dev dependency via fraiseql-wire)
- **Action**: Update to latest testcontainers version
- **Command**: `cargo update -p testcontainers`

---

## Remediation Roadmap

### Short-term (v2.0.0-beta.1)

- [ ] Update `testcontainers` to latest version
- [ ] Document MySQL security recommendations in deployment guide

### Medium-term (v2.0.1)

- [ ] Monitor and update `instant` when `notify` crate provides fix
- [ ] Update `rustls-pemfile` when rustls ecosystem stabilizes

### Long-term (v3.0.0+)

- [ ] Consider removing SQL Server support if `tiberius`/`rustls-pemfile` ecosystem remains unstable
- [ ] Evaluate alternatives to AWS SDK if `lru` soundness remains unresolved

---

## Security Policies

### Dependency Update Process

1. **Automated**: Dependabot checks weekly for updates
2. **Manual Review**: Security advisories reviewed via `cargo audit`
3. **Testing**: All updates tested against full test suite
4. **Documentation**: Advisories documented in this file

### Reporting Security Issues

For security vulnerabilities in FraiseQL itself, please:

1. **DO NOT** open a public GitHub issue
2. Email security report to: security@fraiseql.dev
3. Allow 7-10 days for response and coordinated disclosure

### Production Recommendations

- **Database**: Use PostgreSQL (no RSA timing attack exposure)
- **No MySQL**: If MySQL required, use non-SSL connections in trusted networks
- **Updates**: Keep dependencies updated via CI/CD
- **Monitoring**: Subscribe to security advisories

---

## References

- [RustSec Advisory Database](https://rustsec.org/)
- [Cargo-Audit Documentation](https://rustsec.org/cargo-audit/intro/)
- [FraiseQL Security Policy](./SECURITY.md)
