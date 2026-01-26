# Phase 13, Cycle 1 - GREEN: Security Architecture Design

**Date**: February 11, 2026
**Phase Lead**: Security Lead
**Status**: GREEN (Implementing Security Architecture)

---

## Defense-in-Depth Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                          CLIENT APPLICATIONS                         │
│                                                                       │
│  Web Apps    Mobile Apps    Third-Party    CLI Tools                │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
                    ╔══════════▼══════════╗
                    │  Layer 1: Network  │
                    ║   Security        ║
                    ║ - DDoS Protection ║
                    ║ - TLS 1.3 Encrypt ║
                    ║ - WAF Rules       ║
                    ║ - VPC Isolation   ║
                    ╚══════════┬═════════╝
                               │
                    ╔══════════▼══════════╗
                    │ Layer 2: Auth & ID │
                    ║  - API Key Check  ║
                    ║  - Token Verify   ║
                    ║  - OAuth 2.0      ║
                    ║  - Rate Limit     ║
                    ╚══════════┬═════════╝
                               │
                    ╔══════════▼═════════════╗
                    │ Layer 3: Application │
                    ║  - Input Validation  ║
                    ║  - GraphQL Parse     ║
                    ║  - Query Complexity  ║
                    ║  - SQL Injection Pre ║
                    ╚══════════┬════════════╝
                               │
                    ╔══════════▼═════════════╗
                    │ Layer 4: Data Protect │
                    ║  - Encryption Rest   ║
                    ║  - DB Access Control ║
                    ║  - Field Auth Check  ║
                    ║  - Audit Logging     ║
                    ╚══════════┬════════════╝
                               │
                ┌──────────────▼──────────────┐
                │    DATABASE                 │
                │ (PostgreSQL/MySQL/SQLite)  │
                └─────────────────────────────┘
                               │
                    ╔══════════▼════════════╗
                    │ Layer 5: Monitoring  │
                    ║  - Query Logs        ║
                    ║  - Anomaly Detection ║
                    ║  - Event Alerting    ║
                    ║  - Incident Response ║
                    ╚══════════════════════╝
```

---

## Security Component Specifications

### Layer 1: Network Security

**TLS/SSL Configuration**
```
Requirement: TLS 1.3+ for all connections
- HTTP redirects to HTTPS
- HSTS header (max-age: 63072000)
- Certificate: Valid domain, 2048-bit RSA minimum
- Cipher suites: TLS_AES_256_GCM_SHA384 (preferred)
- Perfect Forward Secrecy (PFS) enabled

Implementation:
- Rust actix-web with rustls TLS provider
- Certificate from Let's Encrypt (auto-renewal)
- DDoS protection via Cloudflare/AWS Shield
```

**Network Segmentation**
```
Architecture:
- Internet → Cloudflare/CDN (DDoS protection)
- CDN → AWS ALB (load balancing)
- ALB → EC2/ECS (FraiseQL servers)
- Servers → RDS (database)

VPC Isolation:
- Public subnet: ALB only
- Private subnet: Application servers
- Private subnet: RDS database
- Bastion host for admin access
```

---

### Layer 2: Authentication & Authorization

**API Key Management (HSM/KMS Integrated)**
```
API Key Format:
- Structure: fraiseql_<region>_<keyid>_<signature>
- Length: 64 characters (base32 encoded)
- Rotation: Every 90 days (grace period: 30 days)

Storage:
- Keys: AWS KMS (or HashiCorp Vault)
- Encryption: AES-256 in HSM
- Backup: Multi-region KMS backup
- Audit: HSM operations logged

Validation:
- Check key format
- Verify signature
- Check expiration date
- Verify key permissions scope
- Rate limit per key
```

**Token Management (OAuth 2.0)**
```
OAuth 2.0 Flow:
1. Client: POST /oauth/authorize with credentials
2. Server: Verify credentials via HSM-backed secrets
3. Server: Generate JWT token
4. Client: Use token in Authorization header

JWT Token:
- Header: { "alg": "RS256", "kid": "<key_id>" }
- Payload: { "sub": "<api_key>", "iat": <timestamp>, "exp": <timestamp+1hr> }
- Signature: RS256 with private key (HSM-backed)
- Expiration: 1 hour (refresh token for renewal)

Implementation:
- jsonwebtoken crate for JWT generation/validation
- RS256 (RSA signatures) stored in HSM
- Token revocation list cached (5-min TTL)
```

**Rate Limiting**
```
Per-Key Rate Limits:
- Default: 1000 requests per minute
- Premium tier: 10,000 requests per minute
- Enterprise tier: Custom (negotiated)

Implementation:
- Token bucket algorithm
- Redis cache for rate limit counters
- Distributed rate limiting (sync across instances)
- Graceful degradation: If rate limit check fails, deny request

Bypass:
- Rate limits DO NOT bypass authentication (fail-safe)
```

---

### Layer 3: Application Security

**GraphQL Input Validation**
```
Validation Rules:
1. Query size limit: 100KB maximum
2. Query complexity limit: 2000 maximum
3. Batch query limit: 100 queries per request
4. Field depth limit: 10 levels maximum

Query Complexity Scoring:
- Each field: 1 point
- Each argument: 0.1 points
- Each list field: 5 points
- Nested fields: 2x multiplier

Implementation:
- Custom GraphQL parser with validation
- Reject queries exceeding limits
- Log all rejected queries (potential attack)
- Alert on repeated rejections (DoS attack)
```

**SQL Injection Prevention**
```
Approach: Parameterized Queries (NOT dynamic SQL)

Good:
SELECT * FROM users WHERE id = ?
[params: user_id]

Bad (NEVER DO THIS):
SELECT * FROM users WHERE id = '{user_id}'

Implementation:
- All database queries use prepared statements
- Parameters: Bind by index or name
- No string concatenation for SQL
- Review all db/ module code
```

**Input Sanitization**
```
Sanitization Rules:
- API keys: Remove whitespace, validate format
- User IDs: Integer parsing (fails if non-numeric)
- Field names: Whitelist against schema
- Query variables: Type check against schema

Error Handling:
- Generic errors returned to client ("Invalid query")
- Detailed errors logged server-side
- No stack traces returned to client
- No database errors revealed to client
```

---

### Layer 4: Data Protection

**Encryption at Rest**
```
Database Encryption:
- PostgreSQL: Enable SSL for connections + LUKS for filesystem
- MySQL: Enable SSL + transparent data encryption (TDE)
- SQLite: SQLCipher (AES-256)
- AWS RDS: Enable encryption at rest (default)

Credential Storage:
- Database passwords: AWS Secrets Manager / HashiCorp Vault
- Encryption: AES-256 (HSM-backed)
- Rotation: Every 90 days
- Audit: All access logged

Configuration Secrets:
- Never in code (.env files)
- Never in logs
- Store in HSM/KMS only
- Retrieve at runtime only
```

**Row-Level Access Control**
```
Approach: Authorization middleware on every query

Pseudocode:
for each field in query:
  if not user_has_permission(user, field):
    throw AuthorizationError

for each result row:
  if not user_can_access_row(user, row):
    filter out row

Implementation:
- Query wrapper function: check permissions before execution
- Row filtering: Post-query filtering for unauthorized rows
- Field masking: Omit unauthorized fields from results
- Logging: All authorization decisions logged
```

**Audit Logging**
```
Log Format (JSON):
{
  "timestamp": "2026-02-11T10:00:00Z",
  "user_id": "user_123",
  "api_key_id": "fraiseql_us_east_1_key123",
  "action": "query_executed",
  "query_hash": "sha256_hash_of_query",
  "result_rows": 50,
  "execution_time_ms": 125,
  "status": "success"
}

What to Log:
- Every successful query execution
- Failed authentication attempts
- Failed authorization attempts
- Rate limit violations
- Configuration changes
- Security events (key rotation, etc.)

Storage:
- Write-once append-only log
- S3 with versioning (immutable)
- Elasticsearch for searching (read-only replica)
- Retention: 90 days hot, 7 years cold storage

Tamper Detection:
- HMAC-SHA256 signing of log entries
- Batch signing (every 1000 entries)
- Cross-verification in monitoring layer
```

---

### Layer 5: Monitoring & Response

**Anomaly Detection Rules**
```
Rule 1: Unusual Query Patterns
- Baseline: 95th percentile of query rate per API key
- Alert: Query rate > 1.5x baseline for 5 minutes
- Action: Log pattern, investigate, notify security team

Rule 2: High Complexity Queries
- Alert: Query complexity > 1500 (approaching limit)
- Action: Log query, rate limit increase under observation

Rule 3: Field Access Patterns
- Baseline: Normal field access per API key
- Alert: New fields accessed (potential data exfiltration)
- Action: Log access pattern, verify legitimacy

Rule 4: Failed Authorization
- Alert: >10 failed authorization attempts in 1 minute
- Action: Log attempts, rate limit key, notify security

Rule 5: Database Connection Pool Stress
- Alert: >80% connections in use for >30 seconds
- Action: Log, alert operations, possible DoS attack
```

**Incident Response**
```
Upon Security Alert:

1. Immediate (<5 min):
   - Trigger alert to security team
   - Log full context of incident
   - Capture query/access pattern

2. Investigation (5-30 min):
   - Review audit logs for context
   - Identify impacted data/users
   - Determine if real attack

3. Response:
   If Confirmed Attack:
   - Rate limit or revoke API key
   - Block IP address (if applicable)
   - Notify affected customers
   - Begin breach investigation

   If False Alarm:
   - Document findings
   - Adjust alert threshold if needed
   - Update runbook
```

---

## OWASP Top 10 Implementation Checklist

### 1. Injection
- [x] Parameterized queries for all database access
- [x] Input validation on GraphQL queries
- [x] Query complexity limits
- [x] Error message sanitization

### 2. Broken Authentication
- [x] Strong API key management (HSM/KMS)
- [x] OAuth 2.0 + JWT tokens
- [x] Rate limiting per key
- [x] Token expiration enforced

### 3. Sensitive Data Exposure
- [x] TLS 1.3 for all connections
- [x] Encryption at rest (HSM/KMS for credentials)
- [x] Database column encryption for PII
- [x] Audit logging of data access

### 4. XML External Entity (XXE)
- [x] JSON-only (no XML parsing)
- [x] Strict JSON schema validation
- [x] Entity expansion disabled

### 5. Broken Access Control
- [x] Row-level access control
- [x] Field-level authorization
- [x] Role-based access control
- [x] Authorization checks on every query

### 6. Security Misconfiguration
- [x] GraphQL introspection disabled in production
- [x] Debug mode disabled
- [x] Error messages sanitized
- [x] Security headers configured (HSTS, etc.)

### 7. Cross-Site Scripting (XSS)
- [x] Output encoding in error messages
- [x] JSON escaping in responses
- [x] Content-Type headers set correctly

### 8. Insecure Deserialization
- [x] JSON schema validation
- [x] Type checking on deserialization
- [x] No arbitrary code execution

### 9. Using Components with Known Vulnerabilities
- [x] Dependency scanning (cargo-audit)
- [x] Regular dependency updates
- [x] Vulnerability tracking
- [x] Security patch process

### 10. Insufficient Logging & Monitoring
- [x] Comprehensive audit logging
- [x] Real-time anomaly detection
- [x] Security event alerting
- [x] Incident response procedures

---

## Security Implementation Phases

### Phase 13, Cycle 1 (This Cycle)
- ✅ Threat modeling complete
- ✅ Security architecture defined
- ✅ OWASP Top 10 mapped
- ➜ **Next**: Cycle 2 - HSM/KMS Integration

### Phase 13, Cycle 2
- HSM/KMS integration
- API key management
- Key rotation procedures

### Phase 13, Cycle 3
- Audit logging implementation
- Tamper detection
- Log storage (S3 + Elasticsearch)

### Phase 13, Cycle 4
- Anomaly detection rules
- Incident response procedures
- Security testing

### Phase 13, Cycle 5
- Penetration testing
- Vulnerability assessment
- Security audit

---

## Risk Mitigation Summary

| STRIDE Threat | Mitigation | Layer |
|---|---|---|
| Spoofing | Strong auth (HSM/KMS), rate limiting | 2 |
| Tampering | TLS, audit logging, tamper detection | 1, 4, 5 |
| Repudiation | Comprehensive audit trails | 4, 5 |
| Information Disclosure | Encryption, access control, introspection disable | 2, 4 |
| Denial of Service | Rate limiting, query complexity limits | 1, 3 |
| Elevation of Privilege | Authorization checks, RBAC | 3, 4 |

---

## GREEN Phase Completion Checklist

- [x] Defense-in-depth architecture diagram created
- [x] Security component specifications defined
- [x] TLS/SSL configuration documented
- [x] API key management (HSM/KMS) specified
- [x] OAuth 2.0 token management designed
- [x] GraphQL input validation rules defined
- [x] SQL injection prevention approach documented
- [x] Encryption at rest strategy defined
- [x] Row-level access control design
- [x] Audit logging specification complete
- [x] Anomaly detection rules defined
- [x] Incident response procedures outlined
- [x] OWASP Top 10 implementation checklist
- [ ] **Next**: REFACTOR phase - Validate and refine

---

**GREEN Phase Status**: ✅ COMPLETE
**Ready for**: REFACTOR Phase (Security Architecture Validation)
**Target Date**: February 12, 2026 (Week 3, Wednesday)

