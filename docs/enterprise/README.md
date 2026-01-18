# FraiseQL v2 Enterprise Features

Enterprise-grade security, compliance, and audit capabilities.

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
