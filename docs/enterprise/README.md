# FraiseQL v2 Enterprise Features

Enterprise-grade security, compliance, and audit capabilities for production deployments.

**Phase 7 (v2.0.0)** adds comprehensive runtime security hardening including error sanitization, rate limiting, token protection, and encrypted state management.

---

## 🔐 Phase 7: Runtime Security Features

Configured via `fraiseql.toml` with environment variable overrides.

### Error Sanitization

**Hide implementation details from client errors**, preventing information leakage:

- `error_sanitization.enabled` — Enable/disable error message masking
- `error_sanitization.level` — Control verbosity (internal/user/public)
- Client receives generic "An error occurred" instead of SQL details
- Server logs full errors for debugging

**Example:**

```toml
[fraiseql.security.error_sanitization]
enabled = true
level = "user"  # Only expose user-friendly messages to clients
```

### Constant-Time Token Comparison

**Prevent timing attacks** on token validation:

- Token comparison uses constant-time algorithm (bitwise operations)
- Attack duration independent of token position in storage
- Prevents brute-force attacks via timing analysis
- Automatic for all authentication tokens

### PKCE State Encryption

**Protect OAuth state parameters** from inspection:

- State parameter encrypted before transmission
- Prevents state parameter tampering
- REQUIRED for public clients (SPAs, mobile apps)
- Configurable encryption algorithm (AES-256 default)

**Example:**

```toml
[fraiseql.security.pkce]
state_encryption_enabled = true
encryption_algorithm = "aes-256-gcm"
```

### Rate Limiting

**Brute-force protection** on authentication endpoints:

- Per-IP rate limiting (configurable window)
- Per-user rate limiting (account lockout protection)
- Exponential backoff on repeated failures
- Configurable thresholds per endpoint

**Example:**

```toml
[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
auth_verify_max_requests = 50
auth_verify_window_secs = 60
```

### Audit Logging (v2.0)

**Track secret access** with cryptographic verification:

- Log all authentication events (login, token refresh, failures)
- Log all data mutations (create, update, delete)
- HMAC signatures prevent tampering
- Queryable audit trail for compliance

### Full Configuration Reference

All Phase 7 features are configured in `fraiseql.toml` under `[fraiseql.security]`:

```toml
[fraiseql.security]
# Error handling
[fraiseql.security.error_sanitization]
enabled = true
level = "user"  # internal|user|public

# Token security
[fraiseql.security.constant_time_comparison]
enabled = true

# OAuth security
[fraiseql.security.pkce]
state_encryption_enabled = true
encryption_algorithm = "aes-256-gcm"

# Rate limiting
[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
auth_verify_max_requests = 50
auth_verify_window_secs = 60

# Audit logging
[fraiseql.security.audit_logging]
enabled = true
log_level = "info"
secret_access_logging = true
```

Environment variable overrides (production):

```bash
# Rate limiting per environment
FRAISEQL_RATE_LIMITING_ENABLED=true
FRAISEQL_RATE_LIMITING_AUTH_START_MAX_REQUESTS=50  # Stricter in prod

# Audit logging
FRAISEQL_AUDIT_LOGGING_ENABLED=true
FRAISEQL_AUDIT_LOGGING_LEVEL=debug
```

See [`.claude/CLAUDE.md`](../../.claude/CLAUDE.md) for full configuration management details.

---

## 🔒 Enterprise Features Overview

### Access Control

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [rbac.md](rbac.md) | Role-Based Access Control | 844 | 60 min |

**Topics Covered:**

- Hierarchical role system
- Field-level permissions
- Row-level security
- Authorization enforcement layers
- JWT claims integration
- Dynamic role assignment

---

### Audit & Compliance

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [audit-logging.md](audit-logging.md) | Cryptographic audit trails | 887 | 60 min |

**Topics Covered:**

- Immutable audit log
- HMAC signature chains
- Tamper detection
- Audit columns (created_at, updated_at, deleted_at)
- Compliance with GDPR, SOC2, NIS2
- Retention policies

---

### Data Protection

| Document | Description | Lines | Est. Time |
|----------|-------------|-------|-----------|
| [kms.md](kms.md) | Key Management Service integration | 854 | 50 min |

**Topics Covered:**

- Field-level encryption
- AWS KMS integration
- Azure Key Vault integration
- Google Cloud KMS integration
- Key rotation strategies
- Encryption at rest and in transit

---

## 🎯 Quick Start

**For Security Engineers:**

1. Read [rbac.md](rbac.md) for access control design
2. Review [audit-logging.md](audit-logging.md) for compliance requirements
3. Configure [kms.md](kms.md) for data encryption

**For Compliance Teams:**

1. Start with [audit-logging.md](audit-logging.md)
2. Review security profiles in [Specs: Security Compliance](../specs/security-compliance.md)
3. Understand RBAC enforcement in [rbac.md](rbac.md)

---

## 📚 Related Documentation

- **[Architecture: Security](../architecture/security/)** — Security model and authentication
- **[Specs: Security Compliance](../specs/security-compliance.md)** — Security profiles (STANDARD, REGULATED, RESTRICTED)
- **[Guides: Production Deployment](../guides/production-deployment.md)** — Security hardening checklist

---

## ✅ Compliance Standards Supported

- **GDPR** — Data protection and privacy
- **SOC 2** — Security, availability, confidentiality
- **NIS2** — EU cybersecurity directive
- **HIPAA** — Healthcare data protection (with proper configuration)
- **PCI DSS** — Payment card data security (with proper configuration)

---

**Back to:** [Documentation Home](../README.md)
