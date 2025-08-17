# Security Advisory: GraphQL Introspection Vulnerability (CVE-TBD)

**Severity**: Medium
**CVSS Score**: 5.3 (AV:N/AC:L/PR:N/UI:N/S:U/C:L/I:N/A:N)
**Product**: FraiseQL
**Affected Versions**: < 0.3.5
**Fixed Version**: 0.3.5
**Release Date**: 2025-08-17

## Summary

FraiseQL versions prior to 0.3.5 incorrectly exposed GraphQL schema introspection in production environments, allowing unauthorized discovery of API structure and sensitive information.

## Vulnerability Details

### Description
FraiseQL had a configuration setting `enable_introspection` that was intended to disable GraphQL introspection in production environments. However, this setting was not properly enforced during GraphQL query execution, allowing introspection queries to succeed even when the setting was disabled.

### Attack Vector
An attacker could send introspection queries to production FraiseQL endpoints:

```graphql
# Schema discovery
{
  __schema {
    types {
      name
      fields {
        name
        type {
          name
        }
      }
    }
  }
}

# Type discovery
{
  __type(name: "User") {
    fields {
      name
      type {
        name
      }
    }
  }
}
```

### Information Exposed
- Complete GraphQL schema structure
- All available queries, mutations, and subscriptions
- Field names and types
- Input type structures
- Enum values
- Documentation strings
- Deprecated field information

### Impact
**Information Disclosure (CWE-200)**
- Attackers could map the entire API surface
- Discovery of sensitive field names and structures
- Understanding of business logic through schema
- Identification of potential attack vectors
- Reconnaissance for further exploitation

## Affected Versions

**All versions < 0.3.5** are affected, including:
- 0.3.4 and earlier
- All 0.2.x versions
- All 0.1.x versions
- All beta and development versions

## Fix Details

### Root Cause
The `enable_introspection` configuration was only used for:
1. Security header configuration
2. Authentication bypass logic
3. **NOT** for actual GraphQL query validation

### Solution
FraiseQL 0.3.5 implements proper introspection control by:

1. **Query Validation**: Uses GraphQL's built-in `NoSchemaIntrospectionCustomRule`
2. **Early Blocking**: Validation occurs before query execution
3. **Comprehensive Coverage**: Blocks all introspection fields (`__schema`, `__type`, etc.)
4. **Production Default**: Automatically disabled when `environment="production"`

### Technical Implementation
```python
# Added to src/fraiseql/graphql/execute.py
if not enable_introspection:
    from graphql import NoSchemaIntrospectionCustomRule
    validation_errors = validate(schema, document, [NoSchemaIntrospectionCustomRule])
    if validation_errors:
        return ExecutionResult(data=None, errors=validation_errors)
```

## Remediation

### Immediate Action
**Upgrade to FraiseQL 0.3.5 immediately**

```bash
pip install --upgrade fraiseql==0.3.5
```

### Verification
Test that introspection is blocked in production:

```bash
# Should return an error, not schema data
curl -X POST http://your-api.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { queryType { name } } }"}'
```

Expected response:
```json
{
  "errors": [
    {
      "message": "GraphQL introspection has been disabled, but the requested query contained the field '__schema'."
    }
  ]
}
```

### Configuration Check
Ensure production configuration:

```python
config = FraiseQLConfig(
    environment="production",  # This automatically disables introspection
    # enable_introspection is automatically False in production
)
```

### Emergency Mitigation (if upgrade not possible immediately)
If immediate upgrade is not possible, implement WAF rules to block introspection:

```nginx
# Nginx WAF rule
location /graphql {
    if ($request_body ~ "__schema|__type") {
        return 403 "Introspection not allowed";
    }
    proxy_pass http://backend;
}
```

## Timeline

- **2025-08-17**: Vulnerability discovered during security review
- **2025-08-17**: Fix implemented using TDD methodology
- **2025-08-17**: Comprehensive test suite added
- **2025-08-17**: Version 0.3.5 released with fix
- **2025-08-17**: Security advisory published

## Detection

### Log Analysis
Look for introspection queries in your logs:
```bash
grep -i "__schema\|__type" /var/log/your-app.log
```

### Monitoring
Add alerts for introspection attempts:
```python
# Add to your monitoring
if "__schema" in query or "__type" in query:
    security_logger.warning("Introspection attempt detected", extra={
        "query": query,
        "ip": client_ip,
        "timestamp": datetime.utcnow()
    })
```

## References

- [GraphQL Security Best Practices](https://cheatsheetseries.owasp.org/cheatsheets/GraphQL_Cheat_Sheet.html#introspection)
- [OWASP API Security Top 10 - API3:2023 Broken Object Property Level Authorization](https://owasp.org/API-Security/editions/2023/en/0xa3-broken-object-property-level-authorization/)
- [CWE-200: Information Exposure](https://cwe.mitre.org/data/definitions/200.html)

## Contact

For questions about this security advisory:
- **Security**: Create an issue at https://github.com/fraiseql/fraiseql/issues
- **General**: lionel.hamayon@evolution-digitale.fr

## Acknowledgments

This vulnerability was discovered and fixed through comprehensive security review and Test-Driven Development (TDD) methodology.
