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

- Type Safety: 100% safe Rust (no `unsafe` blocks)
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

- Configuration Profiles: STANDARD, REGULATED, RESTRICTED
- Monitoring: Comprehensive logging
- Incident Response: Emergency runbooks
- Backup/Recovery: Documented procedures

---

## Reporting Security Vulnerabilities

**DO NOT** open public issues for security vulnerabilities.

Instead, email security@fraiseql.dev with:

- Description of vulnerability
- Steps to reproduce
- Potential impact
- Any known workarounds

We aim to acknowledge reports within 48 hours.

---

## Security Audit Status

**Latest audit**: 2026-06-11, external, whole-workspace. It found 1 critical, 46 high and 56
medium findings. The report is tracked verbatim at
[docs/security/audits/2026-06-11.md](docs/security/audits/2026-06-11.md); the status of every
finding, with the changelog entry, test or issue that proves it, is in
[docs/security/audits/2026-06-11-status.md](docs/security/audits/2026-06-11-status.md)
(as of 2026-09-21: 92 fixed, 6 unreachable because the server refuses to boot with the
feature configured, 4 deleted with their feature, 1 accepted). `tools/check-audit-ledger.sh`
keeps that ledger complete. A public retrospective is being written and will be linked here.

---

## Compliance

FraiseQL supports deployments with three security profiles:

- **STANDARD**: Basic security, internal applications
- **REGULATED**: Enhanced controls, compliance ready
- **RESTRICTED**: Maximum security, air-gapped deployments

See [docs/guides/production-security-checklist.md](docs/guides/production-security-checklist.md) for the production configuration.

---

**Last Updated**: 2026-09-21
**Maintained By**: FraiseQL Security Team
