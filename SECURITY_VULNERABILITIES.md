# FraiseQL Security Vulnerability Assessment

## Summary

FraiseQL v2 has been scanned for security vulnerabilities using `cargo audit`. This document documents known vulnerabilities and their risk assessment.

## Critical Issues

### None

## Medium Issues

### RUSTSEC-2023-0071: Marvin Attack (RSA Timing Sidechannel)

- **Crate**: `rsa` 0.9.10
- **Severity**: 5.9 (MEDIUM)
- **CVE**: [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071)
- **Date Discovered**: 2023-11-22
- **Status**: **NO UPSTREAM FIX AVAILABLE**

#### Impact

- **Scope**: Only affects MySQL SSL/TLS negotiation
- **Database Impact**: MySQL connections with SSL enabled
- **Attack Vector**: Timing attack during RSA key exchange to recover private key material
- **Likelihood**: Low in production (requires sophisticated attacker with network timing measurements)

#### Mitigation

1. **Recommended**: Use PostgreSQL (primary database) instead of MySQL
   - PostgreSQL driver uses modern Rustls/OpenSSL implementations
   - PostgreSQL is the recommended and most-tested FraiseQL backend

2. **If MySQL is required**:
   - Run database connections over VPN or private network (reduce timing attack surface)
   - Use connection pooling (reduces number of handshakes)
   - Monitor for unusual connection patterns
   - Plan migration to PostgreSQL

3. **Upstream Status**:
   - The `rsa` crate maintainers have not provided a fix
   - Fix would require upstream to use constant-time RSA padding verification
   - No timeline for fix provided

#### Why We Cannot Fix This

- Transitive dependency through `sqlx` → `sqlx-mysql` → `rsa`
- The `rsa` crate is a third-party maintained crate
- We cannot downgrade due to ecosystem compatibility
- Alternative MySQL drivers for Rust also depend on the same vulnerable crate

---

## Warning Issues (Non-Critical)

### RUSTSEC-2024-0384: `instant` Crate Unmaintained

- **Crate**: `instant` 0.1.13
- **Advisory**: [RUSTSEC-2024-0384](https://rustsec.org/advisories/RUSTSEC-2024-0384)
- **Impact**: Provides timing utilities; used by `notify` file watcher
- **Risk**: Low (stable, no functional issues, just no longer maintained)
- **Status**: Can be suppressed

### RUSTSEC-2024-0436: `paste` Macro Crate Unmaintained

- **Crate**: `paste` 1.0.15
- **Advisory**: [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436)
- **Impact**: Proc macro for code generation; used by `arrow-flight` and type utilities
- **Risk**: Low (well-tested, stable API, just no longer maintained)
- **Status**: Can be suppressed

### RUSTSEC-2025-0134: `rustls-pemfile` Unmaintained

- **Crate**: `rustls-pemfile` 1.0.4 and 2.2.0
- **Advisory**: [RUSTSEC-2025-0134](https://rustsec.org/advisories/RUSTSEC-2025-0134)
- **Impact**: Used by `tiberius` SQL Server driver for certificate parsing
- **Risk**: Low (no functional security issues, just no longer maintained)
- **Status**: Can be suppressed

### testcontainers 0.26.4 Yanked

- **Crate**: `testcontainers` 0.26.4
- **Advisory**: Yanked from crates.io
- **Impact**: Used for integration testing with Docker containers
- **Risk**: Low (test-only dependency, no runtime impact)
- **Status**: Can be suppressed; upgrade available if needed

---

## Dependency Tree

### RSA Vulnerability Dependency Path

```
rsa 0.9.10 (VULNERABLE)
└── sqlx-mysql 0.8.6
    └── sqlx 0.8.6
        ├── fraiseql-server
        ├── fraiseql-core
        └── fraiseql-arrow
```

---

## Remediation Timeline

| Issue | Recommended Action | Timeline |
|-------|-------------------|----------|
| RSA Marvin Attack | Migrate to PostgreSQL | Immediate for new deployments; plan migration for existing MySQL users |
| instant unmaintained | Monitor for replacement; suppress warning | Next 6 months (ecosystem evaluation) |
| paste unmaintained | Suppress warning; no action needed | No immediate action required |
| rustls-pemfile unmaintained | Suppress warning; monitor for replacement | Can wait for ecosystem updates |
| testcontainers yanked | Suppress warning; consider upgrade for CI/CD | No immediate action required |

---

## Running Vulnerability Scans

```bash
# Full audit report
cargo audit

# Only show advisories (ignore warnings)
cargo audit --deny advisories

# Show only vulnerabilities
cargo audit --severity medium
```

---

## Future Work

1. **PostgreSQL as default**: Strongly recommend PostgreSQL for all new deployments
2. **MySQL deprecation**: Plan sunset of MySQL support in favor of PostgreSQL/SQLite
3. **Dependency updates**: Monitor `rsa` crate for security updates
4. **CI/CD integration**: Add `cargo audit` to CI pipeline with suppression file

---

## Last Updated

2026-02-05

## Contact

For security concerns, please follow the process described in the main SECURITY.md file.
