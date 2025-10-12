# Error Handling Patterns

Best practices and patterns for handling errors in FraiseQL applications.

## Core Principles

1. **Fail Fast** - Detect and report errors early
2. **Be Specific** - Provide detailed, actionable error messages
3. **Preserve Context** - Maintain error context through the stack
4. **Security First** - Never leak sensitive data in production
5. **User-Friendly** - Help developers fix issues quickly

## Query Error Handling

### Basic Pattern

```python
from fraiseql import query
from graphql import GraphQLError

@query
async def get_post(info, id: str):
    """Safe query with proper error handling."""
    repo = info.context["repo"]

    # Validate input
    if not id:
        raise GraphQLError(
            message="Post ID is required",
            extensions={"code": "VALIDATION_ERROR", "field": "id"}
        )

    # Execute query with error handling
    try:
        post = await repo.find_one("v_post", where={"id": id})

        if not post:
            raise GraphQLError(
                message=f"Post {id} not found",
                extensions={"code": "NOT_FOUND", "id": id}
            )

        return post

    except DatabaseError as e:
        # Log the actual error for debugging
        logger.error(f"Database error fetching post {id}: {e}")

        # Return user-friendly error
        raise GraphQLError(
            message="Failed to fetch post",
            extensions={"code": "INTERNAL_SERVER_ERROR"}
        ) from e
```

### Batch Query Pattern

```python
@query
async def get_posts(info, ids: List[str]):
    """Batch query with partial failure handling."""
    repo = info.context["repo"]
    results = []
    errors = []

    for id in ids:
        try:
            post = await repo.find_one("v_post", where={"id": id})
            if post:
                results.append(post)
            else:
                errors.append({
                    "id": id,
                    "error": "NOT_FOUND"
                })
        except Exception as e:
            logger.error(f"Error fetching post {id}: {e}")
            errors.append({
                "id": id,
                "error": "FETCH_ERROR"
            })

    # Return partial results with error info
    return {
        "posts": results,
        "errors": errors if errors else None
    }
```

## Mutation Error Handling

### Transaction Pattern

```python
from fraiseql import mutation
from contextlib import asynccontextmanager

@mutation
async def transfer_funds(info, from_account: str, to_account: str, amount: float):
    """Mutation with transaction and rollback on error."""
    repo = info.context["repo"]

    async with repo.transaction() as tx:
        try:
            # Validate accounts exist
            sender = await tx.find_one("v_account", where={"id": from_account})
            if not sender:
                raise GraphQLError(
                    message="Sender account not found",
                    extensions={"code": "NOT_FOUND", "account": from_account}
                )

            receiver = await tx.find_one("v_account", where={"id": to_account})
            if not receiver:
                raise GraphQLError(
                    message="Receiver account not found",
                    extensions={"code": "NOT_FOUND", "account": to_account}
                )

            # Check balance
            if sender["balance"] < amount:
                raise GraphQLError(
                    message="Insufficient funds",
                    extensions={
                        "code": "INSUFFICIENT_FUNDS",
                        "available": sender["balance"],
                        "requested": amount
                    }
                )

            # Perform transfer
            await tx.call_function(
                "fn_transfer_funds",
                p_from=from_account,
                p_to=to_account,
                p_amount=amount
            )

            # Return updated accounts
            return {
                "from_account": await tx.find_one("v_account", where={"id": from_account}),
                "to_account": await tx.find_one("v_account", where={"id": to_account})
            }

        except GraphQLError:
            # Re-raise GraphQL errors
            raise
        except Exception as e:
            # Log unexpected errors
            logger.error(f"Transfer failed: {e}")
            raise GraphQLError(
                message="Transfer failed",
                extensions={"code": "INTERNAL_SERVER_ERROR"}
            ) from e
```

### Status-Based Pattern

```python
@mutation
async def create_order(info, input):
    """Mutation using PostgreSQL function status codes."""
    repo = info.context["repo"]

    result = await repo.call_function(
        "fn_create_order",
        p_user_id=input.user_id,
        p_items=input.items
    )

    # Handle different status codes
    status = result.get("status")

    if status == "success":
        return await repo.find_one("v_order", where={"id": result["id"]})

    elif status == "invalid_items":
        raise GraphQLError(
            message="One or more items are invalid",
            extensions={
                "code": "VALIDATION_ERROR",
                "invalid_items": result.get("invalid_items", [])
            }
        )

    elif status == "out_of_stock":
        raise GraphQLError(
            message="Items out of stock",
            extensions={
                "code": "OUT_OF_STOCK",
                "items": result.get("out_of_stock_items", [])
            }
        )

    elif status == "user_not_found":
        raise GraphQLError(
            message="User not found",
            extensions={"code": "NOT_FOUND"}
        )

    else:
        # Unexpected status
        logger.error(f"Unexpected status from fn_create_order: {status}")
        raise GraphQLError(
            message="Failed to create order",
            extensions={"code": "INTERNAL_SERVER_ERROR"}
        )
```

## Authentication Error Patterns

### Auth Middleware Pattern

```python
from functools import wraps

def require_auth(f):
    """Decorator to require authentication."""
    @wraps(f)
    async def wrapper(info, *args, **kwargs):
        user = info.context.get("user")

        if not user:
            raise GraphQLError(
                message="Authentication required",
                extensions={"code": "UNAUTHENTICATED"}
            )

        return await f(info, *args, **kwargs)

    return wrapper

@query
@require_auth
async def get_profile(info):
    """Query requiring authentication."""
    user = info.context["user"]
    return await info.context["repo"].find_one(
        "v_user",
        where={"id": user.id}
    )
```

### Permission Check Pattern

```python
def require_permission(permission: str):
    """Decorator to check specific permission."""
    def decorator(f):
        @wraps(f)
        async def wrapper(info, *args, **kwargs):
            user = info.context.get("user")

            if not user:
                raise GraphQLError(
                    message="Authentication required",
                    extensions={"code": "UNAUTHENTICATED"}
                )

            if not user.has_permission(permission):
                raise GraphQLError(
                    message=f"Permission '{permission}' required",
                    extensions={
                        "code": "FORBIDDEN",
                        "required_permission": permission
                    }
                )

            return await f(info, *args, **kwargs)

        return wrapper
    return decorator

@mutation
@require_permission("posts.delete")
async def delete_post(info, id: str):
    """Mutation requiring specific permission."""
    # Implementation
    pass
```

## Validation Error Patterns

### Input Validation Pattern

```python
from pydantic import BaseModel, validator
from typing import Optional

class CreateUserInput(BaseModel):
    email: str
    name: str
    age: Optional[int] = None

    @validator("email")
    def validate_email(cls, v):
        if "@" not in v:
            raise ValueError("Invalid email format")
        return v.lower()

    @validator("age")
    def validate_age(cls, v):
        if v is not None and (v < 0 or v > 150):
            raise ValueError("Age must be between 0 and 150")
        return v

@mutation
async def create_user(info, input: dict):
    """Mutation with input validation."""
    try:
        # Validate input
        validated_input = CreateUserInput(**input)
    except ValidationError as e:
        # Convert Pydantic errors to GraphQL errors
        errors = []
        for error in e.errors():
            errors.append({
                "field": ".".join(str(loc) for loc in error["loc"]),
                "message": error["msg"]
            })

        raise GraphQLError(
            message="Invalid input",
            extensions={
                "code": "VALIDATION_ERROR",
                "errors": errors
            }
        )

    # Process validated input
    # ...
```

### Business Rule Validation

```python
@mutation
async def publish_post(info, id: str):
    """Mutation with business rule validation."""
    repo = info.context["repo"]
    user = info.context["user"]

    # Get post
    post = await repo.find_one("v_post", where={"id": id})
    if not post:
        raise GraphQLError(
            message="Post not found",
            extensions={"code": "NOT_FOUND"}
        )

    # Check ownership
    if post["author_id"] != user.id:
        raise GraphQLError(
            message="You can only publish your own posts",
            extensions={"code": "FORBIDDEN"}
        )

    # Check post status
    if post["status"] == "published":
        raise GraphQLError(
            message="Post is already published",
            extensions={
                "code": "INVALID_STATE",
                "current_state": "published"
            }
        )

    # Check content requirements
    if len(post["content"]) < 100:
        raise GraphQLError(
            message="Post must have at least 100 characters",
            extensions={
                "code": "VALIDATION_ERROR",
                "field": "content",
                "min_length": 100,
                "current_length": len(post["content"])
            }
        )

    # Publish post
    # ...
```

## Database Error Patterns

### Connection Retry Pattern

```python
import asyncio
from typing import TypeVar, Callable

T = TypeVar('T')

async def with_retry(
    func: Callable[..., T],
    max_retries: int = 3,
    delay: float = 1.0,
    backoff: float = 2.0
) -> T:
    """Execute function with exponential backoff retry."""
    last_exception = None

    for attempt in range(max_retries):
        try:
            return await func()
        except (ConnectionError, TimeoutError) as e:
            last_exception = e
            if attempt < max_retries - 1:
                wait_time = delay * (backoff ** attempt)
                logger.warning(f"Attempt {attempt + 1} failed, retrying in {wait_time}s: {e}")
                await asyncio.sleep(wait_time)
            else:
                logger.error(f"All {max_retries} attempts failed")

    raise GraphQLError(
        message="Database connection failed",
        extensions={"code": "DATABASE_CONNECTION_ERROR"}
    ) from last_exception

@query
async def get_data(info):
    """Query with connection retry."""
    async def fetch():
        return await info.context["repo"].find_many("v_data")

    return await with_retry(fetch)
```

### Deadlock Handling Pattern

```python
@mutation
async def complex_update(info, input):
    """Mutation with deadlock retry logic."""
    repo = info.context["repo"]
    max_retries = 3

    for attempt in range(max_retries):
        try:
            async with repo.transaction() as tx:
                # Complex multi-table update
                await tx.execute("UPDATE table1 SET ...")
                await tx.execute("UPDATE table2 SET ...")
                return {"success": True}

        except DeadlockError as e:
            if attempt < max_retries - 1:
                # Exponential backoff
                await asyncio.sleep(0.1 * (2 ** attempt))
                logger.warning(f"Deadlock detected, retry {attempt + 1}")
            else:
                raise GraphQLError(
                    message="Operation failed due to concurrent updates",
                    extensions={"code": "DEADLOCK_DETECTED"}
                ) from e
```

## Error Recovery Patterns

### Graceful Degradation

```python
@query
async def get_user_with_stats(info, id: str):
    """Query with graceful degradation for optional data."""
    repo = info.context["repo"]

    # Get core user data (required)
    user = await repo.find_one("v_user", where={"id": id})
    if not user:
        raise GraphQLError(
            message="User not found",
            extensions={"code": "NOT_FOUND"}
        )

    # Try to get stats (optional, degrade gracefully)
    try:
        stats = await repo.find_one("v_user_stats", where={"user_id": id})
    except Exception as e:
        logger.warning(f"Failed to fetch user stats: {e}")
        stats = None  # Degrade gracefully

    # Try to get recent activity (optional)
    try:
        activity = await repo.find_many(
            "v_user_activity",
            where={"user_id": id},
            limit=10
        )
    except Exception as e:
        logger.warning(f"Failed to fetch user activity: {e}")
        activity = []  # Degrade gracefully

    return {
        **user,
        "stats": stats,
        "recent_activity": activity
    }
```

### Circuit Breaker Pattern

```python
class CircuitBreaker:
    """Circuit breaker for external service calls."""

    def __init__(self, failure_threshold: int = 5, timeout: float = 60):
        self.failure_threshold = failure_threshold
        self.timeout = timeout
        self.failures = 0
        self.last_failure = None
        self.state = "closed"  # closed, open, half-open

    async def call(self, func, *args, **kwargs):
        # Check if circuit is open
        if self.state == "open":
            if time.time() - self.last_failure > self.timeout:
                self.state = "half-open"
            else:
                raise GraphQLError(
                    message="Service temporarily unavailable",
                    extensions={"code": "SERVICE_UNAVAILABLE"}
                )

        try:
            result = await func(*args, **kwargs)
            # Reset on success
            if self.state == "half-open":
                self.state = "closed"
                self.failures = 0
            return result

        except Exception as e:
            self.failures += 1
            self.last_failure = time.time()

            if self.failures >= self.failure_threshold:
                self.state = "open"
                logger.error(f"Circuit breaker opened after {self.failures} failures")

            raise

# Usage
email_service_breaker = CircuitBreaker()

@mutation
async def send_notification(info, user_id: str, message: str):
    """Send notification with circuit breaker."""
    try:
        await email_service_breaker.call(
            send_email,
            user_id,
            message
        )
        return {"sent": True}
    except GraphQLError:
        # Service unavailable, could queue for later
        await queue_notification(user_id, message)
        return {"sent": False, "queued": True}
```

## Logging and Monitoring

### Structured Error Logging

```python
import structlog

logger = structlog.get_logger()

@mutation
async def critical_operation(info, input):
    """Mutation with structured error logging."""
    try:
        # Operation
        result = await perform_operation(input)

        logger.info(
            "operation_completed",
            operation="critical_operation",
            user_id=info.context["user"].id,
            input=input
        )

        return result

    except ValidationError as e:
        logger.warning(
            "validation_error",
            operation="critical_operation",
            error=str(e),
            input=input
        )
        raise GraphQLError(
            message="Invalid input",
            extensions={"code": "VALIDATION_ERROR"}
        )

    except Exception as e:
        logger.error(
            "operation_failed",
            operation="critical_operation",
            error=str(e),
            error_type=type(e).__name__,
            user_id=info.context.get("user", {}).get("id"),
            input=input,
            exc_info=True
        )
        raise GraphQLError(
            message="Operation failed",
            extensions={"code": "INTERNAL_SERVER_ERROR"}
        ) from e
```

## Testing Error Scenarios

### Error Testing Pattern

```python
import pytest
from graphql import GraphQLError

@pytest.mark.asyncio
async def test_user_not_found_error():
    """Test NOT_FOUND error handling."""

    # Setup
    repo = MockRepository()
    info = create_mock_info(repo=repo)

    # Configure mock to return None
    repo.find_one.return_value = None

    # Test
    with pytest.raises(GraphQLError) as exc_info:
        await get_user(info, id="nonexistent")

    # Verify error
    error = exc_info.value
    assert error.message == "User nonexistent not found"
    assert error.extensions["code"] == "NOT_FOUND"
    assert error.extensions["id"] == "nonexistent"

@pytest.mark.asyncio
async def test_validation_error():
    """Test validation error handling."""

    # Test with invalid input
    with pytest.raises(GraphQLError) as exc_info:
        await create_user(info, {"email": "invalid"})

    error = exc_info.value
    assert error.extensions["code"] == "VALIDATION_ERROR"
    assert "errors" in error.extensions
```

## Next Steps

- Review [common troubleshooting scenarios](./troubleshooting.md)
- Set up [debugging and monitoring](./debugging.md)
- Implement [error recovery strategies](./troubleshooting.md#recovery-strategies)
