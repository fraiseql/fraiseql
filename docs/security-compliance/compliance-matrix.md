# Compliance Matrix

**Version:** 1.0
**Last Updated:** 2025-12-08
**Audience:** Security officers, compliance auditors, procurement officers
**Time to Review:** 20-30 minutes

---

## Overview

This compliance matrix maps FraiseQL's security features to **international standards** and **regional regulatory frameworks** across multiple jurisdictions. The document follows a universal-to-specific approach:

1. **International Standards** - ISO, GDPR, PCI-DSS, SOC 2 (applicable globally)
2. **Regional Frameworks** - EU, UK, Australia, Singapore, Canada, United States
3. **Security Profiles** - Mapping profiles to compliance requirements

**Key Principle:** FraiseQL's security architecture is designed around **internationally recognized standards** rather than any single nation's requirements. This ensures global applicability and reduces compliance complexity for multinational organizations.

---

## Quick Reference: Security Profile Selection

| Your Requirement | Recommended Profile | Key Features |
|------------------|---------------------|--------------|
| **General applications** (any region) | STANDARD | Basic security, audit logging (optional), HTTPS |
| **ISO 27001, GDPR, PCI-DSS, HIPAA** | REGULATED | KMS encryption, mandatory audit trails, RLS, field-level auth |
| **Critical Infrastructure** (🇪🇺 NIS2, 🇸🇬 CII, 🇦🇺 Essential Eight L3) | RESTRICTED | HSM-backed encryption, immutable audit chains, real-time monitoring |
| **Government/Defence** (🇺🇸 FedRAMP, 🇨🇦 CPCSC, 🇬🇧 NCSC) | RESTRICTED | Air-gapped support, zero-trust architecture, cryptographic integrity |

---

## Part 1: International Standards

### ISO/IEC 27001:2022 - Information Security Management

**Applicability:** Global standard for information security management systems (ISMS)

#### Key Controls Mapping

| Control ID | Control Name | FraiseQL Implementation | Profile | Evidence |
|------------|--------------|-------------------------|---------|----------|
| **5.21** | Managing information security in the ICT supply chain | SBOM generation (CycloneDX), dependency tracking, cryptographic verification | ALL | [SBOM Guide](./slsa-provenance.md#sbom-verification) |
| **5.23** | Information security for use of cloud services | KMS integration (AWS, Azure, GCP, Vault), encryption at rest/transit | REGULATED+ | [KMS Architecture](../architecture/decisions/0003-kms-architecture.md) |
| **8.1** | User endpoint devices | Session management, token expiration, device authentication | ALL | [Auth Security](../production/security.md#authentication-security) |
| **8.2** | Privileged access rights | RBAC with PostgreSQL roles, row-level security (RLS), field-level auth | ALL | [RBAC Tests](../../tests/integration/enterprise/rbac/test_row_level_security.py) |
| **8.3** | Information access restriction | Field-level authorization, GraphQL resolver checks, RLS policies | ALL | [Field Auth Tests](../../tests/integration/enterprise/rbac/test_field_level_auth.py) |
| **8.8** | Management of technical vulnerabilities | Automated dependency scanning, SBOM for vulnerability tracking, container scanning | ALL | [Security Config](../security/configuration.md) |
| **8.9** | Configuration management | Security profiles (STANDARD/REGULATED/RESTRICTED), immutable infrastructure | ALL | [Security Profiles](./security-profiles.md) |
| **8.10** | Information deletion | GDPR right to erasure, data retention policies, soft deletes with audit | REGULATED+ | [Production Security](../production/security.md#gdpr-compliance) |
| **8.11** | Data masking | PII anonymization, field masking for dev environments, resolver-level masking | REGULATED+ | [Production Security](../production/security.md#sensitive-data-handling) |
| **8.12** | Data leakage prevention | PII sanitization in logs, secure error handling, rate limiting | ALL | [Observability](../production/observability.md) |
| **8.15** | Logging | Structured logging, security event logging, distributed tracing (OpenTelemetry) | ALL | [Production Security](../production/security.md#audit-logging) |
| **8.16** | Monitoring activities | Real-time security monitoring, anomaly detection (RESTRICTED), audit trail analysis | REGULATED+ | [Production Monitoring](../production/monitoring.md) |

**Compliance Status:** ✅ **Fully Supported** for all profiles
**Recommended Profile:** REGULATED or RESTRICTED for ISO 27001 certification

---

### GDPR (General Data Protection Regulation)

**Applicability:** European Economic Area (EEA) + any organization processing EU residents' data globally

#### Key Requirements Mapping

| Article | Requirement | FraiseQL Implementation | Profile | Evidence |
|---------|-------------|-------------------------|---------|----------|
| **Art. 5(1)(f)** | Integrity and confidentiality (security) | Encryption at rest/transit, access control, audit logging | REGULATED+ | [Security Architecture](../features/security-architecture.md) |
| **Art. 15** | Right of access | Data export API, JSON format for portability | REGULATED+ | [Production Security](../production/security.md#gdpr-compliance) |
| **Art. 16** | Right to rectification | GraphQL mutations with audit trails | ALL | [Audit Tests](../../tests/integration/enterprise/audit/test_unified_audit.py) |
| **Art. 17** | Right to erasure ("right to be forgotten") | Soft deletes with anonymization, audit retention, data deletion API | REGULATED+ | [Production Security](../production/security.md#gdpr-compliance) |
| **Art. 20** | Right to data portability | JSON export of all user data | REGULATED+ | [Production Security](../production/security.md#gdpr-compliance) |
| **Art. 25** | Data protection by design and by default | Security profiles, field-level encryption, minimal data collection | REGULATED+ | [Security Profiles](./security-profiles.md) |
| **Art. 30** | Records of processing activities | Audit logging, change data capture (CDC), before/after snapshots | REGULATED+ | [Audit Schema](../../tests/integration/enterprise/audit/test_audit_schema.py) |
| **Art. 32** | Security of processing | KMS encryption, cryptographic audit chains, RLS, field-level auth | REGULATED+ | [Security Controls](../security/controls-matrix.md) |
| **Art. 33-34** | Breach notification | Security event logging, real-time alerting, incident detection | REGULATED+ | [Production Security](../production/security.md#audit-logging) |

**Compliance Status:** ✅ **Fully Supported** with REGULATED or RESTRICTED profile
**Recommended Profile:** REGULATED minimum for GDPR compliance

---

### PCI-DSS 4.0 (Payment Card Industry Data Security Standard)

**Applicability:** Global - any organization processing, storing, or transmitting payment card data
**Effective Date:** March 31, 2025

#### Key Requirements Mapping

| Requirement | Description | FraiseQL Implementation | Profile | Evidence |
|-------------|-------------|-------------------------|---------|----------|
| **1.2.1** | Network segmentation | Network-level configuration (infrastructure), restrictive CORS | REGULATED+ | [Security Config](../production/security.md#cors-configuration) |
| **2.2.2** | Secure configuration standards | Security profiles with predefined controls, minimal container images | ALL | [Security Profiles](./security-profiles.md) |
| **3.4.1** | Render PAN unreadable | Field-level encryption (KMS), data masking, PII anonymization | REGULATED+ | [Production Security](../production/security.md#sensitive-data-handling) |
| **4.2.1** | Strong cryptography for transmission (TLS) | TLS 1.2+ (STANDARD/REGULATED), TLS 1.3 only (RESTRICTED) | ALL | [Security Controls](../security/controls-matrix.md#encryption-controls) |
| **6.2.4** | Inventory of software components (SBOM) | Automated SBOM generation (CycloneDX), dependency tracking | ALL | [SLSA Provenance](./slsa-provenance.md) |
| **6.3.2** | Maintain software component inventory | SBOM with direct + transitive dependencies, version tracking | ALL | [SLSA Provenance](./slsa-provenance.md#sbom-verification) |
| **8.2.1** | Strong authentication controls | JWT/OAuth2, MFA (REGULATED+), session timeout, password complexity | REGULATED+ | [Production Security](../production/security.md#authentication-security) |
| **10.2.1** | Audit trail for all access to cardholder data | Comprehensive audit logging, immutable audit chains, field-level tracking | REGULATED+ | [Audit Bridge](../../tests/integration/enterprise/audit/test_audit_bridge.py) |
| **11.3.1** | External and internal penetration testing | Testing infrastructure (documented), security controls validation | RESTRICTED | [Security Controls](../security/controls-matrix.md#control-testing) |

**Compliance Status:** ✅ **Supported** with REGULATED or RESTRICTED profile
**Recommended Profile:** REGULATED for Level 2, RESTRICTED for Level 1
**Note:** PCI-DSS compliance requires full environment assessment, not just application-level controls

---

### SOC 2 Type II (Trust Service Criteria)

**Applicability:** Global - SaaS providers, cloud services, data processors

#### Trust Service Criteria Mapping

| Criterion | Category | FraiseQL Implementation | Profile | Evidence |
|-----------|----------|-------------------------|---------|----------|
| **CC1.1** | Integrity and ethical values | Security profiles enforce consistent controls across environments | ALL | [Security Profiles](./security-profiles.md) |
| **CC2.1** | Communication of responsibilities | Role-based access control (RBAC), permission management APIs | ALL | [RBAC Tests](../../tests/integration/enterprise/rbac/test_rbac_management_apis.py) |
| **CC3.1** | Risk assessment | Threat model, security controls matrix, vulnerability scanning | REGULATED+ | [Threat Model](../security/threat-model.md) |
| **CC5.1** | Control activities | Input validation, rate limiting, query complexity analysis, parameterized queries | ALL | [Production Security](../production/security.md) |
| **CC6.1** | Logical access controls | RBAC, RLS, field-level authorization, session management | ALL | [Security Controls](../security/controls-matrix.md#access-controls) |
| **CC6.6** | Encryption | KMS integration, envelope encryption, TLS enforcement, key rotation | REGULATED+ | [KMS Architecture](../architecture/decisions/0003-kms-architecture.md) |
| **CC7.2** | Monitoring activities | Security event logging, distributed tracing, real-time alerting | REGULATED+ | [Production Monitoring](../production/monitoring.md) |
| **CC7.3** | Audit logging | Immutable audit trails, cryptographic chains, change data capture | REGULATED+ | [Unified Audit](../../tests/integration/enterprise/audit/test_unified_audit.py) |
| **A1.2** | System availability | Health checks, monitoring, incident response | REGULATED+ | [Production Deployment](../production/deployment.md) |
| **C1.2** | Data confidentiality | Encryption at rest/transit, access control, data classification | REGULATED+ | [Security Architecture](../features/security-architecture.md) |
| **P3.1** | Data privacy | Consent management, data minimization, privacy by design | REGULATED+ | [Production Security](../production/security.md#gdpr-compliance) |

**Compliance Status:** ✅ **Architecture supports** all Trust Service Criteria
**Recommended Profile:** REGULATED for SOC 2 Type II certification
**Note:** SOC 2 certification requires organizational controls (policies, procedures) beyond technical implementation

---

## Part 2: Regional Frameworks

### 🇪🇺 European Union

#### NIS2 Directive (Network and Information Security Directive)

**Applicability:** Essential and important entities in EU member states
**Effective:** October 2024
**Sectors:** Energy, transport, healthcare, finance, digital infrastructure, manufacturing, public administration

| Requirement | FraiseQL Implementation | Profile | Evidence |
|-------------|-------------------------|---------|----------|
| **Art. 21(1)** | Risk management measures | Security profiles, threat model, vulnerability management | REGULATED+ | [Security Controls](../security/controls-matrix.md) |
| **Art. 21(2)(a)** | Policies on risk analysis | Threat model, compliance matrix, risk-based profile selection | REGULATED+ | [Threat Model](../security/threat-model.md) |
| **Art. 21(2)(b)** | Incident handling | Security event logging, real-time alerting, audit trails | REGULATED+ | [Production Security](../production/security.md#audit-logging) |
| **Art. 21(2)(c)** | Business continuity | High availability, backup/recovery, monitoring | REGULATED+ | [Production Deployment](../production/deployment.md) |
| **Art. 21(2)(d)** | Supply chain security | SBOM generation, dependency tracking, cryptographic verification | ALL | [SLSA Provenance](./slsa-provenance.md) |
| **Art. 21(2)(e)** | Security in acquisition, development | Secure development practices, security profiles, code scanning | ALL | [Security Configuration](../security/configuration.md) |
| **Art. 21(2)(f)** | Access control | RBAC, RLS, MFA (REGULATED+), session management | REGULATED+ | [Security Controls](../security/controls-matrix.md#access-controls) |
| **Art. 21(2)(g)** | Asset management | SBOM for software assets, dependency inventory | ALL | [SLSA Provenance](./slsa-provenance.md#sbom-verification) |
| **Art. 23** | Reporting obligations (24h significant incidents) | Security event logging, incident detection, alerting | RESTRICTED | [Production Monitoring](../production/monitoring.md) |

**Compliance Status:** ✅ **Supports** NIS2 Essential and Important Entity requirements
**Recommended Profile:** REGULATED (minimum), RESTRICTED for Essential Entities
**Note:** Requires organizational incident response procedures beyond technical controls

---

### 🇬🇧 United Kingdom

#### UK NCSC Cyber Essentials Plus / High Security Guidance

**Applicability:** UK government contractors, critical national infrastructure

| Principle | FraiseQL Implementation | Profile | Evidence |
|-----------|-------------------------|---------|----------|
| **A. Firewalls** | Network configuration (infrastructure), restrictive CORS, IP allowlisting (RESTRICTED) | REGULATED+ | [Security Config](../production/security.md#cors-configuration) |
| **B. Secure Configuration** | Security profiles, minimal container images, read-only filesystem (RESTRICTED) | ALL | [Security Controls](../security/controls-matrix.md#infrastructure-controls) |
| **C. User Access Control** | RBAC, RLS, MFA (REGULATED+), session timeout | REGULATED+ | [Security Controls](../security/controls-matrix.md#access-controls) |
| **D. Malware Protection** | Container scanning, dependency scanning, SBOM for vulnerability tracking | ALL | [Security Configuration](../security/configuration.md) |
| **E. Security Update Management** | Automated dependency scanning, SBOM for patch tracking, CI/CD integration | ALL | [SLSA Provenance](./slsa-provenance.md) |

**Compliance Status:** ✅ **Supports** Cyber Essentials and Cyber Essentials Plus
**Recommended Profile:** REGULATED for Cyber Essentials Plus, RESTRICTED for high-security environments

---

### 🇦🇺 Australia

#### Essential Eight (ACSC) - Maturity Level 3

**Applicability:** Australian government, defence contractors, high-security organizations

| Mitigation Strategy | FraiseQL Implementation | Profile | Evidence |
|---------------------|-------------------------|---------|----------|
| **1. Application Control** | Code signing, container image verification, SBOM integrity checks | RESTRICTED | [SLSA Provenance](./slsa-provenance.md) |
| **2. Patch Applications** | Automated dependency scanning, SBOM for vulnerability tracking | ALL | [Security Configuration](../security/configuration.md) |
| **3. Configure Microsoft Office Macros** | N/A (backend framework) | - | - |
| **4. User Application Hardening** | Input validation, XSS prevention, CSRF protection, query complexity limits | ALL | [Production Security](../production/security.md#query-complexity-limits) |
| **5. Restrict Administrative Privileges** | RBAC with principle of least privilege, PostgreSQL role separation | ALL | [RBAC Tests](../../tests/integration/enterprise/rbac/test_role_hierarchy.py) |
| **6. Patch Operating Systems** | Container-based deployment, automated OS updates (infrastructure) | ALL | Infrastructure responsibility |
| **7. Multi-factor Authentication** | MFA enforcement (REGULATED+), integration with external IdP | REGULATED+ | [Security Controls](../security/controls-matrix.md#access-controls) |
| **8. Regular Backups** | Database backup support, audit log retention (365-2555 days) | REGULATED+ | [Security Controls](../security/controls-matrix.md#observability--monitoring-controls) |

**Compliance Status:** ✅ **Supports** Essential Eight Maturity Level 3
**Recommended Profile:** RESTRICTED for ML3
**Note:** Levels 1-2 can use STANDARD or REGULATED profiles

---

### 🇸🇬 Singapore

#### Critical Information Infrastructure (CII) Protection

**Applicability:** CII operators in 11 critical sectors (energy, water, healthcare, finance, etc.)
**Effective:** October 2025 amendments

| Requirement | FraiseQL Implementation | Profile | Evidence |
|-------------|-------------------------|---------|----------|
| **Risk Management** | Security profiles, threat model, vulnerability management | RESTRICTED | [Threat Model](../security/threat-model.md) |
| **Cybersecurity Audits** | Audit logging, compliance reporting, security controls documentation | RESTRICTED | [Security Controls](../security/controls-matrix.md) |
| **Incident Reporting** | Security event logging, real-time alerting, incident detection | RESTRICTED | [Production Monitoring](../production/monitoring.md) |
| **Supply Chain Security** | SBOM generation, dependency tracking, cryptographic verification | ALL | [SLSA Provenance](./slsa-provenance.md) |
| **Data Protection** | Encryption at rest/transit, KMS integration, access control | RESTRICTED | [KMS Architecture](../architecture/decisions/0003-kms-architecture.md) |

**Compliance Status:** ✅ **Supports** CII protection requirements
**Recommended Profile:** RESTRICTED for all CII operators

---

### 🇨🇦 Canada

#### CPCSC (Canadian Program for Cyber Security Certification)

**Applicability:** Defence contractors and suppliers
**Effective:** Phased rollout 2025-2027

| Control Area | FraiseQL Implementation | Profile | Evidence |
|--------------|-------------------------|---------|----------|
| **Access Control** | RBAC, RLS, MFA, session management | RESTRICTED | [Security Controls](../security/controls-matrix.md#access-controls) |
| **Audit Logging** | Immutable audit trails, cryptographic chains, 7-year retention | RESTRICTED | [Unified Audit](../../tests/integration/enterprise/audit/test_unified_audit.py) |
| **Cryptography** | KMS integration (HSM-backed for RESTRICTED), AES-256-GCM, TLS 1.3 | RESTRICTED | [KMS Architecture](../architecture/decisions/0003-kms-architecture.md) |
| **Incident Response** | Security event logging, real-time monitoring, alerting | RESTRICTED | [Production Monitoring](../production/monitoring.md) |
| **Supply Chain Security** | SBOM, dependency scanning, cryptographic verification | RESTRICTED | [SLSA Provenance](./slsa-provenance.md) |

**Compliance Status:** ✅ **Architecture supports** CPCSC requirements
**Recommended Profile:** RESTRICTED mandatory for defence contractors

---

### 🇺🇸 United States

#### NIST SP 800-53 Rev. 5 (Moderate/High Baselines)

**Applicability:** US federal agencies, contractors, critical infrastructure

| Family | Control | FraiseQL Implementation | Profile | Evidence |
|--------|---------|-------------------------|---------|----------|
| **AC** | AC-2 (Account Management) | RBAC, role hierarchy, permission management | ALL | [RBAC Management](../../tests/integration/enterprise/rbac/test_rbac_management_apis.py) |
| **AC** | AC-3 (Access Enforcement) | RLS policies, field-level authorization, GraphQL resolver checks | ALL | [Field Auth](../../tests/integration/enterprise/rbac/test_field_level_auth.py) |
| **AC** | AC-7 (Unsuccessful Logon Attempts) | Rate limiting on auth endpoints, account lockout (external IdP) | REGULATED+ | [Production Security](../production/security.md#rate-limiting) |
| **AU** | AU-2 (Audit Events) | Comprehensive audit logging, security events, CDC | REGULATED+ | [Unified Audit](../../tests/integration/enterprise/audit/test_unified_audit.py) |
| **AU** | AU-9 (Protection of Audit Information) | Immutable audit trails, cryptographic chains (RESTRICTED) | RESTRICTED | [Audit Tests](../../tests/integration/enterprise/audit/test_unified_audit.py) |
| **CM** | CM-7 (Least Functionality) | Minimal container images, disabled introspection (REGULATED+) | REGULATED+ | [Security Controls](../security/controls-matrix.md#api-security-controls) |
| **CM** | CM-8 (System Component Inventory) | SBOM generation, dependency tracking | ALL | [SLSA Provenance](./slsa-provenance.md#sbom-verification) |
| **IA** | IA-2 (Identification and Authentication) | JWT/OAuth2, MFA (REGULATED+), unique user IDs | REGULATED+ | [Production Security](../production/security.md#authentication-security) |
| **IA** | IA-5 (Authenticator Management) | Token rotation, password hashing (bcrypt), key management | ALL | [Production Security](../production/security.md#password-security) |
| **SC** | SC-8 (Transmission Confidentiality) | TLS 1.2+ (STANDARD/REGULATED), TLS 1.3 (RESTRICTED) | ALL | [Security Controls](../security/controls-matrix.md#encryption-controls) |
| **SC** | SC-13 (Cryptographic Protection) | AES-256-GCM, KMS integration, envelope encryption | REGULATED+ | [KMS Architecture](../architecture/decisions/0003-kms-architecture.md) |
| **SC** | SC-28 (Protection of Information at Rest) | Database encryption, KMS for key management | REGULATED+ | [KMS Architecture](../architecture/decisions/0003-kms-architecture.md) |
| **SI** | SI-3 (Malicious Code Protection) | Container scanning, dependency scanning, SBOM vulnerability tracking | ALL | [Security Configuration](../security/configuration.md) |
| **SI** | SI-10 (Information Input Validation) | GraphQL validation, parameterized queries, input sanitization | ALL | [Production Security](../production/security.md#sql-injection-prevention) |

**Compliance Status:** ✅ **Supports** NIST 800-53 Moderate and High baselines
**Recommended Profile:** REGULATED for Moderate, RESTRICTED for High

#### FedRAMP (Federal Risk and Authorization Management Program)

| Baseline | Description | FraiseQL Profile | Evidence |
|----------|-------------|------------------|----------|
| **Low** | Low-impact SaaS | STANDARD or REGULATED | [Security Controls](../security/controls-matrix.md) |
| **Moderate** | Moderate-impact SaaS | REGULATED | NIST 800-53 Moderate controls above |
| **High** | High-impact SaaS | RESTRICTED | NIST 800-53 High controls above |

**Compliance Status:** 🔄 **Architecture supports** FedRAMP requirements
**Note:** FedRAMP certification requires agency-specific assessment and authorization

#### DoD IL4/IL5 (Impact Levels)

| Impact Level | Description | FraiseQL Profile | Key Requirements |
|--------------|-------------|------------------|------------------|
| **IL4** | Controlled Unclassified Information (CUI) | RESTRICTED | NIST 800-171, CMMC Level 2 |
| **IL5** | CUI with higher security requirements | RESTRICTED | NIST 800-53 High baseline, CMMC Level 3 |

**Compliance Status:** ✅ **Architecture supports** DoD IL4/IL5 requirements
**Recommended Profile:** RESTRICTED mandatory for DoD contractors

---

## Part 3: Industry-Specific Standards

### Healthcare

#### HIPAA (Health Insurance Portability and Accountability Act)

**Applicability:** US healthcare providers, business associates
**Note:** HIPAA principles are increasingly adopted globally for health data protection

| HIPAA Rule | Requirement | FraiseQL Implementation | Profile | Evidence |
|------------|-------------|-------------------------|---------|----------|
| **§164.308(a)(1)(i)** | Security management process | Security profiles, threat model, risk assessment | REGULATED+ | [Threat Model](../security/threat-model.md) |
| **§164.308(a)(3)(i)** | Workforce security (authorization) | RBAC, role hierarchy, permission management | REGULATED+ | [RBAC Tests](../../tests/integration/enterprise/rbac/test_role_hierarchy.py) |
| **§164.308(a)(5)(i)** | Security awareness and training | Security documentation, best practices guides | REGULATED+ | [Production Security](../production/security.md) |
| **§164.310(d)(1)** | Device and media controls | Encryption at rest, KMS key lifecycle | REGULATED+ | [KMS Architecture](../architecture/decisions/0003-kms-architecture.md) |
| **§164.312(a)(1)** | Access control | RBAC, RLS, unique user IDs, session timeout | REGULATED+ | [Security Controls](../security/controls-matrix.md#access-controls) |
| **§164.312(a)(2)(i)** | Unique user identification | JWT with unique sub claim, user_id tracking | ALL | [Production Security](../production/security.md#authentication-security) |
| **§164.312(b)** | Audit controls | Comprehensive audit logging, PHI access tracking | REGULATED+ | [Unified Audit](../../tests/integration/enterprise/audit/test_unified_audit.py) |
| **§164.312(c)(1)** | Integrity controls | Cryptographic audit chains, data integrity checks | RESTRICTED | [Audit Tests](../../tests/integration/enterprise/audit/test_unified_audit.py) |
| **§164.312(e)(1)** | Transmission security | TLS 1.2+, encryption in transit | ALL | [Security Controls](../security/controls-matrix.md#encryption-controls) |

**Compliance Status:** ✅ **HIPAA-Ready** with REGULATED profile
**Recommended Profile:** REGULATED minimum for HIPAA compliance
**Requirements:** Business Associate Agreement (BAA) with hosting provider, policies/procedures documentation

---

## Part 4: Security Profile Recommendations

### Profile Selection Matrix

| Compliance Requirement | Minimum Profile | Recommended Profile | Key Considerations |
|-------------------------|-----------------|---------------------|---------------------|
| **ISO 27001 (any industry)** | REGULATED | REGULATED | Add certification audit prep |
| **GDPR (EU data processing)** | REGULATED | REGULATED | Implement consent management |
| **PCI-DSS Level 2** | REGULATED | REGULATED | Full environment assessment required |
| **PCI-DSS Level 1** | REGULATED | RESTRICTED | Quarterly scans, annual pentests |
| **SOC 2 Type II** | REGULATED | REGULATED | Organizational controls needed |
| **HIPAA (US healthcare)** | REGULATED | REGULATED | BAA + policies/procedures |
| **NIS2 Important Entities** | REGULATED | REGULATED | Incident response procedures |
| **NIS2 Essential Entities** | REGULATED | RESTRICTED | 24h incident reporting |
| **UK Cyber Essentials** | STANDARD | REGULATED | Basic controls sufficient |
| **UK Cyber Essentials Plus** | REGULATED | REGULATED | Testing + verification |
| **AU Essential Eight ML1-2** | STANDARD | REGULATED | Basic maturity levels |
| **AU Essential Eight ML3** | RESTRICTED | RESTRICTED | Advanced protection |
| **SG CII Operators** | RESTRICTED | RESTRICTED | Critical infrastructure |
| **CA CPCSC (Defence)** | RESTRICTED | RESTRICTED | Mandatory for contractors |
| **US FedRAMP Moderate** | REGULATED | REGULATED | Agency assessment needed |
| **US FedRAMP High** | RESTRICTED | RESTRICTED | Agency assessment needed |
| **US DoD IL4** | RESTRICTED | RESTRICTED | NIST 800-171 required |
| **US DoD IL5** | RESTRICTED | RESTRICTED | NIST 800-53 High required |

### Profile Feature Comparison

| Feature | STANDARD | REGULATED | RESTRICTED |
|---------|----------|-----------|------------|
| **Audit Logging** | Optional | **Mandatory** | **Mandatory (immutable)** |
| **MFA** | Optional | **Required** | **Required** |
| **KMS Integration** | Optional | **Required** | **HSM-backed** |
| **Session Timeout** | 24 hours | 4 hours | **1 hour** |
| **TLS Version** | 1.2+ | 1.2+ | **1.3 only** |
| **Rate Limiting** | 100/min | 60/min | **30/min** |
| **Query Depth Limit** | 10 levels | 7 levels | **5 levels** |
| **Introspection** | Enabled | **Disabled** | **Disabled** |
| **IP Allowlisting** | Disabled | Optional | **Required** |
| **Mutual TLS** | Disabled | Optional | **Required** |
| **Log Retention** | 30 days | 365 days | **2555 days (7 years)** |
| **Real-time Alerting** | Optional | **Required** | **Required** |
| **Anomaly Detection** | Disabled | Optional | **Enabled** |
| **Air-gapped Support** | No | No | **Yes** |

---

## Evidence Links Quick Reference

### Code Implementation
- **[RBAC Management](../../tests/integration/enterprise/rbac/test_rbac_management_apis.py)** - Role and permission management
- **[Row-Level Security](../../tests/integration/enterprise/rbac/test_row_level_security.py)** - Multi-tenant isolation tests
- **[Field-Level Authorization](../../tests/integration/enterprise/rbac/test_field_level_auth.py)** - Fine-grained access control
- **[Unified Audit](../../tests/integration/enterprise/audit/test_unified_audit.py)** - Comprehensive audit logging
- **[Audit Schema](../../tests/integration/enterprise/audit/test_audit_schema.py)** - Immutable audit table structure

### Documentation
- **[SLSA Provenance & SBOM](./slsa-provenance.md)** - Supply chain security verification
- **[Security Profiles](./security-profiles.md)** - Configuration for different compliance levels
- **[Production Security](../production/security.md)** - SQL injection, rate limiting, CORS, auth
- **[Security Controls Matrix](../security/controls-matrix.md)** - Detailed control implementation
- **[KMS Architecture](../architecture/decisions/0003-kms-architecture.md)** - Encryption key management
- **[Threat Model](../security/threat-model.md)** - Security risk assessment
- **[Security Configuration](../security/configuration.md)** - Configuration best practices

---

## How to Use This Matrix

### For Security Officers
1. **Identify your requirements** - Find your compliance framework(s) in the matrix
2. **Select security profile** - Use the Profile Selection Matrix above
3. **Review evidence links** - Verify technical implementation matches requirements
4. **Generate compliance report** - Use `fraiseql compliance report --framework [iso27001|gdpr|pci-dss|...]`

### For Procurement Officers
1. **Check vendor claims** - Verify FraiseQL's SLSA provenance ([guide](./slsa-provenance.md))
2. **Review SBOM** - Inspect dependencies and vulnerabilities
3. **Assess compliance posture** - Use this matrix as checklist
4. **Request evidence** - All test files and documentation linked above

### For Auditors
1. **Test access controls** - Run RBAC and RLS test suites ([RBAC tests](../../tests/integration/enterprise/rbac/))
2. **Verify audit trails** - Inspect audit logging implementation ([Audit tests](../../tests/integration/enterprise/audit/))
3. **Review security architecture** - Check threat model and controls matrix
4. **Validate encryption** - Verify KMS integration and key management

---

## Compliance Reporting

FraiseQL includes CLI tools for automated compliance reporting:

```bash
# Generate compliance report
fraiseql compliance report --framework iso27001 --output report.pdf

# Supported frameworks
fraiseql compliance report --framework [iso27001|gdpr|pci-dss|soc2|nist-800-53|fedramp|nis2|hipaa]

# Export control evidence
fraiseql compliance export-evidence --output evidence/
```

---

## Frequently Asked Questions

**Q: Does FraiseQL have [ISO 27001 / SOC 2 / FedRAMP] certification?**

A: FraiseQL provides the **technical controls** required for these certifications. Certification is organization-specific and requires both technical and organizational controls (policies, procedures, training). This matrix documents FraiseQL's technical implementation to support your certification process.

**Q: Which profile should I use for multiple compliance requirements?**

A: Use the **highest required profile**. For example, if you need both GDPR (REGULATED) and AU Essential Eight ML3 (RESTRICTED), use RESTRICTED as it includes all REGULATED controls plus additional protections.

**Q: Can I customize security profiles?**

A: Yes. Profiles are configuration templates. You can enable/disable specific controls based on your risk assessment. See [Security Configuration Guide](../security/configuration.md) for details.

**Q: Are these controls auditable?**

A: Yes. All controls have **test evidence** (linked in Evidence column) and comprehensive documentation. Automated test suites verify control implementation on every commit.

**Q: What about compliance in regions not listed?**

A: Start with **ISO 27001** (international standard) as baseline. Most regional frameworks align with or reference ISO 27001. Contact us if you need specific guidance for your region.

**Q: How do I demonstrate compliance to auditors?**

A: Use this matrix as checklist, provide evidence links (test results, documentation), generate compliance reports using CLI tools, and document your configuration choices in security policies.

---

## Maintenance

**Review Frequency:** Quarterly or when regulatory requirements change

**Last Review:** 2025-12-08
**Next Review:** 2026-03-08

**Change Control:** All compliance mappings reviewed by security team before updates

---

## Related Documentation

- **[Security & Compliance Hub](./README.md)** - Overview and quick start
- **[SLSA Provenance Verification](./slsa-provenance.md)** - Supply chain security
- **[Security Profiles Guide](./security-profiles.md)** - Configuration for compliance
- **[Global Regulations Guide](../compliance/global-regulations.md)** - Detailed regulatory analysis
- **[Production Security](../production/security.md)** - Implementation best practices
- **[Security Controls Matrix](../security/controls-matrix.md)** - Technical control details

---

**For Questions or Support:**
- **Email:** security@fraiseql.com
- **Enterprise Support:** Available for REGULATED/RESTRICTED deployments
- **GitHub Discussions:** Community support for compliance questions

---

*This compliance matrix provides a comprehensive mapping of FraiseQL security features to international and regional compliance requirements. For legal advice on compliance obligations, consult qualified legal counsel in your jurisdiction.*
