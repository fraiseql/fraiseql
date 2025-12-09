# Security & Compliance Hub

**Audience:** Compliance officers, security auditors, procurement officers, CTOs
**Technical Level:** Non-technical overview with links to detailed guides
**Time to Review:** 10-15 minutes

---

## Overview

FraiseQL is designed for **production-grade security and regulatory compliance** out of the box. This hub provides a comprehensive overview of security features, compliance capabilities, and verification guides for regulated industries.

### Key Security Features

✅ **Supply Chain Security**
- SLSA Level 3 provenance with cryptographic signing
- Automated SBOM generation (CycloneDX, SPDX)
- Reproducible builds with integrity verification

✅ **Data Protection**
- Multi-provider KMS integration (AWS, Azure, GCP, HashiCorp Vault)
- Row-level security (RLS) with PostgreSQL policies
- Encrypted audit trails with tamper-proof cryptographic chain
- PII anonymization for development environments

✅ **Access Control**
- Role-based access control (RBAC)
- Multi-tenant data isolation
- Fine-grained authorization policies
- JWT-based authentication

✅ **Audit & Compliance**
- Comprehensive audit logging with Change Data Capture (CDC)
- Immutable audit trails with event hashing
- Compliance profiles (GDPR, HIPAA, SOC 2, FedRAMP)
- Real-time security monitoring

---

## Quick Compliance Checklist

Use this checklist to assess FraiseQL's compliance with your organization's requirements:

### Supply Chain Security

- [ ] **SBOM Available** - Software Bill of Materials in CycloneDX/SPDX format
- [ ] **Provenance Verified** - SLSA provenance with cryptographic signatures
- [ ] **Dependencies Tracked** - Complete dependency tree with vulnerability status
- [ ] **Build Reproducibility** - Verifiable builds with integrity checks

**Guide:** [SLSA Provenance Verification](./slsa-provenance.md) *(coming in WP-011)*

### Data Privacy & Protection

- [ ] **GDPR Compliance** - Right to erasure, data portability, consent tracking
- [ ] **HIPAA Compliance** - PHI encryption, access logging, BAA support
- [ ] **Data Encryption** - At-rest and in-transit encryption
- [ ] **Key Management** - KMS integration for cryptographic key lifecycle

**Guide:** [Compliance Matrix](./compliance-matrix.md) *(coming in WP-012)*

### Access Control

- [ ] **RBAC Implemented** - Role-based permissions at database level
- [ ] **Multi-tenancy** - Tenant isolation with Row-Level Security
- [ ] **Authentication** - JWT/OAuth2 integration
- [ ] **Authorization Policies** - Fine-grained field and operation permissions

**Guide:** [Security Profiles](./security-profiles.md) *(coming in WP-013)*

### Audit & Monitoring

- [ ] **Audit Logging** - Immutable audit trails for all mutations
- [ ] **Change Tracking** - Before/after snapshots with field-level changes
- [ ] **Security Monitoring** - Real-time alerts for suspicious activity
- [ ] **Incident Response** - Automated responses to security events

**Guides:**
- [Audit Trails Deep Dive](../advanced/event-sourcing.md#audit-trails)
- [Production Monitoring](../production/monitoring.md)

---

## Regulatory Compliance

### United States

#### Executive Order 14028 (Cybersecurity)
**Status:** ✅ Fully Compliant

FraiseQL meets federal software procurement requirements:
- SBOM generation for dependency visibility
- Cryptographic signing with Cosign
- Secure software development practices
- Supply chain risk management

**Reference:** [White House EO 14028](https://www.whitehouse.gov/briefing-room/presidential-actions/2021/05/12/executive-order-on-improving-the-nations-cybersecurity/)

#### NIST SP 800-218 (Secure Software Development Framework)
**Status:** ✅ Compliant

FraiseQL implements SSDF practices:
- **Prepare the Organization:** Security profiles and KMS architecture
- **Protect the Software:** SBOM, provenance, signing
- **Produce Well-Secured Software:** Parameterized queries, input validation
- **Respond to Vulnerabilities:** Automated alerts, patch tracking

**Reference:** [NIST SP 800-218](https://csrc.nist.gov/publications/detail/sp/800-218/final)

#### FedRAMP (Federal Risk and Authorization Management Program)
**Status:** 🔄 Architecture supports FedRAMP requirements

FraiseQL provides FedRAMP-compatible controls:
- Moderate and High baseline controls
- Continuous monitoring capabilities
- Audit logging and incident response
- Configuration management

**Note:** FedRAMP certification requires agency-specific assessment.

### European Union

#### GDPR (General Data Protection Regulation)
**Status:** ✅ Compliant

FraiseQL supports GDPR requirements:
- **Right to Erasure:** Soft deletes with audit retention
- **Data Portability:** JSON export of user data
- **Consent Management:** Tracking consent status and changes
- **Privacy by Design:** Encryption, anonymization, minimal data collection

**Implementation:** [GDPR Compliance Features](../compliance/global-regulations.md#gdpr)

#### NIS2 Directive (Network and Information Security)
**Status:** ✅ Supports essential entities requirements

FraiseQL provides NIS2 compliance support:
- Incident reporting capabilities
- Supply chain security (SBOM, provenance)
- Security risk management
- Business continuity features

**Reference:** [EU NIS2 Directive](https://digital-strategy.ec.europa.eu/en/policies/nis2-directive)

### Canada

#### CCCS SBOM Guidance
**Status:** ✅ Compliant

FraiseQL follows joint US-Canada SBOM guidance:
- CycloneDX/SPDX format support
- Vulnerability tracking integration
- Critical infrastructure transparency

**Reference:** [CCCS SBOM Guidance](https://www.cyber.gc.ca/)

### United Kingdom

#### UK Cyber Essentials
**Status:** ✅ Technical controls implemented

FraiseQL provides Cyber Essentials technical controls:
- Secure configuration management
- Access control and authentication
- Malware protection (dependency scanning)
- Patch management visibility

**Reference:** [UK Cyber Essentials](https://www.ncsc.gov.uk/cyberessentials/overview)

---

## Industry Standards

### Healthcare (HIPAA)

**Status:** ✅ HIPAA-Ready

FraiseQL supports HIPAA compliance for Protected Health Information (PHI):

**Technical Safeguards:**
- Encryption at rest and in transit
- Access control with audit logging
- Unique user identification
- Automatic logoff (session management)

**Administrative Safeguards:**
- Role-based access control
- Audit logging and monitoring
- Security incident procedures

**Physical Safeguards:**
- Workstation security (application-level)
- Device and media controls (KMS integration)

**Requirements:**
- Business Associate Agreement (BAA) with hosting provider
- Security risk assessment
- Policies and procedures documentation

**Implementation Guide:** [HIPAA Security Profile](./security-profiles.md#hipaa) *(coming in WP-013)*

### Financial Services (PCI-DSS)

**Status:** 🔄 Architecture supports PCI-DSS requirements

FraiseQL provides PCI-DSS compatible controls:
- Strong access control measures (Requirement 7, 8)
- Encryption of cardholder data (Requirement 3)
- Audit logging and monitoring (Requirement 10)
- Secure development practices (Requirement 6)

**Note:** PCI-DSS compliance requires full environment assessment.

### SOC 2 (Service Organization Control)

**Status:** ✅ Architecture supports Trust Service Criteria

FraiseQL implements controls for SOC 2 Type II:

**Security:**
- Access control and authentication
- Logical and physical access controls
- Encryption and key management

**Availability:**
- System monitoring and alerting
- Incident response procedures
- Backup and recovery

**Confidentiality:**
- Data encryption
- Secure transmission protocols
- Data classification

**Processing Integrity:**
- Input validation
- Error handling and logging
- Data integrity checks

**Privacy:**
- Data minimization
- Consent management
- Data retention policies

**Implementation:** [SOC 2 Controls Mapping](./compliance-matrix.md#soc2) *(coming in WP-012)*

---

## Security Profiles

FraiseQL provides three security profiles for different regulatory environments:

### 🟢 STANDARD (Default)
**Use Cases:** General applications, internal tools, non-regulated industries

**Features:**
- Basic audit logging
- Standard encryption
- RBAC with PostgreSQL roles
- Session management

**Setup Time:** < 5 minutes
**Overhead:** Minimal (~5% performance impact)

### 🟡 REGULATED
**Use Cases:** Healthcare (HIPAA), finance (PCI-DSS), government contractors

**Features:**
- Comprehensive audit trails with CDC
- KMS integration (AWS KMS, Azure Key Vault, GCP KMS)
- Field-level encryption for sensitive data
- Enhanced access control with RLS
- Automated compliance reporting

**Setup Time:** 15-30 minutes
**Overhead:** Moderate (~10-15% performance impact)

**Requirements:**
- KMS provider account
- Audit log storage (PostgreSQL or external)
- Monitoring infrastructure

### 🔴 RESTRICTED
**Use Cases:** Defense contractors, critical infrastructure, classified systems

**Features:**
- All REGULATED features plus:
- Air-gapped deployment support
- Hardware security module (HSM) integration
- Zero-trust architecture
- Immutable audit trails with cryptographic chain
- Real-time anomaly detection

**Setup Time:** 1-2 hours
**Overhead:** Higher (~20-25% performance impact)

**Requirements:**
- HSM or FIPS 140-2 Level 3 KMS
- Dedicated audit infrastructure
- Security Operations Center (SOC) integration

**Configuration Guide:** [Security Profiles Setup](./security-profiles.md) *(coming in WP-013)*

---

## Verification Guides

### For Procurement Officers

**Verify FraiseQL's Security Claims:**

1. **Check SLSA Provenance** - Verify build integrity and supply chain security
   - Guide: [SLSA Provenance Verification](./slsa-provenance.md) *(coming in WP-011)*
   - Time: 10-15 minutes
   - Technical Skill: None (uses web tools)

2. **Review SBOM** - Inspect software dependencies and known vulnerabilities
   - Guide: [SBOM Generation and Analysis](./slsa-provenance.md#sbom-verification) *(coming in WP-011)*
   - Time: 5-10 minutes
   - Technical Skill: Basic (command line)

3. **Assess Compliance Posture** - Check regulatory compliance status
   - Guide: [Compliance Matrix](./compliance-matrix.md) *(coming in WP-012)*
   - Time: 15-20 minutes
   - Technical Skill: None (checklist-based)

### For Security Auditors

**Audit FraiseQL Deployments:**

1. **Review Security Architecture** - Assess defense-in-depth implementation
   - Guide: [Security Architecture Overview](../production/security.md)
   - Time: 30-45 minutes
   - Technical Skill: Advanced

2. **Test Access Controls** - Verify RBAC and RLS policies
   - Guide: [RBAC Testing Guide](../enterprise/rbac-postgresql-assessment.md)
   - Time: 1-2 hours
   - Technical Skill: Advanced (SQL)

3. **Validate Audit Trails** - Ensure audit logging completeness
    - Guide: [Audit Trails Deep Dive](../advanced/event-sourcing.md#audit-trails)
    - Time: 30-60 minutes
    - Technical Skill: Intermediate

### For Compliance Officers

**Demonstrate Compliance:**

1. **Generate Compliance Report** - Automated compliance status report
   - Tool: `fraiseql compliance report`
   - Time: < 5 minutes
   - Output: PDF/JSON report

2. **Map Controls to Regulations** - Cross-reference FraiseQL controls with requirements
   - Guide: [Compliance Matrix](./compliance-matrix.md) *(coming in WP-012)*
   - Time: 20-30 minutes
   - Technical Skill: None

3. **Prepare for Audit** - Gather evidence for external audits
   - Checklist: [Audit Preparation Checklist](../production/README.md#security-audit-preparation)
   - Time: 2-4 hours
   - Technical Skill: Basic

---

## Architecture Decisions

FraiseQL's security architecture is documented in Architecture Decision Records (ADRs):

- **[ADR-003: KMS Architecture](../architecture/decisions/0003-kms-architecture.md)** - Multi-provider key management
- **[ADR-005: Unified Audit Table](../architecture/decisions/003-unified-audit-table.md)** - Immutable audit logging design
- **[ADR-006: Simplified CDC](../architecture/decisions/005-simplified-single-source-cdc.md)** - Change Data Capture approach

---

## Related Documentation

### Detailed Technical Guides

- **[Production Security Guide](../production/security.md)** - SQL injection prevention, rate limiting, CORS, authentication
- **[Audit Trails Deep Dive](../advanced/event-sourcing.md#audit-trails)** - Comprehensive audit logging implementation
- **[RBAC Implementation](../enterprise/rbac-postgresql-refactored.md)** - Role-based access control with PostgreSQL
- **[KMS Integration](../architecture/decisions/0003-kms-architecture.md)** - Key management for data encryption

### Deployment & Operations

- **[Production Deployment](../production/deployment.md)** - Secure deployment configurations
- **[Monitoring & Observability](../production/observability.md)** - Security monitoring setup
- **[Production Checklist](../production/README.md)** - Pre-deployment security review

### Compliance

- **[Global Regulations Guide](../compliance/global-regulations.md)** - Detailed regulatory requirements by jurisdiction
- **[Compliance Matrix](./compliance-matrix.md)** - Control mapping (coming in WP-012)
- **[Security Profiles](./security-profiles.md)** - Configuration for regulated industries (coming in WP-013)

---

## Getting Started

### For New Projects

**1. Choose Security Profile**

```bash
# Standard profile (default)
fraiseql init --profile standard

# HIPAA-compliant healthcare application
fraiseql init --profile regulated --compliance hipaa

# Defense contractor (NIST 800-171)
fraiseql init --profile restricted --compliance nist-800-171
```

**2. Configure KMS (REGULATED/RESTRICTED profiles)**

```bash
# AWS KMS
fraiseql kms configure --provider aws --region us-east-1

# Azure Key Vault
fraiseql kms configure --provider azure --vault your-keyvault

# HashiCorp Vault
fraiseql kms configure --provider vault --address https://vault.example.com
```

**3. Enable Audit Logging**

```bash
# Generate audit table migration
fraiseql audit init

# Apply migration
fraiseql migrate up
```

**4. Verify Security Configuration**

```bash
# Run security checks
fraiseql security check

# Generate compliance report
fraiseql compliance report --format pdf
```

### For Existing Projects

**Upgrade to Compliance-Ready:**

1. **Assess Current Security Posture**
   ```bash
   fraiseql security audit
   ```

2. **Add Audit Logging**
   ```bash
   fraiseql audit init --retroactive
   ```

3. **Configure Encryption**
   ```bash
   fraiseql kms configure --provider [aws|azure|gcp|vault]
   fraiseql encrypt sensitive-fields
   ```

4. **Enable RLS Policies**
   ```bash
   fraiseql rbac enable --multi-tenant
   ```

5. **Test Compliance**
   ```bash
   fraiseql compliance test --standard [gdpr|hipaa|soc2]
   ```

---

## Support & Resources

### Documentation
- **[Security Best Practices](../production/security.md)**
- **[Architecture Decisions](../architecture/decisions/README.md)**
- **[Production Deployment](../production/deployment.md)**

### Tools
- **SBOM Generator:** `fraiseql sbom generate`
- **Security Scanner:** `fraiseql security check`
- **Compliance Reporter:** `fraiseql compliance report`

### Community
- **GitHub Discussions:** Security questions and best practices
- **Security Advisories:** Subscribe for vulnerability notifications
- **Bug Bounty Program:** Report security issues responsibly

### Professional Services
For compliance consulting, security audits, or custom implementations:
- Email: security@fraiseql.com
- Enterprise Support: Available for REGULATED/RESTRICTED deployments

---

## Frequently Asked Questions

### General

**Q: Is FraiseQL certified for [GDPR/HIPAA/SOC 2]?**

A: FraiseQL provides the technical controls and features required for compliance, but certification is organization-specific. Our security profiles (STANDARD/REGULATED/RESTRICTED) implement industry best practices, and our compliance matrix maps features to specific regulatory requirements.

**Q: Does FraiseQL require external security tools?**

A: No. Core security features (RBAC, audit logging, input validation) are built-in. However, for REGULATED/RESTRICTED profiles, you'll need a KMS provider (AWS KMS, Azure Key Vault, GCP KMS, or HashiCorp Vault) for encryption key management.

**Q: What's the performance impact of security features?**

A:
- STANDARD profile: ~5% overhead (minimal)
- REGULATED profile: ~10-15% overhead (moderate)
- RESTRICTED profile: ~20-25% overhead (comprehensive protection)

### Supply Chain Security

**Q: How do I verify FraiseQL's SLSA provenance?**

A: See [SLSA Provenance Verification Guide](./slsa-provenance.md) *(coming in WP-011)*. Verification takes 10-15 minutes using web-based tools and requires no specialized knowledge.

**Q: Can I generate SBOMs for my application?**

A: Yes. FraiseQL includes SBOM generation for your entire application stack:
```bash
fraiseql sbom generate --format cyclonedx --output sbom.json
```

**Q: Is FraiseQL vulnerable to supply chain attacks?**

A: FraiseQL implements multiple protections:
- Cryptographically signed releases
- SLSA Level 3 provenance
- Reproducible builds
- Dependency pinning with integrity checks

### Data Protection

**Q: Does FraiseQL encrypt data at rest?**

A: Yes, when using REGULATED or RESTRICTED profiles with KMS integration. FraiseQL supports:
- Database-level encryption (PostgreSQL)
- Field-level encryption (sensitive data)
- Key rotation and lifecycle management

**Q: How are audit logs protected from tampering?**

A: Audit logs use cryptographic chaining - each event includes a hash of the previous event, making tampering detectable. RESTRICTED profile adds real-time integrity monitoring.

**Q: Can I anonymize PII for development environments?**

A: Yes. FraiseQL includes PII anonymization tools:
```bash
fraiseql data anonymize --env development
```

### Compliance

**Q: Does FraiseQL support air-gapped deployments?**

A: Yes. RESTRICTED profile supports fully air-gapped deployments for classified systems. Contact enterprise support for implementation guidance.

**Q: How long are audit logs retained?**

A: Configurable per regulatory requirements:
- GDPR: Typically 6-12 months
- HIPAA: Minimum 6 years
- SOC 2: Varies by control

**Q: Can I export audit logs for external SIEM?**

A: Yes. FraiseQL supports audit log export to:
- Splunk, Datadog, New Relic (via OpenTelemetry)
- AWS CloudWatch, Azure Monitor, GCP Cloud Logging
- Custom SIEM via webhook or API

---

**Last Updated:** 2025-12-08
**Version:** 1.0
**Maintainer:** FraiseQL Security Team

---

**Next Steps:**
- Review [SLSA Provenance Verification](./slsa-provenance.md) *(coming in WP-011)*
- Check [Compliance Matrix](./compliance-matrix.md) *(coming in WP-012)*
- Configure [Security Profiles](./security-profiles.md) *(coming in WP-013)*
