# SLSA Compliance Documentation

**Document Version:** 1.0
**Last Updated:** 2025-11-21
**Classification:** UNCLASSIFIED
**Applicable Standard:** SLSA (Supply-chain Levels for Software Artifacts)

## Executive Summary

FraiseQL implements **SLSA Level 2** provenance attestations to ensure supply chain integrity and build transparency. SLSA (Supply-chain Levels for Software Artifacts) is a framework developed by Google and the OpenSSF to prevent tampering, improve integrity, and secure packages and infrastructure.

**Current SLSA Level**: ✅ **Level 2** (Build Provenance)
**Target SLSA Level**: Level 3 (Hardened Builds) - Roadmap item

## What is SLSA?

SLSA (pronounced "salsa") is a security framework addressing supply chain attacks. It provides four levels of increasing assurance:

| Level | Requirements | FraiseQL Status |
|-------|-------------|-----------------|
| **Level 0** | No guarantees | N/A |
| **Level 1** | Build process fully scripted/automated | ✅ **ACHIEVED** |
| **Level 2** | Hosted source/build, provenance attestations | ✅ **ACHIEVED** |
| **Level 3** | Hardened builds, non-falsifiable provenance | 🔄 Roadmap |
| **Level 4** | Two-party review, hermetic builds | ⏳ Future |

### Why SLSA Matters

**Supply Chain Attack Prevention:**
- **SolarWinds (2020)**: Build system compromise affected 18,000+ organizations
- **Codecov (2021)**: Bash Uploader tampered, exposed CI/CD secrets
- **ua-parser-js (2021)**: Compromised npm package installed malware

SLSA prevents these attacks by providing **verifiable build provenance**.

## SLSA Level Achievements

### ✅ Level 1: Build Process Automated

**Requirements:**
- Build process fully scripted/automated
- Source code versioned with commit history
- Build from source

**FraiseQL Implementation:**
- ✅ GitHub Actions fully automated build (`.github/workflows/publish.yml`)
- ✅ Git version control with full commit history
- ✅ Maturin builds from source (Python + Rust)
- ✅ Reproducible build steps (documented in workflow)

**Evidence:** All builds triggered via GitHub Actions on tag push (`v*`). No manual build steps.

---

### ✅ Level 2: Build Provenance

**Requirements:**
- Hosted source (e.g., GitHub)
- Hosted build service (e.g., GitHub Actions)
- Provenance attestation generated automatically
- Provenance includes build process metadata

**FraiseQL Implementation:**

#### 1. Hosted Source
- **Platform**: GitHub (github.com/fraiseql/fraiseql)
- **Branch Protection**: Enforced on main branch
- **Commit Signing**: Recommended (GPG)

#### 2. Hosted Build Service
- **Platform**: GitHub Actions
- **Isolated Runners**: GitHub-managed Ubuntu, macOS, Windows runners
- **Ephemeral Environments**: Fresh runner per build

#### 3. Provenance Generation
Automatic generation in `.github/workflows/publish.yml`:

```yaml
generate-provenance:
  name: Generate SLSA Provenance
  runs-on: ubuntu-latest
  needs: [validate]
  permissions:
    actions: read
    id-token: write
    contents: write

  steps:
  - name: Generate provenance attestation
    run: |
      # Generates in-toto attestation with:
      # - Subject: All wheels + sdist with SHA256
      # - Builder: GitHub Actions workflow@ref
      # - Materials: Git commit SHA
      # - Metadata: Build invocation ID, timestamp
```

#### 4. Provenance Contents

**Subject (What was built):**
```json
"subject": [
  {
    "name": "fraiseql-1.5.0-cp313-cp313-manylinux_2_34_x86_64.whl",
    "digest": {
      "sha256": "abc123..."
    }
  },
  // ... all wheels and sdist
]
```

**Predicate (How it was built):**
```json
"predicate": {
  "builder": {
    "id": "https://github.com/fraiseql/fraiseql/actions/workflows/publish.yml@refs/tags/v1.5.0"
  },
  "buildType": "https://github.com/fraiseql/fraiseql/publish@v1",
  "invocation": {
    "configSource": {
      "uri": "git+https://github.com/fraiseql/fraiseql@refs/tags/v1.5.0",
      "digest": {
        "sha1": "commit-sha"
      }
    }
  },
  "materials": [
    {
      "uri": "git+https://github.com/fraiseql/fraiseql",
      "digest": {
        "sha1": "commit-sha"
      }
    }
  ]
}
```

**Evidence:**
- Provenance file: `fraiseql-{version}-provenance.intoto.json`
- Published with every GitHub Release
- SHA256 checksums: `SHA256SUMS`

---

### 🔄 Level 3: Hardened Builds (Roadmap)

**Requirements (Not Yet Implemented):**
- Non-falsifiable provenance (signed by build service)
- Isolation from other tenants
- No access to other tenants' secrets
- Parameterless builds (no environment variables)

**Planned Implementation:**
1. **Sigstore Integration**: Sign provenance with Fulcio (keyless signing)
2. **GitHub OIDC Tokens**: Use OIDC for non-falsifiable identity
3. **Isolated Runners**: Consider self-hosted hardened runners
4. **Hermetic Builds**: Remove external dependency fetching during build

**Timeline**: Q1 2026

---

## Provenance File Structure

FraiseQL provenance follows the **in-toto attestation** format (SLSA v0.2):

```json
{
  "_type": "https://in-toto.io/Statement/v0.1",
  "predicateType": "https://slsa.dev/provenance/v0.2",
  "subject": [
    // List of all built artifacts with SHA256
  ],
  "predicate": {
    "builder": {
      // GitHub Actions workflow identifier
    },
    "buildType": "https://github.com/fraiseql/fraiseql/publish@v1",
    "invocation": {
      // Source code reference and entry point
    },
    "metadata": {
      // Build metadata (ID, timestamp, completeness)
    },
    "materials": [
      // Source materials (Git commit)
    ]
  }
}
```

### Provenance Schema

| Field | Description | Example |
|-------|-------------|---------|
| `_type` | Statement type (in-toto) | `https://in-toto.io/Statement/v0.1` |
| `predicateType` | Provenance type (SLSA) | `https://slsa.dev/provenance/v0.2` |
| `subject` | Artifacts produced | `[{name, digest}]` |
| `predicate.builder.id` | Build service identity | GitHub Actions workflow@ref |
| `predicate.materials` | Source inputs | Git commit SHA |
| `predicate.metadata.buildInvocationId` | Unique build ID | `{run_id}-{run_attempt}` |

## Verification

### 1. Verify SHA256 Checksums

```bash
# Download SHA256SUMS from GitHub Release
curl -LO https://github.com/fraiseql/fraiseql/releases/download/v1.5.0/SHA256SUMS

# Verify all artifacts
sha256sum -c SHA256SUMS
```

**Expected Output:**
```
fraiseql-1.5.0-cp313-...-manylinux_2_34_x86_64.whl: OK
fraiseql-1.5.0.tar.gz: OK
...
```

### 2. Inspect Provenance Manually

```bash
# Download provenance
curl -LO https://github.com/fraiseql/fraiseql/releases/download/v1.5.0/fraiseql-v1.5.0-provenance.intoto.json

# Pretty-print provenance
jq . fraiseql-v1.5.0-provenance.intoto.json

# Verify builder identity
jq '.predicate.builder.id' fraiseql-v1.5.0-provenance.intoto.json
# Should be: "https://github.com/fraiseql/fraiseql/actions/workflows/publish.yml@refs/tags/v1.5.0"

# Verify source material
jq '.predicate.materials[0].uri' fraiseql-v1.5.0-provenance.intoto.json
# Should be: "git+https://github.com/fraiseql/fraiseql@refs/tags/v1.5.0"
```

### 3. Automated Verification (Future)

```bash
# Install slsa-verifier (requires Go)
go install github.com/slsa-framework/slsa-verifier/v2/cli/slsa-verifier@latest

# Verify artifact (coming with SLSA Level 3)
slsa-verifier verify-artifact \
  fraiseql-1.5.0-cp313-...-manylinux_2_34_x86_64.whl \
  --provenance-path fraiseql-v1.5.0-provenance.intoto.json \
  --source-uri github.com/fraiseql/fraiseql \
  --source-tag v1.5.0
```

**Note:** Full automated verification requires SLSA Level 3 (signed provenance). Currently available for manual inspection.

## Threat Model

SLSA provenance protects against:

| Threat | Mitigation | SLSA Level |
|--------|-----------|------------|
| **Source Code Tampering** | Git commit SHA in provenance | Level 2 ✅ |
| **Build Script Tampering** | Workflow ref in builder ID | Level 2 ✅ |
| **Artifact Substitution** | SHA256 in subject | Level 2 ✅ |
| **Malicious Build Service** | Signed provenance (future) | Level 3 🔄 |
| **Compromised Dependencies** | SBOM tracks dependencies | N/A (complementary) |

### Attack Scenarios Prevented

**Scenario 1: Compromised PyPI Account**
- Attacker gains access to PyPI publishing credentials
- Attempts to upload malicious wheel
- **Defense**: Users verify provenance shows GitHub Actions as builder
- **Result**: Malicious upload detected (no GitHub Actions provenance)

**Scenario 2: GitHub Actions Compromise**
- Attacker compromises GitHub Actions runner
- Modifies artifact before upload
- **Defense**: SHA256 checksums in provenance don't match
- **Result**: Tampering detected

**Scenario 3: Build Script Injection**
- Attacker submits PR with malicious build script
- PR is merged and tagged
- **Defense**: Provenance shows exact commit SHA
- **Result**: Audit trail links artifact to malicious commit

## Integration with Other Security Measures

SLSA provenance complements FraiseQL's security stack:

| Security Measure | Purpose | Relationship to SLSA |
|-----------------|---------|---------------------|
| **SBOM** | Dependency inventory | Tracks *what* is in artifacts |
| **SLSA Provenance** | Build transparency | Tracks *how* artifacts were built |
| **Artifact Signing** | Integrity verification | Cryptographic proof of publisher |
| **Vulnerability Scanning** | Known CVE detection | Identifies risks in SBOM components |

**Together**: Defense-in-depth supply chain security

## Compliance Mapping

### Executive Order 14028 (May 2021)

| Requirement | SLSA Implementation | Status |
|-------------|---------------------|--------|
| Secure development practices | GitHub Actions CI/CD | ✅ |
| Automated testing | Quality gate workflow | ✅ |
| SBOM generation | CycloneDX SBOM | ✅ |
| **Provenance attestations** | **SLSA Level 2** | ✅ |
| Integrity mechanisms | SHA256 + provenance | ✅ |

### NIST SP 800-218 (SSDF)

| Practice | Task | SLSA Mapping |
|----------|------|--------------|
| PW.1.1 | Design software securely | GitHub-hosted source |
| PW.7.1 | Review code before release | Commit SHA in provenance |
| PW.9.1 | Verify integrity | SHA256 + provenance |
| PS.3.2 | Scan for vulnerabilities | Pre-release security job |

### CISA Zero Trust

| Pillar | SLSA Contribution |
|--------|-------------------|
| Identity | Builder ID (GitHub Actions) |
| Device | Ephemeral runners |
| Network | GitHub-hosted infrastructure |
| Application | Artifact integrity (SHA256) |
| Data | Source material tracking |

## Roadmap

### Q4 2025 ✅
- [x] Implement SLSA Level 1 (automated builds)
- [x] Implement SLSA Level 2 (provenance generation)
- [x] Publish provenance with releases
- [x] Document verification process

### Q1 2026 🔄
- [ ] Implement SLSA Level 3 (signed provenance)
- [ ] Integrate Sigstore Fulcio for keyless signing
- [ ] Add automated provenance verification
- [ ] GitHub OIDC token-based signing

### Q2 2026 ⏳
- [ ] Hermetic builds (no network during build)
- [ ] Reproducible builds validation
- [ ] Build parameter isolation
- [ ] Explore SLSA Level 4 requirements

## For Federal Procurement Officers

### Questions to Ask Vendors About SLSA

✅ FraiseQL provides:

1. **SLSA Level**: Level 2 (Provenance)
2. **Provenance Format**: in-toto attestation (industry standard)
3. **Provenance Availability**: Every release on GitHub
4. **Verification Method**: SHA256 checksums + provenance inspection
5. **Builder Identity**: GitHub Actions (verifiable)
6. **Source Traceability**: Git commit SHA to artifact
7. **Roadmap**: Level 3 by Q1 2026

### SLSA Attestation Statement

> FraiseQL achieves SLSA Level 2 compliance through automated, hosted builds on GitHub Actions with comprehensive provenance attestations. Every release includes an in-toto provenance file that maps built artifacts to specific Git commits, enabling full supply chain auditability. Provenance files are published alongside release artifacts and include SHA256 checksums for integrity verification.
>
> **Signed**: Lionel Hamayon, Project Maintainer
> **Date**: 2025-11-21
> **Effective**: FraiseQL v1.5.0 and later

## References

- **SLSA Specification**: https://slsa.dev
- **in-toto Project**: https://in-toto.io
- **OpenSSF**: https://openssf.org
- **NIST SSDF**: https://csrc.nist.gov/publications/detail/sp/800-218/final
- **EO 14028**: https://www.whitehouse.gov/briefing-room/presidential-actions/2021/05/12/executive-order-on-improving-the-nations-cybersecurity/

---

**Document Control:**
- **Author**: Security Team
- **Reviewers**: Project Maintainers
- **Next Review**: 2026-02-21
- **Distribution**: Public
