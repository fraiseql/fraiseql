# FraiseQL Compliance Control Matrix

> **Status:** Reference document for compliance mapping
> **Last Updated:** 2025-11-22
> **Standards:** NIST 800-53 Rev 5, NIST 800-218, EO 14028

## Overview

This matrix maps FraiseQL security controls to relevant compliance frameworks.

## NIST 800-53 Rev 5 Control Mapping

### Access Control (AC)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| AC-1 | Policy and Procedures | SECURITY.md, this document | Partial | SECURITY.md |
| AC-2 | Account Management | JWT tokens, Auth0 integration | Implemented | auth/ module |
| AC-3 | Access Enforcement | @authorized decorator, RBAC | Implemented | enterprise/rbac/ |
| AC-4 | Information Flow | GraphQL field-level auth | Implemented | decorators.py |
| AC-5 | Separation of Duties | Role-based authorization | Implemented | rbac/models.py |
| AC-6 | Least Privilege | Database user separation | Partial | Hardening Guide |
| AC-7 | Unsuccessful Logon | Not implemented | Gap | Needs work |
| AC-8 | System Use Notification | Application configurable | N/A | User implementation |
| AC-14 | Permitted Actions | Public vs authenticated queries | Implemented | auth decorators |
| AC-17 | Remote Access | TLS, JWT authentication | Implemented | Production config |

### Audit and Accountability (AU)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| AU-1 | Policy and Procedures | Audit logging documentation | Partial | This document |
| AU-2 | Event Logging | Cryptographic audit chains | Implemented | enterprise/audit/ |
| AU-3 | Content of Audit Records | User, action, timestamp, hash | Implemented | audit/types.py |
| AU-4 | Audit Log Storage | PostgreSQL, configurable retention | Implemented | Database config |
| AU-5 | Response to Failures | Graceful degradation | Implemented | Error handling |
| AU-6 | Audit Review | Grafana dashboards | Partial | grafana/ |
| AU-9 | Protection of Audit Info | HMAC-SHA256 signatures | Implemented | crypto/signing.py |
| AU-11 | Audit Record Retention | Configurable | Implemented | Database config |
| AU-12 | Audit Record Generation | Automatic via decorators | Implemented | audit decorators |

### Security Assessment (CA)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| CA-2 | Security Assessments | Pentagon Readiness Audit | Partial | This folder |
| CA-7 | Continuous Monitoring | CI/CD security scanning | Implemented | GitHub workflows |
| CA-8 | Penetration Testing | Not included | Gap | User responsibility |

### Configuration Management (CM)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| CM-2 | Baseline Configuration | Docker/K8s configs | Implemented | deployment/ |
| CM-3 | Configuration Change Control | Git, branch protection | Implemented | .github/ |
| CM-4 | Impact Analysis | PR reviews, CI tests | Implemented | quality-gate.yml |
| CM-6 | Configuration Settings | FraiseQLConfig class | Implemented | config/ module |
| CM-7 | Least Functionality | Hardening guide | Partial | HARDENING_GUIDE.md |
| CM-8 | System Component Inventory | SBOM generation needed | Gap | Action required |

### Identification and Authentication (IA)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| IA-2 | User Identification | JWT, Auth0 integration | Implemented | auth/ module |
| IA-4 | Identifier Management | UUID-based identification | Implemented | Core types |
| IA-5 | Authenticator Management | Token rotation support | Implemented | token_revocation.py |
| IA-8 | Non-Organization Users | OAuth2/OIDC support | Partial | Auth0 integration |
| IA-9 | Service Identification | Service accounts | Partial | K8s config |

### Incident Response (IR)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| IR-1 | Policy and Procedures | Incident Response Plan | Template | INCIDENT_RESPONSE.md |
| IR-4 | Incident Handling | Procedures documented | Template | INCIDENT_RESPONSE.md |
| IR-5 | Incident Monitoring | Logging, Prometheus | Implemented | Observability stack |
| IR-6 | Incident Reporting | SECURITY.md | Implemented | SECURITY.md |

### Risk Assessment (RA)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| RA-3 | Risk Assessment | Threat Model | Template | THREAT_MODEL.md |
| RA-5 | Vulnerability Scanning | Multiple scanners | Implemented | CI workflows |

### System and Services Acquisition (SA)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| SA-8 | Security Design Principles | Explicit contracts, CQRS | Implemented | Architecture docs |
| SA-10 | Developer Configuration | Pre-commit, CI/CD | Implemented | .pre-commit-config.yaml |
| SA-11 | Developer Security Testing | Bandit, Trivy, pytest | Implemented | CI workflows |
| SA-12 | Supply Chain Protection | Dependabot, SBOM needed | Partial | Action required |
| SA-15 | Development Process | CONTRIBUTING.md | Implemented | CONTRIBUTING.md |

### System and Communications Protection (SC)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| SC-5 | Denial of Service | Query complexity, rate limits | Implemented | Config options |
| SC-8 | Transmission Confidentiality | TLS 1.2+ | Implemented | Nginx config |
| SC-12 | Cryptographic Key Management | Environment-based | Partial | Needs KMS docs |
| SC-13 | Cryptographic Protection | SHA-256, HMAC-SHA256 | Partial | Not FIPS validated |
| SC-23 | Session Authenticity | CSRF protection | Implemented | csrf_protection.py |
| SC-28 | Protection at Rest | PostgreSQL encryption | Partial | Database config |

### System and Information Integrity (SI)

| Control | Title | FraiseQL Implementation | Status | Evidence |
|---------|-------|------------------------|--------|----------|
| SI-2 | Flaw Remediation | Dependabot, pip-audit | Implemented | CI workflows |
| SI-3 | Malicious Code Protection | Container scanning | Implemented | Trivy |
| SI-4 | System Monitoring | Prometheus, Grafana | Implemented | grafana/ |
| SI-10 | Information Input Validation | Pydantic, GraphQL schema | Implemented | Core framework |
| SI-11 | Error Handling | Structured error responses | Implemented | errors/ module |

## NIST 800-218 (SSDF) Mapping

| Practice | Description | FraiseQL Implementation | Status |
|----------|-------------|------------------------|--------|
| PO.1.1 | Define security requirements | SECURITY.md | Partial |
| PO.1.2 | Identify/document security requirements | Compliance matrix | Partial |
| PS.1.1 | Protect development environments | Pre-commit hooks | Implemented |
| PS.2.1 | Protect code integrity | Git + branch protection | Implemented |
| PS.3.1 | Archive and protect releases | PyPI + GitHub releases | Partial |
| PW.1.1 | Design software to meet security requirements | Explicit contracts | Implemented |
| PW.4.1 | Acquire and maintain well-secured components | Dependabot | Implemented |
| PW.5.1 | Create source code | Ruff linting | Implemented |
| PW.6.1 | Configure compilation/build | Maturin + release profile | Implemented |
| PW.7.1 | Review and/or analyze code | Bandit + Ruff | Implemented |
| PW.8.1 | Test executable code | pytest + coverage | Implemented |
| PW.9.1 | Configure software | FraiseQLConfig | Implemented |
| PW.9.2 | Identify and confirm vulnerabilities | pip-audit, Trivy | Implemented |
| RV.1.1 | Receive and respond to vulnerability reports | SECURITY.md | Implemented |
| RV.2.1 | Analyze vulnerabilities | CI security scanning | Implemented |
| RV.3.1 | Remediate vulnerabilities | Active maintenance | Implemented |

## Executive Order 14028 Compliance

| Requirement | Description | FraiseQL Status | Action Required |
|-------------|-------------|-----------------|-----------------|
| SBOM | Software Bill of Materials | Not implemented | Generate CycloneDX |
| Attestation | Build provenance | Not implemented | SLSA Level 3 |
| Signing | Artifact signing | Not implemented | Sigstore integration |
| VDP | Vulnerability Disclosure | Implemented | SECURITY.md |
| SSDF | Secure Development | Partial | Complete documentation |

## Control Status Summary

| Category | Implemented | Partial | Gap | N/A |
|----------|-------------|---------|-----|-----|
| Access Control (AC) | 6 | 3 | 1 | 0 |
| Audit (AU) | 8 | 2 | 0 | 0 |
| Assessment (CA) | 1 | 1 | 1 | 0 |
| Configuration (CM) | 4 | 1 | 1 | 0 |
| Identification (IA) | 3 | 2 | 0 | 0 |
| Incident Response (IR) | 2 | 0 | 2 | 0 |
| Risk Assessment (RA) | 1 | 0 | 1 | 0 |
| Acquisition (SA) | 4 | 1 | 0 | 0 |
| Communications (SC) | 4 | 2 | 0 | 0 |
| Integrity (SI) | 5 | 0 | 0 | 0 |
| **Total** | **38** | **12** | **6** | **0** |

## Evidence Collection

### Automated Evidence

- CI/CD scan results: `security-scan-results` artifact
- Audit logs: PostgreSQL `audit.events` table
- Coverage reports: Codecov integration
- Dependency audits: `audit-results.json`

### Manual Evidence

- Security assessments: This folder
- Penetration test reports: User-provided
- Training records: User responsibility
- Access reviews: User responsibility

---

**Classification:** INTERNAL
**Distribution:** Compliance/Security Teams
