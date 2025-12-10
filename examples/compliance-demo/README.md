# Compliance Demo - SLSA Provenance & Cryptographic Audit Trails

**Production-ready compliance features for regulated environments**

This example demonstrates FraiseQL's enterprise compliance capabilities required for SOC 2, HIPAA, GDPR, and FedRAMP environments.

## 🎯 What This Example Demonstrates

- ✅ **Cryptographic Audit Trails** - HMAC-signed audit chain with tamper detection
- ✅ **SLSA Provenance** - Software supply chain security verification
- ✅ **KMS Integration** - Key Management System patterns (AWS KMS, GCP KMS, Vault)
- ✅ **Immutable Logs** - PostgreSQL-enforced immutability
- ✅ **REGULATED Security Profile** - Compliance-focused configuration
- ✅ **Data Integrity** - SHA-256 checksums and verification
- ✅ **Chain Validation** - Automatic detection of audit trail tampering

## 🏗️ Architecture

### Cryptographic Audit Trail

```
┌─────────────────────────────────────────────────────────────┐
│ Audit Entry #1 (Genesis)                                    │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ Event: document_created                                 │ │
│ │ Actor: admin@example.com                                │ │
│ │ Timestamp: 2024-01-01 10:00:00                          │ │
│ │ Previous Hash: NULL                                     │ │
│ │ Current Hash: sha256(event + actor + timestamp)        │ │
│ │ HMAC Signature: hmac_sha256(current_hash, secret_key)  │ │
│ └─────────────────────────────────────────────────────────┘ │
└───────────────┬─────────────────────────────────────────────┘
                │ current_hash passed to next entry
                ▼
┌─────────────────────────────────────────────────────────────┐
│ Audit Entry #2                                              │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ Event: document_accessed                                │ │
│ │ Actor: user@example.com                                 │ │
│ │ Timestamp: 2024-01-01 10:05:00                          │ │
│ │ Previous Hash: <hash from entry #1>  ← CHAIN LINK      │ │
│ │ Current Hash: sha256(... + previous_hash)               │ │
│ │ HMAC Signature: hmac_sha256(current_hash, secret_key)  │ │
│ └─────────────────────────────────────────────────────────┘ │
└───────────────┬─────────────────────────────────────────────┘
                │
                ▼ Chain continues...
```

**Why This Matters:**
- **Tamper Detection**: Any modification breaks the chain
- **Cryptographic Proof**: HMAC signatures verify authenticity
- **Compliance**: Required for SOC 2, HIPAA, FedRAMP
- **Forensics**: Complete audit trail for security incidents

### SLSA Provenance

```
┌──────────────────────────────────────────────────────────┐
│ Software Artifact: fraiseql@0.1.0                        │
├──────────────────────────────────────────────────────────┤
│ Build Information:                                       │
│   Builder: GitHub Actions                                │
│   Source: github.com/fraiseql/fraiseql@commit-abc123    │
│   Build Time: 2024-01-01 10:00:00                        │
│                                                          │
│ Materials (Dependencies):                                │
│   pkg:pypi/fastapi@0.104.0 (sha256:...)                │
│   pkg:pypi/asyncpg@0.29.0 (sha256:...)                 │
│                                                          │
│ Attestation:                                            │
│   ✓ Signed by: sigstore (keyless signing)              │
│   ✓ Verified: GitHub Actions OIDC identity             │
│   ✓ SLSA Level: 3                                       │
└──────────────────────────────────────────────────────────┘
```

**Why This Matters:**
- **Supply Chain Security**: Verify software origins
- **Zero-Trust**: Don't trust, verify every artifact
- **Compliance**: Required for FedRAMP, DoD IL4+
- **Procurement**: Evidence for security questionnaires

## 🚀 Quick Start

### 1. Install Dependencies

```bash
cd examples/compliance-demo
pip install -r requirements.txt
```

### 2. Setup Database

```bash
# Create database
createdb compliance_demo

# Run schema (includes seed data)
psql compliance_demo < schema.sql
```

### 3. Run the Application

```bash
python main.py
```

The server starts on http://localhost:8000 with:
- **GraphQL Playground**: http://localhost:8000/graphql
- **Verify Audit Chain**: GET http://localhost:8000/compliance/verify-audit-chain
- **Verify SLSA**: POST http://localhost:8000/compliance/slsa/verify/{artifact}
- **Rotate KMS Key**: GET http://localhost:8000/compliance/kms/rotate/{key_id}

## 📖 Usage Examples

### Cryptographic Audit Trail

#### Query Audit Trail

```graphql
query AuditTrail {
  auditTrail(resourceType: "document", limit: 100) {
    sequenceNumber
    eventType
    actor
    timestamp
    currentHash
    previousHash
    hmacSignature
    chainValid
  }
}
```

#### Verify Audit Chain Integrity

```bash
curl http://localhost:8000/compliance/verify-audit-chain
```

Response:
```json
{
  "total_entries": 150,
  "valid_entries": 150,
  "invalid_entries": 0,
  "chain_intact": true,
  "details": [
    {
      "sequence": 1,
      "valid": true,
      "message": "First entry (genesis)"
    },
    {
      "sequence": 2,
      "valid": true,
      "message": "Valid chain link"
    },
    ...
  ]
}
```

**Chain Integrity Check:**
- ✅ `chain_intact: true` - No tampering detected
- ❌ `chain_intact: false` - Audit trail has been modified

#### Query with Chain Validation

```graphql
query AuditChainValidation {
  auditChainValidation {
    sequenceNumber
    eventType
    actor
    timestamp
    chainValid  # ✓ true if chain is intact
  }
}
```

### SLSA Provenance

#### Query SLSA Attestations

```graphql
query SLSAProvenance {
  slsaProvenance(verifiedOnly: true) {
    artifactName
    artifactVersion
    artifactDigest
    buildType
    builderId
    sourceRepo
    sourceCommit
    verified
    verifiedAt
  }
}
```

#### Verify SLSA Provenance

```bash
curl -X POST http://localhost:8000/compliance/slsa/verify/fraiseql?version=0.1.0
```

Response:
```json
{
  "artifact": "fraiseql@0.1.0",
  "verified": true,
  "slsa_level": "SLSA Level 3",
  "builder": "https://github.com/fraiseql/fraiseql/.github/workflows/publish.yml",
  "source": {
    "repo": "https://github.com/fraiseql/fraiseql",
    "commit": "abc123def456"
  },
  "build_time": "2024-01-01T10:00:00Z",
  "verification_steps": [
    {"step": "Signature verification", "status": "✓ PASSED"},
    {"step": "Builder identity check", "status": "✓ PASSED"},
    {"step": "Source provenance", "status": "✓ PASSED"},
    {"step": "Materials completeness", "status": "✓ PASSED"}
  ]
}
```

#### Record SLSA Provenance

```graphql
mutation RecordSLSA {
  recordSLSAProvenance(input: {
    artifactName: "my-app"
    artifactVersion: "1.0.0"
    artifactDigest: "sha256:abc123..."
    buildType: "github-actions"
    sourceCommit: "def456..."
    attestation: "base64-encoded-attestation"
  }) {
    id
    verified
  }
}
```

### KMS Integration

#### List KMS Keys

```graphql
query KMSKeys {
  kmsKeys(provider: "aws-kms", status: "active") {
    id
    keyId
    keyProvider
    keyPurpose
    keyAlgorithm
    keyStatus
    createdAt
  }
}
```

#### Rotate KMS Key

```bash
curl http://localhost:8000/compliance/kms/rotate/arn:aws:kms:us-east-1:123456789012:key/audit-hmac
```

Response:
```json
{
  "status": "success",
  "message": "Key rotated successfully",
  "old_key_status": "rotated",
  "new_key_version": "v2",
  "rotation_timestamp": "2024-01-01T10:00:00Z"
}
```

### Documents with Integrity Checksums

#### Create Document with Audit Trail

```graphql
mutation CreateDocument {
  createDocument(input: {
    title: "Security Policy v2.0"
    content: "Updated security requirements..."
    classification: "confidential"
    createdBy: "admin@example.com"
  }) {
    id
    title
    checksum  # SHA-256 hash for integrity
  }
}
```

**What Happens:**
1. Document created with SHA-256 checksum
2. Audit entry automatically created in chain
3. HMAC signature generated
4. Previous hash linked to maintain chain

#### Query Document with Audit Statistics

```graphql
query Document {
  document(id: "11111111-1111-1111-1111-111111111111") {
    id
    title
    content
    classification
    checksum
    auditCount  # Total audit entries for this document
    lastAuditAt # Last time document was audited
  }
}
```

## 🔒 Compliance Features

### 1. Cryptographic Audit Trail

**Implementation:**
```sql
-- Each audit entry is cryptographically linked
CREATE TABLE tb_audit_trail (
    sequence_number BIGSERIAL UNIQUE,
    previous_hash TEXT,  -- Hash of previous entry
    current_hash TEXT,   -- SHA-256 of this entry
    hmac_signature TEXT, -- HMAC-SHA256 for authenticity
    ...
);
```

**Chain Verification Function:**
```sql
CREATE FUNCTION fn_verify_audit_chain() RETURNS TABLE(...) AS $$
BEGIN
    -- Verifies each entry's previous_hash matches
    -- the previous entry's current_hash
    RETURN QUERY SELECT ... WHERE a.previous_hash = prev.current_hash;
END;
$$ LANGUAGE plpgsql;
```

**Benefits:**
- **Tamper-Evident**: Any modification breaks the chain
- **Non-Repudiation**: HMAC signatures prove authenticity
- **Forensic Ready**: Complete audit history for investigations
- **Compliance**: SOC 2, HIPAA, GDPR, FedRAMP requirements

### 2. SLSA Provenance Tracking

**SLSA Levels:**
- **Level 1**: Documentation of build process
- **Level 2**: Automated build service
- **Level 3**: Provenance attestation + tamper resistance (FraiseQL target)
- **Level 4**: Two-person review + hermetic builds

**FraiseQL SLSA Support:**
```sql
CREATE TABLE tb_slsa_provenance (
    artifact_digest TEXT,     -- SHA-256 of artifact
    builder_id TEXT,          -- GitHub Actions workflow URI
    source_commit TEXT,       -- Git commit SHA
    attestation TEXT,         -- Signed provenance
    verified BOOLEAN,         -- Verification status
    ...
);
```

**Verification Process:**
1. Fetch attestation from artifact
2. Verify signature with `cosign`
3. Check builder identity (OIDC)
4. Validate source claims
5. Mark as verified in database

### 3. KMS Integration Patterns

**Supported KMS Providers:**
- **AWS KMS**: `arn:aws:kms:...`
- **GCP KMS**: `projects/.../keyRings/.../cryptoKeys/...`
- **HashiCorp Vault**: `vault/.../...`

**Use Cases:**
- **Audit Signing**: HMAC keys for audit trail
- **Data Encryption**: AES-256-GCM for sensitive fields
- **Document Signing**: RSA-2048 for digital signatures

**Example:**
```sql
-- Reference KMS key
INSERT INTO tb_kms_key (key_id, key_provider, key_purpose)
VALUES (
    'arn:aws:kms:us-east-1:123456789012:key/abc123',
    'aws-kms',
    'audit-signing'
);

-- Encrypted field registry
INSERT INTO tb_encrypted_field (
    table_name, field_name, record_id,
    kms_key_id, encrypted_value
) VALUES (...);
```

### 4. Immutable Audit Logs

**PostgreSQL-Level Immutability:**
```sql
-- Revoke DELETE and UPDATE on audit table
REVOKE DELETE, UPDATE ON tb_audit_trail FROM PUBLIC;

-- Grant only INSERT
GRANT INSERT ON tb_audit_trail TO app_user;
```

**Application-Level Protection:**
- No DELETE mutations exposed in GraphQL
- No UPDATE mutations for audit entries
- Read-only access via views
- Retention policies for compliance

## 📊 Compliance Mappings

### SOC 2 Controls

| Control | Implementation | Evidence |
|---------|---------------|----------|
| **CC6.1** (Logical Access) | Audit trail logs all access | Query `auditTrail` |
| **CC7.2** (Security Monitoring) | Cryptographic chain verification | `/compliance/verify-audit-chain` |
| **CC7.3** (System Changes) | Immutable audit log | `tb_audit_trail` schema |

### HIPAA Requirements

| Requirement | Implementation | Evidence |
|------------|---------------|----------|
| **164.308(a)(1)(ii)(D)** (Audit Controls) | Complete audit trail | All actions logged |
| **164.312(c)(1)** (Integrity) | SHA-256 checksums + HMAC | Document checksums |
| **164.312(c)(2)** (Mechanism) | Cryptographic chain | Chain validation |

### FedRAMP Controls

| Control ID | Name | Implementation |
|-----------|------|---------------|
| **AU-2** | Audit Events | All events logged |
| **AU-3** | Content of Audit Records | Complete event details |
| **AU-9** | Protection of Audit | Immutable + cryptographic chain |
| **AU-10** | Non-repudiation | HMAC signatures |
| **SA-15** | SLSA Provenance | Attestation tracking |

### GDPR Requirements

| Article | Requirement | Implementation |
|---------|------------|---------------|
| **Art. 5(1)(f)** | Integrity and confidentiality | Cryptographic integrity |
| **Art. 32** | Security of processing | Audit trail + encryption |
| **Art. 33** | Breach notification | Complete audit history |

## 🧪 Testing Compliance Features

### Test Audit Chain Integrity

```bash
# Verify chain is intact
curl http://localhost:8000/compliance/verify-audit-chain | jq '.chain_intact'
# Expected: true

# If someone tampers with audit log:
psql compliance_demo -c "UPDATE tb_audit_trail SET actor = 'hacker' WHERE sequence_number = 2;"

# Verify again
curl http://localhost:8000/compliance/verify-audit-chain | jq '.chain_intact'
# Expected: false (chain broken!)
```

### Test SLSA Verification

```bash
# Verify known-good artifact
curl -X POST http://localhost:8000/compliance/slsa/verify/fraiseql?version=0.1.0 | jq '.verified'
# Expected: true

# Try non-existent artifact
curl -X POST http://localhost:8000/compliance/slsa/verify/fake-package?version=1.0.0
# Expected: 404 Not Found
```

### Test KMS Key Rotation

```bash
# List active keys
curl http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ kmsKeys(status: \"active\") { keyId keyStatus } }"}' \
  | jq

# Rotate key
curl http://localhost:8000/compliance/kms/rotate/arn:aws:kms:us-east-1:123456789012:key/audit-hmac

# Verify key is now rotated
curl http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ kmsKeys(status: \"rotated\") { keyId keyStatus } }"}' \
  | jq
```

## 🚀 Production Deployment

### Checklist

- [ ] Use real KMS (AWS KMS, GCP KMS, Vault) instead of hardcoded keys
- [ ] Enable PostgreSQL-level audit log immutability (REVOKE DELETE/UPDATE)
- [ ] Set up log aggregation (send to SIEM)
- [ ] Configure audit log retention policies
- [ ] Implement automated chain verification (cron job)
- [ ] Set up alerting for chain integrity failures
- [ ] Configure SLSA provenance verification in CI/CD
- [ ] Enable database backups with point-in-time recovery
- [ ] Document incident response procedures
- [ ] Train team on audit trail investigation

### KMS Integration

**AWS KMS Example:**
```python
import boto3

kms = boto3.client('kms')

# Generate HMAC signature
response = kms.generate_mac(
    KeyId='arn:aws:kms:us-east-1:123456789012:key/audit-hmac',
    Message=audit_entry_hash.encode(),
    MacAlgorithm='HMAC_SHA_256'
)
hmac_signature = response['Mac']
```

**GCP KMS Example:**
```python
from google.cloud import kms

client = kms.KeyManagementServiceClient()
key_name = 'projects/myproject/locations/global/keyRings/compliance/cryptoKeys/audit'

# Sign audit entry
response = client.mac_sign(
    request={'name': key_name, 'data': audit_entry_hash.encode()}
)
hmac_signature = response.mac
```

### Monitoring

**Metrics to Track:**
- Audit entries per hour
- Chain verification failures
- SLSA verification attempts
- KMS key usage
- Average audit query latency

**Alerts to Configure:**
- Chain integrity failure (CRITICAL)
- Unusual audit volume (WARNING)
- SLSA verification failure (HIGH)
- KMS key rotation needed (INFO)

## 📚 Related Examples

- [`../multi-tenant-saas/`](../multi-tenant-saas/) - Multi-tenancy with RLS
- [`../blog_enterprise/`](../blog_enterprise/) - Enterprise CQRS patterns
- [`../security/`](../security/) - Additional security features

## 📄 Further Reading

- [SLSA Framework](https://slsa.dev/)
- [Sigstore Documentation](https://docs.sigstore.dev/)
- [SOC 2 Compliance Guide](https://www.aicpa.org/soc2)
- [NIST 800-53 Controls](https://csrc.nist.gov/publications/detail/sp/800-53/rev-5/final)
- [PostgreSQL Audit Logging](https://www.postgresql.org/docs/current/pgaudit.html)

---

**This example demonstrates production-grade compliance features. Use it as a foundation for SOC 2, HIPAA, GDPR, and FedRAMP applications!** 🔒
