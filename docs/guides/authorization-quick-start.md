# Authorization & RBAC Quick Start (5 Minutes)

**Status:** ✅ Production Ready
**Audience:** Developers, DevOps, Architects
**Reading Time:** 5-7 minutes
**Last Updated:** 2026-02-05

Get field-level and operation-level authorization working in 5 minutes.

## Prerequisites

- Basic FraiseQL project setup (see [Getting Started](../../))
- Understanding of roles (admin, user, guest)
- Knowledge of your authentication provider (see [Auth Provider Selection](../integrations/authentication/provider-selection-guide.md))

## Step 1: Define Authorization Rules (1 minute)

```python
# users_service/schema.py
from fraiseql import type, field, authorize

@type
class User:
    id: str
    name: str
    email: str = field(authorize={"read": ["admin", "self"]})  # Only admin or user's own
    salary: float = field(authorize={"read": ["admin"]})       # Admin only
    role: str = field(authorize={"read": ["admin"]})           # Admin only

@type
class Order:
    id: str
    user_id: str
    total: float = field(authorize={"read": ["admin", "owner"]})  # Admin or order owner
```

---

## Step 2: Add Authorization to Queries (1 minute)

```python
# users_service/schema.py (continued)
from fraiseql import query, authorize

@query
@authorize(roles=["admin", "user"])
def users(limit: int = 10) -> list[User]:
    """List users - requires admin or user role"""
    pass

@query
@authorize(roles=["admin"])
def all_users(limit: int = 100) -> list[User]:
    """List all users with sensitive data - admin only"""
    pass

@query
@authorize(requires_context={"user_id"})  # Must have user_id in context
def my_orders(limit: int = 50) -> list[Order]:
    """List current user's orders"""
    pass
```

---

## Step 3: Configure Authorization Provider (1 minute)

```toml
# fraiseql.toml
[fraiseql.authentication]
provider = "oauth2"
discovery_url = "https://auth.example.com/.well-known/openid-configuration"

[fraiseql.authorization]
strategy = "jwt-claims"
roles_claim = "roles"
user_id_claim = "sub"
cache_ttl_seconds = 300

# Field-level enforcement
[fraiseql.authorization.enforcement]
level = "field"          # "operation" (queries only) or "field" (queries + fields)
fail_closed = true       # Deny if no explicit permission
audit_log = true         # Log all authorization decisions
```

---

## Step 4: Deploy and Test (2 minutes)

```bash
# Compile schema with authorization
fraiseql compile --config ./fraiseql.toml

# Start server
fraiseql run --port 8000

# Test: Query as admin (should see all fields)
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <admin_token>" \
  -d '{
    "query": "{ users(limit: 1) { id name email salary } }"
  }'

# Expected response (admin sees everything):
# {
#   "data": {
#     "users": [
#       {
#         "id": "1",
#         "name": "Alice",
#         "email": "alice@example.com",
#         "salary": 100000
#       }
#     ]
#   }
# }

# Test: Query as regular user (should NOT see salary)
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <user_token>" \
  -d '{
    "query": "{ users(limit: 1) { id name email salary } }"
  }'

# Expected response (user blocked from salary field):
# {
#   "errors": [
#     {
#       "message": "Unauthorized to access field 'salary'",
#       "extensions": {
#         "field": "salary",
#         "required_roles": ["admin"]
#       }
#     }
#   ],
#   "data": {
#     "users": [
#       {
#         "id": "1",
#         "name": "Alice",
#         "email": null  # Blocked
#       }
#     ]
#   }
# }
```

---

## That's It

You now have role-based field-level authorization! 🔐

### Next Steps

- Set up RBAC with Azure AD, Auth0, or Keycloak (see [Auth Provider Selection](../integrations/authentication/provider-selection-guide.md))
- Configure audit logging for compliance (see [Observability Guide](../guides/observability.md))
- Implement attribute-based access control (ABAC) for fine-grained control (see [RBAC Patterns](./PATTERNS.md#role-based-access-control))
- Understand authorization in federation (see [Federation Guide](../integrations/federation/guide.md))

### Common Issues

**"Unauthorized to access query"**
→ Token missing or expired. Check `Authorization: Bearer <token>` header and verify token is valid for the required roles.

**"Field blocked but no error"**
→ With `fail_closed = true`, fields are silently filtered. Set `fail_closed = false` in tests to see blocking errors.

**"Same token works in dev, fails in production"**
→ Check `discovery_url` in `fraiseql.toml` is accessible in production, and CORS headers are configured correctly.

**"Authorization too slow"**
→ Increase `cache_ttl_seconds` from 300 to 3600. Or use JWT claims directly instead of external provider calls.

See [Troubleshooting](../../TROUBLESHOOTING.md) for complete troubleshooting guide.

---

## See Also

- **[Auth Provider Selection](../integrations/authentication/provider-selection-guide.md)** - Choosing your auth provider
- **[Observability](./observability.md)** - Logging and monitoring authorization
- **[RBAC Patterns](./PATTERNS.md#role-based-access-control)** - Real-world RBAC examples
- **[Federation](../integrations/federation/guide.md)** - Cross-service authorization in federation
