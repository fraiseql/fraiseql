# SBOM Generation Process

**Document Version:** 1.0
**Last Updated:** 2025-11-21
**Classification:** Public
**Applicable Standards:** Industry supply chain security standards (CycloneDX, SLSA, SPDX)

## Executive Summary

FraiseQL implements automated Software Bill of Materials (SBOM) generation to comply with industry supply chain security standards. SBOMs are generated in CycloneDX format and cryptographically signed for integrity verification.

## What is an SBOM?

A Software Bill of Materials (SBOM) is a formal, machine-readable inventory of software components and dependencies. It serves as a "nutrition label" for software, enabling:

- **Transparency**: Know exactly what's in your software
- **Vulnerability Management**: Quickly identify affected systems when CVEs are disclosed
- **License Compliance**: Ensure all dependencies meet legal requirements
- **Supply Chain Security**: Verify component integrity and provenance

## Regulatory Compliance

### Global Supply Chain Security Standards

FraiseQL implements comprehensive SBOM generation following industry best practices and regulatory requirements across multiple jurisdictions:

1. ✅ Provide an SBOM to customers
2. ✅ Use a standardized format (SPDX or CycloneDX)
3. ✅ Include all software components (direct and transitive dependencies)
4. ✅ Update the SBOM with each software release
5. ✅ Enable vulnerability tracking via unique identifiers

**FraiseQL Compliance Status**: ✅ **FULLY COMPLIANT**

### Supported Jurisdictions

#### 🇺🇸 North America
- **United States**
  - [Executive Order 14028](https://www.whitehouse.gov/briefing-room/presidential-actions/2021/05/12/executive-order-on-improving-the-nations-cybersecurity/) (May 2021) - Software supply chain security for federal procurement
  - [NIST SP 800-161](https://csrc.nist.gov/publications/detail/sp/800-161/rev-1/final) - Cybersecurity Supply Chain Risk Management
  - [NIST SP 800-218](https://csrc.nist.gov/publications/detail/sp/800-218/final) - Secure Software Development Framework (SSDF)
- **Canada**
  - [CCCS SBOM Guidance](https://www.cyber.gc.ca/en/news-events/joint-guidance-shared-vision-software-bill-materials-cyber-security) - Joint guidance with US CISA
  - [Canadian Program for Cyber Security Certification (CPCSC)](https://www.canada.ca/en/public-services-procurement/services/industrial-security/security-requirements-contracting/cyber-security-certification-defence-suppliers-canada.html) - Defence procurement (2025)

#### 🇪🇺 Europe
- **European Union**
  - [NIS2 Directive](https://digital-strategy.ec.europa.eu/en/policies/nis2-directive) (Directive 2022/2555) - Supply chain security requirements (effective Oct 2024)
  - [EU Cyber Resilience Act (CRA)](https://fossa.com/blog/sbom-requirements-cra-cyber-resilience-act/) - **Explicit SBOM requirement** for products with software (phasing in 2025-2027)
- **United Kingdom**
  - [UK NCSC Supply Chain Security Guidance](https://www.ncsc.gov.uk/collection/supply-chain-security) - 12 principles for supply chain security

#### 🌏 Asia-Pacific
- **Australia**
  - [Essential Eight Framework](https://www.cyber.gov.au/business-government/asds-cyber-security-frameworks/essential-eight) (ACSC) - Third-party vendor security requirements (2025 updates)
- **Singapore**
  - [Cybersecurity Act Amendments](https://www.csa.gov.sg/legislation/cybersecurity-act/) - CII supply chain incident reporting (effective Oct 2025)
  - [CSA SBOM Advisory](https://www.csa.gov.sg/about-csa/who-we-are/committees-and-panels/operational-technology-cybersecurity-expert-panel/evolving-security-threats-emerging-regualtions) - Automated SBOM generation guidance

#### 🌐 International Standards
- **ISO/IEC Standards**
  - [ISO 27001:2022](https://www.iso.org/standard/27001) Control 5.21 - Managing Information Security in ICT Supply Chain
  - [ISO 5962:2021](https://www.iso.org/standard/81870.html) - SPDX format standardization
- **Industry Regulations**
  - [PCI-DSS 4.0](https://www.cybeats.com/blog/pci-dss-4-0-sboms-a-2025-readiness-guide) Requirement 6.3.2 - Software component inventory (effective **March 31, 2025**)
  - HIPAA - Healthcare data security (US, influences global healthcare software)
  - SOC 2 Type II - Trust Services Criteria (global standard)

### SBOM Format Standards

- **[CycloneDX](https://cyclonedx.org)** (OWASP) - Security-focused SBOM format, FraiseQL default
- **[SPDX](https://spdx.dev)** (Linux Foundation) - ISO/IEC 5962:2021 standard
- **[SLSA Framework](https://slsa.dev)** - Supply chain Levels for Software Artifacts

## SBOM Format

FraiseQL generates SBOMs in **CycloneDX 1.5** format (JSON or XML).

### Why CycloneDX?

- ✅ OWASP standard designed for security use cases
- ✅ Comprehensive metadata (licenses, hashes, vulnerabilities)
- ✅ Wide tool support (vulnerability scanners, compliance tools)
- ✅ Industry-standard format supported by major security platforms

### SBOM Structure

```json
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.5",
  "serialNumber": "urn:uuid:3e671687-395b-41f5-a30f-a58921a69b79",
  "version": 1,
  "metadata": {
    "timestamp": "2025-11-21T00:00:00Z",
    "tools": [{"name": "fraiseql-sbom-generator", "vendor": "FraiseQL"}],
    "component": {
      "type": "application",
      "name": "fraiseql",
      "version": "1.6.2",
      "description": "GraphQL framework for PostgreSQL"
    }
  },
  "components": [
    {
      "bom-ref": "uuid-here",
      "type": "library",
      "name": "fastapi",
      "version": "0.115.12",
      "purl": "pkg:pypi/fastapi@0.115.12",
      "licenses": [{"license": {"id": "MIT", "name": "MIT License"}}],
      "hashes": [{"alg": "SHA-256", "content": "abc123..."}]
    }
  ]
}
```

## Generating SBOMs

### Automated Generation (CI/CD)

SBOMs are automatically generated on every release via GitHub Actions:

```yaml
# .github/workflows/sbom-generation.yml
- name: Generate SBOM
  run: |
    fraiseql sbom generate \
      --output fraiseql-1.6.2-sbom.json \
      --component-name fraiseql \
      --component-version 1.6.2
```

**Artifacts Published:**
1. `fraiseql-{version}-sbom.json` - CycloneDX SBOM
2. `fraiseql-{version}-sbom.json.sig` - Cosign signature (keyless)
3. `fraiseql-{version}-sbom.json.pem` - Cosign certificate
4. `fraiseql-{version}-sbom.json.sha256` - SHA256 checksum

### Manual Generation

#### Using CLI

```bash
# Generate SBOM for current project
fraiseql sbom generate --output fraiseql-sbom.json

# Generate with custom metadata
fraiseql sbom generate \
  --output sbom.json \
  --component-name "my-app" \
  --component-version "1.0.0" \
  --supplier-name "My Organization" \
  --supplier-url "https://example.com" \
  --author "Developer Name"

# Generate XML format
fraiseql sbom generate --format xml --output sbom.xml
```

#### Using Python API

```python
from fraiseql.sbom.application.sbom_generator import SBOMGenerator
from fraiseql.sbom.infrastructure.package_scanner import PythonPackageScanner
from pathlib import Path

# Initialize scanner and generator
scanner = PythonPackageScanner(project_root=Path.cwd())
generator = SBOMGenerator(metadata_repository=scanner)

# Generate and save SBOM
generator.generate_and_save(
    output_path=Path("fraiseql-sbom.json"),
    component_name="fraiseql",
    component_version="1.6.2",
    format="json"
)
```

## Validating SBOMs

### Using FraiseQL CLI

```bash
# Validate SBOM structure
fraiseql sbom validate --input fraiseql-1.6.2-sbom.json

# Verbose output with component details
fraiseql sbom validate --input fraiseql-sbom.json --verbose
```

### Using CycloneDX CLI

```bash
# Install CycloneDX CLI
npm install -g @cyclonedx/cyclonedx-cli

# Validate against CycloneDX schema
cyclonedx validate --input-file fraiseql-sbom.json
```

## Verifying SBOM Integrity

### 1. Verify SHA256 Checksum

```bash
sha256sum -c fraiseql-1.6.2-sbom.json.sha256
```

**Expected Output:**
```
fraiseql-1.6.2-sbom.json: OK
```

### 2. Verify Cosign Signature (Keyless)

```bash
# Install Cosign
brew install cosign  # macOS
# or download from https://github.com/sigstore/cosign/releases

# Verify signature
cosign verify-blob \
  --signature fraiseql-1.6.2-sbom.json.sig \
  --certificate fraiseql-1.6.2-sbom.json.pem \
  --certificate-identity-regexp "https://github.com/fraiseql/fraiseql" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  fraiseql-1.6.2-sbom.json
```

**Expected Output:**
```
Verified OK
```

## SBOM Update Frequency

| Event | SBOM Action |
|-------|-------------|
| **New Release** | Generate new SBOM with version-specific filename |
| **Dependency Update** | Regenerate SBOM if dependencies change |
| **Security Patch** | Generate updated SBOM for patched version |
| **Development** | Optional SBOM generation for internal testing |

**Best Practice**: Generate SBOM for every versioned release (not development branches).

## Using SBOMs for Vulnerability Management

### 1. Import into Vulnerability Scanners

```bash
# Using Grype
grype sbom:fraiseql-1.6.2-sbom.json

# Using Trivy
trivy sbom --severity HIGH,CRITICAL fraiseql-1.6.2-sbom.json

# Using Dependency-Track
# Upload SBOM to Dependency-Track web UI
# http://localhost:8080/projects/{project-id}/upload
```

### 2. Continuous Monitoring

```yaml
# GitHub Actions - Daily vulnerability scan
- name: Scan SBOM for Vulnerabilities
  uses: aquasecurity/trivy-action@master
  with:
    scan-type: 'sbom'
    scan-ref: 'fraiseql-1.6.2-sbom.json'
    severity: 'CRITICAL,HIGH'
```

### 3. Enterprise Integration

Organizations can:
1. Download SBOM from GitHub Releases
2. Verify signature with Cosign
3. Import into vulnerability management systems
4. Monitor for new CVEs affecting FraiseQL dependencies
5. Receive alerts when action is required

## License Compliance

SBOMs include license information for all components, enabling:

### Automated License Scanning

```bash
# Check for copyleft licenses (GPL)
fraiseql sbom validate --input fraiseql-sbom.json

# Output will warn about GPL-licensed components
```

### License Compliance Requirements

- ✅ **Permissive Licenses**: MIT, Apache-2.0, BSD (enterprise-friendly)
- ⚠️ **Copyleft Licenses**: GPL, AGPL (may have restrictions)
- ✅ **FraiseQL Core**: MIT License (fully compliant)

## Architecture (Domain-Driven Design)

FraiseQL's SBOM implementation follows DDD principles:

```
Domain Layer (src/fraiseql/sbom/domain/)
├── models.py          # Aggregates, Entities, Value Objects
│   ├── SBOM (Aggregate Root)
│   ├── Component (Entity)
│   ├── License (Value Object)
│   ├── Hash (Value Object)
│   └── ComponentIdentifier (Value Object)
└── repositories.py    # Repository interfaces

Application Layer (src/fraiseql/sbom/application/)
└── sbom_generator.py  # Application Service

Infrastructure Layer (src/fraiseql/sbom/infrastructure/)
├── package_scanner.py        # Concrete repository implementation
└── cyclonedx_adapter.py      # CycloneDX serialization
```

**Benefits:**
- **Domain Independence**: Core logic doesn't depend on infrastructure
- **Testability**: Domain models are easily unit-tested
- **Extensibility**: Can add SPDX format without changing domain

## Troubleshooting

### Issue: SBOM generation fails with "package not found"

**Solution**: Ensure dependencies are installed:
```bash
uv pip install ".[dev,all]"
```

### Issue: SBOM contains no components

**Solution**: SBOM reads from installed packages. Ensure you're running in a virtual environment with dependencies installed.

### Issue: License information missing for some packages

**Cause**: Some packages don't properly declare licenses in metadata.

**Solution**: This is expected. SBOM will show "no license information" warning, but still generates valid SBOM.

### Issue: Cosign verification fails

**Cause**: Certificate identity mismatch or OIDC issuer mismatch.

**Solution**: Ensure you're using the correct certificate identity:
```bash
--certificate-identity-regexp "https://github.com/fraiseql/fraiseql"
```

## For Procurement Officers

### Questions to Ask Vendors About SBOMs

✅ FraiseQL provides:
1. **SBOM Format**: CycloneDX 1.5 (OWASP standard)
2. **Update Frequency**: Every release
3. **Verification**: Cryptographic signatures (Cosign + SHA256)
4. **Vulnerability Tracking**: Package URLs (PURL) for CVE matching
5. **License Compliance**: Complete license inventory
6. **Automation**: CI/CD-generated, human-error free

### SBOM Attestation Statement

> FraiseQL provides a complete, accurate, and machine-readable SBOM in CycloneDX 1.5 format with every versioned release. SBOMs are cryptographically signed using Sigstore Cosign with keyless signing. All software components, including transitive dependencies, are inventoried with license and hash information. SBOMs are generated automatically via CI/CD pipelines and published to GitHub Releases alongside software artifacts.
>
> **Signed**: Lionel Hamayon, Project Maintainer
> **Date**: 2025-11-21
> **Effective**: FraiseQL v1.6.2 and later

## Continuous Improvement

### Roadmap

- [ ] XML format support (CycloneDX XML)
- [ ] SPDX 2.3 format support (alternative to CycloneDX)
- [ ] Dependency graph visualization
- [ ] VEX (Vulnerability Exploitability eXchange) integration
- [ ] SLSA Level 3 provenance attestations

### Feedback

For SBOM-related questions or suggestions:
- **GitHub Issues**: https://github.com/fraiseql/fraiseql/issues
- **Security Reports**: [Create a Security Advisory](https://github.com/fraiseql/fraiseql/security/advisories/new)
- **Email**: security@fraiseql.com (for non-security questions only)

---

**Document Control:**
- **Author**: Security Team
- **Reviewers**: Project Maintainers
- **Next Review**: 2026-11-21
- **Distribution**: Public
