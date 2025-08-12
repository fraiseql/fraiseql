# Mutations Migration Guide

This guide helps you migrate existing GraphQL mutations to FraiseQL's PostgreSQL function-based approach. Learn how to convert resolver-based mutations to database functions while maintaining type safety and error handling.

## Default Schema Configuration (v0.1.3+)

Before diving into migration, note that FraiseQL v0.1.3+ supports default schema configuration, significantly reducing boilerplate:

```python
# Configure once in your app
from fraiseql import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    default_mutation_schema="app",  # All mutations use this by default
)

# Before v0.1.3: Repetitive schema specification
@mutation(function="create_user", schema="app")
@mutation(function="update_user", schema="app")
@mutation(function="delete_user", schema="app")

# After v0.1.3: Clean and DRY
@mutation(function="create_user")  # Automatically uses "app" schema
@mutation(function="update_user")  # Automatically uses "app" schema
@mutation(function="delete_user")  # Automatically uses "app" schema
```

This eliminates 90% of schema boilerplate in typical applications where most mutations use the same PostgreSQL schema.

## Migration Strategy Overview

FraiseQL's mutation philosophy differs fundamentally from traditional GraphQL frameworks:

| Traditional Approach | FraiseQL Approach |
|---------------------|-------------------|
| Resolvers handle business logic | PostgreSQL functions handle business logic |
| Code-based validation | Database constraints + function validation |
| Manual transaction management | Automatic transaction boundaries |
| ORM-based operations | Direct SQL with JSONB |
| N+1 queries common | Single atomic operation |

## PostgreSQL Function Structure

### Basic Function Template

```sql
-- Function naming convention: fn_<action>_<entity>
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSON)
RETURNS JSON AS $$
DECLARE
    -- Declare variables for intermediate results
    new_id UUID;
    result JSON;
BEGIN
    -- 1. Input validation
    IF input_data->>'email' IS NULL THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Email is required',
            'code', 'VALIDATION_ERROR'
        );
    END IF;

    -- 2. Business logic validation
    IF EXISTS (SELECT 1 FROM tb_users WHERE email = input_data->>'email') THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Email already exists',
            'code', 'DUPLICATE_EMAIL'
        );
    END IF;

    -- 3. Perform the operation
    INSERT INTO tb_users (email, name, created_at)
    VALUES (
        input_data->>'email',
        input_data->>'name',
        NOW()
    )
    RETURNING id INTO new_id;

    -- 4. Return success result
    RETURN json_build_object(
        'success', true,
        'id', new_id,
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
            'code', 'DATABASE_ERROR'
        );
END;
$$ LANGUAGE plpgsql;
```

## Converting Common Mutation Patterns

### 1. Create Operations

**Traditional Resolver:**
```python
# Old approach with ORM
async def create_post(parent, info, title, content, author_id):
    try:
        post = Post(
            title=title,
            content=content,
            author_id=author_id
        )
        db.session.add(post)
        db.session.commit()
        return {"success": True, "post": post}
    except Exception as e:
        db.session.rollback()
        return {"success": False, "error": str(e)}
```

**FraiseQL Migration:**

SQL Function:
```sql
CREATE OR REPLACE FUNCTION fn_create_post(input_data JSON)
RETURNS JSON AS $$
DECLARE
    new_post_id UUID;
    generated_slug VARCHAR(500);
BEGIN
    -- Generate slug from title
    generated_slug := LOWER(REGEXP_REPLACE(
        input_data->>'title',
        '[^a-zA-Z0-9]+', '-', 'g'
    ));

    -- Ensure uniqueness
    WHILE EXISTS (SELECT 1 FROM tb_posts WHERE slug = generated_slug) LOOP
        generated_slug := generated_slug || '-' ||
            EXTRACT(EPOCH FROM NOW())::INTEGER;
    END LOOP;

    -- Insert with all fields
    INSERT INTO tb_posts (
        title, slug, content, author_id,
        tags, is_published, published_at
    )
    VALUES (
        input_data->>'title',
        generated_slug,
        input_data->>'content',
        (input_data->>'author_id')::UUID,
        COALESCE(
            ARRAY(SELECT json_array_elements_text(input_data->'tags')),
            ARRAY[]::TEXT[]
        ),
        COALESCE((input_data->>'is_published')::BOOLEAN, false),
        CASE
            WHEN (input_data->>'is_published')::BOOLEAN
            THEN NOW()
            ELSE NULL
        END
    )
    RETURNING id INTO new_post_id;

    RETURN json_build_object(
        'success', true,
        'post_id', new_post_id,
        'slug', generated_slug
    );
END;
$$ LANGUAGE plpgsql;
```

Python Integration:
```python
from fraiseql import type, input, success, failure

@input
class CreatePostInput:
    title: str
    content: str
    author_id: UUID
    tags: list[str] | None = None
    is_published: bool = False

@success
class CreatePostSuccess:
    post: Post
    message: str = "Post created successfully"

@failure
class CreatePostError:
    message: str
    code: str

async def create_post(
    info,
    input: CreatePostInput
) -> CreatePostSuccess | CreatePostError:
    """Create a new post via PostgreSQL function."""
    db = info.context["db"]

    # Call the PostgreSQL function
    result = await db.execute_function(
        "fn_create_post",
        {
            "title": input.title,
            "content": input.content,
            "author_id": str(input.author_id),
            "tags": input.tags,
            "is_published": input.is_published
        }
    )

    if result["success"]:
        # Fetch the created post from the view
        post_data = await db.get_post_by_id(result["post_id"])
        return CreatePostSuccess(
            post=Post.from_dict(post_data)
        )
    else:
        return CreatePostError(
            message=result["error"],
            code=result.get("code", "UNKNOWN_ERROR")
        )
```

### 2. Update Operations

**Traditional Resolver:**
```python
# Old approach with field-by-field updates
async def update_user(parent, info, id, **kwargs):
    user = User.query.get(id)
    if not user:
        raise GraphQLError("User not found")

    for key, value in kwargs.items():
        if value is not None:
            setattr(user, key, value)

    db.session.commit()
    return user
```

**FraiseQL Migration:**

SQL Function with Optimistic Locking:
```sql
CREATE OR REPLACE FUNCTION fn_update_user(input_data JSON)
RETURNS JSON AS $$
DECLARE
    current_version INTEGER;
    rows_updated INTEGER;
BEGIN
    -- Validate ID
    IF input_data->>'id' IS NULL THEN
        RETURN json_build_object(
            'success', false,
            'error', 'User ID is required',
            'code', 'MISSING_ID'
        );
    END IF;

    -- Get current version for optimistic locking
    SELECT version INTO current_version
    FROM tb_users
    WHERE id = (input_data->>'id')::UUID;

    IF current_version IS NULL THEN
        RETURN json_build_object(
            'success', false,
            'error', 'User not found',
            'code', 'NOT_FOUND'
        );
    END IF;

    -- Check version if provided
    IF input_data->>'version' IS NOT NULL AND
       (input_data->>'version')::INTEGER != current_version THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Data has been modified by another user',
            'code', 'VERSION_CONFLICT'
        );
    END IF;

    -- Update with COALESCE for partial updates
    UPDATE tb_users
    SET
        name = COALESCE(input_data->>'name', name),
        email = COALESCE(input_data->>'email', email),
        bio = COALESCE(input_data->>'bio', bio),
        avatar_url = COALESCE(input_data->>'avatar_url', avatar_url),
        settings = COALESCE(input_data->'settings', settings),
        version = version + 1,
        updated_at = NOW()
    WHERE id = (input_data->>'id')::UUID
    RETURNING 1 INTO rows_updated;

    RETURN json_build_object(
        'success', true,
        'user_id', input_data->>'id',
        'new_version', current_version + 1
    );
END;
$$ LANGUAGE plpgsql;
```

### 3. Delete Operations

**Traditional Resolver:**
```python
# Old approach with cascade handling
async def delete_post(parent, info, id):
    post = Post.query.get(id)
    if not post:
        return {"success": False, "error": "Post not found"}

    # Manual cascade for related data
    Comment.query.filter_by(post_id=id).delete()
    db.session.delete(post)
    db.session.commit()
    return {"success": True}
```

**FraiseQL Migration:**

SQL Function with Soft Delete:
```sql
CREATE OR REPLACE FUNCTION fn_delete_post(input_data JSON)
RETURNS JSON AS $$
DECLARE
    post_exists BOOLEAN;
    is_soft_delete BOOLEAN;
BEGIN
    -- Check delete type
    is_soft_delete := COALESCE(
        (input_data->>'soft_delete')::BOOLEAN,
        true  -- Default to soft delete
    );

    -- Verify post exists
    SELECT EXISTS (
        SELECT 1 FROM tb_posts
        WHERE id = (input_data->>'id')::UUID
        AND (deleted_at IS NULL OR NOT is_soft_delete)
    ) INTO post_exists;

    IF NOT post_exists THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Post not found or already deleted',
            'code', 'NOT_FOUND'
        );
    END IF;

    IF is_soft_delete THEN
        -- Soft delete: just mark as deleted
        UPDATE tb_posts
        SET
            deleted_at = NOW(),
            deleted_by = (input_data->>'user_id')::UUID
        WHERE id = (input_data->>'id')::UUID;
    ELSE
        -- Hard delete: remove permanently
        -- Comments will cascade due to FK constraint
        DELETE FROM tb_posts
        WHERE id = (input_data->>'id')::UUID;
    END IF;

    RETURN json_build_object(
        'success', true,
        'message', CASE
            WHEN is_soft_delete
            THEN 'Post archived successfully'
            ELSE 'Post permanently deleted'
        END
    );
END;
$$ LANGUAGE plpgsql;
```

## Advanced Migration Patterns

### Batch Operations

```sql
-- Batch update with transaction safety
CREATE OR REPLACE FUNCTION fn_batch_update_posts(input_data JSON)
RETURNS JSON AS $$
DECLARE
    post_update JSON;
    success_count INTEGER := 0;
    error_count INTEGER := 0;
    errors JSON[] := ARRAY[]::JSON[];
BEGIN
    -- Process each post update
    FOR post_update IN
        SELECT json_array_elements(input_data->'posts')
    LOOP
        BEGIN
            UPDATE tb_posts
            SET
                title = COALESCE(post_update->>'title', title),
                content = COALESCE(post_update->>'content', content),
                updated_at = NOW()
            WHERE id = (post_update->>'id')::UUID;

            success_count := success_count + 1;
        EXCEPTION WHEN OTHERS THEN
            error_count := error_count + 1;
            errors := array_append(errors, json_build_object(
                'id', post_update->>'id',
                'error', SQLERRM
            ));
        END;
    END LOOP;

    RETURN json_build_object(
        'success', error_count = 0,
        'success_count', success_count,
        'error_count', error_count,
        'errors', errors
    );
END;
$$ LANGUAGE plpgsql;
```

### Complex Business Logic

```sql
-- Publishing workflow with validation
CREATE OR REPLACE FUNCTION fn_publish_post(input_data JSON)
RETURNS JSON AS $$
DECLARE
    post_record RECORD;
    can_publish BOOLEAN;
BEGIN
    -- Get post details
    SELECT * INTO post_record
    FROM tb_posts
    WHERE id = (input_data->>'post_id')::UUID;

    IF NOT FOUND THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Post not found',
            'code', 'NOT_FOUND'
        );
    END IF;

    -- Check publishing requirements
    can_publish :=
        LENGTH(post_record.title) >= 10 AND
        LENGTH(post_record.content) >= 100 AND
        post_record.excerpt IS NOT NULL AND
        array_length(post_record.tags, 1) >= 1;

    IF NOT can_publish THEN
        RETURN json_build_object(
            'success', false,
            'error', 'Post does not meet publishing requirements',
            'code', 'VALIDATION_ERROR',
            'requirements', json_build_object(
                'title_length', LENGTH(post_record.title) >= 10,
                'content_length', LENGTH(post_record.content) >= 100,
                'has_excerpt', post_record.excerpt IS NOT NULL,
                'has_tags', array_length(post_record.tags, 1) >= 1
            )
        );
    END IF;

    -- Publish the post
    UPDATE tb_posts
    SET
        is_published = true,
        published_at = NOW(),
        publish_version = publish_version + 1
    WHERE id = post_record.id;

    -- Log the publishing event
    INSERT INTO tb_audit_log (
        entity_type, entity_id, action,
        user_id, metadata
    )
    VALUES (
        'post',
        post_record.id,
        'published',
        (input_data->>'user_id')::UUID,
        json_build_object(
            'previous_version', post_record.publish_version,
            'published_at', NOW()
        )
    );

    RETURN json_build_object(
        'success', true,
        'post_id', post_record.id,
        'published_at', NOW(),
        'version', post_record.publish_version + 1
    );
END;
$$ LANGUAGE plpgsql;
```

## Testing Migration Strategies

### Unit Testing Functions

```sql
-- Test helper function
CREATE OR REPLACE FUNCTION test_fn_create_user()
RETURNS TABLE(test_name TEXT, passed BOOLEAN, message TEXT) AS $$
BEGIN
    -- Test 1: Valid input
    RETURN QUERY
    SELECT
        'Valid user creation'::TEXT,
        (fn_create_user(json_build_object(
            'email', 'test@example.com',
            'name', 'Test User'
        ))->>'success')::BOOLEAN,
        'Should create user successfully'::TEXT;

    -- Test 2: Missing required field
    RETURN QUERY
    SELECT
        'Missing email validation'::TEXT,
        NOT (fn_create_user(json_build_object(
            'name', 'Test User'
        ))->>'success')::BOOLEAN,
        'Should fail with missing email'::TEXT;

    -- Clean up test data
    DELETE FROM tb_users WHERE email = 'test@example.com';
END;
$$ LANGUAGE plpgsql;
```

### Integration Testing

```python
import pytest
from fraiseql.testing import TestClient

@pytest.mark.asyncio
async def test_create_post_mutation():
    """Test the migrated create post mutation."""
    async with TestClient() as client:
        # Setup: Create author
        author_result = await client.mutate(
            """
            mutation CreateAuthor($input: CreateUserInput!) {
                createUser(input: $input) {
                    ... on CreateUserSuccess {
                        user { id email }
                    }
                    ... on CreateUserError {
                        message
                        code
                    }
                }
            }
            """,
            variables={
                "input": {
                    "email": "author@test.com",
                    "name": "Test Author",
                    "password": "secure123"
                }
            }
        )

        author_id = author_result["data"]["createUser"]["user"]["id"]

        # Test: Create post
        result = await client.mutate(
            """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    ... on CreatePostSuccess {
                        post {
                            id
                            title
                            slug
                            author { name }
                        }
                    }
                    ... on CreatePostError {
                        message
                        code
                    }
                }
            }
            """,
            variables={
                "input": {
                    "title": "Test Post",
                    "content": "Test content",
                    "authorId": author_id,
                    "tags": ["test", "migration"]
                }
            }
        )

        assert result["data"]["createPost"]["post"]["title"] == "Test Post"
        assert result["data"]["createPost"]["post"]["slug"] == "test-post"
```

## Migration Checklist

### Pre-Migration
- [ ] Identify all mutations to migrate
- [ ] Document current business logic
- [ ] Map error codes and messages
- [ ] Plan transaction boundaries
- [ ] Design return types

### Function Creation
- [ ] Follow `fn_` naming convention
- [ ] Accept JSON input parameter
- [ ] Return JSON with success/error structure
- [ ] Include proper error handling
- [ ] Add input validation
- [ ] Implement business logic checks
- [ ] Use transactions appropriately

### Python Integration
- [ ] Define input types with `@input`
- [ ] Create success types with `@success`
- [ ] Create failure types with `@failure`
- [ ] Implement mutation function
- [ ] Map function results to types
- [ ] Add authentication/authorization
- [ ] Handle context properly

### Testing
- [ ] Write SQL function tests
- [ ] Create integration tests
- [ ] Test error cases
- [ ] Verify transaction rollback
- [ ] Check optimistic locking
- [ ] Test batch operations
- [ ] Validate performance

### Documentation
- [ ] Document function parameters
- [ ] Explain business logic
- [ ] List error codes
- [ ] Provide usage examples
- [ ] Note migration decisions

## Common Pitfalls and Solutions

### 1. Type Casting Issues

**Problem:** JSON values need explicit casting
```sql
-- Wrong
WHERE author_id = input_data->>'author_id'

-- Correct
WHERE author_id = (input_data->>'author_id')::UUID
```

### 2. Array Handling

**Problem:** JSON arrays need special handling
```sql
-- Convert JSON array to PostgreSQL array
ARRAY(SELECT json_array_elements_text(input_data->'tags'))
```

### 3. Null vs Missing Fields

**Problem:** Distinguishing between null and missing
```sql
-- Check if field exists
CASE
    WHEN input_data ? 'field_name' THEN
        -- Field exists (might be null)
        input_data->>'field_name'
    ELSE
        -- Field doesn't exist, use current value
        current_field_value
END
```

### 4. Transaction Scope

**Problem:** Functions run in single transaction
```sql
-- Use savepoints for partial rollback
BEGIN
    -- Create savepoint
    SAVEPOINT before_risky_operation;

    -- Risky operation
    PERFORM risky_operation();

EXCEPTION WHEN OTHERS THEN
    -- Rollback to savepoint
    ROLLBACK TO SAVEPOINT before_risky_operation;
    -- Continue with alternative logic
END;
```

## Performance Optimization

### 1. Use RETURNING Clauses

```sql
-- Avoid separate SELECT after INSERT
INSERT INTO tb_posts (title, content)
VALUES (input_data->>'title', input_data->>'content')
RETURNING id, slug, created_at INTO new_id, new_slug, created_time;
```

### 2. Batch Operations

```sql
-- Use unnest for bulk inserts
INSERT INTO tb_tags (post_id, tag_name)
SELECT
    (input_data->>'post_id')::UUID,
    unnest(ARRAY(SELECT json_array_elements_text(input_data->'tags')))
ON CONFLICT (post_id, tag_name) DO NOTHING;
```

### 3. Prepared Statements

```python
# Reuse prepared statements in repository
class Repository:
    async def execute_function(self, name: str, params: dict):
        # FraiseQL handles prepared statement caching
        return await self.db.function(name, params)
```

## Next Steps

After migrating your mutations:

1. **Optimize views** - Ensure your views efficiently support the migrated mutations
2. **Add caching** - Implement FraiseQL's caching strategies for read-after-write scenarios
3. **Monitor performance** - Use EXPLAIN ANALYZE on your functions
4. **Implement audit logging** - Add audit trails within your functions
5. **Set up testing** - Create comprehensive test suites for your functions

## Related Documentation

- [Mutations Overview](./index.md) - Understanding FraiseQL's mutation philosophy
- [Type System](../core-concepts/type-system.md) - Type definitions and decorators
- [Database Views](../core-concepts/database-views.md) - Creating views for your mutations
- [Blog API Tutorial](../tutorials/blog-api.md) - Complete example with mutations
