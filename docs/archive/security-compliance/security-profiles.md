# Security Profiles Guide

**Version:** 1.0
**Last Updated:** 2025-12-08
**Audience:** DevOps engineers, security officers, compliance teams
**Time to Review:** 20-30 minutes

---

## Overview

FraiseQL provides **three pre-configured security profiles** that implement progressively stricter controls for different deployment scenarios. Each profile balances security, performance, and compliance requirements.

**Available Profiles:**
- 🟢 **STANDARD** - General-purpose applications (default)
- 🟡 **REGULATED** - Compliance-driven industries (HIPAA, PCI-DSS, SOC 2)
- 🔴 **RESTRICTED** - High-security environments (government, defense, critical infrastructure)

**Key Principle:** Security profiles provide **sensible defaults** while remaining fully customizable. Start with a profile that matches your requirements, then adjust specific controls as needed.

---

## Quick Start

### Choose Your Profile

**Answer these three questions:**

1. **Do you process sensitive personal data or payment information?**
   - No → Consider STANDARD
   - Yes → Continue to question 2

2. **Are you subject to specific compliance requirements (HIPAA, PCI-DSS, SOC 2, GDPR, ISO 27001)?**
   - No → STANDARD is sufficient
   - Yes → Continue to question 3

3. **Are you in a high-security environment (government, defense, critical infrastructure)?**
   - No → Use **REGULATED**
   - Yes → Use **RESTRICTED**

### Basic Configuration

```python
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.security.profiles import SecurityProfile

# STANDARD profile (default)
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.STANDARD
)

# REGULATED profile (compliance-driven)
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.REGULATED,
    kms_provider="aws",  # Required for REGULATED
    audit_enabled=True   # Required for REGULATED
)

# RESTRICTED profile (high-security)
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.RESTRICTED,
    kms_provider="vault",  # HSM-backed recommended
    audit_enabled=True,
    audit_retention_days=2555,  # 7 years
    mtls_enabled=True,
    ip_allowlist=["10.0.0.0/8"]
)
```

---

## Profile Comparison

### Feature Matrix

| Feature | STANDARD | REGULATED | RESTRICTED |
|---------|----------|-----------|------------|
| **Target Environment** | General apps, internal tools | Healthcare, finance, SaaS | Government, defense, CII |
| **Compliance Support** | Best practices | HIPAA, PCI-DSS, SOC 2, GDPR, ISO 27001 | FedRAMP, NIST 800-53, NIS2 Essential, DoD IL4/IL5 |
| **Setup Time** | < 5 minutes | 15-30 minutes | 1-2 hours |
| **Performance Overhead** | ~5% | ~10-15% | ~20-25% |

### Security Controls

| Control | STANDARD | REGULATED | RESTRICTED |
|---------|----------|-----------|------------|
| **Authentication** | ✅ Required | ✅ Required | ✅ Required |
| **Multi-Factor Auth** | ⚠️ Optional | ✅ Required | ✅ Required |
| **TLS Version** | 1.2+ | 1.2+ | **1.3 only** |
| **Mutual TLS (mTLS)** | ❌ No | ⚠️ Optional | ✅ Required |
| **Session Timeout** | 60 minutes | 15 minutes | **5 minutes** |
| **GraphQL Introspection** | Authenticated | **Disabled** | **Disabled** |
| **Query Depth Limit** | 15 levels | 10 levels | **5 levels** |
| **Query Complexity** | 1000 | 1000 | **500** |
| **Rate Limit (per min)** | 100 requests | 50 requests | **10 requests** |
| **Request Body Size** | 1 MB | 1 MB | **512 KB** |
| **Audit Logging** | Standard | **Enhanced** | **Verbose** |
| **Field-Level Audit** | ❌ No | ✅ Yes | ✅ Yes |
| **Error Details** | Safe | Safe | **Minimal** |
| **KMS Integration** | Optional | ✅ Required | ✅ Required (HSM) |
| **IP Allowlisting** | ❌ No | ⚠️ Optional | ✅ Required |

**Legend:**
- ✅ Enabled/Required
- ⚠️ Optional (recommended)
- ❌ Disabled/Not required

---

## STANDARD Profile

### When to Use

✅ **Ideal for:**
- Internal applications with trusted users
- Development and staging environments
- Applications without sensitive data
- Prototypes and MVPs
- Non-regulated industries

❌ **Not suitable for:**
- Processing payment card data
- Handling protected health information (PHI)
- SOC 2 Type II compliance
- Government/defense applications

### Key Features

**Authentication & Access Control:**
- JWT-based authentication required
- Session timeout: 60 minutes
- RBAC with PostgreSQL roles
- Row-level security (RLS) available
- Field-level authorization

**Network Security:**
- HTTPS recommended (not enforced)
- TLS 1.2+ support
- Permissive CORS (configure for production)
- Rate limiting: 100 requests/minute

**Input Validation:**
- GraphQL query depth limit: 15 levels
- Query complexity limit: 1000
- Request body size: 1 MB
- SQL injection prevention (architecture)

**Monitoring:**
- Standard logging
- Optional audit logging
- Distributed tracing with OpenTelemetry
- PII sanitization in logs

### Configuration Example

```python
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.security.profiles import SecurityProfile

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.STANDARD,

    # Optional: Customize specific controls
    cors_origins=[
        "https://app.yourcompany.com",
        "https://admin.yourcompany.com"
    ],
    rate_limit_requests_per_minute=200,  # Higher for internal apps
    enable_tracing=True,
    tracing_endpoint="http://jaeger:4318/v1/traces"
)
```

### Performance Impact

- **Latency:** ~5% overhead compared to no security controls
- **Throughput:** 95% of baseline
- **Memory:** +50MB for security middleware

---

## REGULATED Profile

### When to Use

✅ **Ideal for:**
- Healthcare applications (HIPAA compliance)
- Payment processing (PCI-DSS compliance)
- SaaS applications requiring SOC 2 Type II
- Financial services applications
- GDPR-compliant applications
- ISO 27001 certified environments
- Applications handling sensitive personal data

❌ **Not suitable for:**
- Air-gapped deployments
- Classified data (CUI, Secret, etc.)
- DoD contractors requiring IL4/IL5
- Critical infrastructure (NIS2 Essential Entities)

### Key Features

**Authentication & Access Control:**
- JWT-based authentication required
- **Multi-factor authentication (MFA) required**
- Session timeout: **15 minutes**
- RBAC with role hierarchy
- Row-level security (RLS) enforced
- Field-level authorization with audit

**Network Security:**
- **HTTPS enforced** (no HTTP allowed)
- TLS 1.2+ required
- Restrictive CORS policies
- Rate limiting: **50 requests/minute**
- Optional: IP allowlisting
- Optional: Mutual TLS (mTLS)

**Encryption:**
- **KMS integration required** (AWS KMS, Azure Key Vault, GCP KMS, HashiCorp Vault)
- Envelope encryption for sensitive fields
- Key rotation: 30 days
- Encryption at rest and in transit

**Input Validation:**
- GraphQL query depth limit: **10 levels**
- Query complexity limit: 1000
- Request body size: 1 MB
- **GraphQL introspection disabled**

**Audit & Monitoring:**
- **Comprehensive audit logging required**
- **Field-level access tracking**
- Immutable audit trails
- Change Data Capture (CDC)
- Before/after snapshots
- Log retention: **365 days** (1 year minimum)
- Real-time security alerts

### Configuration Example

```python
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.security.profiles import SecurityProfile

# AWS KMS configuration
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.REGULATED,

    # Required: KMS provider
    kms_provider="aws",
    kms_config={
        "region": "us-east-1",
        "key_id": "arn:aws:kms:us-east-1:123456789012:key/12345678-1234-1234-1234-123456789012",
        "key_rotation_days": 30
    },

    # Required: Audit logging
    audit_enabled=True,
    audit_retention_days=365,  # 1 year for SOC 2
    audit_field_access=True,

    # Required: MFA enforcement
    require_mfa=True,
    mfa_providers=["totp", "webauthn"],

    # CORS configuration
    cors_origins=[
        "https://app.yourcompany.com"  # Restrictive origins only
    ],

    # Optional: Enhanced monitoring
    enable_real_time_alerts=True,
    alert_webhook_url="https://security.yourcompany.com/webhooks/fraiseql"
)
```

### Azure Key Vault Configuration

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.REGULATED,

    kms_provider="azure",
    kms_config={
        "vault_url": "https://yourkeyvault.vault.azure.net/",
        "key_name": "fraiseql-master-key",
        "credential": "DefaultAzureCredential"  # Use managed identity
    },

    audit_enabled=True,
    audit_retention_days=2555,  # 7 years for HIPAA
    require_mfa=True
)
```

### HashiCorp Vault Configuration

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.REGULATED,

    kms_provider="vault",
    kms_config={
        "vault_addr": "https://vault.yourcompany.com:8200",
        "vault_namespace": "production",
        "transit_mount": "transit",
        "key_name": "fraiseql-master-key",
        "token": "${VAULT_TOKEN}"  # Use environment variable
    },

    audit_enabled=True,
    audit_retention_days=2555,
    require_mfa=True
)
```

### Performance Impact

- **Latency:** ~10-15% overhead (primarily from KMS calls and audit logging)
- **Throughput:** 85-90% of baseline
- **Memory:** +150MB for audit buffers and KMS caches
- **Optimization:** Enable KMS data key caching to reduce latency

### Compliance Mapping

| Compliance Framework | Supported | Notes |
|----------------------|-----------|-------|
| **HIPAA** | ✅ Yes | Requires BAA with hosting provider |
| **PCI-DSS Level 2** | ✅ Yes | Full environment assessment required |
| **SOC 2 Type II** | ✅ Yes | Organizational controls needed |
| **GDPR** | ✅ Yes | Implement consent management |
| **ISO 27001** | ✅ Yes | Add certification audit prep |
| **NIS2 Important** | ✅ Yes | Incident response procedures needed |

See [Compliance Matrix](./compliance-matrix.md) for detailed control mapping.

---

## RESTRICTED Profile

### When to Use

✅ **Ideal for:**
- Government and defense applications
- Federal agencies (FedRAMP High, DoD IL4/IL5)
- Critical infrastructure (NIS2 Essential Entities)
- Banking and financial critical systems
- Classified data (CUI, Secret)
- Air-gapped deployments
- Zero-trust architecture environments
- Singapore CII operators
- Australian Essential Eight Maturity Level 3
- Canadian defence contractors (CPCSC)

❌ **Not suitable for:**
- General SaaS applications (too restrictive)
- High-throughput public APIs (rate limits too strict)
- Prototypes and MVPs (setup complexity)

### Key Features

**Authentication & Access Control:**
- JWT-based authentication required
- **Multi-factor authentication (MFA) required**
- Session timeout: **5 minutes** (very short)
- RBAC with principle of least privilege
- Row-level security (RLS) enforced
- Field-level authorization with audit
- **Zero-trust network policies**

**Network Security:**
- **HTTPS enforced (TLS 1.3 only)**
- **Mutual TLS (mTLS) required** (client certificates)
- **IP allowlisting required**
- Very restrictive CORS policies
- Rate limiting: **10 requests/minute** (very strict)
- Network segmentation enforced
- Optional: Air-gapped deployment support

**Encryption:**
- **HSM-backed KMS required** (FIPS 140-2 Level 3)
- Envelope encryption for all sensitive fields
- Key rotation: **7 days** (weekly)
- Certificate pinning enabled
- Encryption context (AAD) required

**Input Validation:**
- GraphQL query depth limit: **5 levels** (very strict)
- Query complexity limit: **500** (half of standard)
- Request body size: **512 KB** (half of standard)
- **GraphQL introspection disabled**

**Audit & Monitoring:**
- **Verbose audit logging required**
- **Field-level access tracking**
- **Immutable audit trails with cryptographic chains**
- Tamper-proof event hashing
- Log retention: **2555 days (7 years)**
- **Real-time anomaly detection**
- Security Operations Center (SOC) integration

**Infrastructure:**
- Non-root container user enforced
- Read-only filesystem required
- Resource limits enforced
- Container scanning with zero critical vulnerabilities
- SBOM generation and verification

### Configuration Example

```python
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.security.profiles import SecurityProfile

# HSM-backed Vault configuration for RESTRICTED
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.RESTRICTED,

    # Required: HSM-backed KMS
    kms_provider="vault",
    kms_config={
        "vault_addr": "https://vault.internal:8200",
        "vault_namespace": "classified",
        "transit_mount": "transit",
        "key_name": "fraiseql-restricted-key",
        "token": "${VAULT_TOKEN}",
        "seal_type": "pkcs11",  # HSM-backed
        "key_rotation_days": 7
    },

    # Required: Comprehensive audit logging
    audit_enabled=True,
    audit_retention_days=2555,  # 7 years
    audit_field_access=True,
    audit_crypto_chain=True,  # Cryptographic integrity

    # Required: MFA enforcement
    require_mfa=True,
    mfa_providers=["webauthn", "hardware_token"],  # No TOTP allowed

    # Required: Network restrictions
    mtls_enabled=True,
    mtls_client_ca_cert="/path/to/client_ca.crt",
    ip_allowlist=["10.0.0.0/8"],  # Internal network only

    # Required: Strict rate limiting
    rate_limit_requests_per_minute=10,

    # CORS configuration (very restrictive)
    cors_origins=[
        "https://classified.internal.gov"  # Single trusted origin
    ],

    # Required: Real-time monitoring
    enable_real_time_alerts=True,
    enable_anomaly_detection=True,
    soc_integration_url="https://soc.internal.gov/api/alerts",

    # Required: Error handling
    error_detail_level="minimal",  # No stack traces

    # Optional: Air-gapped deployment
    air_gapped=True,
    offline_sbom_path="/opt/fraiseql/sbom.json"
)
```

### Air-Gapped Deployment

For classified environments without internet access:

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.RESTRICTED,

    # Air-gapped configuration
    air_gapped=True,

    # Use local HSM
    kms_provider="local_hsm",
    kms_config={
        "hsm_type": "pkcs11",
        "library_path": "/usr/lib/libsofthsm2.so",
        "slot": 0,
        "key_label": "fraiseql-master-key"
    },

    # Offline SBOM verification
    offline_sbom_path="/opt/fraiseql/sbom.json",
    verify_sbom_signature=True,
    sbom_public_key_path="/opt/fraiseql/sbom.pub",

    # No external services
    enable_tracing=False,
    enable_external_alerts=False,

    audit_enabled=True,
    audit_retention_days=2555,
    require_mfa=True,
    mtls_enabled=True,
    ip_allowlist=["192.168.0.0/16"]
)
```

### Performance Impact

- **Latency:** ~20-25% overhead (from mTLS, HSM calls, cryptographic chains)
- **Throughput:** 75-80% of baseline
- **Memory:** +300MB for crypto operations, audit buffers, and anomaly detection
- **Trade-off:** Security over performance

**Optimization Tips:**
- Use HSM with hardware acceleration
- Enable aggressive caching (with encryption)
- Deploy multiple instances behind load balancer
- Pre-warm connections and crypto contexts

### Compliance Mapping

| Compliance Framework | Supported | Notes |
|----------------------|-----------|-------|
| **FedRAMP High** | ✅ Yes | Agency assessment required |
| **NIST 800-53 High** | ✅ Yes | All controls implemented |
| **DoD IL4** | ✅ Yes | NIST 800-171 compliant |
| **DoD IL5** | ✅ Yes | CMMC Level 3 compliant |
| **NIS2 Essential** | ✅ Yes | 24h incident reporting needed |
| **PCI-DSS Level 1** | ✅ Yes | Quarterly scans, annual pentests |
| **AU Essential Eight ML3** | ✅ Yes | All 8 strategies implemented |
| **SG CII** | ✅ Yes | All protection requirements met |
| **CA CPCSC** | ✅ Yes | Defence contractor certified |
| **UK NCSC High** | ✅ Yes | High-security guidance compliant |

See [Compliance Matrix](./compliance-matrix.md) for detailed control mapping.

---

## Profile Selection Decision Tree

```
START: What type of application are you deploying?

┌─────────────────────────────────────────────────────────────┐
│ Do you handle classified data or work for government/       │
│ defense/critical infrastructure?                            │
└─────────────────┬───────────────────────────────────────────┘
                  │
         ┌────────┴────────┐
         │ YES             │ NO
         │                 │
         v                 v
    ┌────────┐      ┌──────────────────────────────────────────┐
    │ RESTRICT│      │ Do you need compliance certification?     │
    │ ED      │      │ (HIPAA, PCI-DSS, SOC 2, ISO 27001, etc.) │
    └────────┘      └───────────┬──────────────────────────────┘
                                │
                       ┌────────┴────────┐
                       │ YES             │ NO
                       │                 │
                       v                 v
                  ┌─────────┐      ┌──────────────────────────┐
                  │REGULATED│      │ Do you process sensitive  │
                  └─────────┘      │ personal data (PII, PHI,  │
                                   │ payment cards)?           │
                                   └───────┬──────────────────┘
                                           │
                                  ┌────────┴────────┐
                                  │ YES             │ NO
                                  │                 │
                                  v                 v
                             ┌─────────┐      ┌─────────┐
                             │REGULATED│      │ STANDARD│
                             └─────────┘      └─────────┘
```

---

## Customizing Profiles

### Override Specific Controls

You can start with a profile and customize individual controls:

```python
from fraiseql.security.profiles import get_profile, SecurityProfile

# Start with REGULATED profile
base_config = get_profile(SecurityProfile.REGULATED)

# Customize specific controls
custom_config = base_config.copy()
custom_config.rate_limit_requests_per_minute = 200  # Increase rate limit
custom_config.token_expiry_minutes = 30  # Extend session

# Apply custom configuration
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_config=custom_config,
    kms_provider="aws",
    audit_enabled=True
)
```

### Profile Templates by Use Case

#### E-Commerce Platform (PCI-DSS Level 2)

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.REGULATED,

    # PCI-DSS specific
    kms_provider="aws",
    audit_retention_days=365,
    require_mfa=True,

    # Field-level encryption for cardholder data
    encrypted_fields=["credit_card_number", "cvv"],

    # Quarterly vulnerability scans
    enable_vulnerability_scanning=True,
    scan_schedule="0 0 1 */3 *"  # First day of quarter
)
```

#### Healthcare Application (HIPAA)

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.REGULATED,

    # HIPAA specific
    kms_provider="azure",
    audit_retention_days=2555,  # 7 years
    require_mfa=True,

    # PHI encryption
    encrypted_fields=["ssn", "medical_record_number", "diagnosis"],

    # BAA compliance
    baa_enabled=True,
    baa_provider="azure"
)
```

#### SaaS Platform (SOC 2 Type II)

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.REGULATED,

    # SOC 2 specific
    kms_provider="vault",
    audit_retention_days=365,
    require_mfa=True,

    # Multi-tenant isolation
    enable_rls=True,
    tenant_isolation_column="tenant_id",

    # Continuous monitoring
    enable_real_time_alerts=True,
    enable_compliance_reporting=True
)
```

#### Defense Contractor Application (DoD IL4)

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_profile=SecurityProfile.RESTRICTED,

    # DoD IL4 specific (NIST 800-171)
    kms_provider="vault",
    kms_config={
        "seal_type": "pkcs11",  # HSM required
        "key_rotation_days": 7
    },
    audit_retention_days=2555,
    audit_crypto_chain=True,
    require_mfa=True,
    mfa_providers=["webauthn", "piv_card"],

    # NIST 800-171 controls
    mtls_enabled=True,
    ip_allowlist=["192.168.0.0/16"],
    enable_anomaly_detection=True,

    # CUI marking and handling
    data_classification_enabled=True,
    cui_fields=["controlled_technical_info", "export_controlled_data"]
)
```

---

## Migration Between Profiles

### Upgrading from STANDARD to REGULATED

**Timeline:** 1-2 weeks

**Steps:**

1. **Assess Current State** (1-2 days)
   ```bash
   fraiseql security audit
   ```

2. **Set Up KMS** (1-2 days)
   - Create KMS key in cloud provider
   - Configure encryption policies
   - Test key access from application

3. **Enable Audit Logging** (1 day)
   ```bash
   fraiseql audit init --profile regulated
   fraiseql migrate up
   ```

4. **Configure MFA** (1-2 days)
   - Integrate external IdP (Auth0, Okta, Cognito)
   - Test MFA workflows
   - Roll out to users

5. **Update Application Config** (1 day)
   ```python
   # Old (STANDARD)
   app = create_fraiseql_app(
       database_url="postgresql://localhost/mydb"
   )

   # New (REGULATED)
   app = create_fraiseql_app(
       database_url="postgresql://localhost/mydb",
       security_profile=SecurityProfile.REGULATED,
       kms_provider="aws",
       audit_enabled=True,
       require_mfa=True
   )
   ```

6. **Test in Staging** (2-3 days)
   - Verify all features work
   - Check performance impact
   - Test MFA flows
   - Review audit logs

7. **Deploy to Production** (1 day)
   - Blue-green deployment
   - Monitor for issues
   - Verify compliance controls

### Upgrading from REGULATED to RESTRICTED

**Timeline:** 2-4 weeks

**Steps:**

1. **Assess Requirements** (3-5 days)
   ```bash
   fraiseql compliance check --standard [fedramp-high|nist-800-53-high|dod-il4]
   ```

2. **Set Up HSM** (3-5 days)
   - Provision HSM or HSM-backed Vault
   - Configure PKCS#11 integration
   - Test cryptographic operations

3. **Implement mTLS** (2-3 days)
   - Generate client certificates
   - Configure client CA
   - Update infrastructure (load balancer, ingress)

4. **Configure Network Restrictions** (1-2 days)
   - Set up IP allowlists
   - Configure firewall rules
   - Test network policies

5. **Enable Cryptographic Audit Chain** (1-2 days)
   ```bash
   fraiseql audit upgrade --crypto-chain
   ```

6. **Configure Anomaly Detection** (2-3 days)
   - Set up baseline behavior
   - Configure alert thresholds
   - Integrate with SOC

7. **Update Application Config** (1 day)
   ```python
   # Old (REGULATED)
   app = create_fraiseql_app(
       database_url="postgresql://localhost/mydb",
       security_profile=SecurityProfile.REGULATED,
       kms_provider="aws",
       audit_enabled=True,
       require_mfa=True
   )

   # New (RESTRICTED)
   app = create_fraiseql_app(
       database_url="postgresql://localhost/mydb",
       security_profile=SecurityProfile.RESTRICTED,
       kms_provider="vault",
       kms_config={"seal_type": "pkcs11"},
       audit_enabled=True,
       audit_crypto_chain=True,
       require_mfa=True,
       mtls_enabled=True,
       ip_allowlist=["10.0.0.0/8"],
       enable_anomaly_detection=True
   )
   ```

8. **Penetration Testing** (1 week)
   - Hire external security firm
   - Fix identified vulnerabilities
   - Retest

9. **Deploy to Production** (1 day)
   - Staged rollout
   - Monitor closely
   - Verify all controls active

---

## Testing Security Profiles

### Verify Profile Configuration

```python
from fraiseql.security.profiles import get_profile, SecurityProfile

# Get current profile
profile = get_profile(SecurityProfile.REGULATED)

# Verify settings
print(f"TLS Required: {profile.tls_required}")
print(f"Session Timeout: {profile.token_expiry_minutes} minutes")
print(f"Introspection: {profile.introspection_policy.value}")
print(f"Audit Level: {profile.audit_level.value}")

# Export configuration
import json
print(json.dumps(profile.to_dict(), indent=2))
```

### Automated Security Testing

```bash
# Run security audit
fraiseql security audit

# Check compliance posture
fraiseql compliance check --profile regulated

# Test specific controls
fraiseql security test --controls auth,encryption,audit

# Generate security report
fraiseql security report --output security-report.pdf
```

### Integration Tests

```python
import pytest
from fraiseql.testing import SecurityTester

@pytest.fixture
def security_tester(app):
    return SecurityTester(app)

def test_mfa_required(security_tester):
    """Test that MFA is enforced in REGULATED profile."""
    response = security_tester.login(
        username="test@example.com",
        password="password123"
        # No MFA token provided
    )
    assert response.status_code == 403
    assert "MFA required" in response.json()["error"]

def test_introspection_disabled(security_tester):
    """Test that introspection is disabled in REGULATED profile."""
    response = security_tester.query(
        "{ __schema { types { name } } }"
    )
    assert response.status_code == 403
    assert "Introspection disabled" in response.json()["error"]

def test_rate_limiting(security_tester):
    """Test rate limiting in REGULATED profile."""
    # Make 51 requests (limit is 50/min)
    for i in range(51):
        response = security_tester.query("{ user { id } }")
        if i < 50:
            assert response.status_code == 200
        else:
            assert response.status_code == 429  # Too Many Requests
```

---

## Troubleshooting

### Common Issues

#### Issue: "KMS key not found"

**Cause:** KMS provider not configured correctly

**Solution:**
```python
# Verify KMS configuration
from fraiseql.kms import test_kms_connection

result = test_kms_connection(
    provider="aws",
    config={"region": "us-east-1", "key_id": "..."}
)
if not result.success:
    print(f"KMS Error: {result.error}")
```

#### Issue: "mTLS certificate validation failed"

**Cause:** Client certificate not trusted by server CA

**Solution:**
```bash
# Verify client certificate
openssl verify -CAfile ca.crt client.crt

# Check certificate chain
openssl s_client -connect api.example.com:443 \
  -cert client.crt -key client.key -CAfile ca.crt
```

#### Issue: "Audit logging not writing events"

**Cause:** Audit table not initialized or permissions issue

**Solution:**
```bash
# Initialize audit infrastructure
fraiseql audit init

# Run migrations
fraiseql migrate up

# Check table exists
psql -c "SELECT COUNT(*) FROM audit_events;"

# Verify permissions
fraiseql audit test
```

#### Issue: "Rate limit too strict, blocking legitimate traffic"

**Cause:** Default RESTRICTED rate limit (10/min) too low for use case

**Solution:**
```python
# Override rate limit for specific use case
from fraiseql.security.profiles import get_profile, SecurityProfile

config = get_profile(SecurityProfile.RESTRICTED)
config.rate_limit_requests_per_minute = 50  # Increase from 10 to 50

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    security_config=config,
    # ... other settings
)
```

#### Issue: "Performance degraded after enabling RESTRICTED profile"

**Cause:** HSM operations and cryptographic chains add latency

**Solutions:**
1. **Enable KMS data key caching:**
   ```python
   kms_config={
       "enable_data_key_caching": True,
       "cache_ttl_seconds": 300
   }
   ```

2. **Use connection pooling:**
   ```python
   database_url="postgresql://localhost/mydb?pool_size=20&max_overflow=10"
   ```

3. **Deploy multiple instances:**
   ```bash
   # Scale horizontally
   kubectl scale deployment fraiseql --replicas=5
   ```

---

## Related Documentation

- **[Compliance Matrix](./compliance-matrix.md)** - Detailed compliance requirements mapping
- **[Security Controls Matrix](../security/controls-matrix.md)** - Technical control implementation
- **[Production Security](../production/security.md)** - Security best practices
- **[KMS Architecture](../architecture/decisions/0003-kms-architecture.md)** - Key management design
- **[Threat Model](../security/threat-model.md)** - Security risk assessment

---

## Frequently Asked Questions

**Q: Can I use STANDARD profile in production?**

A: Yes, for non-regulated applications with non-sensitive data. However, we recommend REGULATED profile for any production application handling user data.

**Q: How much does KMS integration cost?**

A: KMS costs vary by provider:
- AWS KMS: ~$1/month per key + $0.03 per 10,000 requests
- Azure Key Vault: ~$0.03 per 10,000 operations
- GCP KMS: ~$0.03 per 10,000 operations
- HashiCorp Vault: Self-hosted (infrastructure costs)

**Q: Can I switch profiles without downtime?**

A: Switching from STANDARD → REGULATED or REGULATED → RESTRICTED requires configuration changes (KMS setup, audit initialization) that need planned downtime. Use blue-green deployment to minimize impact.

**Q: Do I need to rebuild my application to change profiles?**

A: No. Security profiles are configuration-only. Change the profile setting and redeploy.

**Q: Can I mix profiles (e.g., REGULATED for API, STANDARD for admin)?**

A: No. One application instance uses one profile. Deploy separate instances if you need different security levels for different endpoints.

**Q: How do I prove compliance to auditors?**

A: Use the compliance reporting tools:
```bash
fraiseql compliance report --profile regulated --output compliance-report.pdf
```

Provide auditors with:
1. Compliance report (PDF)
2. Security audit results
3. This documentation
4. Test results from integration tests

---

**For Questions or Support:**
- **Email:** security@fraiseql.com
- **Enterprise Support:** Available for REGULATED/RESTRICTED deployments
- **GitHub Discussions:** Community support for configuration questions

---

*This guide provides comprehensive coverage of FraiseQL's security profiles. For specific compliance requirements, consult the [Compliance Matrix](./compliance-matrix.md). For implementation details, see [Production Security](../production/security.md).*
