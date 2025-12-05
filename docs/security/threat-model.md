# FraiseQL Security Threat Model

**Version**: 1.0
**Last Updated**: 2025-11-24
**Status**: Active

---

## Executive Summary

This document provides a comprehensive threat model for FraiseQL, a high-performance GraphQL framework with Rust-accelerated JSON processing. The threat model identifies assets, potential threats, attack vectors, and corresponding mitigations across the entire application stack.

---

## System Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      External Actors                         │
│  (Authenticated Users, API Clients, Attackers)               │
└────────────────────┬────────────────────────────────────────┘
                     │ HTTPS/TLS
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                   FastAPI Application                        │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Security Middleware Stack                              │ │
│  │  - Rate Limiting                                        │ │
│  │  - CSRF Protection                                      │ │
│  │  - Body Size Validation                                 │ │
│  │  - Security Headers                                     │ │
│  └────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  GraphQL Layer (Strawberry)                             │ │
│  │  - Query Parsing                                        │ │
│  │  - Input Validation                                     │ │
│  │  - Field Authorization                                  │ │
│  └────────────────────────────────────────────────────────┘ │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│              Rust Pipeline (fraiseql_rs)                     │
│  - Zero-copy JSON transformation (6-17ms)                    │
│  - No Python overhead                                        │
│  - Memory-safe operations                                    │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────┐
│                   PostgreSQL Database                        │
│  - Row-Level Security (RLS)                                  │
│  - Stored Functions (SECURITY DEFINER)                       │
│  - Audit Logging                                             │
└─────────────────────────────────────────────────────────────┘

        ┌───────────────────────────────────┐
        │  External KMS Providers            │
        │  - HashiCorp Vault                 │
        │  - AWS KMS                         │
        │  - GCP Cloud KMS                   │
        └───────────────────────────────────┘
```

---

## Assets

### 1. Data Assets

| Asset | Sensitivity | Impact if Compromised |
|-------|-------------|----------------------|
| **User PII** | HIGH | Identity theft, privacy violation, regulatory fines |
| **Authentication Tokens** | CRITICAL | Unauthorized access, session hijacking |
| **Database Credentials** | CRITICAL | Full data breach, data manipulation |
| **API Keys** | HIGH | Unauthorized API access, cost overruns |
| **Encryption Keys (DEKs)** | CRITICAL | Data decryption, loss of confidentiality |
| **GraphQL Schemas** | MEDIUM | Information disclosure, attack surface mapping |
| **Audit Logs** | MEDIUM | Evidence tampering, compliance violations |

### 2. System Assets

| Asset | Impact if Compromised |
|-------|----------------------|
| **FastAPI Application** | Service disruption, data breach |
| **Rust Pipeline** | Performance degradation, memory corruption |
| **PostgreSQL Database** | Complete data loss or corruption |
| **KMS Provider Connection** | Loss of encryption capabilities |

### 3. Configuration Assets

| Asset | Sensitivity | Storage Location |
|-------|-------------|------------------|
| **Security Profiles** | MEDIUM | Application config |
| **KMS Provider Config** | HIGH | Environment variables |
| **Database Connection Strings** | CRITICAL | Environment variables / Vault |
| **TLS Certificates** | HIGH | Filesystem / Secret manager |

---

## Trust Boundaries

### Boundary 1: External Network → FastAPI
- **Protection**: TLS/HTTPS encryption, rate limiting, WAF
- **Trust Level**: UNTRUSTED
- **Validation**: All input validated and sanitized

### Boundary 2: FastAPI → GraphQL Layer
- **Protection**: Authentication, authorization, query validation
- **Trust Level**: SEMI-TRUSTED (authenticated users)
- **Validation**: Query depth, complexity, field-level permissions

### Boundary 3: GraphQL → Rust Pipeline
- **Protection**: Type safety, memory safety, bounds checking
- **Trust Level**: TRUSTED (internal)
- **Validation**: JSON schema validation

### Boundary 4: Application → PostgreSQL
- **Protection**: Parameterized queries, RLS, connection pooling
- **Trust Level**: TRUSTED
- **Validation**: SQL injection prevention, stored function contracts

### Boundary 5: Application → KMS Provider
- **Protection**: Mutual TLS, API authentication, envelope encryption
- **Trust Level**: SEMI-TRUSTED (external service)
- **Validation**: Certificate pinning, request signing

---

## Threat Analysis

### T1: Unauthorized Access to Encryption Keys

**Description**: Attacker gains access to Data Encryption Keys (DEKs) stored in memory.

**Attack Vectors**:
- Memory dump from compromised application server
- Side-channel attacks (timing, cache)
- Debugging interface exploitation
- Container escape to host memory

**Impact**: CRITICAL
- Decryption of all data encrypted with compromised DEK
- Loss of confidentiality for sensitive data

**Mitigations**:
- ✅ DEKs stored in memory only (never on disk)
- ✅ Periodic key rotation via background task
- ✅ Memory protection via OS-level security (DEP, ASLR)
- ✅ KMS provider manages master keys (HSM-backed)
- ✅ Minimal DEK lifetime (rotate every 24 hours)
- 🔄 Consider: Encrypted memory pages for DEK storage
- 🔄 Consider: Hardware Security Module (HSM) for local operations

**Residual Risk**: LOW (with mitigations)

---

### T2: GraphQL Injection Attacks

**Description**: Attacker crafts malicious GraphQL queries to bypass validation or access unauthorized data.

**Attack Vectors**:
- Deeply nested queries causing DoS
- Alias-based query complexity explosion
- Field injection via variables
- Introspection-based reconnaissance

**Impact**: HIGH
- Service disruption (resource exhaustion)
- Unauthorized data access
- Information disclosure

**Mitigations**:
- ✅ Query depth limiting (configured per security profile)
- ✅ Query complexity analysis
- ✅ Rate limiting per user/IP
- ✅ Introspection disabled in REGULATED/RESTRICTED profiles
- ✅ Field-level authorization checks
- ✅ PostgreSQL views enforce data access boundaries
- ✅ Input validation and sanitization

**Residual Risk**: LOW

---

### T3: Data Exfiltration via Tracing/Logging

**Description**: Sensitive data leaks through application logs, traces, or error messages.

**Attack Vectors**:
- OpenTelemetry traces containing PII
- Error messages revealing internal state
- Debug logs in production
- Log aggregation systems accessible to unauthorized parties

**Impact**: HIGH
- Privacy violations (GDPR, HIPAA)
- Credential exposure
- Intellectual property theft

**Mitigations**:
- ✅ TracingConfig.sanitize_patterns for automatic PII redaction
- ✅ Error messages sanitized before returning to client
- ✅ Structured logging with sensitivity levels
- ✅ Audit logs separately secured
- ✅ Production debug mode disabled
- 🔄 Consider: Automated PII detection in logs

**Residual Risk**: MEDIUM (requires ongoing monitoring)

---

### T4: SQL Injection

**Description**: Attacker injects malicious SQL through GraphQL variables or input fields.

**Attack Vectors**:
- Unsanitized GraphQL variables
- Dynamic SQL construction
- Stored function parameter injection
- Second-order SQL injection via stored data

**Impact**: CRITICAL
- Complete database compromise
- Data exfiltration
- Data manipulation or deletion
- Privilege escalation

**Mitigations**:
- ✅ **Architectural defense**: All queries through PostgreSQL views and stored functions
- ✅ No dynamic SQL construction in application code
- ✅ Parameterized queries only
- ✅ PostgreSQL functions with explicit parameter types
- ✅ Input validation at GraphQL layer
- ✅ Database user has minimal privileges (SELECT/EXECUTE only)
- ✅ Row-Level Security (RLS) enforces data boundaries

**Residual Risk**: VERY LOW (architecture prevents this attack class)

---

### T5: Denial of Service (DoS)

**Description**: Attacker overwhelms the system with requests or expensive operations.

**Attack Vectors**:
- High-volume request flooding
- Expensive GraphQL queries
- Large payload uploads
- Connection exhaustion
- Rust pipeline resource starvation

**Impact**: HIGH
- Service unavailability
- Revenue loss
- Reputation damage

**Mitigations**:
- ✅ Rate limiting (configured per security profile)
- ✅ Body size limits (1MB/10MB/100KB based on profile)
- ✅ Query complexity limits
- ✅ Connection pooling with max connections
- ✅ Rust pipeline timeout protection
- ✅ Horizontal scaling capability
- 🔄 Consider: CDN for static content
- 🔄 Consider: DDoS protection service (Cloudflare, AWS Shield)

**Residual Risk**: MEDIUM (depends on infrastructure)

---

### T6: Dependency Vulnerabilities

**Description**: Third-party dependencies contain security vulnerabilities.

**Attack Vectors**:
- Known CVEs in Python packages
- Known CVEs in Rust crates
- Compromised package registries
- Supply chain attacks

**Impact**: VARIES (depending on vulnerability)
- Remote code execution
- Data breach
- Service disruption

**Mitigations**:
- ✅ SBOM generation (CycloneDX format)
- ✅ Automated dependency scanning (Safety, cargo-audit)
- ✅ Container security scanning (Trivy)
- ✅ Regular dependency updates
- ✅ Version pinning in lock files
- ✅ CI/CD security gates
- 🔄 Consider: Private package mirrors
- 🔄 Consider: Dependency signature verification

**Residual Risk**: LOW (with continuous monitoring)

---

### T7: Insufficient Authentication/Authorization

**Description**: Weak or missing authentication/authorization allows unauthorized access.

**Attack Vectors**:
- Missing authentication checks
- Broken session management
- Privilege escalation
- Horizontal/vertical access control bypass

**Impact**: CRITICAL
- Unauthorized data access
- Data manipulation
- Account takeover

**Mitigations**:
- ✅ Field-level authorization in GraphQL resolvers
- ✅ PostgreSQL Row-Level Security (RLS)
- ✅ Stored functions with SECURITY DEFINER controls
- ✅ Security profiles enforce different policies
- ✅ Token validation middleware
- ✅ Session management with secure cookies
- 🔄 Implement: Multi-factor authentication (MFA)
- 🔄 Implement: OAuth2/OIDC integration

**Residual Risk**: MEDIUM (depends on implementation)

---

### T8: Cryptographic Weaknesses

**Description**: Weak or improperly implemented cryptography.

**Attack Vectors**:
- Weak cipher selection
- Improper key derivation
- Insufficient entropy
- Timing attacks on crypto operations

**Impact**: HIGH
- Data decryption
- Authentication bypass
- Integrity violations

**Mitigations**:
- ✅ Industry-standard KMS providers (Vault, AWS, GCP)
- ✅ AES-256-GCM for symmetric encryption
- ✅ Envelope encryption pattern
- ✅ Python `secrets` module for random key generation
- ✅ TLS 1.2+ for transport encryption
- ✅ Encryption context (AAD) for cryptographic binding
- 🔄 Consider: Regular cryptographic audits
- 🔄 Consider: Post-quantum cryptography planning

**Residual Risk**: LOW

---

### T9: Container/Infrastructure Compromise

**Description**: Attacker exploits container escape or infrastructure vulnerabilities.

**Attack Vectors**:
- Container escape via kernel vulnerabilities
- Exposed Docker socket
- Privileged container exploitation
- Kubernetes RBAC misconfiguration

**Impact**: CRITICAL
- Host system compromise
- Multi-tenant data breach
- Infrastructure takeover

**Mitigations**:
- ✅ Non-root container user
- ✅ Read-only root filesystem
- ✅ Container security scanning (Trivy)
- ✅ Minimal container image (distroless)
- ✅ Resource limits (CPU, memory)
- ✅ Security context constraints
- 🔄 Implement: Runtime security monitoring (Falco)
- 🔄 Implement: Network policies
- 🔄 Implement: Pod Security Standards

**Residual Risk**: MEDIUM (depends on deployment)

---

### T10: Insider Threats

**Description**: Malicious or negligent insiders abuse access.

**Attack Vectors**:
- Excessive permissions
- Direct database access
- Credential sharing
- Lack of audit trails

**Impact**: HIGH
- Data exfiltration
- Data manipulation
- Compliance violations

**Mitigations**:
- ✅ Principle of least privilege
- ✅ Audit logging of all operations
- ✅ Database RLS prevents unauthorized queries
- ✅ Security profiles enforce separation of duties
- ✅ Read-only database replicas for analytics
- 🔄 Implement: Database activity monitoring
- 🔄 Implement: Anomaly detection
- 🔄 Implement: Regular access reviews

**Residual Risk**: MEDIUM (requires organizational controls)

---

## Security Controls Summary

| Threat | Primary Control | Secondary Control | Residual Risk |
|--------|----------------|-------------------|---------------|
| T1: Key Access | KMS envelope encryption | Periodic rotation | LOW |
| T2: GraphQL Injection | Query validation | Rate limiting | LOW |
| T3: Data Exfiltration | Sanitization patterns | Structured logging | MEDIUM |
| T4: SQL Injection | Architecture (views/functions) | Parameterized queries | VERY LOW |
| T5: DoS | Rate limiting | Query complexity limits | MEDIUM |
| T6: Dependencies | SBOM + scanning | Version pinning | LOW |
| T7: Auth/Authz | Field-level + RLS | Security profiles | MEDIUM |
| T8: Crypto Weakness | Industry-standard KMS | AES-256-GCM | LOW |
| T9: Container Escape | Non-root + scanning | Resource limits | MEDIUM |
| T10: Insider Threat | Audit logging | Least privilege | MEDIUM |

---

## Compliance Mapping

### PCI-DSS Requirements
- **Req 3**: Protect stored cardholder data → KMS encryption, envelope encryption
- **Req 6**: Secure development → SBOM, dependency scanning
- **Req 8**: Identify and authenticate access → Field-level authz, RLS
- **Req 10**: Track and monitor access → Audit logging, tracing

### HIPAA Requirements
- **§164.312(a)(1)**: Access controls → Security profiles, field authz
- **§164.312(a)(2)(iv)**: Encryption → KMS, TLS
- **§164.312(b)**: Audit controls → Structured logging
- **§164.312(e)(1)**: Transmission security → TLS 1.2+

### GDPR Requirements
- **Art 25**: Data protection by design → Security profiles
- **Art 32**: Security of processing → Encryption, access controls
- **Art 33**: Breach notification → Audit trails
- **Art 35**: Data protection impact assessment → This threat model

---

## Attack Surface Analysis

### Network Attack Surface
- **Exposed**: HTTPS port (443/8000)
- **Risk**: Medium
- **Mitigation**: TLS, rate limiting, WAF

### Application Attack Surface
- **Exposed**: GraphQL endpoint, REST API
- **Risk**: High
- **Mitigation**: Input validation, authentication, authorization

### Database Attack Surface
- **Exposed**: None (internal network only)
- **Risk**: Low
- **Mitigation**: Network segmentation, connection pooling

### KMS Attack Surface
- **Exposed**: Outbound connections to KMS providers
- **Risk**: Medium
- **Mitigation**: Mutual TLS, API authentication

---

## Incident Response

### Detection Mechanisms
1. **Anomalous query patterns** → OpenTelemetry traces
2. **Authentication failures** → Audit logs
3. **Rate limit violations** → Middleware logs
4. **Database errors** → PostgreSQL logs
5. **KMS failures** → Provider alerts

### Response Procedures
1. **Isolate** affected services/users
2. **Investigate** using audit trails and traces
3. **Contain** by revoking credentials/keys
4. **Eradicate** vulnerability or malicious code
5. **Recover** from backups if needed
6. **Document** in incident report

---

## Security Testing Recommendations

### Automated Testing
- ✅ Unit tests for security middleware (83 tests)
- ✅ Integration tests for KMS providers (6 tests)
- 🔄 Add: Fuzzing for GraphQL parser
- 🔄 Add: Load testing for DoS resilience

### Manual Testing
- 🔄 Penetration testing (annually)
- 🔄 Code review (security-focused)
- 🔄 Architecture review (threat modeling update)

### Continuous Monitoring
- ✅ Dependency scanning (CI/CD)
- ✅ Container scanning (CI/CD)
- 🔄 Runtime application self-protection (RASP)

---

## Review and Maintenance

**Review Frequency**: Quarterly or after significant changes

**Last Review**: 2025-11-24
**Next Review**: 2026-02-24

**Change Triggers**:
- New features or APIs
- Security incidents
- New compliance requirements
- Dependency updates

---

*This threat model follows STRIDE methodology (Spoofing, Tampering, Repudiation, Information Disclosure, Denial of Service, Elevation of Privilege) and OWASP threat modeling best practices.*
