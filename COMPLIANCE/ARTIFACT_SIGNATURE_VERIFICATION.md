# Artifact Signature Verification Guide

**Audience:** Federal IT Security Officers, DevSecOps Engineers, Compliance Auditors
**Purpose:** Step-by-step instructions for verifying FraiseQL artifact cryptographic signatures
**Classification:** UNCLASSIFIED
**Last Updated:** 2025-11-21

## Overview

This guide provides detailed instructions for verifying the cryptographic signatures of FraiseQL release artifacts using Sigstore Cosign. All FraiseQL releases are signed using **keyless signing** with GitHub OIDC, eliminating the need for long-term key management while providing strong cryptographic guarantees.

## What is Cosign Keyless Signing?

**Cosign** is a cryptographic signing tool from the Sigstore project that provides:

- ✅ **Keyless Signing**: No private keys to manage or compromise
- ✅ **Certificate Transparency**: All signatures logged to public transparency log
- ✅ **OIDC Authentication**: Signatures bound to GitHub Actions identity
- ✅ **Non-repudiation**: Cryptographic proof of who built the artifact
- ✅ **Tamper Detection**: Any modification invalidates the signature

**How It Works:**
1. GitHub Actions authenticates via OIDC during build
2. Sigstore issues a short-lived certificate (valid 10 minutes)
3. Artifact is signed with ephemeral key
4. Certificate + signature bundled together
5. Logged to Rekor transparency log (immutable)

**Federal Benefits:**
- No key escrow or management required
- Audit trail via transparency log
- Resistant to key compromise (no long-term keys)
- Industry-standard open-source solution

---

## Prerequisites

### Install Cosign

**Linux:**
```bash
# Download latest release
curl -LO https://github.com/sigstore/cosign/releases/latest/download/cosign-linux-amd64
chmod +x cosign-linux-amd64
sudo mv cosign-linux-amd64 /usr/local/bin/cosign

# Verify installation
cosign version
```

**macOS:**
```bash
# Using Homebrew
brew install cosign

# Or download directly
curl -LO https://github.com/sigstore/cosign/releases/latest/download/cosign-darwin-amd64
chmod +x cosign-darwin-amd64
sudo mv cosign-darwin-amd64 /usr/local/bin/cosign

# Verify installation
cosign version
```

**Windows (PowerShell as Administrator):**
```powershell
# Download Cosign
Invoke-WebRequest -Uri "https://github.com/sigstore/cosign/releases/latest/download/cosign-windows-amd64.exe" -OutFile "cosign.exe"

# Move to PATH location
Move-Item -Path cosign.exe -Destination "C:\Windows\System32\cosign.exe"

# Verify installation
cosign version
```

**Container (for air-gapped environments):**
```bash
# Pull official Cosign image
docker pull gcr.io/projectsigstore/cosign:latest

# Run verification in container
docker run --rm -v $(pwd):/workspace gcr.io/projectsigstore/cosign:latest \
  verify-blob /workspace/artifact.whl --bundle=/workspace/artifact.whl.cosign.bundle
```

---

## Quick Verification (2 Minutes)

For rapid verification of artifact authenticity:

```bash
# 1. Download release artifacts
VERSION="1.5.0"
BASE_URL="https://github.com/fraiseql/fraiseql/releases/download/v${VERSION}"

# Download wheel (example: Linux x86_64)
ARTIFACT="fraiseql-${VERSION}-cp313-cp313-manylinux_2_34_x86_64.whl"
curl -LO ${BASE_URL}/${ARTIFACT}

# Download Cosign signature bundle
curl -LO ${BASE_URL}/${ARTIFACT}.cosign.bundle

# 2. Verify signature
cosign verify-blob ${ARTIFACT} \
  --bundle=${ARTIFACT}.cosign.bundle \
  --certificate-identity="https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@refs/tags/v${VERSION}" \
  --certificate-oidc-issuer="https://token.actions.githubusercontent.com"

# Expected output:
# Verified OK
```

**✅ If verification succeeds**: Artifact is authentic and was built by GitHub Actions.

**❌ If verification fails**: DO NOT use the artifact. Report to security@fraiseql.com.

---

## Comprehensive Verification (5 Minutes)

For full signature verification with certificate inspection:

### Step 1: Download All Artifacts

```bash
VERSION="1.5.0"
BASE_URL="https://github.com/fraiseql/fraiseql/releases/download/v${VERSION}"

# Create verification directory
mkdir -p fraiseql-signature-verification
cd fraiseql-signature-verification

# Download example artifact
ARTIFACT="fraiseql-${VERSION}-cp313-cp313-manylinux_2_34_x86_64.whl"
curl -LO ${BASE_URL}/${ARTIFACT}
curl -LO ${BASE_URL}/${ARTIFACT}.cosign.bundle
curl -LO ${BASE_URL}/${ARTIFACT}.sig

echo "✅ Downloaded artifacts"
```

### Step 2: Verify Signature with Bundle

```bash
# Verify using bundle (recommended - includes certificate)
cosign verify-blob ${ARTIFACT} \
  --bundle=${ARTIFACT}.cosign.bundle \
  --certificate-identity="https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@refs/tags/v${VERSION}" \
  --certificate-oidc-issuer="https://token.actions.githubusercontent.com"

# Expected output:
# Verified OK
# tlog entry verified with uuid: "..."
# tlog entry verified with inclusion proof
```

**What This Proves:**
- ✅ Artifact was signed by GitHub Actions (not a compromised developer machine)
- ✅ Signature is valid and matches artifact
- ✅ Certificate is logged to transparency log (Rekor)
- ✅ Signature is bound to specific workflow and Git reference

### Step 3: Inspect Certificate

```bash
# Extract and decode certificate from bundle
echo "=== Certificate Inspection ==="

# Install jq if not available
# Ubuntu/Debian: sudo apt-get install jq
# macOS: brew install jq
# RHEL/CentOS: sudo yum install jq

# Extract certificate from bundle
CERT=$(jq -r '.cert' ${ARTIFACT}.cosign.bundle)

# Decode certificate
echo "$CERT" | base64 -d | openssl x509 -text -noout

# Look for key fields:
# - Issuer: CN = sigstore-intermediate, O = sigstore.dev
# - Subject: (empty - keyless)
# - X509v3 Subject Alternative Name: URI:https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@refs/tags/v1.5.0
# - X509v3 Key Usage: Digital Signature
# - Validity: Not After - should be ~10 minutes after signing
```

**Key Certificate Fields:**

1. **Issuer**: Should be Sigstore CA
2. **Subject Alternative Name (SAN)**: Should contain GitHub Actions workflow URL
3. **OIDC Issuer Extension**: Should be `https://token.actions.githubusercontent.com`
4. **Short Validity**: Certificate expires in ~10 minutes (one-time use)

### Step 4: Verify Transparency Log Entry

```bash
# Extract transparency log index from bundle
TLOG_INDEX=$(jq -r '.rekorBundle.Payload.logIndex' ${ARTIFACT}.cosign.bundle)

echo "Rekor Transparency Log Index: $TLOG_INDEX"

# Verify entry exists in Rekor (requires internet)
curl -s "https://rekor.sigstore.dev/api/v1/log/entries?logIndex=$TLOG_INDEX" | jq .

# Expected: JSON response with entry details
```

**What This Proves:**
- ✅ Signature is publicly logged (tamper-proof)
- ✅ Timestamp of signing operation
- ✅ Cannot be backdated or hidden
- ✅ Audit trail for compliance

### Step 5: Verify Multiple Artifacts

```bash
# Download and verify all wheels
echo "=== Verifying All Artifacts ==="

# Download SHA256SUMS
curl -LO ${BASE_URL}/SHA256SUMS

# For each artifact in SHA256SUMS
while read -r line; do
  CHECKSUM=$(echo "$line" | awk '{print $1}')
  FILE=$(echo "$line" | awk '{print $2}')

  # Skip non-wheel/non-sdist files
  if [[ ! "$FILE" =~ \.(whl|tar\.gz)$ ]]; then
    continue
  fi

  echo ""
  echo "Verifying: $FILE"

  # Download artifact and bundle
  curl -sLO ${BASE_URL}/${FILE}
  curl -sLO ${BASE_URL}/${FILE}.cosign.bundle

  # Verify signature
  cosign verify-blob ${FILE} \
    --bundle=${FILE}.cosign.bundle \
    --certificate-identity="https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@refs/tags/v${VERSION}" \
    --certificate-oidc-issuer="https://token.actions.githubusercontent.com" && \
    echo "✅ $FILE: Signature valid" || \
    echo "❌ $FILE: Signature INVALID"

done < SHA256SUMS

echo ""
echo "✅ Signature verification complete for all artifacts"
```

---

## Advanced Verification

### Verify Signature Without Internet (Offline Mode)

For air-gapped environments:

```bash
# 1. Download artifacts and bundles while online
# 2. Transfer to air-gapped environment
# 3. Verify with offline bundle

# Note: Bundle includes certificate and Rekor entry
# No internet required for verification
cosign verify-blob artifact.whl \
  --bundle=artifact.whl.cosign.bundle \
  --certificate-identity="https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@refs/tags/v1.5.0" \
  --certificate-oidc-issuer="https://token.actions.githubusercontent.com" \
  --offline
```

**Limitation**: Offline mode cannot verify Rekor log freshness.

### Verify with Specific Certificate Identity

```bash
# Verify artifact is from official repository
cosign verify-blob artifact.whl \
  --bundle=artifact.whl.cosign.bundle \
  --certificate-identity="https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@refs/tags/v1.5.0" \
  --certificate-oidc-issuer="https://token.actions.githubusercontent.com"

# Reject if certificate identity doesn't match
# This prevents accepting signatures from forks or other repositories
```

### Verify Signature is Recent

```bash
# Extract signing timestamp from Rekor bundle
TIMESTAMP=$(jq -r '.rekorBundle.Payload.integratedTime' artifact.whl.cosign.bundle)

# Convert to human-readable
date -d @$TIMESTAMP

# Ensure timestamp is within expected release window
# (e.g., within 1 hour of Git tag creation)
```

---

## Automated Verification Script

Save as `verify_signatures.sh`:

```bash
#!/bin/bash
set -euo pipefail

VERSION="${1:-1.5.0}"
BASE_URL="https://github.com/fraiseql/fraiseql/releases/download/v${VERSION}"

echo "🔐 FraiseQL Artifact Signature Verification"
echo "Version: $VERSION"
echo ""

# Check Cosign installation
if ! command -v cosign &> /dev/null; then
  echo "❌ Cosign not installed. Please install Cosign first."
  echo "   https://docs.sigstore.dev/cosign/installation"
  exit 1
fi

echo "✅ Cosign installed: $(cosign version | head -1)"
echo ""

# Create temp directory
TMPDIR=$(mktemp -d)
cd "$TMPDIR"

# Download SHA256SUMS
echo "📥 Downloading artifacts..."
curl -sLO ${BASE_URL}/SHA256SUMS

# Count total artifacts
TOTAL=$(grep -E '\.(whl|tar\.gz)$' SHA256SUMS | wc -l)
VERIFIED=0
FAILED=0

echo "Found $TOTAL artifacts to verify"
echo ""

# Verify each artifact
while read -r line; do
  FILE=$(echo "$line" | awk '{print $2}')

  # Skip non-wheel/non-sdist
  if [[ ! "$FILE" =~ \.(whl|tar\.gz)$ ]]; then
    continue
  fi

  echo "🔍 Verifying: $FILE"

  # Download artifact and bundle
  curl -sLO ${BASE_URL}/${FILE} 2>/dev/null || {
    echo "   ⚠️  Failed to download $FILE"
    FAILED=$((FAILED + 1))
    continue
  }

  curl -sLO ${BASE_URL}/${FILE}.cosign.bundle 2>/dev/null || {
    echo "   ⚠️  Signature bundle not found for $FILE"
    FAILED=$((FAILED + 1))
    continue
  }

  # Verify signature
  if cosign verify-blob ${FILE} \
    --bundle=${FILE}.cosign.bundle \
    --certificate-identity="https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml@refs/tags/v${VERSION}" \
    --certificate-oidc-issuer="https://token.actions.githubusercontent.com" &> /dev/null; then
    echo "   ✅ Signature valid"
    VERIFIED=$((VERIFIED + 1))
  else
    echo "   ❌ Signature INVALID"
    FAILED=$((FAILED + 1))
  fi

  echo ""

done < SHA256SUMS

# Summary
echo "=== Verification Summary ==="
echo "Total artifacts: $TOTAL"
echo "Verified: $VERIFIED"
echo "Failed: $FAILED"
echo ""

if [ $FAILED -eq 0 ]; then
  echo "✅ All signatures verified successfully!"
  echo ""
  echo "Verified artifacts located in: $TMPDIR"
  echo "To clean up: rm -rf $TMPDIR"
  exit 0
else
  echo "❌ Signature verification FAILED for $FAILED artifacts"
  echo ""
  echo "DO NOT use artifacts with invalid signatures."
  echo "Report to: security@fraiseql.com"
  exit 1
fi
```

Usage:
```bash
chmod +x verify_signatures.sh
./verify_signatures.sh 1.5.0
```

---

## Verification Checklist for Federal IT

**Level 1: Basic Signature Verification (Required for all deployments)**
- [ ] Cosign installed and working
- [ ] Artifact downloaded from official GitHub Releases
- [ ] Signature bundle downloaded
- [ ] `cosign verify-blob` executed successfully
- [ ] Certificate identity matches official workflow
- [ ] OIDC issuer is GitHub Actions

**Level 2: Certificate Inspection (Required for DoD IL2+)**
- [ ] Certificate extracted and inspected
- [ ] Issuer is Sigstore CA
- [ ] Subject Alternative Name contains workflow URL
- [ ] Certificate validity is short-lived (~10 minutes)
- [ ] No long-term private keys used

**Level 3: Transparency Log Verification (Required for DoD IL4+)**
- [ ] Rekor log index extracted
- [ ] Log entry verified via Rekor API
- [ ] Timestamp validated against release window
- [ ] Transparency log inclusion proof verified
- [ ] No evidence of signature backdating

---

## Troubleshooting

### Issue: `cosign: command not found`

**Cause:** Cosign not installed or not in PATH.

**Solution:** Install Cosign using instructions in Prerequisites section.

### Issue: Verification fails with "certificate identity mismatch"

**Cause:** Downloaded artifact from unofficial source or wrong version.

**Solution:**
1. Verify download URL is from `github.com/fraiseql/fraiseql/releases`
2. Ensure version in certificate-identity matches downloaded version
3. Re-download from official source

### Issue: `invalid signature when validating ASN.1 encoded signature`

**Cause:** Artifact was modified after signing.

**Solution:**
1. Re-download artifact and signature bundle
2. Verify SHA256 checksum matches SHA256SUMS
3. If still fails, report to security@fraiseql.com

### Issue: `Error: no matching signatures`

**Cause:** Signature bundle is corrupt or wrong file.

**Solution:**
1. Ensure bundle file matches artifact name (e.g., `artifact.whl.cosign.bundle`)
2. Re-download signature bundle
3. Verify bundle is valid JSON: `jq . artifact.whl.cosign.bundle`

### Issue: `failed to verify transparency log entry`

**Cause:** Internet connection required to verify Rekor log, or log entry not yet propagated.

**Solution:**
1. Ensure internet connectivity to `rekor.sigstore.dev`
2. Wait 1-2 minutes for log propagation
3. For air-gapped environments, use `--offline` flag (note: reduces verification guarantees)

### Issue: Certificate expired

**Cause:** Certificate is short-lived by design (10 minutes).

**Solution:** This is NORMAL. Certificate validity is only checked at signing time. The transparency log (Rekor) provides proof the signature was valid when created.

---

## Federal Compliance Notes

### Comparison: Traditional vs. Keyless Signing

| Aspect | Traditional Signing | Cosign Keyless Signing |
|--------|---------------------|------------------------|
| **Key Management** | Long-term private keys | No keys to manage |
| **Key Escrow** | Required for federal | Not applicable |
| **Key Compromise Risk** | High (long-term keys) | None (ephemeral keys) |
| **Audit Trail** | Manual logs | Automatic (Rekor) |
| **Non-repudiation** | Via private key possession | Via OIDC identity + transparency log |
| **Certificate Revocation** | CRL/OCSP required | Not needed (short-lived certs) |
| **NIST 800-53 Controls** | IA-5 (complex) | IA-5, IA-8 (simplified) |

### NIST 800-53 Mapping

**SI-7: Software, Firmware, and Information Integrity**
- SI-7(1): Integrity Checks - ✅ Cosign cryptographic verification
- SI-7(6): Cryptographic Protection - ✅ ECDSA P-256 signatures
- SI-7(15): Code Authentication - ✅ GitHub OIDC identity binding

**SA-10: Developer Configuration Management**
- SA-10(1): Software Configuration Management - ✅ Git commit + workflow binding
- SA-10(6): Integrity Verification - ✅ Signature verification

**IA-5: Authenticator Management**
- IA-5(2): PKI-Based Authentication - ✅ Certificate-based signing
- IA-5(12): Biometric Authentication - ✅ OIDC authentication (GitHub)

**IA-8: Identification and Authentication (Non-Organizational Users)**
- IA-8(1): Acceptance of PIV Credentials - ✅ Federated identity (OIDC)
- IA-8(2): Acceptance of External Authenticators - ✅ GitHub OIDC

### For ATO Packages

**System Security Plan (SSP) Language:**
> FraiseQL uses Sigstore Cosign for cryptographic signing of all release artifacts. Signing is performed using keyless authentication via GitHub OIDC, eliminating long-term key management requirements. All signatures are logged to the Rekor transparency log (rekor.sigstore.dev), providing immutable audit trails and public verifiability. Signature verification is performed using the `cosign verify-blob` command with certificate identity validation.

**Continuous Monitoring Control:**
> Artifact integrity verification is mandatory before deployment. Automated verification scripts validate cryptographic signatures using Cosign with strict certificate identity matching. Failed verifications are escalated to security team per IR-4 (Incident Response).

**Artifact Deployment Evidence:**
> Include Cosign verification output in deployment documentation:
> ```
> Verified OK
> tlog entry verified with uuid: ...
> tlog entry verified with inclusion proof
> ```

### For FedRAMP

- **SA-10 (Developer Configuration Management)**: Satisfied via cryptographic binding to Git workflow
- **SI-7 (Software Integrity)**: Satisfied via Cosign signature verification
- **IA-5 (Authenticator Management)**: Simplified (no long-term key management)
- **IA-8 (Non-Organizational Users)**: Satisfied via GitHub OIDC authentication

### For FISMA

- Map to NIST 800-53 controls: SI-7, SA-10, IA-5, IA-8
- Include verification procedures in Standard Operating Procedures (SOP)
- Maintain verification logs for audit trail
- Escalate signature verification failures per Incident Response plan

---

## Understanding Cosign Artifacts

### File Types

1. **`.sig` (Signature File)**
   - Raw ECDSA signature bytes
   - Used with separate certificate for verification
   - Example: `artifact.whl.sig`

2. **`.cosign.bundle` (Signature Bundle)**
   - **Recommended format**
   - Contains: signature + certificate + Rekor log entry
   - Self-contained verification
   - Example: `artifact.whl.cosign.bundle`

### Bundle Structure

```json
{
  "base64Signature": "...",  // ECDSA signature
  "cert": "...",              // X.509 certificate (base64)
  "rekorBundle": {
    "Payload": {
      "body": "...",          // Transparency log entry
      "integratedTime": ...,  // Unix timestamp
      "logIndex": ...,        // Rekor log index
      "logID": "..."          // Rekor log ID
    }
  }
}
```

**Why Bundle is Preferred:**
- Single file to download
- Includes all verification data
- Works offline (with `--offline` flag)
- Simplifies verification

---

## Contact

**Security Questions:**
- Email: security@fraiseql.com
- GitHub Security Advisories: https://github.com/fraiseql/fraiseql/security/advisories

**Signature Verification Issues:**
If you believe signature verification is failing incorrectly:
1. Document exact steps to reproduce
2. Include Cosign version: `cosign version`
3. Include error output
4. Email security@fraiseql.com with subject: "Cosign Verification Issue"

**False Positive Reports:**
If you believe signatures are being rejected incorrectly:
1. Verify Cosign installation: `cosign version`
2. Test internet connectivity to `rekor.sigstore.dev`
3. Re-download artifact and bundle
4. If still failing, contact security@fraiseql.com

**Response SLA:** 48 hours for initial response

---

**Document Control:**
- **Author**: Security Team
- **Approved**: Project Maintainers
- **Next Review**: 2026-02-21
- **Distribution**: Public
