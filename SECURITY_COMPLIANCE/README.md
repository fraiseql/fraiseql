# Security & Compliance Documentation

This folder contains security, compliance, and federal readiness documentation for FraiseQL.

## Document Index

| Document | Purpose | Status |
|----------|---------|--------|
| [PENTAGON_READINESS_AUDIT.md](PENTAGON_READINESS_AUDIT.md) | Comprehensive security audit | Complete |
| [THREAT_MODEL.md](THREAT_MODEL.md) | Threat analysis and mitigations | Template |
| [INCIDENT_RESPONSE.md](INCIDENT_RESPONSE.md) | IR procedures and playbooks | Template |
| [HARDENING_GUIDE.md](HARDENING_GUIDE.md) | System hardening procedures | Template |
| [COMPLIANCE_MATRIX.md](COMPLIANCE_MATRIX.md) | Control mapping to standards | Template |

## Quick Links

### For Security Teams
- Review the [Pentagon-Readiness Audit](PENTAGON_READINESS_AUDIT.md) for current security posture
- Check [Compliance Matrix](COMPLIANCE_MATRIX.md) for NIST 800-53 control mappings
- Follow [Incident Response](INCIDENT_RESPONSE.md) for security events

### For DevOps/SRE Teams
- Implement [Hardening Guide](HARDENING_GUIDE.md) before deployment
- Configure monitoring per the audit recommendations
- Review CI/CD security controls in the audit

### For Developers
- Follow secure coding practices in [Threat Model](THREAT_MODEL.md)
- Use pre-commit hooks for security scanning
- Review input validation requirements

## Compliance Standards Addressed

- **NIST SP 800-53 Rev 5** - Security and Privacy Controls
- **NIST SP 800-218** - Secure Software Development Framework (SSDF)
- **Executive Order 14028** - Software Supply Chain Security
- **SLSA** - Supply-chain Levels for Software Artifacts
- **CycloneDX/SPDX** - SBOM Standards

## Pentagon-Readiness Score

**Current Score: 62/100**

Key areas for improvement:
1. SBOM generation and attestation
2. SLSA Level 3 provenance
3. Artifact signing
4. FIPS-compliant cryptography documentation
5. Threat model and incident response planning

## Contact

For security concerns, see the main [SECURITY.md](../SECURITY.md) file.
