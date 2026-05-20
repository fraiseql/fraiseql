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

## Accepted Risks (Transitive Dependencies)

The following vulnerabilities are pinned to transitive dependencies we do not control. They have been reviewed and accepted: the workspace direct dependency surface does not expose the vulnerable code path to untrusted input, and remediation is blocked on upstream releases we cannot accelerate.

### GHSA-w8wt-cm9p-jqv2: `rand 0.8` PRNG weakness (transitive)

- **Crate**: `rand` 0.8.6 (transitive only)
- **Advisory**: [GHSA-w8wt-cm9p-jqv2](https://github.com/advisories/GHSA-w8wt-cm9p-jqv2)
- **Severity**: LOW
- **Dependabot alert**: #103 — dismissed `tolerable_risk` on 2026-05-19
- **First patched**: rand 0.9.x

#### Impact

- The advisory concerns a PRNG-seeding weakness in `rand` 0.8.
- Every workspace direct dependency is already on `rand = "0.9"` (fraiseql-core, fraiseql-auth, fraiseql-server, fraiseql-secrets, fraiseql-observers, fraiseql-webhooks, fraiseql-wire).
- `rand` 0.8.6 is dragged in transitively by:
  - `jsonwebtoken` 10.4.0 (declares `rand = "^0.8.5"`)
  - `rsa` 0.9.10 (declares `rand = "^0.8"`)
- No application-level RNG path uses `rand` 0.8 directly. The transitive `rand` 0.8 is exercised only by JWT key generation (jsonwebtoken) and RSA prime generation (rsa via sqlx-mysql), neither of which is exposed to untrusted input.

#### Why We Cannot Fix This

- SemVer-incompatible: `^0.8` and `^0.9` are different majors at the `0.x` boundary; Cargo's resolver must keep both versions when any transitive insists on `^0.8`.
- Blocked on `jsonwebtoken 11.x` and `rsa 0.10.x` upstream releases that bump their `rand` requirement to `0.9`. Neither has shipped a `rand 0.9` build as of 2026-05-20.
- Forking the upstreams to override the dep is not justified for a LOW-severity transitive with no exposed attack path.

#### Re-Open Trigger

Re-open Dependabot alert #103 and revisit when:
- `jsonwebtoken` ships a release that declares `rand = "^0.9"`, **or**
- `rsa` ships a release that declares `rand = "^0.9"`.

### RUSTSEC-2026-0098 / 0099 / 0104: `rustls-webpki` via aws-smithy-http-client

- **Crate**: `rustls-webpki` 0.101.7 (transitive only)
- **Advisories**: [RUSTSEC-2026-0098](https://rustsec.org/advisories/RUSTSEC-2026-0098), [RUSTSEC-2026-0099](https://rustsec.org/advisories/RUSTSEC-2026-0099), [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104)
- **GHSA**: GHSA-965h-392x-2mh5, GHSA-xgp8-3hg3-c2mh, GHSA-82j2-j2ch-gfr8
- **Severity**: LOW / LOW / HIGH respectively
- **Dependabot alerts**: #26, #27, #120 — dismissed `tolerable_risk` on 2026-05-19
- **Tracked in `deny.toml`** with deadline **2026-06-15**

#### Impact

- Webpki name-constraint bugs (#26, #27) and CRL parsing panic (#120) in rustls-webpki 0.101.x.
- The workspace default runtime path uses rustls-webpki 0.103.13 (unaffected).
- rustls-webpki 0.101.7 is reachable only through `aws-smithy-http-client` (rustls 0.21 chain) when the **optional `aws-s3` feature** is enabled.
- Default builds do not exercise this code path.

#### Why We Cannot Fix This

- Blocked on `aws-sdk-rust` migrating from rustls 0.21 to rustls 0.23. No application-layer override is possible without forking `aws-smithy-http-client`.

#### Re-Open Trigger

Re-evaluate when `aws-smithy-http-client` ships a rustls-0.23 build. Re-review deadline 2026-06-15.

### CVE-2026-43868: Apache Thrift via parquet (now mitigated)

- **Crate**: `thrift` 0.17.0 (transitive only)
- **Advisory**: [GHSA-2f9f-gq7v-9h6m](https://github.com/advisories/GHSA-2f9f-gq7v-9h6m) / CVE-2026-43868
- **Severity**: MEDIUM (memory allocation with excessive size value)
- **Dependabot alerts**: #145, #146 — dismissed `tolerable_risk` on 2026-05-19

#### Status

**Mitigated** as of [PR #295](https://github.com/fraiseql/fraiseql/pull/295). The `parquet` Cargo feature in `fraiseql-arrow` is now **OFF by default**; default builds no longer pull `thrift`. Users who opt into `fraiseql-arrow/parquet` accept the residual risk.

#### Why a Full Fix Is Blocked

- Apache Parquet's file format spec requires Thrift-encoded metadata. No spec-compliant Parquet writer can avoid the `thrift` crate.
- `thrift` 0.17.0 has had no upstream release since 2022-11-14; `parquet` 58.3.0 (latest) still declares `thrift = "^0.17"`.
- No drop-in `thrift`-codec replacement exists that `parquet` accepts without forking parquet itself.

#### Re-Open Trigger

Re-evaluate when either:
- `parquet` ships a release that drops or replaces its `thrift` dependency, **or**
- `thrift` ships a release with a CVE-2026-43868 fix.

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
| rand 0.8 transitive | Wait for jsonwebtoken / rsa upstream to bump | Open-ended; monitor crates.io |
| rustls-webpki 0.101 transitive | Wait for aws-sdk to move to rustls 0.23 | Re-review 2026-06-15 |
| thrift CVE-2026-43868 | Use `fraiseql-arrow` default features (parquet OFF) | Mitigated 2026-05-20; track parquet/thrift upstream |

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

2026-05-20 — added "Accepted Risks (Transitive Dependencies)" section covering
`rand` 0.8 (#103), `rustls-webpki` 0.101 (#26/#27/#120), and `thrift`
CVE-2026-43868 (#145/#146); refreshed remediation timeline.

## Contact

For security concerns, please follow the process described in the main SECURITY.md file.
