# Mutations in FraiseQL

FraiseQL implements mutations through PostgreSQL functions, maintaining a clean CQRS separation between reads (views) and writes (functions). This approach leverages database transactions, constraints, and business logic where it belongs.

## Philosophy

In FraiseQL's CQRS architecture:
- **Queries** read from views (`v_` prefix)
- **Mutations** execute PostgreSQL functions (`fn_` prefix)
- **Tables** (`tb_` prefix) are only modified through functions
- Business logic lives in the database for consistency

This design ensures:
- Transactional consistency
- Centralized validation
- Database-enforced constraints
- Reusable business logic across applications

## PostgreSQL Functions as Mutations

Every mutation in FraiseQL maps to a PostgreSQL function:

```sql
-- Function naming convention: fn_<action>_<entity>
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSON)
RETURNS JSON AS $$
DECLARE
    new_user_id UUID;
BEGIN
    -- Validation
    IF input_data->>'email' IS NULL THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Email is required',
            'code', 'VALIDATION_ERROR'
        );
    END IF;
    
    -- Business logic
    IF EXISTS (SELECT 1 FROM tb_users WHERE email = input_data->>'email') THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Email already exists',
            'code', 'DUPLICATE_EMAIL'
        );
    END IF;
    
    -- Insert with transaction
    INSERT INTO tb_users (email, name, roles)
    VALUES (
        input_data->>'email',
        input_data->>'name',
        COALESCE(
            ARRAY(SELECT json_array_elements_text(input_data->'roles')),
            ARRAY['user']::TEXT[]
        )
    )
    RETURNING id INTO new_user_id;
    
    -- Return success
    RETURN json_build_object(
        'success', true,
        'user_id', new_user_id,
        'message', 'User created successfully'
    );
    
EXCEPTION
    WHEN unique_violation THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Email already exists',
            'code', 'DUPLICATE_EMAIL'
        );
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM,
            'code', 'INTERNAL_ERROR'
        );
END;
$$ LANGUAGE plpgsql;
```

## Python Mutation Handlers

Mutations in Python wrap the PostgreSQL functions:

```python
from uuid import UUID
import fraiseql
from fraiseql import mutation

# Input type
@fraiseql.input
class CreateUserInput:
    email: str
    name: str
    roles: list[str] | None = None

# Success type
@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

# Error type
@fraiseql.failure
class CreateUserError:
    message: str
    code: str
    field_errors: dict[str, str] | None = None

# Mutation handler
@mutation
async def create_user(
    info,
    input: CreateUserInput
) -> CreateUserSuccess | CreateUserError:
    """Create a new user account."""
    db = info.context["db"]
    
    # Call PostgreSQL function
    result = await db.execute_function(
        "fn_create_user",
        {
            "email": input.email,
            "name": input.name,
            "roles": input.roles or ["user"]
        }
    )
    
    if result["success"]:
        # Fetch the created user from view
        user_data = await db.get_user_by_id(result["user_id"])
        return CreateUserSuccess(
            user=User.from_dict(user_data)
        )
    else:
        return CreateUserError(
            message=result["error"],
            code=result["code"]
        )
```

## Transaction Management

PostgreSQL functions automatically run in transactions:

```sql
-- Complex mutation with multiple operations
CREATE OR REPLACE FUNCTION fn_transfer_ownership(input_data JSON)
RETURNS JSON AS $$
DECLARE
    from_user_id UUID;
    to_user_id UUID;
    resource_id UUID;
BEGIN
    -- Parse input
    from_user_id := (input_data->>'from_user_id')::UUID;
    to_user_id := (input_data->>'to_user_id')::UUID;
    resource_id := (input_data->>'resource_id')::UUID;
    
    -- Start transaction implicitly
    
    -- Verify ownership
    IF NOT EXISTS (
        SELECT 1 FROM tb_resources 
        WHERE id = resource_id AND owner_id = from_user_id
    ) THEN
        RAISE EXCEPTION 'User does not own this resource';
    END IF;
    
    -- Update ownership
    UPDATE tb_resources
    SET owner_id = to_user_id,
        updated_at = NOW()
    WHERE id = resource_id;
    
    -- Log the transfer
    INSERT INTO tb_audit_log (
        action, 
        resource_id, 
        from_user_id, 
        to_user_id,
        timestamp
    )
    VALUES (
        'OWNERSHIP_TRANSFER',
        resource_id,
        from_user_id,
        to_user_id,
        NOW()
    );
    
    -- Send notification (via NOTIFY)
    PERFORM pg_notify(
        'ownership_changed',
        json_build_object(
            'resource_id', resource_id,
            'new_owner_id', to_user_id
        )::text
    );
    
    -- Transaction commits automatically on success
    RETURN json_build_object(
        'success', true,
        'message', 'Ownership transferred successfully'
    );
    
EXCEPTION
    WHEN OTHERS THEN
        -- Transaction rolls back automatically on error
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;
```

## Error Handling Patterns

### Database-Level Validation

```sql
CREATE OR REPLACE FUNCTION fn_update_profile(input_data JSON)
RETURNS JSON AS $$
BEGIN
    -- Validate email format
    IF input_data->>'email' IS NOT NULL AND 
       input_data->>'email' !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$' THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Invalid email format',
            'field', 'email'
        );
    END IF;
    
    -- Validate age if provided
    IF input_data->>'age' IS NOT NULL AND 
       (input_data->>'age')::INT < 0 THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Age must be positive',
            'field', 'age'
        );
    END IF;
    
    -- Update user
    UPDATE tb_users
    SET 
        email = COALESCE(input_data->>'email', email),
        age = COALESCE((input_data->>'age')::INT, age),
        updated_at = NOW()
    WHERE id = (input_data->>'user_id')::UUID;
    
    RETURN json_build_object('success', true);
END;
$$ LANGUAGE plpgsql;
```

### Python-Level Error Handling

```python
@mutation
async def update_profile(
    info,
    input: UpdateProfileInput
) -> UpdateProfileSuccess | UpdateProfileError:
    """Update user profile with validation."""
    
    # Python-level validation
    errors = {}
    
    if input.email and "@" not in input.email:
        errors["email"] = "Invalid email format"
    
    if input.age and input.age < 0:
        errors["age"] = "Age must be positive"
    
    if errors:
        return UpdateProfileError(
            message="Validation failed",
            code="VALIDATION_ERROR",
            field_errors=errors
        )
    
    # Call database function
    try:
        result = await info.context["db"].execute_function(
            "fn_update_profile",
            {
                "user_id": info.context["user"].id,
                "email": input.email,
                "age": input.age
            }
        )
        
        if result["success"]:
            user = await info.context["db"].get_user_by_id(
                info.context["user"].id
            )
            return UpdateProfileSuccess(user=User.from_dict(user))
        else:
            return UpdateProfileError(
                message=result["error"],
                code="UPDATE_FAILED"
            )
            
    except Exception as e:
        # Log error
        logger.error(f"Profile update failed: {e}")
        return UpdateProfileError(
            message="An unexpected error occurred",
            code="INTERNAL_ERROR"
        )
```

## Return Types and Unions

FraiseQL mutations return unions for success/error handling:

```python
# Define success and failure types
@fraiseql.success
class DeletePostSuccess:
    message: str = "Post deleted successfully"
    deleted_id: UUID

@fraiseql.failure  
class DeletePostError:
    message: str
    code: str  # NOT_FOUND, PERMISSION_DENIED, etc.

# Mutation returns a union
@mutation
async def delete_post(
    info,
    id: UUID
) -> DeletePostSuccess | DeletePostError:
    """Delete a blog post."""
    user = info.context.get("user")
    
    if not user:
        return DeletePostError(
            message="Authentication required",
            code="UNAUTHENTICATED"
        )
    
    db = info.context["db"]
    
    # Check ownership
    post = await db.get_post_by_id(id)
    if not post:
        return DeletePostError(
            message="Post not found",
            code="NOT_FOUND"
        )
    
    if post["author_id"] != user.id:
        return DeletePostError(
            message="You can only delete your own posts",
            code="PERMISSION_DENIED"
        )
    
    # Execute deletion
    result = await db.execute_function(
        "fn_delete_post",
        {"post_id": id, "user_id": user.id}
    )
    
    if result["success"]:
        return DeletePostSuccess(deleted_id=id)
    else:
        return DeletePostError(
            message=result["error"],
            code="DELETE_FAILED"
        )
```

GraphQL client handles the union:

```graphql
mutation DeletePost($id: UUID!) {
  deletePost(id: $id) {
    __typename
    ... on DeletePostSuccess {
      message
      deletedId
    }
    ... on DeletePostError {
      message
      code
    }
  }
}
```

## Batch Mutations

For efficiency, create batch mutation functions:

```sql
-- Batch update function
CREATE OR REPLACE FUNCTION fn_batch_update_status(input_data JSON)
RETURNS JSON AS $$
DECLARE
    updated_count INT;
    failed_ids UUID[];
BEGIN
    -- Parse array of updates
    WITH updates AS (
        SELECT 
            (elem->>'id')::UUID as id,
            (elem->>'status')::TEXT as status
        FROM json_array_elements(input_data->'items') elem
    )
    UPDATE tb_items i
    SET 
        status = u.status,
        updated_at = NOW()
    FROM updates u
    WHERE i.id = u.id;
    
    GET DIAGNOSTICS updated_count = ROW_COUNT;
    
    -- Find any IDs that weren't updated
    SELECT ARRAY_AGG(id) INTO failed_ids
    FROM json_array_elements(input_data->'items') elem
    WHERE NOT EXISTS (
        SELECT 1 FROM tb_items 
        WHERE id = (elem->>'id')::UUID
    );
    
    RETURN json_build_object(
        'success', true,
        'updated_count', updated_count,
        'failed_ids', COALESCE(failed_ids, ARRAY[]::UUID[])
    );
END;
$$ LANGUAGE plpgsql;
```

## Async Operations

For long-running operations, use job queues:

```sql
-- Queue an async job
CREATE OR REPLACE FUNCTION fn_queue_export(input_data JSON)
RETURNS JSON AS $$
DECLARE
    job_id UUID;
BEGIN
    -- Create job record
    INSERT INTO tb_export_jobs (
        user_id,
        export_type,
        parameters,
        status
    )
    VALUES (
        (input_data->>'user_id')::UUID,
        input_data->>'export_type',
        input_data->'parameters',
        'PENDING'
    )
    RETURNING id INTO job_id;
    
    -- Notify job processor
    PERFORM pg_notify(
        'export_job_created',
        json_build_object('job_id', job_id)::text
    );
    
    RETURN json_build_object(
        'success', true,
        'job_id', job_id,
        'message', 'Export queued successfully'
    );
END;
$$ LANGUAGE plpgsql;
```

Python async handler:

```python
@mutation
async def queue_export(
    info,
    export_type: str,
    parameters: dict
) -> QueueExportSuccess | QueueExportError:
    """Queue an async export job."""
    
    result = await info.context["db"].execute_function(
        "fn_queue_export",
        {
            "user_id": info.context["user"].id,
            "export_type": export_type,
            "parameters": parameters
        }
    )
    
    if result["success"]:
        # Start background task
        asyncio.create_task(
            process_export_job(result["job_id"])
        )
        
        return QueueExportSuccess(
            job_id=result["job_id"],
            message="Export queued, you'll be notified when complete"
        )
    else:
        return QueueExportError(
            message=result["error"]
        )
```

## Testing Mutations

```python
import pytest
from unittest.mock import AsyncMock

@pytest.mark.asyncio
async def test_create_user_success():
    # Mock database
    mock_db = AsyncMock()
    mock_db.execute_function.return_value = {
        "success": True,
        "user_id": "123e4567-e89b-12d3-a456-426614174000"
    }
    mock_db.get_user_by_id.return_value = {
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "email": "test@example.com",
        "name": "Test User"
    }
    
    # Mock info context
    info = AsyncMock()
    info.context = {"db": mock_db}
    
    # Test mutation
    result = await create_user(
        info,
        CreateUserInput(
            email="test@example.com",
            name="Test User"
        )
    )
    
    assert isinstance(result, CreateUserSuccess)
    assert result.user.email == "test@example.com"

@pytest.mark.asyncio
async def test_create_user_duplicate_email():
    # Mock database returning error
    mock_db = AsyncMock()
    mock_db.execute_function.return_value = {
        "success": False,
        "error": "Email already exists",
        "code": "DUPLICATE_EMAIL"
    }
    
    info = AsyncMock()
    info.context = {"db": mock_db}
    
    result = await create_user(
        info,
        CreateUserInput(
            email="existing@example.com",
            name="Test User"
        )
    )
    
    assert isinstance(result, CreateUserError)
    assert result.code == "DUPLICATE_EMAIL"
```

## Best Practices

1. **Naming Convention**: Always prefix functions with `fn_` and use snake_case
2. **Input Validation**: Validate in the database function for consistency
3. **Return Format**: Always return JSON with `success` field
4. **Error Codes**: Use consistent error codes across mutations
5. **Idempotency**: Make mutations idempotent where possible
6. **Audit Logging**: Log mutations in the database
7. **Testing**: Test both success and error paths

## Common Patterns

### Soft Deletes

```sql
CREATE OR REPLACE FUNCTION fn_soft_delete(input_data JSON)
RETURNS JSON AS $$
BEGIN
    UPDATE tb_items
    SET 
        deleted_at = NOW(),
        deleted_by = (input_data->>'user_id')::UUID
    WHERE id = (input_data->>'item_id')::UUID
      AND deleted_at IS NULL;
    
    IF NOT FOUND THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Item not found or already deleted'
        );
    END IF;
    
    RETURN json_build_object('success', true);
END;
$$ LANGUAGE plpgsql;
```

### Upserts

```sql
CREATE OR REPLACE FUNCTION fn_upsert_settings(input_data JSON)
RETURNS JSON AS $$
BEGIN
    INSERT INTO tb_user_settings (user_id, settings)
    VALUES (
        (input_data->>'user_id')::UUID,
        input_data->'settings'
    )
    ON CONFLICT (user_id) DO UPDATE
    SET 
        settings = input_data->'settings',
        updated_at = NOW();
    
    RETURN json_build_object('success', true);
END;
$$ LANGUAGE plpgsql;
```

### Optimistic Locking

```sql
CREATE OR REPLACE FUNCTION fn_update_with_version(input_data JSON)
RETURNS JSON AS $$
DECLARE
    rows_updated INT;
BEGIN
    UPDATE tb_documents
    SET 
        content = input_data->>'content',
        version = version + 1,
        updated_at = NOW()
    WHERE id = (input_data->>'id')::UUID
      AND version = (input_data->>'expected_version')::INT;
    
    GET DIAGNOSTICS rows_updated = ROW_COUNT;
    
    IF rows_updated = 0 THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Version conflict - document was modified'
        );
    END IF;
    
    RETURN json_build_object(
        'success', true,
        'new_version', (input_data->>'expected_version')::INT + 1
    );
END;
$$ LANGUAGE plpgsql;
```

## Summary

FraiseQL's mutation pattern provides:
- **Transactional consistency** through PostgreSQL functions
- **Type safety** with Python type hints and GraphQL schema
- **Clear error handling** with union return types
- **Business logic** centralized in the database
- **Testability** through clean separation of concerns

This approach ensures your mutations are reliable, maintainable, and performant.

## Next Steps

- See [Migration Guide](./migration-guide.md) for converting existing mutations
- Review [Blog API Tutorial](../tutorials/blog-api.md) for complete examples
- Learn about [Query Translation](../core-concepts/query-translation.md) for the read side