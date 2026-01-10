# Phase 05: Document IL4/IL5 Deployment Requirements

**Priority:** MEDIUM
**Time Estimate:** 1.5 hours
**Impact:** +0.5 point to Security Architecture score (23/25 → 23.5/25)
**Status:** ⬜ Not Started

---

## Problem Statement

Pentagon-Readiness Assessment recommends "Enhance documentation for classified deployments" and "Add explicit IL4/IL5 deployment guides." While FraiseQL has RESTRICTED security profile, deployment guidance for DoD classified environments (Impact Levels 4 and 5) is missing.

---

## Objective

Create deployment guide for IL4/IL5 classified environments with:
1. Overview of DoD Impact Level requirements
2. IL4 deployment configuration (CUI + Mission Critical)
3. IL5 deployment configuration (Classified/Secret)
4. Air-gapped deployment procedures
5. Pre-deployment checklists
6. Security validation tests

**Deliverable:** `docs/deployment/CLASSIFIED_ENVIRONMENTS.md` (400-600 lines)

---

## Background: DoD Impact Levels

**Impact Level 4 (IL4):**
- **Data Classification:** Controlled Unclassified Information (CUI)
- **Systems:** Mission-critical systems, DoD networks
- **Requirements:** FIPS 140-2, mTLS, RLS, strict audit logging
- **Deployment:** AWS GovCloud, Azure Government, or DoD-approved clouds
- **Clearance:** Secret clearance for administrators

**Impact Level 5 (IL5):**
- **Data Classification:** Classified information (Secret level)
- **Systems:** National Security Systems
- **Requirements:** All IL4 + air-gapped deployment, HSM, continuous monitoring
- **Deployment:** Dedicated environments, often air-gapped
- **Clearance:** Secret clearance required

---

## Context Files

**Review these files before writing (orchestrator will copy to `context/`):**
- `docs/security/PROFILES.md` - Security profiles (especially RESTRICTED)
- `docs/security/KMS.md` - KMS/encryption configuration (if exists)
- `COMPLIANCE/AUDIT/AUDIT_LOGGING.md` - Audit logging capabilities
- Any existing deployment documentation
- `docs/production/MONITORING.md` - Observability setup

**External References:**
- DoD Cloud Computing SRG: https://dl.dod.cyber.mil/wp-content/uploads/cloud/pdf/Cloud_Computing_SRG_v1r3.pdf
- DISA STIGs: https://public.cyber.mil/stigs/
- NIST FIPS 140-2: https://csrc.nist.gov/publications/detail/fips/140/2/final

---

## Deliverable

**File:** `.phases/05-classified-deployment/output/CLASSIFIED_ENVIRONMENTS.md`

**Target Location:** `docs/deployment/CLASSIFIED_ENVIRONMENTS.md`

---

## Required Structure

### 1. Overview

**Introduction section:**
- Purpose of this guide
- When to use IL4 vs IL5
- Prerequisites (clearances, approvals, infrastructure)
- Scope (deployment only, not development)

**Impact Level Comparison Table:**

| Aspect | IL4 | IL5 |
|--------|-----|-----|
| Data Classification | CUI | Secret |
| Network | DoD networks, GovCloud | Air-gapped, classified networks |
| Authentication | mTLS, PKI | mTLS + HSM-backed keys |
| Encryption | KMS (GovCloud) | HSM-based encryption |
| Audit Retention | 7 years | 7 years |
| Rate Limiting | 10 req/min | 5 req/min |
| Token Expiration | 15 minutes | 5 minutes |
| Deployment | GovCloud, Azure Gov | Dedicated, air-gapped |

---

### 2. Prerequisites

**Before deploying FraiseQL in classified environments:**

- [ ] **Authority to Operate (ATO)** process initiated
- [ ] **SLSA provenance verification** capability in place
- [ ] **SBOM review** and approval from security team
- [ ] **Security clearances** for all administrators
- [ ] **DoD PKI certificates** obtained
- [ ] **Air-gapped deployment plan** (for IL5)
- [ ] **STIG compliance verification** tools available
- [ ] **Approved cloud environment** (AWS GovCloud, Azure Government)

---

### 3. Impact Level 4 (IL4) Deployment

#### Security Profile Configuration

**Document FraiseQL RESTRICTED profile configuration for IL4:**

```python
from fraiseql.security import SecurityProfile

# IL4 Configuration
config = SecurityProfile.RESTRICTED.configure(
    # Authentication & Encryption
    require_tls=True,
    tls_version="1.3",
    require_mtls=True,
    jwt_expiration_minutes=15,

    # Authorization
    enable_rls=True,
    enable_field_level_security=True,

    # GraphQL Security
    enable_introspection=False,
    enable_apq=True,
    apq_mode="required",  # Only pre-registered queries
    max_query_depth=5,
    max_query_complexity=1000,

    # Rate Limiting
    rate_limit_enabled=True,
    rate_limit_requests_per_minute=10,
    rate_limit_burst=2,

    # Audit & Logging
    audit_mode="VERBOSE",  # Field-level tracking
    audit_pii_fields=True,
    audit_retention_days=2555,  # 7 years

    # Error Handling
    error_detail_level="MINIMAL",  # No stack traces

    # KMS Configuration
    kms_provider="aws",  # AWS GovCloud KMS
    kms_key_rotation_days=90,
)
```

#### Environment Variables

**Document required environment variables:**

```bash
# TLS/mTLS Configuration
FRAISEQL_TLS_ENABLED=true
FRAISEQL_TLS_VERSION=1.3
FRAISEQL_MTLS_ENABLED=true
FRAISEQL_TLS_CERT_PATH=/etc/fraiseql/certs/server.crt
FRAISEQL_TLS_KEY_PATH=/etc/fraiseql/certs/server.key
FRAISEQL_MTLS_CA_PATH=/etc/fraiseql/certs/dod-pki-ca.crt

# Authentication
FRAISEQL_JWT_SECRET=<from-vault>
FRAISEQL_JWT_EXPIRATION_MINUTES=15
FRAISEQL_JWT_ALGORITHM=RS256
FRAISEQL_JWT_PUBLIC_KEY_PATH=/etc/fraiseql/keys/jwt-public.pem

# Database (PostgreSQL with SSL)
FRAISEQL_DB_SSL_MODE=verify-full
FRAISEQL_DB_SSL_CERT=/etc/fraiseql/certs/db-client.crt
FRAISEQL_DB_SSL_KEY=/etc/fraiseql/certs/db-client.key
FRAISEQL_DB_SSL_ROOT_CERT=/etc/fraiseql/certs/db-ca.crt

# KMS (AWS GovCloud)
FRAISEQL_KMS_PROVIDER=aws
AWS_KMS_KEY_ID=<kms-key-arn>
AWS_REGION=us-gov-west-1

# Audit Logging
FRAISEQL_AUDIT_MODE=VERBOSE
FRAISEQL_AUDIT_LOG_PATH=/var/log/fraiseql/audit.log
FRAISEQL_AUDIT_RETENTION_DAYS=2555

# Observability
FRAISEQL_OTEL_ENABLED=true
FRAISEQL_OTEL_ENDPOINT=https://otel-collector.mil
FRAISEQL_OTEL_TRACE_SAMPLING=1.0  # 100% sampling
```

#### Network Configuration

**AWS GovCloud Security Group Example:**

```hcl
# Terraform configuration for IL4 security group
resource "aws_security_group" "fraiseql_il4" {
  name        = "fraiseql-il4"
  description = "FraiseQL IL4 Security Group"
  vpc_id      = var.vpc_id

  # Ingress: HTTPS only from approved CIDR blocks
  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = var.dod_network_cidrs
    description = "HTTPS from DoD networks"
  }

  # Egress: PostgreSQL database only
  egress {
    from_port   = 5432
    to_port     = 5432
    protocol    = "tcp"
    cidr_blocks = [var.db_subnet_cidr]
    description = "PostgreSQL database"
  }

  # Egress: KMS endpoint
  egress {
    from_port       = 443
    to_port         = 443
    protocol        = "tcp"
    prefix_list_ids = [var.kms_prefix_list_id]
    description     = "AWS KMS"
  }

  tags = {
    ImpactLevel = "IL4"
    Compliance  = "DoD Cloud SRG"
  }
}
```

#### Database Configuration (RLS)

**Required PostgreSQL RLS policies:**

```sql
-- Enable RLS on all tables
ALTER TABLE <table_name> ENABLE ROW LEVEL SECURITY;

-- Tenant isolation policy
CREATE POLICY tenant_isolation ON <table_name>
  USING (tenant_id = current_setting('app.tenant_id')::uuid);

-- Classification-based access control
CREATE POLICY classification_access ON documents
  USING (
    classification_level <= current_setting('app.user_clearance')::int
  );

-- Audit all access
-- (FraiseQL handles this via audit_log table)
```

#### Pre-Deployment Checklist (IL4)

**Security:**
- [ ] SLSA provenance verified for all artifacts
- [ ] SBOM reviewed and all dependencies approved
- [ ] DoD PKI certificates obtained and installed
- [ ] KMS keys created in AWS GovCloud
- [ ] mTLS certificates distributed to all clients
- [ ] GraphQL introspection disabled
- [ ] APQ "required" mode enabled (only pre-registered queries)
- [ ] Rate limiting configured (10 req/min)

**Compliance:**
- [ ] ATO package submitted
- [ ] STIG compliance scan completed (Trivy, DISA SCAP)
- [ ] Audit logging verified (VERBOSE mode, 7-year retention)
- [ ] Security scan results reviewed (no critical vulnerabilities)

**Operations:**
- [ ] Incident response plan approved
- [ ] Backup/restore tested in GovCloud
- [ ] Disaster recovery plan documented
- [ ] Monitoring dashboards deployed (Grafana)
- [ ] On-call rotation established with cleared personnel

---

### 4. Impact Level 5 (IL5) Deployment

#### Additional Requirements Beyond IL4

**IL5 enhancements:**
- **Air-gapped deployment:** No internet access, all artifacts transferred via approved media
- **Hardware Security Modules (HSM):** For key storage and cryptographic operations
- **Continuous monitoring:** SIEM integration (Splunk, ArcSight)
- **Enhanced audit logging:** Every field access logged
- **Stricter rate limiting:** 5 req/min (half of IL4)
- **Shorter token expiration:** 5 minutes (vs 15 for IL4)
- **Two-person integrity:** Dual control for administrative operations

#### Security Profile Configuration (IL5)

```python
# IL5 Configuration (stricter than IL4)
config = SecurityProfile.RESTRICTED.configure(
    # Authentication
    jwt_expiration_minutes=5,  # Stricter than IL4
    require_hardware_backed_keys=True,  # HSM required

    # Rate Limiting
    rate_limit_requests_per_minute=5,  # Half of IL4

    # Audit
    audit_every_field_access=True,  # Log every field read
    audit_failed_attempts=True,

    # KMS
    kms_provider="hsm",  # Hardware Security Module
    kms_require_dual_control=True,  # Two-person integrity
)
```

#### Air-Gapped Deployment Process

**Artifact transfer workflow:**

1. **On internet-connected system (outside classified network):**
   ```bash
   # Download release artifacts
   wget https://github.com/fraiseql/fraiseql/releases/download/vX.Y.Z/fraiseql-X.Y.Z.tar.gz
   wget https://github.com/fraiseql/fraiseql/releases/download/vX.Y.Z/fraiseql-X.Y.Z.tar.gz.sig
   wget https://github.com/fraiseql/fraiseql/releases/download/vX.Y.Z/fraiseql-X.Y.Z.sbom.json

   # Verify SLSA provenance
   cosign verify-blob --signature fraiseql-X.Y.Z.tar.gz.sig fraiseql-X.Y.Z.tar.gz

   # Verify checksums
   sha256sum -c fraiseql-X.Y.Z.tar.gz.sha256
   ```

2. **Transfer via approved media:**
   - Burn to DVD or copy to approved USB drive
   - Transfer through SCIF or via secure courier
   - Log transfer in chain-of-custody form

3. **On air-gapped system (inside classified network):**
   ```bash
   # Verify checksums again
   sha256sum -c fraiseql-X.Y.Z.tar.gz.sha256

   # Extract to local repository
   tar -xzf fraiseql-X.Y.Z.tar.gz -C /local/repo/

   # Install from local repository (no internet access)
   pip install --no-index --find-links=/local/repo fraiseql
   ```

#### HSM Integration

**AWS CloudHSM example:**

```python
from fraiseql.kms import HSMProvider

hsm = HSMProvider(
    cluster_id="<cloudhsm-cluster-id>",
    hsm_ip="<hsm-ip>",
    hsm_port=2225,
    crypto_user="<crypto-user>",
    crypto_password="<from-vault>",
    require_dual_control=True,  # Two-person integrity
)
```

#### SIEM Integration

**Required logging to SIEM:**
- All authentication attempts (success and failure)
- All GraphQL queries with user context
- All field-level access (VERBOSE audit mode)
- All errors and exceptions
- All administrative actions

**Example Splunk integration:**

```python
import logging
from splunk_handler import SplunkHandler

splunk_handler = SplunkHandler(
    host="siem.mil",
    port=8088,
    token="<hec-token>",
    index="fraiseql_il5",
    sourcetype="fraiseql:audit",
)

logging.getLogger("fraiseql.audit").addHandler(splunk_handler)
```

#### Pre-Deployment Checklist (IL5)

**All IL4 requirements PLUS:**
- [ ] Air-gapped artifact transfer completed and verified
- [ ] HSM configured and tested (dual control)
- [ ] SIEM integration configured and tested
- [ ] Field-level audit logging verified
- [ ] 5-minute token expiration tested
- [ ] Rate limiting reduced to 5 req/min
- [ ] Two-person integrity for admin operations
- [ ] Continuous monitoring dashboard deployed

---

### 5. Deployment Validation

#### Security Validation Tests

**Required validation after deployment:**

```bash
# 1. Verify TLS 1.3
openssl s_client -connect fraiseql.mil:443 -tls1_3

# 2. Verify mTLS is required (should fail without client cert)
curl -k https://fraiseql.mil/graphql
# Expected: Connection rejected or TLS error

# 3. Verify mTLS with client cert (should succeed)
curl -X POST https://fraiseql.mil/graphql \
  --cert client.crt --key client.key --cacert ca.crt \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __typename }"}'

# 4. Verify introspection is disabled
curl -X POST https://fraiseql.mil/graphql \
  --cert client.crt --key client.key --cacert ca.crt \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { types { name } } }"}'
# Expected: Error response (introspection disabled)

# 5. Verify APQ required mode (non-persisted query should fail)
curl -X POST https://fraiseql.mil/graphql \
  --cert client.crt --key client.key --cacert ca.crt \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id } }"}'
# Expected: Error (persisted query not found)

# 6. Verify rate limiting
for i in {1..20}; do
  curl -X POST https://fraiseql.mil/graphql \
    --cert client.crt --key client.key --cacert ca.crt \
    -H "Content-Type: application/json" \
    -d '{"query": "{ __typename }"}' \
    -w "\nStatus: %{http_code}\n"
done
# Expected: 429 Too Many Requests after 10 requests (IL4) or 5 requests (IL5)

# 7. Verify audit logging
tail -f /var/log/fraiseql/audit.log
# Expected: All requests logged with full detail
```

#### Compliance Validation

```bash
# 1. Verify SLSA provenance
cosign verify-blob --signature fraiseql.tar.gz.sig fraiseql.tar.gz

# 2. Run DISA SCAP scan (if available)
oscap xccdf eval --profile stig fraiseql-stig.xml

# 3. Verify audit log retention
find /var/log/fraiseql/audit -type f -mtime +2555
# Expected: No files older than 7 years (2555 days)

# 4. Verify encryption at rest (KMS)
aws kms describe-key --key-id <kms-key-arn> --region us-gov-west-1
# Expected: Key rotation enabled, key state active
```

---

### 6. Troubleshooting

#### Issue: mTLS Connection Failures

**Symptoms:** `TLS handshake failed`, connection rejected

**Resolution:**
1. Verify client certificate is signed by DoD PKI CA
2. Check certificate chain includes all intermediate CAs
3. Verify certificate not expired
4. Check CN/SAN matches expected identity

```bash
# Verify certificate chain
openssl verify -CAfile dod-pki-ca.crt client.crt

# Check certificate expiration
openssl x509 -in client.crt -noout -dates

# Check certificate details
openssl x509 -in client.crt -noout -text | grep -E "(Subject|Issuer|Not Before|Not After)"
```

#### Issue: APQ "Query Not Found" Errors

**Symptoms:** All queries fail with "persisted query not found"

**Resolution:**
1. Pre-register all queries before enabling APQ required mode
2. Generate query hashes: `sha256sum query.graphql`
3. Upload to APQ store

```bash
# Pre-register query (requires admin cert)
curl -X POST https://fraiseql.mil/graphql/apq \
  --cert admin.crt --key admin.key --cacert ca.crt \
  -H "Content-Type: application/json" \
  -d '{"query": "<full_query>", "sha256Hash": "<hash>"}'
```

#### Issue: Audit Logs Filling Disk

**Symptoms:** Disk usage high, audit logs growing rapidly

**Resolution:**
1. Configure log rotation: `/etc/logrotate.d/fraiseql-audit`
2. Compress old logs: `gzip /var/log/fraiseql/audit.log.*`
3. Archive to S3 GovCloud or approved storage

```bash
# Log rotation config
cat > /etc/logrotate.d/fraiseql-audit <<EOF
/var/log/fraiseql/audit.log {
    daily
    rotate 2555
    compress
    delaycompress
    missingok
    notifempty
    create 0640 fraiseql fraiseql
}
EOF

# Archive to S3 GovCloud
aws s3 cp /var/log/fraiseql/audit.log.gz \
  s3://fraiseql-audit-logs-il4/$(date +%Y/%m/%d)/ \
  --region us-gov-west-1
```

---

### 7. References

**Internal Documentation:**
- FraiseQL Security Profiles: `docs/security/PROFILES.md`
- FraiseQL KMS Configuration: `docs/security/KMS.md`
- FraiseQL Audit Logging: `COMPLIANCE/AUDIT/AUDIT_LOGGING.md`
- Operations Runbook: `OPERATIONS_RUNBOOK.md`

**External Standards:**
- DoD Cloud Computing SRG: https://dl.dod.cyber.mil/wp-content/uploads/cloud/pdf/Cloud_Computing_SRG_v1r3.pdf
- DISA STIGs: https://public.cyber.mil/stigs/
- NIST FIPS 140-2: https://csrc.nist.gov/publications/detail/fips/140/2/final
- AWS GovCloud Compliance: https://aws.amazon.com/compliance/dod/

---

## Requirements Summary

**Content Quality:**
- [ ] 400-600 lines total
- [ ] Clear distinction between IL4 and IL5 requirements
- [ ] Configuration examples use actual FraiseQL settings
- [ ] Pre-deployment checklists are comprehensive
- [ ] Validation tests are specific and actionable
- [ ] Troubleshooting section covers common issues

**Technical Accuracy:**
- [ ] DoD Impact Level descriptions are accurate
- [ ] Security configurations match RESTRICTED profile capabilities
- [ ] Network configurations follow least-privilege principle
- [ ] Audit retention matches 7-year DoD requirement
- [ ] References to standards are correct and current

---

## Verification (Orchestrator)

```bash
# Check file exists and line count
wc -l .phases/05-classified-deployment/output/CLASSIFIED_ENVIRONMENTS.md
# Should be 400-600 lines

# Verify required sections
grep -E "^## (Overview|Prerequisites|Impact Level 4|Impact Level 5|Validation|Troubleshooting)" .phases/05-classified-deployment/output/CLASSIFIED_ENVIRONMENTS.md

# Check for configuration examples
grep -c '```python\|```bash\|```hcl\|```sql' .phases/05-classified-deployment/output/CLASSIFIED_ENVIRONMENTS.md
# Should have many code blocks

# Verify checklists are present
grep -c "\- \[ \]" .phases/05-classified-deployment/output/CLASSIFIED_ENVIRONMENTS.md
# Should have multiple checklist items

# Test Markdown rendering
uv run python -m markdown .phases/05-classified-deployment/output/CLASSIFIED_ENVIRONMENTS.md > /dev/null
```

---

## Final Placement (Orchestrator)

```bash
# Create directory
mkdir -p docs/deployment

# Move to final location
cp .phases/05-classified-deployment/output/CLASSIFIED_ENVIRONMENTS.md docs/deployment/CLASSIFIED_ENVIRONMENTS.md

# Commit
git add docs/deployment/CLASSIFIED_ENVIRONMENTS.md
git commit -m "docs(deployment): add IL4/IL5 classified environment deployment guide

Add comprehensive DoD classified deployment documentation:
- Overview of Impact Level 4 (CUI) and Level 5 (Secret) requirements
- IL4 configuration (mTLS, KMS, RLS, 10 req/min rate limit)
- IL5 configuration (HSM, air-gapped, SIEM, 5 req/min rate limit)
- Air-gapped deployment procedures for IL5
- Pre-deployment checklists for both impact levels
- Security validation tests (TLS, mTLS, introspection, APQ, rate limiting)
- Troubleshooting guide for common issues
- References to DoD Cloud SRG and DISA STIGs

Impact: +0.5 point to Security Architecture score (23/25 → 23.5/25)

Refs: Pentagon-Readiness Assessment - Phase 05"
```

---

## Tips for Documentation Writer

1. **Research IL4/IL5:** Review DoD Cloud SRG to understand requirements
2. **Use FraiseQL features:** Reference actual security profiles, RLS, audit logging
3. **Be specific:** Use real configuration values, not generic placeholders
4. **Think practical:** Focus on deployment, not development or theory
5. **Checklists matter:** Make them comprehensive but realistic
6. **Validation tests:** These should actually work when someone runs them
7. **Air-gapped is different:** IL5 has unique constraints - emphasize offline deployment

---

## Success Criteria

- [ ] File created: `CLASSIFIED_ENVIRONMENTS.md`
- [ ] 400-600 lines of content
- [ ] IL4 and IL5 configurations documented
- [ ] Air-gapped deployment process for IL5
- [ ] Pre-deployment checklists for both levels
- [ ] Security validation tests included
- [ ] Troubleshooting section with common issues
- [ ] References to official DoD/NIST standards
- [ ] Written for DoD compliance officers and cleared engineers
