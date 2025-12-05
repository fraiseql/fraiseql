# FraiseQL Security Controls Matrix

**Version**: 1.0
**Last Updated**: 2025-11-24
**Status**: Active

---

## Overview

This document provides a comprehensive mapping of security controls across FraiseQL's three security profiles: **STANDARD**, **REGULATED**, and **RESTRICTED**. Each profile implements progressively stricter controls to meet different compliance and security requirements.

---

## Security Profile Definitions

### STANDARD Profile
**Target Environment**: General purpose applications, development, staging
**Compliance**: General security best practices
**Risk Tolerance**: Medium

### REGULATED Profile
**Target Environment**: PCI-DSS, HIPAA, SOC 2 compliant applications
**Compliance**: Industry-specific regulations
**Risk Tolerance**: Low

### RESTRICTED Profile
**Target Environment**: Highly regulated industries, government, defense, classified data
**Compliance Examples**:
- 🇺🇸 NIST 800-53, FedRAMP, DoD requirements
- 🇪🇺 NIS2 Essential Entities, EU Cyber Resilience Act
- 🇬🇧 UK NCSC High-Security Guidance
- 🇨🇦 CPCSC (Canadian defence contractors)
- 🇦🇺 Essential Eight Maturity Level 3
- 🇸🇬 Singapore CII (Critical Information Infrastructure) operators
- 🌐 PCI-DSS Level 1, HIPAA, ISO 27001 High-Risk environments
**Risk Tolerance**: Very Low

---

## Access Controls

| Control | STANDARD | REGULATED | RESTRICTED | Implementation |
|---------|----------|-----------|------------|----------------|
| **Authentication Required** | ✅ Required | ✅ Required | ✅ Required | FastAPI dependency injection |
| **Multi-Factor Authentication (MFA)** | ⚠️ Optional | ✅ Required | ✅ Required | External IdP integration |
| **Session Timeout** | 24 hours | 4 hours | 1 hour | Token expiration |
| **Password Complexity** | Medium | High | Very High | External IdP policy |
| **API Key Rotation** | Manual | 90 days | 30 days | KMS key rotation |
| **Field-Level Authorization** | ✅ Enabled | ✅ Enabled | ✅ Enabled | GraphQL resolver checks |
| **Row-Level Security (RLS)** | ✅ Enabled | ✅ Enabled | ✅ Enabled | PostgreSQL RLS policies |

---

## Encryption Controls

| Control | STANDARD | REGULATED | RESTRICTED | Implementation |
|---------|----------|-----------|------------|----------------|
| **Data at Rest Encryption** | ⚠️ Optional | ✅ Required | ✅ Required | KMS + database encryption |
| **Data in Transit Encryption** | ✅ TLS 1.2+ | ✅ TLS 1.2+ | ✅ TLS 1.3 only | FastAPI SSL config |
| **KMS Provider** | Local/Vault | Vault/AWS/GCP | Vault/AWS (HSM-backed) | KMS infrastructure module |
| **Envelope Encryption** | ✅ Enabled | ✅ Enabled | ✅ Enabled | KeyManager service |
| **Key Rotation** | 90 days | 30 days | 7 days | Automated background task |
| **Encryption Context (AAD)** | ⚠️ Optional | ✅ Required | ✅ Required | KMS provider config |
| **Certificate Pinning** | ❌ Disabled | ⚠️ Optional | ✅ Required | TLS configuration |

---

## Network Controls

| Control | STANDARD | REGULATED | RESTRICTED | Implementation |
|---------|----------|-----------|------------|----------------|
| **HTTPS Only** | ✅ Enforced | ✅ Enforced | ✅ Enforced | HTTPS redirect middleware |
| **HSTS Headers** | ✅ Enabled | ✅ Enabled (2 years) | ✅ Enabled (2 years) | Security headers middleware |
| **CORS Policy** | Permissive | Restrictive | Very Restrictive | FastAPI CORS config |
| **Rate Limiting (per minute)** | 100 requests | 60 requests | 30 requests | RateLimitMiddleware |
| **IP Allowlisting** | ❌ Disabled | ⚠️ Optional | ✅ Required | Firewall/WAF rules |
| **Mutual TLS (mTLS)** | ❌ Disabled | ⚠️ Optional | ✅ Required | TLS client certificate |
| **Network Segmentation** | ⚠️ Optional | ✅ Required | ✅ Required | Infrastructure config |

---

## Input Validation Controls

| Control | STANDARD | REGULATED | RESTRICTED | Implementation |
|---------|----------|-----------|------------|----------------|
| **GraphQL Query Depth Limit** | 10 levels | 7 levels | 5 levels | QueryValidator config |
| **GraphQL Query Complexity** | 1000 | 500 | 250 | Complexity analyzer |
| **Request Body Size Limit** | 10 MB | 1 MB | 100 KB | BodySizeLimiter middleware |
| **SQL Injection Prevention** | ✅ Architecture | ✅ Architecture | ✅ Architecture | Views + stored functions |
| **XSS Prevention** | ✅ Enabled | ✅ Enabled | ✅ Enabled | Content-Security-Policy |
| **CSRF Protection** | ✅ Enabled | ✅ Enabled | ✅ Enabled | CSRF token validation |
| **Input Sanitization** | ✅ Enabled | ✅ Enabled | ✅ Enabled | Validation schemas |

---

## Observability & Monitoring Controls

| Control | STANDARD | REGULATED | RESTRICTED | Implementation |
|---------|----------|-----------|------------|----------------|
| **Application Logging** | ✅ Enabled | ✅ Enabled | ✅ Enabled | Structured logging |
| **Audit Logging** | ⚠️ Optional | ✅ Required | ✅ Required | Dedicated audit table |
| **Security Event Logging** | ⚠️ Optional | ✅ Required | ✅ Required | Security event handler |
| **Distributed Tracing** | ✅ Enabled | ✅ Enabled | ✅ Enabled | OpenTelemetry |
| **PII Sanitization in Logs** | ✅ Enabled | ✅ Enabled | ✅ Enabled | TracingConfig patterns |
| **Log Retention** | 30 days | 365 days | 2555 days (7 years) | Log rotation policy |
| **Real-time Alerting** | ⚠️ Optional | ✅ Required | ✅ Required | External monitoring |
| **Introspection Endpoint** | ✅ Enabled | ❌ Disabled | ❌ Disabled | GraphQL config |

---

## API Security Controls

| Control | STANDARD | REGULATED | RESTRICTED | Implementation |
|---------|----------|-----------|------------|----------------|
| **API Versioning** | ✅ Enabled | ✅ Enabled | ✅ Enabled | URL path versioning |
| **Schema Validation** | ✅ Enabled | ✅ Enabled | ✅ Enabled | Pydantic models |
| **Error Message Sanitization** | ✅ Basic | ✅ Strict | ✅ Very Strict | Error handler middleware |
| **Query Batching Limit** | 10 queries | 5 queries | 3 queries | GraphQL executor config |
| **File Upload Restrictions** | ✅ Enabled | ✅ Enabled | ✅ Enabled | File type validation |
| **External API Calls** | ✅ Allowed | ⚠️ Logged | ❌ Blocked | Security profile enforcer |
| **Webhook Validation** | ⚠️ Optional | ✅ Required | ✅ Required | Signature verification |

---

## Infrastructure Controls

| Control | STANDARD | REGULATED | RESTRICTED | Implementation |
|---------|----------|-----------|------------|----------------|
| **Container Scanning** | ✅ Enabled | ✅ Enabled | ✅ Enabled | Trivy in CI/CD |
| **Dependency Scanning** | ✅ Enabled | ✅ Enabled | ✅ Enabled | Safety, cargo-audit |
| **SBOM Generation** | ✅ Enabled | ✅ Enabled | ✅ Enabled | CycloneDX format |
| **Secrets Management** | ✅ Env vars | ✅ Vault/Secrets Manager | ✅ HSM-backed Vault | KMS integration |
| **Non-root Container** | ✅ Enforced | ✅ Enforced | ✅ Enforced | Dockerfile USER directive |
| **Read-only Filesystem** | ⚠️ Optional | ✅ Required | ✅ Required | Container security context |
| **Resource Limits** | ✅ Enabled | ✅ Enabled | ✅ Enabled | Kubernetes limits |
| **Vulnerability Threshold** | Medium | Low | Critical only | Security gate policy |

---

## Data Protection Controls

| Control | STANDARD | REGULATED | RESTRICTED | Implementation |
|---------|----------|-----------|------------|----------------|
| **Data Masking** | ⚠️ Optional | ✅ Required | ✅ Required | Field resolvers |
| **Data Anonymization** | ⚠️ Optional | ✅ Required | ✅ Required | ETL pipeline |
| **Data Retention Policy** | Custom | Defined | Strictly Enforced | Automated cleanup jobs |
| **Right to Erasure (GDPR)** | ⚠️ Optional | ✅ Required | ✅ Required | Delete API endpoints |
| **Data Export (Portability)** | ⚠️ Optional | ✅ Required | ✅ Required | Export API endpoints |
| **Backup Encryption** | ⚠️ Optional | ✅ Required | ✅ Required | Encrypted backups |
| **Data Classification** | ⚠️ Optional | ✅ Required | ✅ Required | Metadata tagging |

---

## Compliance Controls Mapping

### PCI-DSS v4.0 Compliance

| Requirement | Control | STANDARD | REGULATED | RESTRICTED |
|-------------|---------|----------|-----------|------------|
| **1.2.1** | Network segmentation | ⚠️ | ✅ | ✅ |
| **2.2.2** | Secure configuration | ✅ | ✅ | ✅ |
| **3.4.1** | Render PAN unreadable | ⚠️ | ✅ | ✅ |
| **4.2.1** | Strong cryptography (TLS) | ✅ | ✅ | ✅ |
| **6.2.4** | Inventory of components (SBOM) | ✅ | ✅ | ✅ |
| **8.2.1** | Authentication controls | ✅ | ✅ | ✅ |
| **10.2.1** | Audit trail logging | ⚠️ | ✅ | ✅ |
| **11.3.1** | Penetration testing | ❌ | ⚠️ | ✅ |

### HIPAA Security Rule

| Standard | Control | STANDARD | REGULATED | RESTRICTED |
|----------|---------|----------|-----------|------------|
| **§164.308(a)(1)(i)** | Security management | ✅ | ✅ | ✅ |
| **§164.308(a)(3)(i)** | Workforce access | ✅ | ✅ | ✅ |
| **§164.308(a)(5)(i)** | Security awareness | ⚠️ | ✅ | ✅ |
| **§164.310(d)(1)** | Device controls | ⚠️ | ✅ | ✅ |
| **§164.312(a)(1)** | Access control | ✅ | ✅ | ✅ |
| **§164.312(a)(2)(i)** | Unique user ID | ✅ | ✅ | ✅ |
| **§164.312(b)** | Audit controls | ⚠️ | ✅ | ✅ |
| **§164.312(e)(1)** | Transmission security | ✅ | ✅ | ✅ |

### NIST 800-53 Controls (RESTRICTED Profile)

| Family | Control ID | Control Name | Implementation |
|--------|-----------|--------------|----------------|
| **AC** | AC-2 | Account Management | IAM integration |
| **AC** | AC-3 | Access Enforcement | RLS + field authorization |
| **AU** | AU-2 | Audit Events | Comprehensive audit logging |
| **CM** | CM-7 | Least Functionality | Minimal container image |
| **IA** | IA-2 | Identification & Authentication | MFA required |
| **SC** | SC-8 | Transmission Confidentiality | TLS 1.3 |
| **SC** | SC-13 | Cryptographic Protection | AES-256-GCM |
| **SI** | SI-3 | Malicious Code Protection | Container scanning |

---

## Control Implementation Matrix

### Legend
- ✅ **Enabled/Required**: Control is active and enforced
- ⚠️ **Optional/Recommended**: Control is available but not enforced
- ❌ **Disabled/Not Required**: Control is not active
- 🔄 **Planned**: Control is planned for future implementation

---

## Risk Acceptance

### STANDARD Profile
**Accepted Risks**:
- Optional MFA
- Optional audit logging
- Permissive CORS
- Higher rate limits

**Justification**: Development and low-risk production environments where convenience and performance are prioritized.

### REGULATED Profile
**Accepted Risks**:
- Optional IP allowlisting
- Optional mTLS
- No penetration testing requirement

**Justification**: Balanced approach for regulated industries with managed risk tolerance.

### RESTRICTED Profile
**Accepted Risks**:
- Minimal (all controls enforced)

**Justification**: Zero-trust architecture for high-security environments.

---

## Control Testing

### Automated Testing
| Control Category | Test Type | Frequency |
|------------------|-----------|-----------|
| Authentication | Unit tests | Every commit |
| Encryption | Unit + integration | Every commit |
| Rate limiting | Integration tests | Every commit |
| Input validation | Unit + fuzzing | Every commit |
| SQL injection | Architecture tests | Every commit |

### Manual Testing
| Control Category | Test Type | Frequency |
|------------------|-----------|-----------|
| Penetration testing | External audit | Annually |
| Configuration review | Internal audit | Quarterly |
| Access control | Compliance review | Quarterly |

---

## Profile Selection Guide

### Choose STANDARD if:
- Development or staging environment
- Internal applications with trusted users
- Performance is critical
- Compliance requirements are minimal

### Choose REGULATED if:
- Handling payment card data (PCI-DSS)
- Handling health information (HIPAA)
- SOC 2 compliance required
- Customer data protection is important

### Choose RESTRICTED if:
- Government or defense applications (any jurisdiction)
- Classified data handling
- Critical Infrastructure (CII) operations
- High regulatory compliance required:
  - 🇺🇸 FedRAMP, DoD, NIST 800-53
  - 🇪🇺 NIS2 Essential Entities
  - 🇨🇦 CPCSC certification
  - 🇦🇺 Essential Eight Level 3
  - 🇸🇬 Singapore CII
- Zero-trust architecture needed

---

## Configuration Example

```python
from fraiseql.security.profiles import SecurityProfile, ProfileEnforcer

# STANDARD profile (default)
standard = ProfileEnforcer(
    profile=SecurityProfile.STANDARD,
    enable_rate_limit=True,
    enable_audit_log=False,  # Optional
)

# REGULATED profile (PCI-DSS, HIPAA)
regulated = ProfileEnforcer(
    profile=SecurityProfile.REGULATED,
    enable_rate_limit=True,
    enable_audit_log=True,  # Required
    require_mfa=True,
    kms_provider=vault_provider,
)

# RESTRICTED profile (Highly regulated, government, defense, CII)
restricted = ProfileEnforcer(
    profile=SecurityProfile.RESTRICTED,
    enable_rate_limit=True,
    enable_audit_log=True,
    require_mfa=True,
    require_mtls=True,
    kms_provider=vault_hsm_provider,
)
```

---

## Maintenance and Review

**Review Frequency**: Quarterly or when:
- New compliance requirements emerge
- Security incidents occur
- Architecture changes significantly
- New threat vectors identified

**Last Review**: 2025-11-24
**Next Review**: 2026-02-24

**Change Control**: All control changes require security review and approval.

---

## References

- [FraiseQL Security Configuration Guide](./configuration.md)
- [FraiseQL Threat Model](./threat-model.md)
- [KMS Architecture ADR](../architecture/decisions/0003-kms-architecture.md)
- [PCI-DSS v4.0](https://www.pcisecuritystandards.org/)
- [HIPAA Security Rule](https://www.hhs.gov/hipaa/for-professionals/security/)
- [NIST 800-53](https://csrc.nist.gov/publications/detail/sp/800-53/rev-5/final)

---

*This controls matrix provides a comprehensive view of security controls across all FraiseQL security profiles. For implementation details, refer to the Security Configuration Guide.*
