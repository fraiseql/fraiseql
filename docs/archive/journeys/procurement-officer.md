# Procurement Officer Journey - SLSA Provenance & Supply Chain Verification

**Time to Complete:** 15 minutes
**Prerequisites:** Basic command-line familiarity or technical assistant available
**Goal:** Verify FraiseQL's supply chain security and obtain procurement evidence

## Overview

As a procurement officer evaluating FraiseQL for enterprise use, you need to verify supply chain security and obtain evidence for procurement documentation. This journey provides step-by-step verification procedures that are **copy-paste ready** and **non-technical**.

By the end of this journey, you'll have:
- Verified SLSA Level 3 provenance with cryptographic signatures
- Downloaded Software Bill of Materials (SBOM) for audit trail
- Vulnerability scan results
- Procurement evidence package for documentation
- Confidence in supply chain integrity

## What is SLSA and Why It Matters

**SLSA (Supply-chain Levels for Software Artifacts)** is a security framework that prevents tampering with software between the developer and your organization.

**Analogy:** Like a tamper-evident seal on medication - SLSA ensures the software you receive is exactly what the developer built, with no modifications.

**What SLSA Level 3 Guarantees:**
- ✅ Software built by verified GitHub Actions workflow
- ✅ No human can inject malicious code during build
- ✅ Complete audit trail of build process
- ✅ Cryptographic proof of integrity
- ✅ All dependencies documented (SBOM)

**Why This Matters for Procurement:**
- **Risk Reduction:** Prevents supply chain attacks (like SolarWinds)
- **Compliance:** Meets Executive Order 14028 (US), NIS2 (EU) requirements
- **Audit Trail:** Complete documentation for security auditors
- **Vendor Trust:** Verifiable claims (not just vendor statements)

## Step-by-Step Verification

### Step 1: Install Verification Tools (5 minutes)

**Goal:** Set up the tools needed for verification

**For macOS/Linux:**
```bash
# Install GitHub CLI (for attestation verification)
brew install gh

# Install cosign (for cryptographic verification)
brew install cosign

# Authenticate with GitHub (one-time setup)
gh auth login
```

**For Windows:**
```powershell
# Install using winget (Windows Package Manager)
winget install GitHub.cli
winget install sigstore.cosign

# Authenticate with GitHub (one-time setup)
gh auth login
```

**For Organizations Without Install Permissions:**
- **Option 1:** Request IT to install `gh` (GitHub CLI) and `cosign`
- **Option 2:** Use GitHub's web interface for manual verification (slower)
- **Option 3:** Have security team perform verification and provide report

**Success Check:** Run `gh --version` and `cosign version` to verify installation

### Step 2: Download FraiseQL Package (2 minutes)

**Goal:** Download the official FraiseQL package for verification

```bash
# Create verification directory
mkdir fraiseql-verification
cd fraiseql-verification

# Download latest FraiseQL wheel package
pip download fraiseql

# List downloaded files
ls -lh
```

**Expected Output:**
```
fraiseql-1.8.0-py3-none-any.whl
```

**What You Downloaded:**
- The FraiseQL Python package (`.whl` file)
- This is the exact package that would be installed in production

**Success Check:** You have a `.whl` file in your directory

### Step 3: Verify SLSA Attestations (3 minutes)

**Goal:** Cryptographically verify the package was built securely

**Command (Copy-Paste Ready):**
```bash
# Verify SLSA provenance using GitHub CLI
gh attestation verify fraiseql-*.whl --owner fraiseql
```

**Expected Output:**
```
✅ Verification succeeded!

Attestation verified against https://github.com/fraiseql/fraiseql
Repository: fraiseql/fraiseql
Workflow: .github/workflows/publish.yml
Build ID: 1234567890
Build Date: 2025-12-08T10:30:00Z
```

**What This Means:**
- ✅ Package was built by official GitHub Actions (not a human)
- ✅ Build process is auditable (workflow file is public)
- ✅ No tampering after build (cryptographic proof)
- ✅ Build environment is documented (reproducible)

**If Verification Fails:**
- ❌ **DO NOT PROCEED** - Contact FraiseQL security team
- ❌ **DO NOT INSTALL** - Package may be compromised
- 📧 Report to: security@fraiseql.com

**Success Check:** You see "✅ Verification succeeded!"

### Step 4: Verify Cryptographic Signatures (3 minutes)

**Goal:** Additional verification using Sigstore (industry standard)

**Command (Copy-Paste Ready):**
```bash
# Verify cryptographic signature using cosign
cosign verify-attestation --type slsaprovenance \
  --certificate-identity-regexp='^https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@.*$' \
  --certificate-oidc-issuer=https://token.actions.githubusercontent.com \
  fraiseql-*.whl
```

**Expected Output:**
```
Verification for fraiseql-1.8.0-py3-none-any.whl --
The following checks were performed on each of these signatures:
  - The signature was verified against the specified public key
  - The certificate identity matched the expected pattern
  - The build provenance was validated
✅ Verified OK
```

**What This Verifies:**
- ✅ Signature matches public certificate (keyless signing)
- ✅ Build identity is correct (official GitHub workflow)
- ✅ Timestamp is valid (recent build)
- ✅ Certificate chain is trusted (Sigstore root of trust)

**Why Two Verifications?**
- **Defense in depth:** Multiple verification methods reduce risk
- **Industry standard:** Both `gh` and `cosign` are widely used
- **Compliance:** Meets different regulatory requirements

**Success Check:** You see "✅ Verified OK"

### Step 5: Download and Verify SBOM (2 minutes)

**Goal:** Obtain Software Bill of Materials for audit documentation

**SBOM (Software Bill of Materials)** is like an ingredients label - it lists all software components and dependencies.

**Download SBOM:**
```bash
# Download SBOM from GitHub releases
curl -L -O https://github.com/fraiseql/fraiseql/releases/latest/download/fraiseql-sbom.json

# Download SBOM signature
curl -L -O https://github.com/fraiseql/fraiseql/releases/latest/download/fraiseql-sbom.json.sig

# Download SBOM certificate
curl -L -O https://github.com/fraiseql/fraiseql/releases/latest/download/fraiseql-sbom.json.cert

# Verify SBOM signature
cosign verify-blob \
  --certificate fraiseql-sbom.json.cert \
  --signature fraiseql-sbom.json.sig \
  fraiseql-sbom.json
```

**Expected Output:**
```
✅ Verified OK
```

**What the SBOM Contains:**
- Complete list of dependencies (libraries FraiseQL uses)
- Version numbers for all components
- License information
- Vulnerability status (CVEs if any)

**View SBOM Summary:**
```bash
# Count total dependencies
cat fraiseql-sbom.json | grep '"name"' | wc -l

# View top-level dependencies
cat fraiseql-sbom.json | grep '"name"' | head -20
```

**Success Check:** You have `fraiseql-sbom.json` and signature verified

## Procurement Evidence Package

### Documents to Include in Procurement File

**1. Verification Report**

Create a file called `FraiseQL-Verification-Report.txt`:

```
FraiseQL Supply Chain Verification Report
==========================================

Date: [Today's Date]
Verified By: [Your Name]
Package Version: fraiseql-1.8.0

SLSA Provenance Verification
-----------------------------
✅ PASSED - GitHub Attestation Verified
✅ PASSED - Cosign Cryptographic Signature Verified
✅ Build Workflow: .github/workflows/publish.yml
✅ Build Date: [Date from output]
✅ Repository: https://github.com/fraiseql/fraiseql

SBOM Verification
-----------------
✅ PASSED - SBOM Signature Verified
✅ Total Dependencies: [Number from SBOM]
✅ SBOM Format: CycloneDX 1.5
✅ License Compliance: Verified

Vulnerability Scan
------------------
✅ No critical vulnerabilities detected
✅ Scan Date: [Date]
✅ Tool: GitHub Security Advisories

Conclusion
----------
FraiseQL package integrity verified successfully.
Supply chain security meets enterprise procurement standards.

RECOMMENDATION: APPROVED for procurement
```

**2. SBOM (Software Bill of Materials)**
- File: `fraiseql-sbom.json`
- Purpose: Complete dependency list for audit trail
- Retention: Store with procurement records (7+ years typical)

**3. Verification Screenshots**
- Screenshot of `gh attestation verify` output
- Screenshot of `cosign verify-attestation` output
- Include in procurement documentation for non-technical stakeholders

**4. Compliance Mapping** (Reference Document)
- Link to [Compliance Matrix](../security-compliance/compliance-matrix.md)
- Highlight relevant frameworks (ISO 27001, FedRAMP, etc.)
- Include in vendor evaluation matrix

### Procurement Checklist

Use this checklist in your vendor evaluation:

- [ ] **SLSA Level 3 Verified** - Cryptographic provenance confirmed
- [ ] **SBOM Available** - Complete dependency list obtained
- [ ] **Vulnerability Scan Clean** - No critical/high CVEs
- [ ] **License Compliance** - All licenses reviewed and acceptable
- [ ] **Build Reproducibility** - Official build process documented
- [ ] **Security Contact** - Responsible disclosure program available
- [ ] **Support Options** - Commercial support available if needed
- [ ] **Community Health** - Active development and security updates
- [ ] **Compliance Evidence** - ISO 27001, GDPR, FedRAMP mappings available
- [ ] **Audit Trail** - Complete documentation for auditors

## Compliance Evidence

### For ISO 27001 (Control 5.21 - Supply Chain Security)

**Evidence:**
- ✅ SLSA Level 3 provenance (cryptographic verification)
- ✅ Automated SBOM generation (CycloneDX format)
- ✅ Dependency tracking with vulnerability monitoring
- ✅ Reproducible builds with integrity checks

**Control Implementation:**
- Verification procedures documented (this guide)
- Evidence collected and retained (SBOM, verification reports)
- Regular vulnerability scanning (automated via GitHub)

### For Executive Order 14028 (US Federal - Software Supply Chain)

**Requirements:**
- ✅ SBOM provided (CycloneDX and SPDX formats)
- ✅ Secure software development practices
- ✅ Cryptographic verification (Sigstore)
- ✅ Vulnerability disclosure program

**Evidence:**
- SBOM: `fraiseql-sbom.json`
- Provenance: GitHub Attestations
- Vulnerability Scan: GitHub Security Advisories
- Security Contact: security@fraiseql.com

### For NIS2 Directive (EU - Cybersecurity Requirements)

**Requirements (Article 21):**
- ✅ Supply chain security measures
- ✅ Vulnerability handling and disclosure
- ✅ Security by design practices
- ✅ Risk assessment procedures

**Evidence:**
- SLSA provenance verification
- Vulnerability monitoring (automated)
- Security architecture documentation
- Compliance matrix with NIS2 controls

## Frequently Asked Questions

### Q1: What if our organization doesn't allow command-line tools?

**Option 1 - Web-Based Verification:**
Visit https://github.com/fraiseql/fraiseql/attestations
- View attestations in web browser
- Download SBOM directly from releases page
- Provide screenshots for procurement evidence

**Option 2 - Security Team Verification:**
- Request IT security to perform verification
- They provide signed verification report
- Include in procurement documentation

**Option 3 - Vendor-Provided Evidence:**
- Request pre-verified evidence package from FraiseQL
- Email: procurement@fraiseql.com
- Includes: verification report, SBOM, compliance mappings

### Q2: How often should we re-verify?

**Recommended Schedule:**
- **Initial procurement:** Full verification (this guide)
- **Major version updates:** Full verification
- **Minor/patch updates:** SBOM review + vulnerability scan
- **Annual audit:** Full verification for compliance documentation

**Automated Monitoring:**
- Subscribe to GitHub Security Advisories
- Automated vulnerability notifications
- No re-verification needed unless CVE detected

### Q3: What if verification fails?

**Immediate Actions:**
1. ❌ **DO NOT INSTALL** the package
2. ❌ **DO NOT PROCEED** with procurement
3. 📧 Contact security@fraiseql.com immediately
4. 📋 Document failure details (error messages, screenshots)
5. ⏸️ Pause procurement until resolved

**Escalation Path:**
- **Internal:** Notify IT security team
- **Vendor:** security@fraiseql.com (24-hour response SLA)
- **Community:** GitHub security advisory if appropriate

### Q4: Can we verify older versions?

**Yes!** Historical verification is supported:

```bash
# Download specific version
pip download fraiseql==1.7.0

# Verify specific version
gh attestation verify fraiseql-1.7.0-py3-none-any.whl --owner fraiseql
```

**Use Cases:**
- Audit of existing deployments
- Compliance documentation for older versions
- Historical evidence for regulatory review

### Q5: What about air-gapped environments?

**Verification in Air-Gapped Networks:**

**Preparation (Connected Network):**
1. Download package + attestations
2. Download SBOM + signatures
3. Download cosign public keys
4. Transfer to air-gapped network (secure media)

**Verification (Air-Gapped Network):**
```bash
# Verify using offline public keys
cosign verify-attestation --key cosign.pub \
  --type slsaprovenance \
  fraiseql-*.whl
```

**Documentation Available:**
- Air-gapped deployment guide (contact support)
- Offline verification procedures
- Public key management

### Q6: How do we verify the verification tools themselves?

**Trust Chain:**
1. **GitHub CLI** - Signed by GitHub (Microsoft-owned)
2. **Cosign** - Part of Sigstore (Linux Foundation project)
3. **Package managers** (brew/winget) - OS-level trust

**Verification:**
- Download from official sources only
- Verify checksums (provided by official sites)
- Use organizational-approved package repositories

## Summary

You now have:
- ✅ SLSA Level 3 provenance verified cryptographically
- ✅ SBOM downloaded for dependency audit
- ✅ Verification report for procurement documentation
- ✅ Compliance evidence package ready
- ✅ Procurement checklist completed

**Time to Complete Verification:** 15 minutes
**Procurement Recommendation:** ✅ **APPROVED** - Supply chain security verified

## Next Steps

### For Procurement Approval
1. **Complete verification** - Follow all steps above
2. **Collect evidence** - SBOM, verification reports, screenshots
3. **Review checklist** - Ensure all items checked
4. **Submit for approval** - Include evidence package
5. **Retain documentation** - 7+ years for compliance

### For Technical Team
1. **Security assessment** - [Security Officer Journey](./security-officer.md)
2. **Technical evaluation** - [Backend Engineer Journey](./backend-engineer.md)
3. **Deployment planning** - [DevOps Engineer Journey](./devops-engineer.md)

### For Ongoing Compliance
- **Quarterly SBOM review** - Check for new vulnerabilities
- **Annual re-verification** - Full supply chain verification
- **Version update verification** - Verify major version upgrades
- **Audit preparation** - Maintain evidence package

## Related Resources

### Documentation
- [SLSA Provenance Guide](../security-compliance/slsa-provenance.md) - Detailed technical guide
- [Compliance Matrix](../security-compliance/compliance-matrix.md) - Regulatory framework mappings
- [Security & Compliance Hub](../security-compliance/README.md) - Overview

### External Resources
- [SLSA Framework](https://slsa.dev/) - Supply chain security standard
- [Sigstore](https://www.sigstore.dev/) - Keyless signing infrastructure
- [SBOM Formats](https://cyclonedx.org/) - CycloneDX specification

### Support
- **Procurement Questions:** procurement@fraiseql.com
- **Security Questions:** security@fraiseql.com
- **Discord Community:** #procurement channel

## Troubleshooting

### Verification Tool Errors

**Error: `gh: command not found`**
```bash
# Verify installation
gh --version

# If not installed, reinstall:
# macOS/Linux: brew install gh
# Windows: winget install GitHub.cli
```

**Error: `Authentication required`**
```bash
# Login to GitHub
gh auth login

# Follow prompts to authenticate
```

**Error: `No attestations found`**
- Verify package name is correct (check spelling)
- Ensure you have the latest version
- Check GitHub releases page manually
- Contact support if issue persists

### SBOM Download Errors

**Error: `404 Not Found` when downloading SBOM**
```bash
# Check latest release version
curl -s https://api.github.com/repos/fraiseql/fraiseql/releases/latest | grep "tag_name"

# Update download URL with correct version
curl -L -O https://github.com/fraiseql/fraiseql/releases/download/v1.8.0/fraiseql-sbom.json
```

**Error: `Signature verification failed`**
- Ensure all three files downloaded (SBOM, sig, cert)
- Verify no corruption during download (re-download)
- Check file sizes match expected values
- Contact security@fraiseql.com if persistent

## Conclusion

FraiseQL's supply chain security is **verifiable, transparent, and meets enterprise procurement standards**. The combination of SLSA Level 3 provenance, cryptographic verification, and comprehensive SBOM provides confidence for procurement approval.

**Procurement Status:** ✅ **READY FOR APPROVAL**

---

**Questions?** Contact procurement@fraiseql.com or join [Discord](https://discord.gg/fraiseql) #procurement channel
