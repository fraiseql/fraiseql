# Security Configuration Guide

This guide covers configuring FraiseQL's enterprise-grade security features including Key Management Service (KMS) providers, security profiles, and observability.

## Quick Start

```python
from fraiseql.security import setup_security
from fastapi import FastAPI

app = FastAPI()

# Production setup with Vault KMS
security = setup_security(
    app=app,
    secret_key="your-secret-key",
    environment="production",
    kms_provider="vault",
    vault_config={
        "vault_addr": "https://vault.example.com:8200",
        "token": os.environ["VAULT_TOKEN"],
        "mount_path": "transit"
    }
)
```

## KMS Provider Setup

FraiseQL supports multiple KMS providers for envelope encryption, ensuring keys never leave the KMS while maintaining Rust pipeline performance.

### HashiCorp Vault

Production-ready with transit engine for envelope encryption.

```python
from fraiseql.security.kms import VaultKMSProvider, VaultConfig

config = VaultConfig(
    vault_addr="https://vault.example.com:8200",
    token=os.environ["VAULT_TOKEN"],
    mount_path="transit"
)
provider = VaultKMSProvider(config)

# Use with security setup
security = setup_security(
    app=app,
    secret_key="your-secret-key",
    kms_provider=provider
)
```

**Environment Variables:**
- `VAULT_ADDR`: Vault server URL
- `VAULT_TOKEN`: Authentication token
- `VAULT_CACERT`: CA certificate path (optional)

### AWS KMS

Native integration with GenerateDataKey for envelope encryption.

```python
from fraiseql.security.kms import AWSKMSProvider, AWSKMSConfig

config = AWSKMSConfig(
    region_name="us-east-1",
    key_id="alias/fraiseql-encryption-key"  # Optional: specific key
)
provider = AWSKMSProvider(config)

# Uses IAM role or profile authentication
security = setup_security(
    app=app,
    secret_key="your-secret-key",
    kms_provider=provider
)
```

**Environment Variables:**
- `AWS_REGION`: AWS region
- `AWS_PROFILE`: AWS profile (optional)
- `AWS_ACCESS_KEY_ID`: Access key (optional)
- `AWS_SECRET_ACCESS_KEY`: Secret key (optional)

### GCP Cloud KMS

Envelope encryption with Cloud KMS asymmetric keys.

```python
from fraiseql.security.kms import GCPKMSProvider, GCPKMSConfig

config = GCPKMSConfig(
    project_id="my-project",
    location="global",
    key_ring="fraiseql-keyring",
    key_id="fraiseql-encryption-key"
)
provider = GCPKMSProvider(config)

# Uses Application Default Credentials
security = setup_security(
    app=app,
    secret_key="your-secret-key",
    kms_provider=provider
)
```

**Setup:**
1. Enable Cloud KMS API
2. Create key ring and asymmetric encryption key
3. Set `GOOGLE_APPLICATION_CREDENTIALS` environment variable

### Local Provider (Development Only)

**⚠️ WARNING:** Only for development. Shows security warnings in logs.

```python
from fraiseql.security.kms import LocalKMSProvider, LocalKMSConfig

config = LocalKMSConfig(
    # Keys stored in memory only
)
provider = LocalKMSProvider(config)

security = setup_security(
    app=app,
    secret_key="dev-secret-key",
    kms_provider=provider,
    environment="development"
)
```

## Security Profiles

FraiseQL provides three security profiles for different compliance requirements:

### STANDARD Profile (Default)

Balanced security for most applications.

```python
from fraiseql.security.profiles import STANDARD_PROFILE

security = setup_security(
    app=app,
    secret_key="your-secret-key",
    security_profile=STANDARD_PROFILE
)
```

**Features:**
- Input validation enabled
- Basic rate limiting (100 req/min)
- CSRF protection for mutations
- Standard security headers
- Optional KMS encryption

### REGULATED Profile

PCI-DSS/HIPAA compliance requirements.

```python
from fraiseql.security.profiles import REGULATED_PROFILE

security = setup_security(
    app=app,
    secret_key="your-secret-key",
    security_profile=REGULATED_PROFILE,
    kms_provider=vault_provider  # Required
)
```

**Features:**
- All STANDARD features
- **Required KMS encryption** for sensitive data
- Strict rate limiting (10 req/min)
- Audit logging enabled
- Enhanced input validation
- External call restrictions

### RESTRICTED Profile

Government/defense requirements.

```python
from fraiseql.security.profiles import RESTRICTED_PROFILE

security = setup_security(
    app=app,
    secret_key="your-secret-key",
    security_profile=RESTRICTED_PROFILE,
    kms_provider=vault_provider,  # Required
    trusted_origins={"https://trusted-domain.com"}  # Required
)
```

**Features:**
- All REGULATED features
- **External calls blocked** (whitelist only)
- **Strict CSP headers**
- Enhanced audit logging
- Additional security headers

## Observability Configuration

### OpenTelemetry Tracing

```python
from fraiseql.security.tracing import TracingConfig

tracing_config = TracingConfig(
    service_name="fraiseql-api",
    sanitize_patterns=[
        r"password.*",
        r"token.*",
        r"secret.*",
        r"authorization.*"
    ]
)

security = setup_security(
    app=app,
    secret_key="your-secret-key",
    tracing_config=tracing_config
)
```

### Security Event Logging

```python
from fraiseql.security.audit import AuditConfig

audit_config = AuditConfig(
    log_level="INFO",
    include_request_body=True,
    include_response_body=False,  # For performance
    storage_backend="postgresql"  # or "file", "syslog"
)

security = setup_security(
    app=app,
    secret_key="your-secret-key",
    audit_config=audit_config
)
```

## Advanced Configuration

### Custom Rate Limiting

```python
from fraiseql.security.rate_limiting import RateLimitRule, RateLimit

custom_rules = [
    RateLimitRule(
        path_pattern="/graphql",
        rate_limit=RateLimit(requests=60, window=60),
        message="GraphQL rate limit exceeded"
    ),
    RateLimitRule(
        path_pattern="/auth/*",
        rate_limit=RateLimit(requests=5, window=300),
        message="Authentication rate limit exceeded"
    )
]

security = setup_security(
    app=app,
    secret_key="your-secret-key",
    custom_rate_limits=custom_rules
)
```

### Custom Security Headers

```python
from fraiseql.security.headers import SecurityHeadersConfig

headers_config = SecurityHeadersConfig(
    content_security_policy="default-src 'self'",
    frame_options="DENY",
    hsts_max_age=31536000,
    custom_headers={
        "X-Custom-Security": "enabled"
    }
)

security = setup_security(
    app=app,
    secret_key="your-secret-key",
    custom_security_headers=headers_config
)
```

## Environment-Specific Setup

### Production Configuration

```python
import os
from fraiseql.security import create_security_config_for_graphql

config = create_security_config_for_graphql(
    secret_key=os.environ["SECRET_KEY"],
    environment="production",
    trusted_origins={"https://app.example.com"},
    enable_introspection=False,
    redis_client=redis_client  # For distributed rate limiting
)

security = setup_security(app, **config.__dict__)
```

### Development Configuration

```python
from fraiseql.security import setup_development_security

# Simple development setup
security = setup_development_security(app)
```

## Troubleshooting

### KMS Connection Issues

**Vault Connection Failed:**
```bash
# Check Vault status
curl -H "X-Vault-Token: $VAULT_TOKEN" $VAULT_ADDR/v1/sys/health

# Verify transit engine
curl -H "X-Vault-Token: $VAULT_TOKEN" $VAULT_ADDR/v1/transit/keys
```

**AWS KMS Access Denied:**
```bash
# Check IAM permissions
aws iam simulate-principal-policy \
  --policy-source-arn arn:aws:iam::123456789012:user/MyUser \
  --action-names kms:GenerateDataKey kms:Decrypt \
  --resource arn:aws:kms:us-east-1:123456789012:key/12345678-1234-1234-1234-123456789012
```

### Security Profile Validation

```python
# Validate profile configuration
from fraiseql.security.profiles import validate_profile_config

errors = validate_profile_config(app, REGULATED_PROFILE)
if errors:
    print("Configuration errors:", errors)
```

## Migration from Basic Security

If upgrading from basic FastAPI security:

```python
# Before
from fastapi.middleware.cors import CORSMiddleware
from fastapi.middleware.trustedhost import TrustedHostMiddleware

app.add_middleware(CORSMiddleware, allow_origins=["*"])
app.add_middleware(TrustedHostMiddleware, allowed_hosts=["*"])

# After
from fraiseql.security import setup_security

security = setup_security(
    app=app,
    secret_key="your-secret-key",
    environment="production",
    trusted_origins={"https://yourdomain.com"}
)
```

## Performance Considerations

- **KMS calls**: Only at startup/rotation (not per request)
- **Local encryption**: < 1ms per operation
- **Rate limiting**: Redis recommended for multi-instance
- **Audit logging**: PostgreSQL backend for ACID compliance

## Security Checklist

- [ ] KMS provider configured for production
- [ ] Security profile matches compliance requirements
- [ ] Trusted origins configured
- [ ] Secret key is strong and rotated regularly
- [ ] Audit logging enabled for regulated environments
- [ ] Rate limiting configured appropriately
- [ ] Security headers tested with security scanner

**[🔐 Security Architecture](https://github.com/fraiseql/fraiseql/blob/main/docs/features/security-architecture.md)** • **[📋 Threat Model](https://github.com/fraiseql/fraiseql/blob/main/docs/security/threat-model.md)**
