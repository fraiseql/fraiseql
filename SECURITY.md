# FraiseQL v2 Security Model

**Last Updated:** January 26, 2026
**Version:** 2.0.0-a1

---

## Table of Contents

1. [Security Philosophy](#security-philosophy)
2. [Threat Model](#threat-model)
3. [Implemented Security Controls](#implemented-security-controls)
4. [Authentication & Authorization](#authentication--authorization)
5. [Data Protection](#data-protection)
6. [SQL Injection Prevention](#sql-injection-prevention)
7. [Query Complexity Limits](#query-complexity-limits)
8. [Audit Logging](#audit-logging)
9. [Security Incident Response](#security-incident-response)
10. [Known Limitations](#known-limitations)

---

## Security Philosophy

FraiseQL v2's security model is based on these core principles:

### 1. **Schema-Driven Security**
Authorization rules are defined as **schema metadata**, not runtime logic. This enables:

- Compile-time validation of access patterns
- Static analysis for security violations
- Deterministic behavior across all queries

### 2. **Defense in Depth**
Multiple layers of protection:

1. **Transport Layer** - TLS/SSL encryption
2. **Authentication Layer** - JWT/OIDC token validation
3. **Authorization Layer** - Field-level and operation-level access control
4. **Query Layer** - Complexity limits and timeout enforcement
5. **Database Layer** - Parameterized queries, database-level permissions

### 3. **Fail Secure**

- Default to denying access (unless explicitly allowed)
- Errors don't leak sensitive information
- Graceful degradation under load
- No silent failures

### 4. **Zero Trust**

- All requests validated, even from "trusted" internal services
- All database connections use credentials
- All user input treated as potentially malicious
- No implicit trust based on network position

---

## Threat Model

### In Scope: Attacks FraiseQL Defends Against

| Threat | Defense | Status |
|--------|---------|--------|
| SQL Injection | Parameterized queries, input validation | ✅ Implemented |
| GraphQL Injection | Schema validation, query complexity limits | ✅ Implemented |
| Cross-Site Request Forgery (CSRF) | CORS restrictions, CSRF tokens | ✅ Implemented |
| Excessive Resource Usage | Query depth limits, complexity limits, timeouts | ✅ Implemented |
| Information Disclosure | Error message sanitization, introspection control | ✅ Implemented |
| Unauthorized Access | JWT/OIDC auth, field-level authorization | ✅ Implemented |
| Man-in-the-Middle | TLS/SSL encryption, certificate validation | ✅ Implemented |
| Denial of Service | Rate limiting, connection pooling, backpressure | ✅ Implemented |
| Authentication Bypass | JWT validation, token expiration, refresh tokens | ✅ Implemented |
| Privilege Escalation | Role-based access control, audit logging | ✅ Implemented |

### Out of Scope: Attacks FraiseQL Doesn't Defend Against

| Threat | Why | Mitigation |
|--------|-----|-----------|
| Database-level attacks | Database security is responsibility of database provider | Database hardening, network isolation |
| Compromised credentials | Can't detect if credentials are leaked | MFA, rotation policy, monitoring |
| Application-level business logic errors | Authorization rules reflect application design | Thorough security review during schema authoring |
| Physical attacks on servers | Physical security is operational responsibility | Data center security, access controls |
| Malicious database administrator | Trust model assumes database security | Database auditing, monitoring, least privilege |

---

## Implemented Security Controls

### ✅ Verified and Tested

**Authentication**

- JWT token validation (HS256, RS256, RS384, RS512)
- OAuth2/OIDC provider support (GitHub, Google, Keycloak, Azure AD)
- Token expiration and refresh mechanism
- Secure token storage and transmission

**Authorization**

- Role-Based Access Control (RBAC) with multiple strategies
- Attribute-Based Access Control (ABAC)
- Field-level access control
- Operation-specific authorization (read/create/update/delete)
- Custom authorization rules with context variables

**Encryption**

- TLS/SSL for transport (configurable min version)
- HTTPS enforcement (optional in production)
- Certificate pinning support
- Modern cipher suite selection

**Input Validation**

- GraphQL query schema validation
- Field name validation
- Type checking at compile time
- Parameterized queries (prevent SQL injection)

**Query Safety**

- Query depth limits (prevents deeply nested attacks)
- Query complexity scoring
- Timeout enforcement (default: 30 seconds, configurable)
- Rate limiting (per-IP or global)

**Error Handling**

- Error message sanitization (no schema leakage)
- Introspection control (can be disabled)
- Stack trace hiding in production
- Graceful error responses

**Audit Logging**

- Mutation tracking with user context
- Access decision logging
- Authentication event logging
- Query performance logging
- Configurable log levels

---

## Authentication & Authorization

### Authentication Methods Supported

**JWT Tokens**:
```json
{
  "sub": "user123",
  "exp": 1706280000,
  "iat": 1706276400,
  "roles": ["user", "editor"],
  "scopes": ["read:public", "write:own"]
}
```

Configuration:
```toml
[auth]
jwt_secret = "${JWT_SECRET}"
jwt_algorithms = ["RS256", "HS256"]
jwt_expiration_secs = 3600
```

**OAuth2/OIDC**:
```toml
[auth.oauth2]
enabled = true
provider = "github"  # or "google", "keycloak", "azure_ad"
client_id = "${OAUTH_CLIENT_ID}"
client_secret = "${OAUTH_CLIENT_SECRET}"
redirect_uri = "https://app.example.com/auth/callback"
```

### Authorization Model

**Three-Tier Authorization**:

1. **Type-Level**: Entire types can be restricted
   ```graphql
   type AdminPanel @authorize(roles: ["admin"]) {
     id: ID!
     users: [User!]!
   }
   ```

2. **Field-Level**: Individual fields restricted
   ```graphql
   type User {
     id: ID!
     name: String!
     ssn: String! @authorize(roles: ["admin", "hr"])
     salary: Int! @authorize(custom: "isManager($user, $field.department)")
   }
   ```

3. **Operation-Level**: Read/write restrictions
   ```graphql
   type User {
     id: ID!
     email: String! @authorize(
       read: true,
       create: "isAdmin($user)",
       update: "isOwner($user, $field.ownerId)",
       delete: "isAdmin($user)"
     )
   }
   ```

### Authorization Strategies

**Role-Based Access Control (RBAC)**:
```
User has roles → Roles mapped to permissions → Access granted/denied
```

**Attribute-Based Access Control (ABAC)**:
```
User attributes + Resource attributes + Environment → Policy evaluation → Access granted/denied
```

**Custom Rules**:
```
Expression evaluated with context variables → Boolean result → Access granted/denied
```

Example context variables:

- `$user.id` - User ID
- `$user.roles` - User's roles
- `$user.attributes` - Custom user attributes
- `$field.ownerId` - Field owner ID
- `$context.timestamp` - Current time
- `$context.source` - Request source (API, internal, etc.)

---

## Data Protection

### In Transit

**TLS/SSL Configuration**:
```toml
[security]
require_https = true
tls_cert = "/path/to/cert.pem"
tls_key = "/path/to/key.pem"
tls_min_version = "TLSv1.2"
```

**Certificate Management**:

- Use certificates issued by trusted CAs (not self-signed in production)
- Implement certificate rotation before expiry
- Monitor certificate expiration dates

### At Rest

**Database Security**:

- Credentials in environment variables (not config files)
- Database user with minimal required privileges
- Connection pooling with idle timeouts
- Encrypted database connections (SSL/TLS)

**No Secrets in Code**:

- All secrets stored as environment variables
- Config files committed to version control must not contain secrets
- Use secret management systems (Vault, sealed-secrets, etc.)

---

## SQL Injection Prevention

### Mechanism

FraiseQL uses **parameterized queries** exclusively:

```rust
// SAFE - FraiseQL implementation:
let sql = "SELECT * FROM users WHERE id = $1 AND status = $2";
let params = [user_id, "active"];
database.execute(sql, &params);

// NOT used - vulnerable pattern:
let sql = format!("SELECT * FROM users WHERE id = {} AND status = '{}'", user_id, status);
```

### Validation

All user input:

1. Validated against schema at parse time
2. Type-checked at compile time
3. Parameterized before database execution
4. Never concatenated into SQL strings

### Tested Against

- Single quote injection: `' OR '1'='1`
- Comment injection: `--`, `/**/`
- Union attacks: `UNION SELECT`
- Boolean-based attacks: `1 AND 1=1`
- Time-based blind attacks: `SLEEP(5)`

---

## Query Complexity Limits

### Depth Limiting

Prevents deeply nested attacks:

```
Query Depth = Deepness of field nesting
Maximum = 10 (configurable)

Valid:   users { id }                           [depth: 1]
Valid:   users { id posts { id } }              [depth: 2]
Invalid: users { id posts { id comments {
           id author { id profile { ... } }
         } } }                                   [depth > 10]
```

Configuration:
```toml
[security]
query_max_depth = 10
```

### Complexity Scoring

Assigns points to each field and multiplies by list multipliers:

```
Scalar field = 1 point
Object field = 1 point
List field = 5 points (can return many rows)

Query:   users { id name posts { id } }
Score:   (1 + 1 + 5 * (1 + 1)) = 14 points
Limit:   1000 points (configurable)
Result:  ALLOWED (14 < 1000)
```

Configuration:
```toml
[security]
query_max_complexity = 1000
```

### Timeout Enforcement

```toml
[performance]
query_timeout_ms = 30000  # 30 seconds
```

If query exceeds limit:
```json
{
  "errors": [{
    "message": "Query execution timeout (exceeded 30000ms)"
  }]
}
```

---

## Audit Logging

### What's Logged

**Authentication Events**:

- Login attempts (success/failure)
- Token validation (success/failure)
- Token refresh operations
- Session creation/destruction

**Authorization Events**:

- Authorization check results (allow/deny)
- Failed access attempts
- Permission changes

**Mutation Events**:

- All INSERT, UPDATE, DELETE operations
- User making change
- Fields modified
- Old vs. new values
- Timestamp

**Security Events**:

- Rate limit violations
- Query complexity violations
- Connection pool exhaustion
- Certificate issues

### Log Format

```json
{
  "timestamp": "2026-01-26T12:34:56Z",
  "level": "WARN",
  "event_type": "AUTHORIZATION_DENIED",
  "user_id": "user123",
  "user_roles": ["user"],
  "resource": "User.ssn",
  "action": "READ",
  "reason": "Missing role: admin",
  "source_ip": "192.0.2.1",
  "trace_id": "abc123def456"
}
```

### Log Configuration

```bash
export RUST_LOG="fraiseql=info,fraiseql_core::security=debug"
```

### Retention

Logs are written to:

- **STDOUT** (development)
- **File** (production - configure in system setup)
- **Cloud logging** (optional - CloudLogging, DataDog, etc.)

Recommend retention:

- **Critical events**: 1 year
- **Mutation events**: 90 days
- **Access logs**: 30 days

---

## Security Incident Response

### If You Suspect a Security Issue

1. **Do not publicly disclose** the vulnerability
2. **Report responsibly**:
   - Email: security@fraiseql.io
   - Include: description, reproduction steps, impact
3. **Allow 90 days** for patch development
4. **Coordinate disclosure** with FraiseQL team

### Security Update Process

1. Issue identified
2. Fix implemented and tested
3. Security advisory prepared
4. Release published
5. Users notified

### Checking for Vulnerabilities

```bash
# Check for known security issues
cargo audit

# Update dependencies to patch versions
cargo update --aggressive
```

---

## Known Limitations

### Limitations of Current Implementation

1. **Database-Level Security**
   - ✋ FraiseQL cannot enforce row-level security (RLS) automatically
   - **Mitigation**: Use database RLS policies in addition to FraiseQL auth

2. **Secrets Management**
   - ✋ Currently no built-in secrets rotation
   - **Mitigation**: Use HashiCorp Vault or cloud secret managers

3. **Field Masking**
   - ✋ Sensitive fields cannot be partially masked (e.g., show last 4 SSN digits)
   - **Mitigation**: Create computed fields in schema

4. **Multi-Tenancy**
   - ✋ Tenant isolation is not automatic
   - **Mitigation**: Explicitly include tenant ID in authorization rules

5. **Encryption at Rest**
   - ✋ Database encryption is database-specific
   - **Mitigation**: Enable database-level encryption

### Security Guarantees vs. Limitations

**What FraiseQL Guarantees**:

- ✅ No SQL injection (parameterized queries)
- ✅ Authorization enforced on every query
- ✅ No unintended schema exposure (introspection disabled)
- ✅ Timeouts prevent DoS via expensive queries

**What Requires Additional Setup**:

- ⚠️ Transport encryption (TLS must be enabled)
- ⚠️ Authentication (JWT/OIDC must be configured)
- ⚠️ Secrets management (use environment variables)
- ⚠️ Database security (database hardening)

---

## Security Hardening Checklist

For production deployments:

### Required

- [ ] Enable HTTPS/TLS
- [ ] Configure CORS to specific origins (not wildcard)
- [ ] Enable JWT/OIDC authentication
- [ ] Configure authorization rules in schema
- [ ] Disable introspection
- [ ] Enable rate limiting
- [ ] Set query timeout
- [ ] Store secrets in environment variables

### Recommended

- [ ] Enable audit logging
- [ ] Set up log monitoring/alerting
- [ ] Configure database TLS
- [ ] Implement database-level access controls
- [ ] Set up backup strategy
- [ ] Enable query complexity limits
- [ ] Implement web application firewall (WAF)
- [ ] Set up security monitoring/SOC

### Optional

- [ ] Implement certificate pinning
- [ ] Use secrets management system
- [ ] Set up database encryption
- [ ] Implement multi-tenancy isolation
- [ ] Set up API gateway with additional auth

---

## Support & Reporting

**Security Documentation**: This file
**Deployment Hardening**: See [DEPLOYMENT.md](DEPLOYMENT.md)
**Incident Reporting**: security@fraiseql.io

---

**Remember**: Security is not a feature; it's a process. Regularly review and update your security posture as threats evolve.
