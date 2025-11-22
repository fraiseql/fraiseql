# FraiseQL Threat Model

> **Status:** Template - Requires completion by security team
> **Last Updated:** 2025-11-22
> **Review Cycle:** Quarterly

## 1. System Overview

### 1.1 Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Client Applications                       │
│                (Web Browser, Mobile App, API Client)            │
└──────────────────────────┬──────────────────────────────────────┘
                           │ HTTPS (TLS 1.2+)
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Load Balancer / CDN                         │
│                    (Rate Limiting, WAF)                         │
└──────────────────────────┬──────────────────────────────────────┘
                           │
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
    ┌────────────┐  ┌────────────┐  ┌────────────┐
    │ FraiseQL   │  │ FraiseQL   │  │ FraiseQL   │
    │ Instance 1 │  │ Instance 2 │  │ Instance N │
    │            │  │            │  │            │
    │ ┌────────┐ │  │ ┌────────┐ │  │ ┌────────┐ │
    │ │FastAPI │ │  │ │FastAPI │ │  │ │FastAPI │ │
    │ │GraphQL │ │  │ │GraphQL │ │  │ │GraphQL │ │
    │ │  Rust  │ │  │ │  Rust  │ │  │ │  Rust  │ │
    │ └────────┘ │  │ └────────┘ │  │ └────────┘ │
    └─────┬──────┘  └─────┬──────┘  └─────┬──────┘
          │               │               │
          └───────────────┼───────────────┘
                          │
                          ▼
               ┌─────────────────────┐
               │   PgBouncer         │
               │   Connection Pool   │
               └──────────┬──────────┘
                          │
                          ▼
               ┌─────────────────────┐
               │   PostgreSQL 16+    │
               │   + pgvector        │
               │                     │
               │ ┌─────────────────┐ │
               │ │ Audit Logs      │ │
               │ │ HMAC Signed     │ │
               │ └─────────────────┘ │
               └─────────────────────┘
```

### 1.2 Trust Boundaries

| Boundary | Description | Controls |
|----------|-------------|----------|
| TB1 | Internet ↔ Load Balancer | TLS, Rate Limiting, WAF |
| TB2 | Load Balancer ↔ App | Internal Network, mTLS (optional) |
| TB3 | App ↔ Database | Connection Pool, TLS, Least Privilege |
| TB4 | Admin ↔ Infrastructure | VPN, MFA, RBAC |

## 2. Assets

### 2.1 Data Assets

| Asset | Classification | Storage | Protection |
|-------|---------------|---------|------------|
| User Credentials | Confidential | PostgreSQL | Argon2 hashing |
| JWT Tokens | Confidential | Memory/Client | HMAC-SHA256 signed |
| API Keys | Confidential | Environment | Encrypted at rest |
| GraphQL Queries | Internal | Memory | Input validation |
| Business Data | Varies | PostgreSQL | Encryption at rest |
| Audit Logs | Internal | PostgreSQL | HMAC chain integrity |

### 2.2 System Assets

| Asset | Criticality | Protection |
|-------|-------------|------------|
| Application Servers | High | Container isolation |
| Database Server | Critical | Network isolation, encryption |
| Connection Pooler | High | Access control |
| Load Balancer | High | DDoS protection |
| CI/CD Pipeline | Critical | Access control, signing |

## 3. Threat Actors

### 3.1 Actor Profiles

| Actor | Capability | Motivation | Target Assets |
|-------|-----------|------------|---------------|
| Script Kiddie | Low | Curiosity, Vandalism | Public endpoints |
| Cybercriminal | Medium | Financial gain | User data, credentials |
| Insider Threat | Medium-High | Revenge, Financial | All internal systems |
| Nation State | Very High | Espionage, Disruption | All systems |
| Supply Chain | High | Various | Dependencies, builds |

## 4. Threats (STRIDE Analysis)

### 4.1 Spoofing

| ID | Threat | Component | Likelihood | Impact | Mitigation |
|----|--------|-----------|------------|--------|------------|
| S1 | JWT Token Forgery | Auth | Medium | High | HMAC-SHA256, key rotation |
| S2 | Session Hijacking | Auth | Medium | High | Secure cookies, short TTL |
| S3 | API Key Theft | Auth | Medium | High | Key rotation, monitoring |
| S4 | Identity Spoofing | Auth | Low | Critical | MFA, strong authentication |

### 4.2 Tampering

| ID | Threat | Component | Likelihood | Impact | Mitigation |
|----|--------|-----------|------------|--------|------------|
| T1 | SQL Injection | Database | Low | Critical | Parameterized queries |
| T2 | GraphQL Injection | API | Low | High | Schema validation |
| T3 | Request Tampering | API | Medium | Medium | CSRF protection |
| T4 | Build Tampering | CI/CD | Low | Critical | Artifact signing |
| T5 | Audit Log Tampering | Logs | Low | High | HMAC chain integrity |

### 4.3 Repudiation

| ID | Threat | Component | Likelihood | Impact | Mitigation |
|----|--------|-----------|------------|--------|------------|
| R1 | Denied Actions | Audit | Medium | Medium | Cryptographic audit chain |
| R2 | False Attribution | Audit | Low | High | User binding in logs |

### 4.4 Information Disclosure

| ID | Threat | Component | Likelihood | Impact | Mitigation |
|----|--------|-----------|------------|--------|------------|
| I1 | Data Leakage via GraphQL | API | Medium | High | Explicit field contracts |
| I2 | Error Message Exposure | API | Medium | Medium | Production error handling |
| I3 | Credential Exposure | Secrets | Low | Critical | Secret scanning, rotation |
| I4 | Side-Channel Attacks | Crypto | Low | Medium | Constant-time comparisons |

### 4.5 Denial of Service

| ID | Threat | Component | Likelihood | Impact | Mitigation |
|----|--------|-----------|------------|--------|------------|
| D1 | Query Complexity Attack | GraphQL | High | High | Complexity limits |
| D2 | Resource Exhaustion | Server | Medium | High | Rate limiting |
| D3 | Recursive Query Attack | GraphQL | Medium | High | Depth limits |
| D4 | Connection Pool Exhaustion | Database | Medium | High | PgBouncer, limits |

### 4.6 Elevation of Privilege

| ID | Threat | Component | Likelihood | Impact | Mitigation |
|----|--------|-----------|------------|--------|------------|
| E1 | RBAC Bypass | Auth | Low | Critical | Role enforcement |
| E2 | Authorization Flaws | API | Medium | High | Field-level auth |
| E3 | Container Escape | Infra | Low | Critical | Security contexts |

## 5. Attack Scenarios

### 5.1 GraphQL-Specific Attacks

```
Scenario: Query Depth Attack
1. Attacker crafts deeply nested query
2. Query bypasses complexity check
3. Server resources exhausted
4. Denial of service achieved

Mitigation:
- Max depth: 10 levels
- Complexity scoring per field
- Query cost analysis
- Rate limiting per client
```

### 5.2 Supply Chain Attack

```
Scenario: Dependency Compromise
1. Malicious package published
2. Dependabot proposes update
3. Update merged without review
4. Malicious code executes

Mitigation:
- SBOM generation
- Dependency pinning with hashes
- SLSA provenance verification
- Manual review of updates
```

## 6. Mitigations Matrix

| Control | NIST 800-53 | Threats Mitigated |
|---------|-------------|-------------------|
| JWT Authentication | IA-2 | S1, S2, S4 |
| Parameterized Queries | SI-10 | T1, T2 |
| CSRF Protection | SC-23 | T3 |
| Audit Logging | AU-2, AU-9 | R1, R2 |
| Field-Level Auth | AC-3 | I1, E2 |
| Rate Limiting | SC-5 | D1, D2 |
| Query Limits | SC-5 | D1, D3 |
| Artifact Signing | SA-12 | T4 |

## 7. Residual Risks

| Risk | Likelihood | Impact | Acceptance Rationale |
|------|------------|--------|---------------------|
| Zero-day in dependencies | Low | High | Regular updates, monitoring |
| Advanced persistent threat | Very Low | Critical | Cost of full mitigation exceeds risk |
| Insider with admin access | Low | Critical | Background checks, monitoring |

## 8. Review History

| Date | Reviewer | Changes |
|------|----------|---------|
| 2025-11-22 | Initial | Template created |
| TBD | Security Team | Complete threat analysis |

---

**Classification:** UNCLASSIFIED
**Distribution:** Internal
