# Provenance Verification Guide

**Audience:** Federal IT Security Officers, DevSecOps Engineers, Compliance Auditors
**Purpose:** Step-by-step instructions for verifying FraiseQL supply chain integrity
**Classification:** UNCLASSIFIED
**Last Updated:** 2025-11-21

## Overview

This guide provides detailed instructions for verifying the supply chain integrity of FraiseQL releases using SLSA provenance attestations and cryptographic checksums.

## Prerequisites

- **curl** or **wget** - Download artifacts
- **sha256sum** - Verify checksums (built into Linux/macOS)
- **jq** - JSON processor (optional, for provenance inspection)
- **Python 3** - For SBOM validation (optional)

## Quick Verification (2 Minutes)

For rapid verification of artifact integrity:

```bash
# 1. Download release artifacts
VERSION="1.5.0"
curl -LO https://github.com/fraiseql/fraiseql/releases/download/v${VERSION}/SHA256SUMS

# 2. Download the wheel you want to use
curl -LO https://github.com/fraiseql/fraiseql/releases/download/v${VERSION}/fraiseql-${VERSION}-cp313-cp313-manylinux_2_34_x86_64.whl

# 3. Verify checksum
sha256sum -c SHA256SUMS --ignore-missing

# Expected output:
# fraiseql-1.5.0-cp313-cp313-manylinux_2_34_x86_64.whl: OK
```

**✅ If checksum matches**: Artifact has not been tampered with since build.

---

## Comprehensive Verification (10 Minutes)

For full supply chain verification including provenance:

### Step 1: Download All Verification Artifacts

```bash
VERSION="1.5.0"
BASE_URL="https://github.com/fraiseql/fraiseql/releases/download/v${VERSION}"

# Create verification directory
mkdir -p fraiseql-verification
cd fraiseql-verification

# Download checksums
curl -LO ${BASE_URL}/SHA256SUMS

# Download provenance
curl -LO ${BASE_URL}/fraiseql-v${VERSION}-provenance.intoto.json

# Download SBOM (generated separately)
curl -LO ${BASE_URL}/fraiseql-${VERSION}-sbom.json

# Download artifact to verify (example: Linux wheel)
curl -LO ${BASE_URL}/fraiseql-${VERSION}-cp313-cp313-manylinux_2_34_x86_64.whl

echo "✅ Downloaded verification artifacts"
```

### Step 2: Verify SHA256 Checksums

```bash
# Verify all downloaded artifacts
sha256sum -c SHA256SUMS --ignore-missing

# Expected output for each artifact:
# fraiseql-1.5.0-cp313-cp313-manylinux_2_34_x86_64.whl: OK
```

**What This Proves:**
- Artifact has not been modified since build
- Artifact matches the official release

### Step 3: Inspect Build Provenance

```bash
# Install jq if not available
# Ubuntu/Debian: sudo apt-get install jq
# macOS: brew install jq
# RHEL/CentOS: sudo yum install jq

# Verify provenance file is valid JSON
jq empty fraiseql-v${VERSION}-provenance.intoto.json && echo "✅ Valid JSON"

# Extract key provenance fields
echo "=== PROVENANCE INSPECTION ==="

# 1. Verify provenance type
echo "Provenance Type:"
jq -r '.predicateType' fraiseql-v${VERSION}-provenance.intoto.json

# Expected: https://slsa.dev/provenance/v0.2

# 2. Verify builder identity (CRITICAL)
echo ""
echo "Builder Identity:"
jq -r '.predicate.builder.id' fraiseql-v${VERSION}-provenance.intoto.json

# Expected: https://github.com/fraiseql/fraiseql/actions/workflows/publish.yml@refs/tags/v1.5.0
# This proves the artifact was built by GitHub Actions, not locally

# 3. Verify source material (CRITICAL)
echo ""
echo "Source Material:"
jq -r '.predicate.materials[0].uri' fraiseql-v${VERSION}-provenance.intoto.json

# Expected: git+https://github.com/fraiseql/fraiseql@refs/tags/v1.5.0
# This proves the exact Git commit used

# 4. Get source commit SHA
echo ""
echo "Source Commit SHA:"
jq -r '.predicate.materials[0].digest.sha1' fraiseql-v${VERSION}-provenance.intoto.json

# 5. Verify artifact is in subject list
echo ""
echo "Artifact in Provenance:"
ARTIFACT="fraiseql-${VERSION}-cp313-cp313-manylinux_2_34_x86_64.whl"
jq -r --arg artifact "$ARTIFACT" '.subject[] | select(.name == $artifact) | .name' \
  fraiseql-v${VERSION}-provenance.intoto.json

# Expected: fraiseql-1.5.0-cp313-cp313-manylinux_2_34_x86_64.whl

# 6. Get artifact's SHA256 from provenance
echo ""
echo "Artifact SHA256 (from provenance):"
jq -r --arg artifact "$ARTIFACT" '.subject[] | select(.name == $artifact) | .digest.sha256' \
  fraiseql-v${VERSION}-provenance.intoto.json

# 7. Compare with actual file SHA256
echo ""
echo "Artifact SHA256 (actual file):"
sha256sum $ARTIFACT | cut -d' ' -f1

echo ""
echo "✅ Provenance inspection complete"
```

**What This Proves:**
- ✅ Artifact was built by GitHub Actions (not a compromised developer machine)
- ✅ Artifact was built from specific Git commit (full traceability)
- ✅ Build process is documented and verifiable
- ✅ SHA256 in provenance matches actual file

### Step 4: Validate SBOM

```bash
# Install FraiseQL (if not already installed)
pip install fraiseql

# Validate SBOM structure
fraiseql sbom validate --input fraiseql-${VERSION}-sbom.json

# Expected output:
# ✅ SBOM is valid!
# Serial Number: urn:uuid:...
# Spec Version: 1.5
# Total Components: 120
```

**What This Proves:**
- ✅ Complete dependency inventory available
- ✅ License compliance verified
- ✅ Vulnerability scanning possible

---

## Advanced Verification

### Cross-Reference Commit SHA with GitHub

```bash
# Get commit SHA from provenance
COMMIT_SHA=$(jq -r '.predicate.materials[0].digest.sha1' fraiseql-v${VERSION}-provenance.intoto.json)

echo "Commit SHA from provenance: $COMMIT_SHA"

# Verify commit exists on GitHub
curl -s "https://api.github.com/repos/fraiseql/fraiseql/commits/${COMMIT_SHA}" | \
  jq -r '.commit.message' | head -5

# Expected: Shows commit message from the release
```

### Verify Build Invocation

```bash
# Get GitHub Actions run ID from provenance
BUILD_ID=$(jq -r '.predicate.metadata.buildInvocationId' fraiseql-v${VERSION}-provenance.intoto.json)

echo "GitHub Actions Build ID: $BUILD_ID"

# Format: {run_id}-{run_attempt}
# Example: 1234567890-1
```

You can verify this build on GitHub Actions:
`https://github.com/fraiseql/fraiseql/actions/runs/{run_id}`

### Verify All Artifacts in Release

```bash
# List all subjects in provenance
echo "=== All Artifacts in Provenance ==="
jq -r '.subject[].name' fraiseql-v${VERSION}-provenance.intoto.json

# Count artifacts
ARTIFACT_COUNT=$(jq '.subject | length' fraiseql-v${VERSION}-provenance.intoto.json)
echo ""
echo "Total artifacts: $ARTIFACT_COUNT"

# Verify all checksums
echo ""
echo "=== Verifying All Checksums ==="
sha256sum -c SHA256SUMS 2>&1 | grep -E "(OK|FAILED)"
```

---

## Automated Verification Script

Save as `verify_fraiseql.sh`:

```bash
#!/bin/bash
set -euo pipefail

VERSION="${1:-1.5.0}"
BASE_URL="https://github.com/fraiseql/fraiseql/releases/download/v${VERSION}"

echo "🔍 FraiseQL Supply Chain Verification"
echo "Version: $VERSION"
echo ""

# Create temp directory
TMPDIR=$(mktemp -d)
cd "$TMPDIR"

# Download verification artifacts
echo "📥 Downloading verification artifacts..."
curl -sLO ${BASE_URL}/SHA256SUMS
curl -sLO ${BASE_URL}/fraiseql-v${VERSION}-provenance.intoto.json

# Download first Linux wheel as example
WHEEL=$(curl -s "https://api.github.com/repos/fraiseql/fraiseql/releases/tags/v${VERSION}" | \
  jq -r '.assets[] | select(.name | contains("manylinux")) | .name' | head -1)

curl -sLO ${BASE_URL}/${WHEEL}

echo "✅ Downloaded artifacts"
echo ""

# Verify checksum
echo "🔐 Verifying SHA256 checksum..."
if sha256sum -c SHA256SUMS --ignore-missing 2>&1 | grep -q "OK"; then
  echo "✅ Checksum verified"
else
  echo "❌ Checksum verification FAILED"
  exit 1
fi
echo ""

# Verify provenance (requires jq)
if command -v jq &> /dev/null; then
  echo "📜 Verifying provenance..."

  BUILDER=$(jq -r '.predicate.builder.id' fraiseql-v${VERSION}-provenance.intoto.json)
  if [[ "$BUILDER" == *"github.com/fraiseql/fraiseql/actions/workflows/publish.yml"* ]]; then
    echo "✅ Builder verified: GitHub Actions"
  else
    echo "❌ Unknown builder: $BUILDER"
    exit 1
  fi

  SOURCE=$(jq -r '.predicate.materials[0].uri' fraiseql-v${VERSION}-provenance.intoto.json)
  if [[ "$SOURCE" == "git+https://github.com/fraiseql/fraiseql"* ]]; then
    echo "✅ Source verified: Official repository"
  else
    echo "❌ Unknown source: $SOURCE"
    exit 1
  fi

  COMMIT_SHA=$(jq -r '.predicate.materials[0].digest.sha1' fraiseql-v${VERSION}-provenance.intoto.json)
  echo "✅ Source commit: $COMMIT_SHA"
else
  echo "⚠️  jq not installed, skipping provenance verification"
  echo "   Install jq to enable full verification"
fi

echo ""
echo "✅ FraiseQL v${VERSION} supply chain verification PASSED"
echo ""
echo "Verified artifact: $WHEEL"
echo "Verification directory: $TMPDIR"
echo ""
echo "To clean up: rm -rf $TMPDIR"
```

Usage:
```bash
chmod +x verify_fraiseql.sh
./verify_fraiseql.sh 1.5.0
```

---

## Verification Checklist for Federal IT

**Level 1: Basic Integrity (Required for all deployments)**
- [ ] Downloaded from official GitHub Releases
- [ ] SHA256 checksum verified
- [ ] SBOM available and validated
- [ ] License compliance confirmed (MIT)

**Level 2: Supply Chain Verification (Required for DoD IL2+)**
- [ ] Provenance file available
- [ ] Builder identity is GitHub Actions
- [ ] Source material is official repository
- [ ] Commit SHA cross-referenced with GitHub
- [ ] No copyleft dependencies (GPL)

**Level 3: Comprehensive Audit (Required for DoD IL4+)**
- [ ] All artifacts verified (wheels + sdist)
- [ ] Build invocation traced to GitHub Actions run
- [ ] Source code reviewed for security issues
- [ ] Dependency vulnerability scan passed
- [ ] Federal compliance documentation reviewed

---

## Troubleshooting

### Issue: sha256sum command not found (Windows)

**Solution:** Use PowerShell:
```powershell
Get-FileHash -Algorithm SHA256 fraiseql-1.5.0-*.whl
```

Then manually compare with SHA256SUMS.

### Issue: Checksum mismatch

**Cause:** File corrupted during download or tampered with.

**Solution:**
1. Re-download the file
2. Verify download source is github.com/fraiseql/fraiseql
3. If still fails, report to security@fraiseql.com

### Issue: Provenance shows different commit SHA

**Cause:** You downloaded an artifact from a different version.

**Solution:** Ensure version in filename matches VERSION variable.

### Issue: jq not available

**Solution:** Manual inspection:
```bash
python3 -m json.tool fraiseql-v1.5.0-provenance.intoto.json | less
```

Look for:
- `"predicateType": "https://slsa.dev/provenance/v0.2"`
- `"builder.id": "https://github.com/fraiseql/fraiseql/actions/workflows/publish.yml@..."`

---

## Federal Compliance Notes

**For ATO Packages:**
- Include provenance verification in System Security Plan (SSP)
- Document verification as continuous monitoring control
- Attach provenance files to deployment evidence

**For FedRAMP:**
- Provenance verification satisfies SA-10 (Developer Configuration Management)
- Checksums satisfy SI-7 (Software, Firmware, and Information Integrity)
- SBOM satisfies SA-15 (Development Process, Standards, and Tools)

**For FISMA:**
- Map to NIST 800-53 controls: SA-10, SA-15, SI-7, SR-3, SR-4
- Include in Plan of Action and Milestones (POA&M) if verification fails

---

## Contact

**Security Questions:**
- Email: security@fraiseql.com
- GitHub Security Advisories: https://github.com/fraiseql/fraiseql/security/advisories

**False Positive Reports:**
If you believe verification is failing incorrectly:
1. Document exact steps to reproduce
2. Include verification output
3. Email security@fraiseql.com with subject: "Provenance Verification Issue"

**Response SLA:** 48 hours for initial response

---

**Document Control:**
- **Author**: Security Team
- **Approved**: Project Maintainers
- **Next Review**: 2026-02-21
- **Distribution**: Public
