<!-- Skip to main content -->
---
title: GraphQL Schema Introspection Specification
description: FraiseQL provides comprehensive control over GraphQL schema introspection through a three-tier policy system. Schema introspection allows clients to query schem
keywords: ["format", "compliance", "schema", "graphql", "protocol", "specification", "standard"]
tags: ["documentation", "reference"]
---

# GraphQL Schema Introspection Specification

**Status:** Stable
**Version**: 1.0
**Last Updated**: 2026-01-11

## Overview

FraiseQL provides comprehensive control over GraphQL schema introspection through a three-tier policy system. Schema introspection allows clients to query schema information (`__schema`, `__type`, `__typename`), which is essential for development tools but poses a security risk in production environments.

This specification defines introspection policies, configuration options, enforcement mechanisms, and best practices for different deployment environments.

### Key Concepts

- **Introspection Query**: Any query accessing `__schema`, `__type`, `__typename`, or `__directive` fields
- **IntrospectionPolicy**: Configuration determining who can execute introspection queries
- **Schema Reflection**: Automatic discovery of database schema for type generation
- **Auto-Discovery**: Generating GraphQL types from PostgreSQL database schema

---

## Introspection Policies

FraiseQL provides three introspection policies to balance developer experience with security.

### DISABLED Policy (Production)

**Configuration**:

```python
<!-- Code example in Python -->
from FraiseQL import FraiseQLConfig
from FraiseQL.security.profiles.definitions import IntrospectionPolicy

config = FraiseQLConfig(
    database_url="postgresql://localhost/fraiseql_db",
    introspection_policy=IntrospectionPolicy.DISABLED,
)
```text
<!-- Code example in TEXT -->

**Environment Variable**:

```bash
<!-- Code example in BASH -->
export FRAISEQL_INTROSPECTION_POLICY=disabled
```text
<!-- Code example in TEXT -->

**Behavior**:

- ❌ No introspection queries allowed
- ❌ Blocks `__schema` queries
- ❌ Blocks `__type` queries
- ❌ Blocks `__typename` fields
- ❌ Blocks `__directive` queries
- ✅ Authentication requirement: None (blocks regardless of auth status)
- ✅ Suitable for production/public APIs

**Client Request** (rejected):

```graphql
<!-- Code example in GraphQL -->
query {
  __schema {
    types {
      name
    }
  }
}
```text
<!-- Code example in TEXT -->

**Server Response**:

```json
<!-- Code example in JSON -->
{
  "errors": [{
    "message": "GraphQL introspection is disabled",
    "extensions": {
      "code": "INTROSPECTION_DISABLED"
    }
  }]
}
```text
<!-- Code example in TEXT -->

**Use Cases**:

- Production GraphQL APIs
- Public-facing APIs with untrusted clients
- Regulated industries (financial, healthcare)
- Security-sensitive systems
- APIs where schema should not be exposed

**Security Benefits**:

- Prevents schema reconnaissance by attackers
- Hides available mutations and their signatures
- Blocks query complexity analysis via introspection
- Prevents automated attack tool operation

### AUTHENTICATED Policy (Default for STANDARD)

**Configuration**:

```python
<!-- Code example in Python -->
config = FraiseQLConfig(
    database_url="postgresql://localhost/fraiseql_db",
    introspection_policy=IntrospectionPolicy.AUTHENTICATED,
)
```text
<!-- Code example in TEXT -->

**Environment Variable**:

```bash
<!-- Code example in BASH -->
export FRAISEQL_INTROSPECTION_POLICY=authenticated
```text
<!-- Code example in TEXT -->

**Behavior**:

- ✅ Introspection allowed only for authenticated users
- ✅ Requires valid authentication (JWT, OAuth, etc.)
- ❌ Unauthenticated users blocked
- ✅ Internal development tools can introspect
- ✅ Production API consumed by internal/trusted clients

**Client Request** (unauthenticated):

```graphql
<!-- Code example in GraphQL -->
query {
  __type(name: "User") {
    name
    fields { name }
  }
}
```text
<!-- Code example in TEXT -->

**Server Response** (unauthenticated):

```json
<!-- Code example in JSON -->
{
  "errors": [{
    "message": "Authentication required for introspection",
    "extensions": {
      "code": "AUTHENTICATION_REQUIRED",
      "introspection_policy": "authenticated"
    }
  }]
}
```text
<!-- Code example in TEXT -->

**Client Request** (authenticated):

```graphql
<!-- Code example in GraphQL -->
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...

query {
  __type(name: "User") {
    name
    fields {
      name
      type { kind name }
    }
  }
}
```text
<!-- Code example in TEXT -->

**Server Response** (authenticated - success):

```json
<!-- Code example in JSON -->
{
  "data": {
    "__type": {
      "name": "User",
      "fields": [
        {"name": "id", "type": {"kind": "SCALAR", "name": "ID"}},
        {"name": "name", "type": {"kind": "SCALAR", "name": "String"}},
        {"name": "email", "type": {"kind": "SCALAR", "name": "String"}}
      ]
    }
  }
}
```text
<!-- Code example in TEXT -->

**Use Cases**:

- Staging environments
- Internal company APIs
- APIs with trusted internal clients
- Development APIs requiring authentication
- Regulatory compliance (STANDARD profile)
- GraphQL playgrounds for internal tools

**Security Characteristics**:

- Prevents external schema reconnaissance
- Allows internal development tools to function
- Requires credential possession (authentication)
- Suitable for internal APIs with known clients

### PUBLIC Policy (Development Only)

**Configuration**:

```python
<!-- Code example in Python -->
config = FraiseQLConfig(
    database_url="postgresql://localhost/fraiseql_db",
    introspection_policy=IntrospectionPolicy.PUBLIC,
)
```text
<!-- Code example in TEXT -->

**Environment Variable**:

```bash
<!-- Code example in BASH -->
export FRAISEQL_INTROSPECTION_POLICY=public
```text
<!-- Code example in TEXT -->

**Behavior**:

- ✅ Introspection allowed for all clients
- ✅ No authentication required
- ✅ Full schema disclosure
- ✅ Developer-friendly (supports IDE tooling, Apollo Studio, etc.)

**Client Request**:

```graphql
<!-- Code example in GraphQL -->
query {
  __schema {
    queryType { name }
    types {
      name
      kind
      fields { name }
    }
  }
}
```text
<!-- Code example in TEXT -->

**Server Response** (success):

```json
<!-- Code example in JSON -->
{
  "data": {
    "__schema": {
      "queryType": {"name": "Query"},
      "types": [
        {
          "name": "String",
          "kind": "SCALAR",
          "fields": null
        },
        {
          "name": "Query",
          "kind": "OBJECT",
          "fields": [
            {"name": "user"},
            {"name": "users"},
            {"name": "posts"}
          ]
        }
        // ... more types ...
      ]
    }
  }
}
```text
<!-- Code example in TEXT -->

**Use Cases**:

- Local development
- CI/CD test environments
- Public/open source APIs
- Learning and tutorial projects
- GraphQL Federation (requires introspection for entity resolution)

⚠️ **Warning**: Never use PUBLIC policy in production environments!

---

## Environment-Based Auto-Configuration

FraiseQL automatically sets introspection policy based on deployment environment:

**Automatic Policy Selection**:

```python
<!-- Code example in Python -->
from FraiseQL import FraiseQLConfig

# Development environment
config = FraiseQLConfig(
    database_url="postgresql://localhost/fraiseql_db",
    environment="development",  # Auto: IntrospectionPolicy.PUBLIC
)

# Staging environment
config = FraiseQLConfig(
    database_url="postgresql://localhost/fraiseql_db",
    environment="staging",      # Auto: IntrospectionPolicy.AUTHENTICATED
)

# Production environment
config = FraiseQLConfig(
    database_url="postgresql://localhost/fraiseql_db",
    environment="production",   # Auto: IntrospectionPolicy.DISABLED
)
```text
<!-- Code example in TEXT -->

**Environment Variables**:

```bash
<!-- Code example in BASH -->
# Automatic policy based on environment
export FRAISEQL_ENVIRONMENT=production  # Auto-sets: INTROSPECTION_POLICY=disabled

# Manual override (takes precedence)
export FRAISEQL_INTROSPECTION_POLICY=disabled
```text
<!-- Code example in TEXT -->

**Default Behavior**:

- Development: PUBLIC
- Staging: AUTHENTICATED
- Production: DISABLED
- Testing: PUBLIC (for test suites)

---

## Security Profiles and Introspection

FraiseQL's pre-configured security profiles automatically set appropriate introspection policies:

### STANDARD Profile

- Introspection Policy: **AUTHENTICATED**
- TLS: Optional
- Audit: Standard
- Rationale: Internal APIs with authentication requirement
- Suitable for: Development, staging, trusted internal users

### REGULATED Profile

- Introspection Policy: **DISABLED**
- TLS: Required (1.2+)
- Audit: Enhanced with field tracking
- Rationale: Financial/healthcare services cannot expose schema
- Suitable for: Financial services, healthcare, PCI-DSS compliance

### RESTRICTED Profile

- Introspection Policy: **DISABLED**
- TLS: Required (1.3+)
- mTLS: Required
- Audit: Verbose
- Rationale: Maximum security, zero schema exposure
- Suitable for: Government systems, critical infrastructure, military

**Usage**:

```python
<!-- Code example in Python -->
from FraiseQL.security.profiles.definitions import get_profile

# STANDARD: AUTHENTICATED introspection
profile = get_profile("standard")
config = FraiseQLConfig(
    security_profile=profile,
    database_url="postgresql://localhost/fraiseql_db",
)
# Automatic: introspection_policy=AUTHENTICATED

# REGULATED: DISABLED introspection
profile = get_profile("regulated")
config = FraiseQLConfig(
    security_profile=profile,
    database_url="postgresql://localhost/fraiseql_db",
)
# Automatic: introspection_policy=DISABLED

# RESTRICTED: DISABLED introspection
profile = get_profile("restricted")
config = FraiseQLConfig(
    security_profile=profile,
    database_url="postgresql://localhost/fraiseql_db",
)
# Automatic: introspection_policy=DISABLED
```text
<!-- Code example in TEXT -->

---

## Introspection Query Detection

FraiseQL detects introspection queries using pattern matching on reserved GraphQL field names.

### Detected Introspection Patterns

FraiseQL blocks queries containing any of these patterns:

- **`__schema`** - Schema type

  ```graphql
<!-- Code example in GraphQL -->
  query {
    __schema { types { name } }
  }
  ```text
<!-- Code example in TEXT -->

- **`__type`** - Specific type inspection

  ```graphql
<!-- Code example in GraphQL -->
  query {
    __type(name: "User") { name fields { name } }
  }
  ```text
<!-- Code example in TEXT -->

- **`__typename`** - Type name of objects

  ```graphql
<!-- Code example in GraphQL -->
  query {
    users {
      __typename
      id
      name
    }
  }
  ```text
<!-- Code example in TEXT -->

- **`__directive`** - Directive inspection

  ```graphql
<!-- Code example in GraphQL -->
  query {
    __schema {
      directives { name args { name } }
    }
  }
  ```text
<!-- Code example in TEXT -->

### Detection Behavior

**Case Insensitive**: Detection is case-insensitive

```graphql
<!-- Code example in GraphQL -->
# All of these are detected and blocked:
query { __schema { ... } }
query { __SCHEMA { ... } }
query { __Schema { ... } }
```text
<!-- Code example in TEXT -->

**Mixed Queries**: Introspection combined with regular queries is blocked

```graphql
<!-- Code example in GraphQL -->
# Blocked (contains introspection)
query {
  users { id name }
  __type(name: "User") { name }
}
```text
<!-- Code example in TEXT -->

**Implementation Detail**: Pattern matching is performed with case-lowering before comparison, as a pragmatic security measure.

---

## Error Responses

When introspection is blocked, FraiseQL returns standardized error responses.

### DISABLED Policy Error

```json
<!-- Code example in JSON -->
{
  "errors": [{
    "message": "GraphQL introspection is disabled",
    "extensions": {
      "code": "INTROSPECTION_DISABLED",
      "policy": "disabled"
    }
  }]
}
```text
<!-- Code example in TEXT -->

### AUTHENTICATED Policy Error (Unauthenticated)

```json
<!-- Code example in JSON -->
{
  "errors": [{
    "message": "Authentication required to access schema information",
    "extensions": {
      "code": "AUTHENTICATION_REQUIRED",
      "policy": "authenticated"
    }
  }]
}
```text
<!-- Code example in TEXT -->

### AUTHENTICATED Policy Error (Invalid Token)

```json
<!-- Code example in JSON -->
{
  "errors": [{
    "message": "Invalid authentication token",
    "extensions": {
      "code": "INVALID_TOKEN",
      "policy": "authenticated"
    }
  }]
}
```text
<!-- Code example in TEXT -->

### Generic Responses in Production

In production environments, error messages are intentionally generic to avoid leaking configuration details:

```json
<!-- Code example in JSON -->
{
  "errors": [{
    "message": "Introspection is not available",
    "extensions": {
      "code": "INTROSPECTION_NOT_AVAILABLE"
    }
  }]
}
```text
<!-- Code example in TEXT -->

---

## Schema Reflection and Auto-Discovery

Beyond security policies, FraiseQL provides tools to reflect on and export schema information programmatically.

### PostgreSQL Introspection

**Auto-Discovery from Database**:

FraiseQL can automatically discover GraphQL types from PostgreSQL database schema:

```python
<!-- Code example in Python -->
from FraiseQL.introspection.postgres_introspector import PostgresIntrospector

introspector = PostgresIntrospector(
    database_url="postgresql://localhost/fraiseql_db"
)

# Discover all views
views = introspector.discover_views(pattern="v_%")  # Views starting with "v_"
# Returns: [ViewMetadata, ViewMetadata, ...]

# Get view details
for view in views:
    print(f"View: {view.name}")
    for column in view.columns:
        print(f"  {column.name}: {column.pg_type} (nullable: {column.nullable})")
```text
<!-- Code example in TEXT -->

**Pattern Matching**:

```python
<!-- Code example in Python -->
# LIKE pattern (SQL wildcards)
views = introspector.discover_views(pattern="v_%")      # "v_*" pattern

# Regular expression
views = introspector.discover_views(
    pattern="^v_(user|post)s?$",
    use_regex=True
)

# Schema filtering
views = introspector.discover_views(
    pattern="%",
    schemas=["public", "staging"]  # Only these schemas
)
```text
<!-- Code example in TEXT -->

**Metadata Extraction**:

```python
<!-- Code example in Python -->
view = views[0]

# View information
print(f"View Name: {view.name}")
print(f"OID: {view.oid}")
print(f"Owner: {view.owner}")
print(f"Comment: {view.comment}")  # From PostgreSQL comment

# Column information
for col in view.columns:
    print(f"  {col.name}")
    print(f"    Type: {col.pg_type}")
    print(f"    Nullable: {col.nullable}")
    print(f"    Default: {col.default_value}")
    print(f"    Comment: {col.comment}")
```text
<!-- Code example in TEXT -->

### Type Generation from Database

**Automatic Type Creation**:

```python
<!-- Code example in Python -->
from FraiseQL.introspection.type_generator import TypeGenerator

generator = TypeGenerator(
    database_url="postgresql://localhost/fraiseql_db"
)

# Generate Python type from database view
User = generator.generate_type_from_view(
    view_name="v_users",
    type_name="User",
    type_comment="User information from database"
)

# Generated type is ready to use with @FraiseQL decorators
@FraiseQL.query
async def get_user(id: ID) -> User | None:
    # ... resolver implementation ...
```text
<!-- Code example in TEXT -->

### Type Introspection API

**Runtime Type Inspection**:

```python
<!-- Code example in Python -->
from FraiseQL.utils.introspection import describe_type

@FraiseQL.type
class User:
    id: ID
    name: str
    email: str | None = None

# Describe type at runtime
description = describe_type(User)
# Returns:
# {
#   "typename": "User",
#   "is_input": False,
#   "is_output": True,
#   "is_frozen": False,
#   "kw_only": False,
#   "fields": {
#     "id": {"type": "ID", "required": True, "description": None},
#     "name": {"type": "String", "required": True, "description": None},
#     "email": {"type": "String", "required": False, "description": None}
#   }
# }

# Access field information
for field_name, field_info in description["fields"].items():
    print(f"{field_name}: {field_info['type']} (required: {field_info['required']})")
```text
<!-- Code example in TEXT -->

---

## Production Best Practices

### Deployment Checklist

- [ ] **Introspection Policy**: Set to `DISABLED` in production
- [ ] **Environment Variable**: `FRAISEQL_INTROSPECTION_POLICY=disabled`
- [ ] **Alternative Documentation**: Provide API documentation via OpenAPI/Swagger or documentation site
- [ ] **Monitoring**: Enable logging of introspection denial attempts
- [ ] **Rate Limiting**: Apply rate limits to prevent DoS attempts
- [ ] **Security Headers**: Include CSP and other headers
- [ ] **Client Preparation**: Ensure all clients have persisted queries (APQ) instead of relying on introspection
- [ ] **Testing**: Verify introspection is blocked before deploying

### Client Alternatives to Introspection

When introspection is disabled, clients need alternative ways to discover the schema:

**1. Automatic Persisted Queries (APQ)**

- Queries pre-registered at build time
- Client sends only hash, not full query
- No introspection needed
- See: [Persisted Queries Specification](persisted-queries.md)

**2. Static Schema Export**

```bash
<!-- Code example in BASH -->
# Export schema at build time
FraiseQL schema export --format graphql --output schema.graphql
```text
<!-- Code example in TEXT -->

**3. API Documentation Site**

- Host schema documentation on separate website
- Markdown, HTML, or interactive explorer
- Updated with each release

**4. GraphQL Code Generation**

```bash
<!-- Code example in BASH -->
# Generate TypeScript types from schema (during build)
graphql-codegen --config codegen.yml
```text
<!-- Code example in TEXT -->

### Monitoring Introspection Attempts

**Security Event Logging**:

Enable security logging to track introspection attempts:

```python
<!-- Code example in Python -->
from FraiseQL.audit.security_logger import SecurityLogger

logger = SecurityLogger(
    log_file="/var/log/FraiseQL-security.log",
    log_stdout=True,
)
```text
<!-- Code example in TEXT -->

**Log Example**:

```json
<!-- Code example in JSON -->
{
  "timestamp": "2025-01-11T10:30:45Z",
  "event_type": "QUERY_REJECTED",
  "severity": "WARNING",
  "ip_address": "192.0.2.1",
  "reason": "GraphQL introspection is disabled",
  "request_id": "req-abc123",
  "metadata": {
    "query_contains": "__schema",
    "policy": "disabled"
  }
}
```text
<!-- Code example in TEXT -->

**WAF Integration** (CrowdSec):

```yaml
<!-- Code example in YAML -->
# Deploy WAF rule to block introspection attempts
type: trigger
name: FraiseQL/graphql-introspection
description: "Detect GraphQL introspection queries"
filter: |
  evt.Meta.log_type == 'nginx' &&
  (evt.Parsed.request contains '__schema' ||
   evt.Parsed.request contains '__type')
blackhole: 1h
```text
<!-- Code example in TEXT -->

### Rate Limiting Introspection

If introspection is AUTHENTICATED, rate-limit it:

```python
<!-- Code example in Python -->
rate_limit_config = RateLimitConfig(
    strategies={
        # Introspection queries allowed but heavily rate-limited
        "introspection": {
            "limit": 5,           # 5 introspection queries/minute
            "window": 60,
            "per": "user",        # Per authenticated user
        },
        "query": {
            "limit": 100,         # Regular queries higher limit
            "window": 60,
        },
    }
)
```text
<!-- Code example in TEXT -->

---

## Testing Introspection Policies

### Test Cases

**DISABLED Policy - All Requests Blocked**:

```python
<!-- Code example in Python -->
import pytest
from FraiseQL import FraiseQLConfig
from FraiseQL.security.profiles.definitions import IntrospectionPolicy

@pytest.mark.asyncio
async def test_introspection_disabled_blocks_schema_query():
    config = FraiseQLConfig(
        database_url="postgresql://test_db",
        introspection_policy=IntrospectionPolicy.DISABLED,
    )

    query = "query { __schema { types { name } } }"
    result = await schema.execute(query, context_value={})

    assert result.errors
    assert any("introspection" in str(e).lower() for e in result.errors)
```text
<!-- Code example in TEXT -->

**AUTHENTICATED Policy - Auth Required**:

```python
<!-- Code example in Python -->
@pytest.mark.asyncio
async def test_introspection_authenticated_requires_auth():
    config = FraiseQLConfig(
        introspection_policy=IntrospectionPolicy.AUTHENTICATED,
    )

    # Unauthenticated request
    query = "query { __type(name: \"User\") { name } }"
    result = await schema.execute(query, context_value={})

    assert result.errors
    assert "authentication" in str(result.errors[0]).lower()

@pytest.mark.asyncio
async def test_introspection_authenticated_succeeds_with_auth():
    config = FraiseQLConfig(
        introspection_policy=IntrospectionPolicy.AUTHENTICATED,
    )

    # Authenticated request
    query = "query { __type(name: \"User\") { name } }"
    context = {"user_id": "user-123"}
    result = await schema.execute(query, context_value=context)

    assert not result.errors
    assert result.data["__type"]["name"] == "User"
```text
<!-- Code example in TEXT -->

**PUBLIC Policy - All Allowed**:

```python
<!-- Code example in Python -->
@pytest.mark.asyncio
async def test_introspection_public_allows_all():
    config = FraiseQLConfig(
        introspection_policy=IntrospectionPolicy.PUBLIC,
    )

    query = "query { __schema { types { name } } }"
    result = await schema.execute(query, context_value={})

    assert not result.errors
    assert result.data["__schema"]["types"]
```text
<!-- Code example in TEXT -->

### Integration Tests

```python
<!-- Code example in Python -->
@pytest.mark.asyncio
async def test_introspection_mixed_query_rejected():
    """Introspection combined with regular query should be rejected."""
    config = FraiseQLConfig(
        introspection_policy=IntrospectionPolicy.DISABLED,
    )

    query = """
    query {
      users { id name }
      __type(name: "User") { name }
    }
    """
    result = await schema.execute(query)

    assert result.errors
    assert "introspection" in str(result.errors[0]).lower()
```text
<!-- Code example in TEXT -->

---

## Configuration Examples

### Development Environment

```python
<!-- Code example in Python -->
# config/development.py
from FraiseQL import FraiseQLConfig
from FraiseQL.security.profiles.definitions import IntrospectionPolicy

config = FraiseQLConfig(
    database_url="postgresql://localhost/fraiseql_dev",
    environment="development",
    introspection_policy=IntrospectionPolicy.PUBLIC,  # Explicit is better
    # OR:
    # security_profile=get_profile("standard"),  # Auto: AUTHENTICATED
)
```text
<!-- Code example in TEXT -->

**Environment Variables**:

```bash
<!-- Code example in BASH -->
FRAISEQL_ENVIRONMENT=development
FRAISEQL_INTROSPECTION_POLICY=public
```text
<!-- Code example in TEXT -->

### Staging Environment

```python
<!-- Code example in Python -->
# config/staging.py
config = FraiseQLConfig(
    database_url="postgresql://pg-staging/fraiseql_db",
    environment="staging",
    introspection_policy=IntrospectionPolicy.AUTHENTICATED,
    # OR:
    # security_profile=get_profile("regulated"),  # Auto: DISABLED + enhanced audit
)
```text
<!-- Code example in TEXT -->

**Environment Variables**:

```bash
<!-- Code example in BASH -->
FRAISEQL_ENVIRONMENT=staging
FRAISEQL_INTROSPECTION_POLICY=authenticated
```text
<!-- Code example in TEXT -->

### Production Environment

```python
<!-- Code example in Python -->
# config/production.py
from FraiseQL.security.profiles.definitions import get_profile

# Maximum security profile
profile = get_profile("restricted")  # Auto: DISABLED introspection
config = FraiseQLConfig(
    database_url="postgresql://pg-prod/fraiseql_db",
    environment="production",
    security_profile=profile,
    # OR explicit:
    # introspection_policy=IntrospectionPolicy.DISABLED,
)
```text
<!-- Code example in TEXT -->

**Environment Variables**:

```bash
<!-- Code example in BASH -->
FRAISEQL_ENVIRONMENT=production
FRAISEQL_INTROSPECTION_POLICY=disabled
FRAISEQL_SECURITY_PROFILE=restricted
```text
<!-- Code example in TEXT -->

---

## API Documentation Without Introspection

When introspection is disabled, provide schema documentation through these alternatives:

### 1. Static Schema Export

```bash
<!-- Code example in BASH -->
# Export schema at build time
FraiseQL schema export \
  --format graphql \
  --output ./schema.graphql

# Upload to documentation site
# Schema version matches app version
```text
<!-- Code example in TEXT -->

### 2. OpenAPI/Swagger Documentation

```bash
<!-- Code example in BASH -->
# Convert GraphQL schema to OpenAPI
graphql-to-openapi \
  --input schema.graphql \
  --output api-docs.json
```text
<!-- Code example in TEXT -->

### 3. Apollo Studio

Apollo Server integration provides a Sandbox editor with SDL (Schema Definition Language):

```python
<!-- Code example in Python -->
from FraiseQL.fastapi import create_fraiseql_app

app = create_fraiseql_app(
    schema,
    config=config,
    # Apollo Sandbox available even with introspection disabled
    sandbox_enabled=True,  # Requires static schema upload
)
```text
<!-- Code example in TEXT -->

### 4. Markdown Documentation

Maintain hand-written documentation:

```markdown
<!-- Code example in MARKDOWN -->
# GraphQL API

## Query: users

Returns a list of users.

**Arguments:**
- `limit: Int!` - Maximum number of users
- `offset: Int` - Skip first N users

**Return Type:** `[User!]!`

**Example:**
```graphql
<!-- Code example in GraphQL -->
query {
  users(limit: 10) {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

```text
<!-- Code example in TEXT -->

---

## Conclusion

FraiseQL's three-tier introspection policy system provides flexible security for different deployment environments. By using DISABLED introspection in production and AUTHENTICATED or PUBLIC in development, you achieve both security (preventing schema reconnaissance) and usability (allowing development tools to function).

**Key Takeaways**:

- ✅ Use DISABLED in production (prevents schema exposure)
- ✅ Use AUTHENTICATED in staging (requires authentication)
- ✅ Use PUBLIC in development (full schema access)
- ✅ Implement security profiles for bundled settings
- ✅ Provide alternative documentation (schema export, OpenAPI)
- ✅ Monitor introspection denial attempts via security logging
- ✅ Rate-limit introspection queries to prevent abuse
