# Error Codes Reference

Complete reference of FraiseQL error codes for programmatic error handling.

## Standard Error Codes

FraiseQL uses standardized error codes in GraphQL error extensions for consistent client-side handling.

| Code | HTTP Status | Category | Description | Common Causes | Resolution |
|------|-------------|----------|-------------|---------------|------------|
| `UNAUTHENTICATED` | 401 | Auth | Authentication required | Missing/invalid token | Provide valid auth token |
| `FORBIDDEN` | 403 | Auth | Permission denied | Insufficient permissions | Check user permissions |
| `NOT_FOUND` | 404 | Query | Resource not found | Invalid ID, deleted resource | Verify resource exists |
| `VALIDATION_ERROR` | 400 | Input | Input validation failed | Invalid format, missing fields | Fix input data |
| `DUPLICATE_ENTRY` | 409 | Constraint | Unique constraint violation | Duplicate key value | Use different value |
| `INTERNAL_SERVER_ERROR` | 500 | System | Unexpected server error | Bug, system failure | Check logs, contact support |
| `TIMEOUT` | 408 | System | Operation timeout | Long-running query | Optimize query, increase timeout |
| `RATE_LIMITED` | 429 | System | Rate limit exceeded | Too many requests | Implement backoff strategy |
| `BAD_REQUEST` | 400 | Input | Malformed request | Invalid query syntax | Fix query structure |
| `CONFLICT` | 409 | State | Resource conflict | Concurrent modification | Retry with latest data |

## Database Error Codes

PostgreSQL-specific error codes mapped to GraphQL errors.

| Code | PostgreSQL Code | Description | Example | Resolution |
|------|----------------|-------------|---------|------------|
| `DATABASE_CONNECTION_ERROR` | 08000 | Connection failure | Server unavailable | Check database status |
| `UNIQUE_VIOLATION` | 23505 | Unique constraint violated | Duplicate email | Use different value |
| `FOREIGN_KEY_VIOLATION` | 23503 | Foreign key constraint violated | Invalid reference | Ensure referenced record exists |
| `CHECK_VIOLATION` | 23514 | Check constraint violated | Invalid value range | Use valid value |
| `NOT_NULL_VIOLATION` | 23502 | NULL in non-null column | Missing required field | Provide required value |
| `DEADLOCK_DETECTED` | 40P01 | Transaction deadlock | Concurrent updates | Retry transaction |
| `SERIALIZATION_FAILURE` | 40001 | Transaction isolation conflict | Concurrent modifications | Retry transaction |
| `INSUFFICIENT_PRIVILEGE` | 42501 | Permission denied | No table access | Grant permissions |
| `UNDEFINED_TABLE` | 42P01 | Table/view doesn't exist | Missing migration | Run migrations |
| `UNDEFINED_COLUMN` | 42703 | Column doesn't exist | Schema mismatch | Update schema |

## Business Logic Error Codes

Application-specific error codes for business rules.

| Code | Category | Description | Use Case | Client Action |
|------|----------|-------------|----------|---------------|
| `INSUFFICIENT_FUNDS` | Payment | Not enough balance | Payment processing | Add funds |
| `EXPIRED_TOKEN` | Auth | Token expired | Session timeout | Refresh token |
| `INVALID_STATE` | Workflow | Invalid state transition | Order already shipped | Check current state |
| `QUOTA_EXCEEDED` | Limits | Resource quota exceeded | Storage limit | Upgrade plan |
| `DUPLICATE_EMAIL` | User | Email already registered | User signup | Use different email |
| `DUPLICATE_USERNAME` | User | Username taken | User signup | Choose different username |
| `INVALID_CREDENTIALS` | Auth | Wrong username/password | Login | Check credentials |
| `ACCOUNT_LOCKED` | Auth | Account temporarily locked | Too many failed attempts | Wait or reset password |
| `EMAIL_NOT_VERIFIED` | Auth | Email verification required | Account activation | Verify email |
| `PAYMENT_REQUIRED` | Billing | Payment needed | Subscription expired | Update payment method |

## Field-Level Error Codes

Validation codes for specific field types.

| Code | Field Type | Description | Example | Fix |
|------|------------|-------------|---------|-----|
| `INVALID_EMAIL` | Email | Invalid email format | `"not-an-email"` | Use valid email format |
| `INVALID_URL` | URL | Invalid URL format | `"not a url"` | Use valid URL with scheme |
| `INVALID_IP` | IP Address | Invalid IP address | `"999.999.999.999"` | Use valid IPv4/IPv6 |
| `INVALID_PORT` | Port | Port out of range | `99999` | Use port 1-65535 |
| `INVALID_HOSTNAME` | Hostname | Invalid hostname | `"host name"` | Remove spaces/special chars |
| `INVALID_UUID` | UUID | Invalid UUID format | `"not-a-uuid"` | Use valid UUID v4 |
| `INVALID_DATE` | Date | Invalid date format | `"32/13/2024"` | Use ISO 8601 format |
| `INVALID_JSON` | JSON | Invalid JSON structure | `"{not json}"` | Fix JSON syntax |
| `FIELD_TOO_LONG` | String | Exceeds max length | 256 char limit | Shorten value |
| `FIELD_TOO_SHORT` | String | Below min length | Min 3 chars | Lengthen value |

## Mutation Status Codes

Status codes returned by PostgreSQL functions in mutations.

| Status | Code | Success | Description | Client Handling |
|--------|------|---------|-------------|-----------------|
| `success` | 200 | ✅ | Operation successful | Continue |
| `created` | 201 | ✅ | Resource created | Get new resource |
| `updated` | 200 | ✅ | Resource updated | Refresh data |
| `deleted` | 204 | ✅ | Resource deleted | Remove from UI |
| `not_found` | 404 | ❌ | Resource not found | Show not found message |
| `unauthorized` | 401 | ❌ | Not authenticated | Redirect to login |
| `forbidden` | 403 | ❌ | Not authorized | Show permission error |
| `invalid_input` | 400 | ❌ | Validation failed | Show validation errors |
| `conflict` | 409 | ❌ | Resource conflict | Handle conflict |
| `error` | 500 | ❌ | Server error | Show error message |

## Using Error Codes

### In Queries

```python
@query
async def get_user(info, id: str):
    user = await repo.find_one("v_user", where={"id": id})
    if not user:
        raise GraphQLError(
            message="User not found",
            extensions={"code": "NOT_FOUND"}
        )
    return user
```

### In Mutations

```python
@mutation
async def create_user(info, input):
    try:
        # Call PostgreSQL function
        result = await repo.call_function(
            "fn_create_user",
            p_email=input.email
        )

        if result["status"] == "duplicate_email":
            raise GraphQLError(
                message="Email already exists",
                extensions={"code": "DUPLICATE_EMAIL"}
            )

        return result

    except IntegrityError as e:
        if "users_email_key" in str(e):
            raise GraphQLError(
                message="Email already in use",
                extensions={"code": "UNIQUE_VIOLATION"}
            )
        raise
```

### Client-Side Handling

```typescript
// TypeScript/JavaScript client
try {
  const result = await client.query(GET_USER, { id: "123" });
  // Handle success
} catch (error) {
  if (error.graphQLErrors?.[0]?.extensions?.code === "NOT_FOUND") {
    // Handle not found
    showNotFoundMessage();
  } else if (error.graphQLErrors?.[0]?.extensions?.code === "UNAUTHENTICATED") {
    // Redirect to login
    redirectToLogin();
  } else {
    // Handle other errors
    showGenericError();
  }
}
```

### Python Client

```python
from gql import gql, Client
from gql.transport.exceptions import TransportQueryError

try:
    result = client.execute(query)
except TransportQueryError as e:
    errors = e.errors
    if errors and errors[0].get("extensions", {}).get("code") == "NOT_FOUND":
        print("User not found")
    elif errors and errors[0].get("extensions", {}).get("code") == "FORBIDDEN":
        print("Permission denied")
    else:
        print(f"Error: {errors[0]['message']}")
```

## Custom Error Codes

Define custom error codes for your application:

```python
# Define custom codes
class CustomErrorCodes:
    TRIAL_EXPIRED = "TRIAL_EXPIRED"
    FEATURE_DISABLED = "FEATURE_DISABLED"
    MAINTENANCE_MODE = "MAINTENANCE_MODE"

# Use in resolver
if user.trial_expired:
    raise GraphQLError(
        message="Trial period has expired",
        extensions={
            "code": CustomErrorCodes.TRIAL_EXPIRED,
            "upgrade_url": "/pricing"
        }
    )
```

## Error Code Best Practices

1. **Use Standard Codes** - Prefer standard codes over custom ones
2. **Be Specific** - Use the most specific code that applies
3. **Include Context** - Add relevant data in extensions
4. **Document Custom Codes** - Document any custom codes you create
5. **Consistent Mapping** - Map database errors consistently
6. **Client Libraries** - Create client error handling utilities

## HTTP Status Mapping

FraiseQL maps error codes to HTTP status codes for REST compatibility:

```python
ERROR_CODE_TO_HTTP = {
    "UNAUTHENTICATED": 401,
    "FORBIDDEN": 403,
    "NOT_FOUND": 404,
    "VALIDATION_ERROR": 400,
    "DUPLICATE_ENTRY": 409,
    "INTERNAL_SERVER_ERROR": 500,
    "TIMEOUT": 408,
    "RATE_LIMITED": 429,
}
```

## Next Steps

- Implement [error handling patterns](./handling-patterns.md)
- Review [troubleshooting guide](./troubleshooting.md)
- Set up [error monitoring](./debugging.md#error-monitoring)
