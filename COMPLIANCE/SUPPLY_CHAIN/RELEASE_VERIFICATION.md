# Release Artifact Verification Guide

**Document Version:** 1.0
**Last Updated:** 2025-12-04
**Classification:** Public
**Applicable Standards:** SLSA Level 3, PEP 740, Sigstore, ISO 27001

## Executive Summary

FraiseQL implements comprehensive cryptographic signing for all release artifacts using modern supply chain security standards. All wheels, source distributions, and SBOMs are signed and can be independently verified.

## Verification Methods

FraiseQL provides **three layers** of verification:

1. **GitHub Attestations** - SLSA Level 3 provenance (recommended)
2. **PyPI Attestations** - PEP 740 standard for Python packages
3. **SHA256 Checksums** - Traditional integrity verification

## Prerequisites

### Install Required Tools

```bash
# GitHub CLI (for GitHub attestations)
# macOS
brew install gh

# Ubuntu/Debian
curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null
sudo apt update
sudo apt install gh

# Windows (using winget)
winget install --id GitHub.cli

# Cosign (for SBOM verification)
# macOS
brew install cosign

# Linux
wget "https://github.com/sigstore/cosign/releases/download/v2.2.2/cosign-linux-amd64"
sudo mv cosign-linux-amd64 /usr/local/bin/cosign
sudo chmod +x /usr/local/bin/cosign

# Windows (using Chocolatey)
choco install cosign
```

## 1. Verify GitHub Attestations (SLSA Provenance)

**Recommended method** - Provides strongest guarantees about build provenance.

### What It Verifies
- ✅ Artifact was built by FraiseQL's GitHub Actions workflow
- ✅ Build happened in the official fraiseql/fraiseql repository
- ✅ No tampering since build
- ✅ Complete build environment details (SLSA Level 3)

### Download and Verify a Wheel

```bash
# Download a wheel from GitHub Release
wget https://github.com/fraiseql/fraiseql/releases/download/v1.7.1/fraiseql-1.7.1-py3-none-any.whl

# Verify with GitHub CLI
gh attestation verify fraiseql-1.7.1-py3-none-any.whl \
  --owner fraiseql \
  --repo fraiseql
```

**Expected Output:**
```
Loaded digest sha256:abc123... for file://fraiseql-1.7.1-py3-none-any.whl
Loaded 1 attestation from GitHub API
✓ Verification succeeded!

sha256:abc123... was attested by:
REPO              PREDICATE_TYPE                  WORKFLOW
fraiseql/fraiseql https://slsa.dev/provenance/v1  .github/workflows/publish.yml@refs/tags/v1.7.1
```

### Verify All Artifacts in a Release

```bash
# Download all artifacts
gh release download v1.7.1 --repo fraiseql/fraiseql --pattern "*.whl"
gh release download v1.7.1 --repo fraiseql/fraiseql --pattern "*.tar.gz"

# Verify all downloaded files
for file in fraiseql-1.7.1-*; do
  echo "Verifying $file..."
  gh attestation verify "$file" --owner fraiseql --repo fraiseql || echo "❌ Verification failed for $file"
done
```

### Programmatic Verification (CI/CD)

```bash
#!/bin/bash
# verify-fraiseql.sh - For use in CI pipelines

VERSION="1.7.1"
ARTIFACT="fraiseql-${VERSION}-py3-none-any.whl"

# Download artifact
wget "https://github.com/fraiseql/fraiseql/releases/download/v${VERSION}/${ARTIFACT}"

# Verify attestation
if gh attestation verify "$ARTIFACT" --owner fraiseql --repo fraiseql; then
  echo "✅ FraiseQL ${VERSION} verified successfully"
  exit 0
else
  echo "❌ Verification failed - DO NOT USE"
  exit 1
fi
```

## 2. Verify PyPI Attestations (PEP 740)

**For PyPI installations** - Built into pip (experimental in pip 25.0+).

### What It Verifies
- ✅ Package on PyPI matches what was built by GitHub Actions
- ✅ No tampering between build and PyPI upload
- ✅ Cryptographic link to source repository

### Using pip (Upcoming)

```bash
# Future pip versions (25.0+) will support attestation verification
pip install fraiseql==1.7.1 --verify-attestations
```

### Using PyPI API (Current Method)

```bash
# Check if attestations are present
curl -s https://pypi.org/simple/fraiseql/ \
  -H "Accept: application/vnd.pypi.simple.v1+json" | \
  jq '.files[] | select(.filename | contains("1.7.1")) | {filename: .filename, has_attestations: (.attestations != null)}'
```

**Expected Output:**
```json
{
  "filename": "fraiseql-1.7.1-py3-none-any.whl",
  "has_attestations": true
}
```

### Verify Attestation Details

```bash
# Get full attestation bundle
curl -s https://pypi.org/simple/fraiseql/ \
  -H "Accept: application/vnd.pypi.simple.v1+json" | \
  jq '.files[] | select(.filename == "fraiseql-1.7.1-py3-none-any.whl") | .attestations'
```

## 3. Verify SHA256 Checksums

**Traditional verification** - Always works, but provides weakest guarantees.

### What It Verifies
- ✅ File was not corrupted during download
- ✅ File matches what was published
- ⚠️  Does NOT verify who built it or how

### Download and Verify

```bash
# Download artifact and checksum
wget https://github.com/fraiseql/fraiseql/releases/download/v1.7.1/fraiseql-1.7.1-py3-none-any.whl
wget https://github.com/fraiseql/fraiseql/releases/download/v1.7.1/fraiseql-1.7.1-py3-none-any.whl.sha256

# Verify checksum
sha256sum -c fraiseql-1.7.1-py3-none-any.whl.sha256
```

**Expected Output:**
```
fraiseql-1.7.1-py3-none-any.whl: OK
```

### Manual Verification

```bash
# Calculate checksum manually
sha256sum fraiseql-1.7.1-py3-none-any.whl

# Compare with published checksum
cat fraiseql-1.7.1-py3-none-any.whl.sha256
```

## 4. Verify SBOM Signatures

**For SBOMs** - Separate workflow with Cosign signing.

### Download and Verify SBOM

```bash
# Download SBOM and signature files
wget https://github.com/fraiseql/fraiseql/releases/download/v1.7.1/fraiseql-1.7.1-sbom.json
wget https://github.com/fraiseql/fraiseql/releases/download/v1.7.1/fraiseql-1.7.1-sbom.json.sig
wget https://github.com/fraiseql/fraiseql/releases/download/v1.7.1/fraiseql-1.7.1-sbom.json.pem

# Verify with Cosign
cosign verify-blob \
  --signature fraiseql-1.7.1-sbom.json.sig \
  --certificate fraiseql-1.7.1-sbom.json.pem \
  --certificate-identity-regexp "https://github.com/fraiseql/fraiseql" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  fraiseql-1.7.1-sbom.json
```

**Expected Output:**
```
Verified OK
```

## Verification Matrix

| Artifact Type | GitHub Attestations | PyPI Attestations | SHA256 | Cosign |
|---------------|---------------------|-------------------|--------|--------|
| **Wheels (.whl)** | ✅ Recommended | ✅ Available | ✅ Yes | ❌ No |
| **Source Distribution (.tar.gz)** | ✅ Recommended | ✅ Available | ✅ Yes | ❌ No |
| **SBOM (.json)** | ❌ No | ❌ No | ✅ Yes | ✅ Recommended |

## For Procurement & Security Teams

### Verification Requirements Checklist

When evaluating FraiseQL for use in your organization:

- [x] **Build Provenance**: SLSA Level 3 attestations available via GitHub
- [x] **Supply Chain Transparency**: All builds in public CI/CD with full logs
- [x] **Keyless Signing**: No secret key management required (Sigstore)
- [x] **Standards Compliance**: PEP 740, SLSA, Sigstore, ISO 27001
- [x] **Regulatory Compliance**: US EO 14028, EU CRA, PCI-DSS 4.0
- [x] **Vulnerability Tracking**: SBOM with Package URLs for CVE matching
- [x] **Independent Verification**: All signatures verifiable without vendor tools

### Attestation Policy Example

```yaml
# Example policy for automated verification
name: Verify FraiseQL Dependencies
on: [pull_request]

jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - name: Verify FraiseQL Wheel
        run: |
          gh attestation verify fraiseql-1.7.1-py3-none-any.whl \
            --owner fraiseql \
            --repo fraiseql
```

## Troubleshooting

### Issue: "No attestations found"

**Cause**: Attestations were added in v1.7.1. Earlier versions don't have them.

**Solution**: Upgrade to v1.7.1 or later, or use SHA256 verification for older releases.

### Issue: "gh attestation command not found"

**Cause**: GitHub CLI version too old or not installed.

**Solution**: Update GitHub CLI to latest version:
```bash
gh --version  # Should be 2.40.0+
gh upgrade
```

### Issue: "Verification failed with certificate identity mismatch"

**Cause**: Wrong repository or branch specified.

**Solution**: Ensure you're using:
- Owner: `fraiseql`
- Repo: `fraiseql`
- For SBOM: Identity regex must match `https://github.com/fraiseql/fraiseql`

### Issue: "PyPI attestations not showing up"

**Cause**: Attestations take a few minutes to appear after publishing.

**Solution**: Wait 5-10 minutes after release, then retry.

## Advanced: Verify Build Reproducibility

FraiseQL builds are **not yet reproducible** (same inputs → same binary), but provenance attestations provide the next best thing: **verifiable build environments**.

### View Full Build Provenance

```bash
# Download attestation bundle
gh attestation download fraiseql-1.7.1-py3-none-any.whl \
  --owner fraiseql \
  --repo fraiseql \
  --bundle fraiseql-1.7.1-attestation.jsonl

# Inspect build details
cat fraiseql-1.7.1-attestation.jsonl | jq '.'
```

This shows:
- Exact GitHub Actions workflow used
- Commit SHA that was built
- Runner environment (OS, architecture)
- Build timestamp
- All build inputs and parameters

## Continuous Verification

### Scheduled Verification (CI/CD)

```yaml
# .github/workflows/verify-dependencies.yml
name: Verify Dependencies Weekly

on:
  schedule:
    - cron: '0 0 * * 0'  # Every Sunday

jobs:
  verify-fraiseql:
    runs-on: ubuntu-latest
    steps:
      - name: Get installed FraiseQL version
        id: version
        run: |
          VERSION=$(pip show fraiseql | grep Version | cut -d' ' -f2)
          echo "version=$VERSION" >> $GITHUB_OUTPUT

      - name: Download wheel
        run: |
          wget "https://pypi.io/packages/py3/f/fraiseql/fraiseql-${{ steps.version.outputs.version }}-py3-none-any.whl"

      - name: Verify attestation
        run: |
          gh attestation verify "fraiseql-${{ steps.version.outputs.version }}-py3-none-any.whl" \
            --owner fraiseql \
            --repo fraiseql
```

## For Contributors

If you're building from source, note that:
- Your local builds will NOT have attestations (GitHub Actions required)
- You can still verify checksums manually
- Official releases should always be used in production

## References

- [PEP 740 - Index support for digital attestations](https://peps.python.org/pep-0740/)
- [GitHub Artifact Attestations Documentation](https://docs.github.com/en/actions/security-guides/using-artifact-attestations-to-establish-provenance-for-builds)
- [SLSA Framework](https://slsa.dev/)
- [Sigstore](https://www.sigstore.dev/)
- [FraiseQL SBOM Process](./SBOM_PROCESS.md)

---

**Document Control:**
- **Author**: Security Team
- **Reviewers**: Project Maintainers
- **Next Review**: 2026-12-04
- **Distribution**: Public
