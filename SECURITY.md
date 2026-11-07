# FraiseQL v2 Security Policy

## Overview

FraiseQL v2 prioritizes security and follows industry best practices for secure development, testing, and deployment. This document outlines our security approach and any known vulnerabilities.

---

## Known Vulnerabilities & Risk Assessment

Accepted advisories, their real dependency paths, the exposure class of each and the date it
must be re-argued are in [docs/dependency-risk-policy.md](docs/dependency-risk-policy.md).
That document is kept in lockstep with `deny.toml` and `.cargo/audit.toml` by
`tools/check-audit-lockstep.sh`, so it is the only place this policy states them. (An earlier
version of this section described RUSTSEC-2023-0071 as reachable only through the
removed MySQL backend; that was wrong — see the policy's correction of 2026-08-16.)

---

## Security Best Practices Implemented

### Development

- Unsafe code: no hand-written `unsafe` in the workspace. `unsafe_code` is denied in every
  workspace crate, with one exception: the module `wasmtime::component::bindgen!` generates
  for fraiseql-functions' WASM runtime (the `runtime-wasm` feature) is allowed the `unsafe`
  glue the macro emits (`crates/fraiseql-functions/src/runtime/wasm/bindings.rs`).
  Dependencies use `unsafe` internally.
- Linting: Clippy pedantic checks enabled
- Testing: the Rust suites run as required checks before merge (`tools/required-checks.toml`)
- Code Review: All changes reviewed

### Cryptography

- Constant-Time Comparison: Using `subtle` crate
- Secure Randomness: Using `getrandom`
- No Hardcoded Secrets: All via environment variables
- Error Sanitization: No sensitive data in errors

### Database

- SQL Injection Prevention: Parameterized queries only
- Type-Safe Compilation: Schema compiler validates all operations
- Property-Based Fuzzing: Tests for escaping vulnerabilities

### Deployment

- Monitoring: Comprehensive logging
- Incident Response: Emergency runbooks
- Backup/Recovery: Documented procedures

---

## Reporting Security Vulnerabilities

**DO NOT** open public issues for security vulnerabilities.

Instead, report it privately, either through GitHub's private vulnerability reporting
([Report a vulnerability](https://github.com/fraiseql/fraiseql/security/advisories/new),
under the repository's Security tab) or by email to security@fraiseql.dev, with:

- Description of vulnerability
- Steps to reproduce
- Potential impact
- Any known workarounds

No response time is promised.

---

## Security Audit Status

**Latest audit**: 2026-06-11, whole-workspace, run by AI agents reading the code statically
(no runtime testing); it was not an independent third-party audit. It found 1 critical,
46 high and 56 medium findings. The report is tracked verbatim at
[docs/security/audits/2026-06-11.md](docs/security/audits/2026-06-11.md); the status of every
finding, with the changelog entry, test or issue that proves it, is in
[docs/security/audits/2026-06-11-status.md](docs/security/audits/2026-06-11-status.md)
(as of 2026-09-21: 92 fixed, 6 unreachable because the server refuses to boot with the
feature configured, 4 deleted with their feature, 1 accepted). `tools/check-audit-ledger.sh`
keeps that ledger complete. The retrospective, what was found and what was done about it, is
[docs/security/audits/2026-06-11-retrospective.md](docs/security/audits/2026-06-11-retrospective.md).

---

## Compliance

FraiseQL makes no compliance claim and holds no certification. There is no security profile
to select: what a production deployment configures (authentication, authorization, rate
limiting, network, error handling, secrets, observability) is listed in
[docs/guides/production-security-checklist.md](docs/guides/production-security-checklist.md).

---

**Last Updated**: 2026-11-07
**Maintained By**: the FraiseQL project (there is no dedicated security team)
