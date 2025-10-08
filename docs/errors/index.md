# Error Handling

FraiseQL provides comprehensive error handling with developer-friendly messages, structured error codes, and production-ready patterns.

## Quick Navigation

- **[Error Types](./error-types.md)** - All error categories and when they occur
- **[Error Codes](./error-codes.md)** - Complete reference of error codes
- **[Handling Patterns](./handling-patterns.md)** - Best practices for error handling
- **[Troubleshooting](./troubleshooting.md)** - Common issues and solutions
- **[Debugging](./debugging.md)** - Debugging strategies and tools

## Error Philosophy

FraiseQL's error handling is designed around three principles:

1. **Developer Experience First** - Errors should help developers fix problems quickly
2. **Security by Default** - Production errors never leak sensitive information
3. **Context Preservation** - Errors maintain full context for debugging

## Error System Architecture

FraiseQL implements a dual-layer error system:

### Core Exceptions
Lightweight exceptions for internal operations:

- `FraiseQLError` - Base exception class
- `SchemaError` - Schema-related issues
- `ValidationError` - Input validation failures
- `AuthenticationError` - Auth failures
- `AuthorizationError` - Permission denied

### Enhanced Exceptions
Rich, context-aware exceptions with:

- Query context information
- Helpful resolution hints
- Structured error codes
- Proper exception chaining
- Documentation links

## Basic Error Handling

### Query Errors

```python
from fraiseql import query
from graphql import GraphQLError

@query
async def get_user(info, id: str):
    repo = info.context["repo"]
    user = await repo.find_one("v_user", where={"id": id})

    if not user:
        raise GraphQLError(
            message=f"User {id} not found",
            extensions={"code": "NOT_FOUND", "id": id}
        )

    return user
```

### Mutation Errors

```python
from fraiseql import mutation
from graphql import GraphQLError

@mutation
async def create_post(info, input):
    repo = info.context["repo"]

    try:
        post_id = await repo.call_function(
            "fn_create_post",
            p_title=input.title,
            p_content=input.content
        )
        return await repo.find_one("v_post", where={"id": post_id})

    except IntegrityError as e:
        if "unique_title" in str(e):
            raise GraphQLError(
                message="Post title must be unique",
                extensions={"code": "DUPLICATE_TITLE"}
            )
        raise
```

## Error Response Format

FraiseQL returns errors in standard GraphQL format:

```json
{
  "data": null,
  "errors": [
    {
      "message": "User not found",
      "path": ["getUser"],
      "extensions": {
        "code": "NOT_FOUND",
        "id": "123"
      }
    }
  ]
}
```

## Production vs Development

### Development Mode

- Full error details with stack traces
- SQL query context
- Helpful hints and suggestions
- Documentation links

### Production Mode

- Sanitized error messages
- No internal details leaked
- Structured error codes for clients
- Full logging for operators

## Common Error Scenarios

### Authentication Required
```python
raise GraphQLError(
    message="Authentication required",
    extensions={"code": "UNAUTHENTICATED"}
)
```

### Permission Denied
```python
raise GraphQLError(
    message="Permission denied",
    extensions={
        "code": "FORBIDDEN",
        "required_permission": "posts.write"
    }
)
```

### Validation Failed
```python
raise GraphQLError(
    message="Invalid email format",
    extensions={
        "code": "VALIDATION_ERROR",
        "field": "email",
        "value": user_input
    }
)
```

## Error Monitoring

FraiseQL integrates with monitoring systems:

```python
# Automatic error tracking
from fraiseql.monitoring import track_error

try:
    # Operation
    pass
except Exception as e:
    track_error(e, context={"user_id": user_id})
    raise
```

## Next Steps

- Learn about [specific error types](./error-types.md)
- Review the [error codes reference](./error-codes.md)
- Implement [proper error handling patterns](./handling-patterns.md)
- Troubleshoot [common issues](./troubleshooting.md)
