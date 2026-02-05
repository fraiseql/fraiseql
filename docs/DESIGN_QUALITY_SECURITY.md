# Design Quality Security Guide

## Overview

FraiseQL's design quality audit APIs are designed with security as a first-class concern. This guide covers security features, best practices, and threat mitigation.

## Security Features

### Input Validation

All design audit endpoints validate input:

- ✅ JSON schema validation
- ✅ Size limits (prevents DoS)
- ✅ Type checking
- ✅ Sanitization of special characters

```bash
# Well-formed request
curl -X POST http://localhost:8080/api/v1/design/audit \
  -H "Content-Type: application/json" \
  -d '{"schema": {"types": [...]}}'

# Invalid request (rejected)
curl -X POST http://localhost:8080/api/v1/design/audit \
  -H "Content-Type: application/json" \
  -d 'invalid json'  # 400 Bad Request
```

### Rate Limiting

Design audit endpoints support rate limiting (configure in deployment):

```toml
# fraiseql-server.toml
[security.rate_limiting]
design_audit_requests_per_second = 100
design_audit_burst_size = 10
```

### Error Messages

Error messages are sanitized to prevent information disclosure:

```json
// ✅ Safe error response
{
  "status": "error",
  "error": "Invalid schema structure: missing required field 'types'"
}

// ❌ Unsafe (never returned)
{
  "error": "/home/user/schemas/private.json not found"
}
```

## Threat Model

### DoS Prevention

**Threat**: Attacker sends extremely large or deeply nested schemas

**Mitigation**:

- Schema size limit: 100MB
- Nesting depth limit: 1000 levels
- Analysis timeout: 5 seconds per schema
- Concurrent request limit: 1000 per server

**Test**: `test_design_audit_rejects_extremely_large_schema`

### Information Disclosure

**Threat**: Error messages leak sensitive information

**Mitigation**:

- Error messages don't contain file paths
- Error messages don't expose internal state
- Schema names are sanitized
- Stack traces not returned to clients

**Tests**:

- `test_design_audit_error_messages_dont_leak_paths`
- `test_design_audit_doesnt_expose_internal_state`
- `test_design_audit_sanitizes_schema_names`

### Injection Attacks

**Threat**: Malicious JSON in schema causes issues

**Mitigation**:

- Input sanitized before processing
- Unicode injection prevention
- Recursive structure handling
- Type validation

**Tests**:

- `test_design_audit_rejects_unicode_injection`
- `test_design_audit_handles_recursive_structures`
- `test_design_audit_recovers_from_invalid_type`

### Resource Exhaustion

**Threat**: Attacker creates schemas that consume excessive resources

**Mitigation**:

- Analysis time limits
- Memory usage limits
- Deep nesting detection
- Concurrent request throttling

**Tests**:

- `test_design_audit_limits_analysis_time`
- `test_design_audit_handles_deeply_nested_json`

## Authorization

### Field-Level Authorization

Design audit respects field-level authorization directives:

```json
{
  "types": [{
    "name": "User",
    "fields": [{
      "name": "email",
      "requires_auth": true,
      "required_scopes": ["user:read"]
    }]
  }]
}
```

Authorization audit verifies these are enforced:

```bash
curl -X POST http://localhost:8080/api/v1/design/auth-audit \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer token" \
  -d '{"schema": {...}}'
```

**Note**: Authorization headers are optional for design audit. Schema analysis is read-only and doesn't modify state.

### Role-Based Access

For production deployments, configure RBAC:

```toml
[security.authorization]
design_audit_required_role = "developer"
design_audit_create_gates_required_role = "architect"
```

## Best Practices

### For Development

```bash
# ✅ Good: Local schema analysis
fraiseql lint ./schema.json

# ✅ Good: Filtered analysis
fraiseql lint ./schema.json --federation

# ❌ Avoid: Exposing internal schemas
fraiseql lint /etc/fraiseql/internal-schema.json
```

### For CI/CD

```yaml
# .github/workflows/design-quality.yml
name: Design Quality Gate
on: [pull_request]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Check design quality
        run: |
          fraiseql lint schema.json \
            --fail-on-critical \
            --fail-on-warning
```

### For API Usage

```bash
# ✅ Good: Use authenticated endpoint
curl -X POST http://localhost:8080/api/v1/design/audit \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"schema": {"types": [...]}}'

# ✅ Good: Validate response before using
response=$(curl -s -X POST http://localhost:8080/api/v1/design/audit \
  -H "Content-Type: application/json" \
  -d @schema.json)

if echo "$response" | jq -e '.status == "success"' > /dev/null; then
  echo "$response" | jq '.data'
fi

# ❌ Avoid: Using untrusted schema sources
curl -X POST http://localhost:8080/api/v1/design/audit \
  -d @"$(curl -s http://untrusted-server/schema.json)"
```

### For Deployment

```toml
# fraiseql-server.toml - Secure configuration

[server]
# Use TLS for all API traffic
tls_enabled = true
tls_cert_path = "/etc/fraiseql/cert.pem"
tls_key_path = "/etc/fraiseql/key.pem"

[security]
# Enable authentication
require_api_auth = true
api_key_required = true

# Rate limiting
rate_limit_enabled = true
requests_per_second = 100
burst_size = 10

# Request validation
max_schema_size_bytes = 104857600  # 100MB
max_nesting_depth = 1000
analysis_timeout_seconds = 5

[logging]
# Log all API access
log_api_requests = true
log_errors = true
```

## Security Testing

### Run Security Tests

```bash
# Test security features
cargo test --test api_design_security_tests

# Results:
# - Input validation: 6 tests
# - Resource protection: 4 tests
# - Information security: 3 tests
# - Authorization: 6 tests
```

### Security Audit Checklist

- [ ] All endpoints use HTTPS/TLS in production
- [ ] Authentication is required (if configured)
- [ ] Rate limiting is enabled
- [ ] Error messages are sanitized
- [ ] Large schemas are rejected (>100MB)
- [ ] Deep nesting is detected (>1000 levels)
- [ ] Logging is configured for audit trail
- [ ] Regular security updates applied

## Vulnerability Reporting

If you discover a security vulnerability in FraiseQL's design quality features:

1. **Do not** post it on public issue trackers
2. Email security details to: <security@fraiseql.dev>
3. Include:
   - Description of vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if available)

## Known Security Limitations

### Current (v2.0.0-alpha.1)

- Authentication is optional (configure to enforce)
- No built-in per-user schema restrictions
- Design audit does not modify persistent state

### Future Improvements

- [ ] Multi-tenant isolation
- [ ] Fine-grained RBAC for schema access
- [ ] Audit logging with cryptographic signatures
- [ ] Schema encryption at rest

## Compliance

### Standards Compliance

- ✅ OWASP Top 10 protection
- ✅ CWE vulnerability prevention
- ✅ Input validation per SANS guidelines

### Certifications

- PENDING: Security audit (Q2 2026)
- PENDING: SOC 2 Type II compliance

## References

- Benchmark tests: `crates/fraiseql-core/benches/design_analysis.rs`
- Security tests: `crates/fraiseql-server/tests/api_design_security_tests.rs`
- OWASP API Security: <https://owasp.org/www-project-api-security/>
