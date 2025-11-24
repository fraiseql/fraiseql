# Global Regulatory Compliance Guide

**Document Version:** 1.0
**Last Updated:** 2025-11-24
**Status:** Active

---

## Overview

FraiseQL's security features support compliance with supply chain security regulations and industry standards across multiple jurisdictions. This document provides a comprehensive mapping of FraiseQL features to global regulatory requirements.

---

## 🌍 Jurisdiction-Specific Requirements

### 🇺🇸 United States

#### Executive Order 14028 (May 2021)
**"Improving the Nation's Cybersecurity"**

**Requirements:**
- Software vendors must provide Software Bill of Materials (SBOM)
- Use of secure software development practices
- Supply chain security for federal procurement

**FraiseQL Support:**
- ✅ Automated SBOM generation (`fraiseql sbom generate`)
- ✅ CycloneDX 1.5 format (OWASP standard)
- ✅ Cryptographic signing with Cosign
- ✅ CI/CD integration for automated compliance

**Reference:** [White House EO 14028](https://www.whitehouse.gov/briefing-room/presidential-actions/2021/05/12/executive-order-on-improving-the-nations-cybersecurity/)

---

#### NIST SP 800-161 Rev. 1
**Cybersecurity Supply Chain Risk Management**

**Requirements:**
- Identify and assess supply chain risks
- Implement risk mitigation strategies
- Continuous monitoring and assessment

**FraiseQL Support:**
- ✅ SBOM provides complete dependency visibility
- ✅ Package URL (PURL) identifiers for vulnerability tracking
- ✅ Cryptographic hashes for integrity verification

**Reference:** [NIST SP 800-161](https://csrc.nist.gov/publications/detail/sp/800-161/rev-1/final)

---

#### NIST SP 800-218
**Secure Software Development Framework (SSDF)**

**Requirements:**
- Prepare the Organization (PO)
- Protect the Software (PS)
- Produce Well-Secured Software (PW)
- Respond to Vulnerabilities (RV)

**FraiseQL Support:**
- ✅ Security profiles (STANDARD, REGULATED, RESTRICTED)
- ✅ Multi-provider KMS for key management
- ✅ Observability with OpenTelemetry
- ✅ SBOM for vulnerability response

**Reference:** [NIST SP 800-218](https://csrc.nist.gov/publications/detail/sp/800-218/final)

---

### 🇨🇦 Canada

#### CCCS SBOM Guidance
**Canadian Centre for Cyber Security**

**Requirements:**
- Joint guidance with US CISA on SBOM adoption
- Software transparency for critical infrastructure

**FraiseQL Support:**
- ✅ CycloneDX/SPDX format support
- ✅ Automated SBOM generation
- ✅ Integration with vulnerability databases

**Reference:** [CCCS SBOM Guidance](https://www.cyber.gc.ca/en/news-events/joint-guidance-shared-vision-software-bill-materials-cyber-security)

---

#### Canadian Program for Cyber Security Certification (CPCSC)
**Effective: March 2025 (phased through 2027)**

**Requirements:**
- Cyber security certification for defence contractors
- Self-assessment and third-party audits
- Continuous compliance monitoring

**FraiseQL Support:**
- ✅ RESTRICTED security profile for defence applications
- ✅ Audit logging and compliance reporting
- ✅ KMS integration for cryptographic requirements

**Reference:** [CPCSC Program](https://www.canada.ca/en/public-services-procurement/services/industrial-security/security-requirements-contracting/cyber-security-certification-defence-suppliers-canada.html)

---

### 🇪🇺 European Union

#### NIS2 Directive (Directive 2022/2555)
**Network and Information Systems Directive**
**Effective: October 2024**

**Requirements:**
- Supply chain security risk management
- Incident reporting (including supply chain incidents)
- Security measures for essential and important entities

**Sectors Covered:**
- Energy, transport, healthcare, finance, water, digital infrastructure, manufacturing, postal services, public administration, space

**FraiseQL Support:**
- ✅ Supply chain transparency via SBOM
- ✅ Security event logging and audit trails
- ✅ Incident detection and reporting capabilities

**Reference:** [NIS2 Directive (EU)](https://digital-strategy.ec.europa.eu/en/policies/nis2-directive)

---

#### EU Cyber Resilience Act (CRA)
**Phasing in: 2025-2027**
**🔥 Explicit SBOM Requirement**

**Requirements:**
- Manufacturers must create and maintain SBOM in machine-readable format
- Must include top-level dependencies
- Update SBOM with each release
- Vulnerability disclosure process

**Products Covered:**
- All products with software components sold in EU

**FraiseQL Support:**
- ✅ **Explicit SBOM compliance** - CycloneDX 1.5
- ✅ Direct and transitive dependencies included
- ✅ Automated CI/CD generation
- ✅ Version-tracked SBOMs

**Reference:** [EU CRA SBOM Requirements](https://fossa.com/blog/sbom-requirements-cra-cyber-resilience-act/)

---

### 🇬🇧 United Kingdom

#### UK NCSC Supply Chain Security Guidance
**12 Principles for Supply Chain Security**

**Key Principles:**
1. Understand the risks
2. Establish control
3. Check your arrangements
4. Continuous improvement

**FraiseQL Support:**
- ✅ SBOM provides risk visibility
- ✅ Security profiles establish control
- ✅ Audit logging for continuous monitoring

**Reference:** [UK NCSC Guidance](https://www.ncsc.gov.uk/collection/supply-chain-security)

---

### 🇦🇺 Australia

#### Essential Eight Framework (ACSC)
**2025 Updates - Supply Chain Focus**

**Maturity Levels:**
- Level 1: Baseline security controls
- Level 2: Enhanced protection
- **Level 3: High-risk environments** (government, defence)

**Supply Chain Controls:**
- Third-party vendor security assessment
- Software component verification
- Supply chain risk management

**FraiseQL Support:**
- ✅ RESTRICTED profile aligns with Level 3
- ✅ SBOM for vendor assessment
- ✅ Cryptographic verification of dependencies

**Reference:** [Essential Eight (ACSC)](https://www.cyber.gov.au/business-government/asds-cyber-security-frameworks/essential-eight)

---

### 🇸🇬 Singapore

#### Cybersecurity Act Amendments
**Effective: October 2025**

**Requirements:**
- Critical Information Infrastructure (CII) supply chain incident reporting
- Data-driven cyber supply chain risk management
- SBOM as software attestation

**FraiseQL Support:**
- ✅ SBOM generation for CII compliance
- ✅ Security event logging
- ✅ Incident detection capabilities

**Reference:** [Singapore Cybersecurity Act](https://www.csa.gov.sg/legislation/cybersecurity-act/)

---

## 🌐 International Standards

### ISO/IEC 27001:2022
**Information Security Management Systems**

#### Control 5.21: Managing Information Security in the ICT Supply Chain

**Requirements:**
- Identify and assess ICT supply chain risks
- Suppliers provide component information
- Security functions and operation guidance
- Verification of component integrity

**FraiseQL Support:**
- ✅ SBOM provides complete component information
- ✅ Cryptographic hashes for integrity
- ✅ Package URLs (PURL) for component identification

**Reference:** [ISO 27001:2022](https://www.iso.org/standard/27001)

---

### PCI-DSS 4.0
**Payment Card Industry Data Security Standard**
**Effective: March 31, 2025** 🔥

#### Requirement 6.3.2: Software Component Inventory

**Requirements:**
- Maintain inventory of bespoke and custom software
- Include all payment software components and dependencies
- Document execution platforms, libraries, and services

**FraiseQL Support:**
- ✅ **Mandatory compliance** via SBOM (most practical approach)
- ✅ Complete dependency inventory (direct + transitive)
- ✅ CycloneDX format widely supported by PCI tools

**Reference:** [PCI-DSS 4.0 SBOM Guide](https://www.cybeats.com/blog/pci-dss-4-0-sboms-a-2025-readiness-guide)

**Applies to:** Any organization processing payment cards **worldwide**

---

### SOC 2 Type II
**Trust Services Criteria**

**Relevant Criteria:**
- **Security:** System protection against unauthorized access
- **Availability:** System available for operation
- **Confidentiality:** Confidential information protected

**FraiseQL Support:**
- ✅ Security profiles for consistent controls
- ✅ Audit logging for compliance evidence
- ✅ KMS for confidentiality

**Reference:** [AICPA SOC 2](https://www.aicpa.org/soc4so)

---

## 📋 FraiseQL Feature Mapping

### SBOM Generation

| Regulatory Requirement | FraiseQL Feature | Compliance Status |
|-------------------------|------------------|-------------------|
| US EO 14028 | `fraiseql sbom generate` | ✅ Compliant |
| EU CRA | CycloneDX 1.5 format | ✅ Compliant |
| PCI-DSS 6.3.2 | Component inventory | ✅ Compliant |
| ISO 27001 Control 5.21 | Component information | ✅ Compliant |
| NIS2 (recommended) | Supply chain transparency | ✅ Supported |

---

### Security Profiles

| Profile | Use Case | Regulatory Alignment |
|---------|----------|----------------------|
| **STANDARD** | Development, staging, general applications | Industry best practices |
| **REGULATED** | PCI-DSS, HIPAA, SOC 2, healthcare, finance | Industry-specific regulations |
| **RESTRICTED** | Government, defence, CII, high-risk | 🇺🇸 FedRAMP/NIST 800-53<br>🇪🇺 NIS2 Essential<br>🇨🇦 CPCSC<br>🇦🇺 Essential Eight L3<br>🇸🇬 CII |

---

### Key Management Service (KMS)

**Supported Providers:**
- HashiCorp Vault (production-ready)
- AWS KMS (multi-region support)
- GCP Cloud KMS
- Local (development only)

**Regulatory Alignment:**
- ✅ NIST SP 800-218 (PS: Protect the Software)
- ✅ ISO 27001:2022 cryptographic controls
- ✅ PCI-DSS encryption requirements
- ✅ HIPAA encryption standards

---

### Observability & Audit Logging

**Features:**
- OpenTelemetry integration
- Structured logging
- Security event tracking
- PII sanitization

**Regulatory Alignment:**
- ✅ NIS2 Directive (incident reporting)
- ✅ ISO 27001 (monitoring requirements)
- ✅ SOC 2 (audit trail requirements)
- ✅ CPCSC (continuous monitoring)

---

## 🚀 Implementation Roadmap

### Phase 1: Assessment
1. Identify applicable regulations for your jurisdiction
2. Determine required security profile (STANDARD/REGULATED/RESTRICTED)
3. Review SBOM requirements

### Phase 2: Configuration
1. Enable security profile:
   ```python
   from fraiseql.security.profiles import SecurityProfile, ProfileEnforcer

   enforcer = ProfileEnforcer(profile=SecurityProfile.REGULATED)
   ```
2. Configure KMS provider (if required):
   ```python
   from fraiseql.security.kms import VaultKMSProvider, VaultConfig

   kms = VaultKMSProvider(VaultConfig(
       vault_addr="https://vault.example.com:8200",
       token=os.environ["VAULT_TOKEN"]
   ))
   ```

### Phase 3: SBOM Generation
1. Generate SBOM:
   ```bash
   fraiseql sbom generate --output fraiseql-1.0.0-sbom.json
   ```
2. Integrate into CI/CD (see `.github/workflows/sbom-generation.yml`)
3. Distribute SBOM to customers/auditors

### Phase 4: Continuous Compliance
1. Enable audit logging
2. Monitor security events
3. Update SBOM with each release
4. Conduct regular security assessments

---

## 📚 Additional Resources

### Official Documentation
- [FraiseQL Security Configuration](../security/configuration.md)
- [FraiseQL Security Controls Matrix](../security/controls-matrix.md)
- [SBOM Process Guide](../../COMPLIANCE/SUPPLY_CHAIN/SBOM_PROCESS.md)

### External Standards
- [NIST Cybersecurity Framework](https://www.nist.gov/cyberframework)
- [OWASP SBOM Forum](https://owasp.org/www-community/Component_Analysis)
- [CycloneDX Specification](https://cyclonedx.org/specification/overview/)
- [SPDX Specification](https://spdx.dev/specifications/)

---

## 🆘 Support

For compliance-related questions:
- **GitHub Issues:** https://github.com/fraiseql/fraiseql/issues
- **Security Reports:** [Create a Security Advisory](https://github.com/fraiseql/fraiseql/security/advisories/new)
- **Documentation:** https://fraiseql.dev
- **Email:** security@fraiseql.com (for non-security questions only)

---

**Disclaimer:** This document provides guidance on FraiseQL features that support regulatory compliance. It is not legal advice. Consult with your legal and compliance teams to ensure your specific regulatory requirements are met.
