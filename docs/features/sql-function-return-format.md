# SQL Function Return Format for FraiseQL Mutations

**⚠️ This document has been consolidated into the new comprehensive guide.**

**📖 Please see: [Mutation SQL Requirements](../guides/mutation-sql-requirements.md)**

---

**Legacy Content Below** (for reference during migration)

**Navigation**: [← Queries & Mutations](../core/queries-and-mutations.md) • [Mutation Result Reference →](mutation-result-reference.md) • [GraphQL Cascade →](graphql-cascade.md)

## Overview

This guide explains the return formats for PostgreSQL functions used with FraiseQL mutations. FraiseQL supports two formats:

- **Legacy Format** (v1.4+): Simple `success`/`data`/`error` structure
- **V2 Format** (v1.7+): Structured `mutation_response` type with comprehensive error handling

See [Mutation Result Reference](mutation-result-reference.md) for complete format specifications.

**Error Detection**: FraiseQL's Rust layer automatically detects errors using a [comprehensive status taxonomy](../mutations/status-strings.md). Status strings like `validation:`, `unauthorized:token_expired`, `conflict:duplicate`, etc. are automatically mapped to appropriate error types and HTTP status codes.

**Note**: The legacy format continues to work but the v2 format is recommended for new implementations.

## Table of Contents

- [Legacy Return Format (v1.4)](#legacy-return-format-v14)
- [V2 Return Format (v1.7+)](#v2-return-format-v17)
- [Ultra-Direct Path Compatibility](#ultra-direct-path-compatibility)
- [GraphQL Cascade Support](#graphql-cascade-support)
- [Complete Examples](#complete-examples)
- [Best Practices](#best-practices)

---

## Legacy Return Format (v1.4)

### Standard Success Response

The legacy format uses a simple JSONB structure with `success`, `data`, and `error` fields:

```sql
CREATE OR REPLACE FUNCTION app.create_user(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_user_id uuid;
BEGIN
    -- Insert user
    INSERT INTO app.tb_user (name, email, created_at)
    VALUES (
        input->>'name',
        input->>'email',
        now()
    )
    RETURNING id INTO v_user_id;

    -- Return success response
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'id', v_user_id,
            'message', 'User created successfully'
        )
    );
END;
$$ LANGUAGE plpgsql;
```

**Key Fields:**
- `success` (boolean): `true` for successful operations
- `data` (jsonb): The success payload containing entity data and messages

**Note**: This format is auto-detected as "simple format" in the v2 system and works with both legacy and modern pipelines.

### Standard Error Response

```sql
EXCEPTION
    WHEN unique_violation THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', jsonb_build_object(
                'code', 'EMAIL_EXISTS',
                'message', 'Email address already exists',
                'field', 'email'
            )
        );
END;
```

**Error Fields:**
- `success` (boolean): `false` for errors
- `error` (jsonb): Error details
  - `code` (text): Error code for client handling
  - `message` (text): Human-readable error message
  - `field` (text, optional): Field that caused the error

---

## V2 Return Format (v1.7+)

For comprehensive mutation handling, use the `mutation_response` composite type:

```sql
-- Enable the v2 types and helpers
-- (Run: migrations/trinity/005_add_mutation_response.sql)

CREATE OR REPLACE FUNCTION graphql.create_user(input jsonb)
RETURNS mutation_response AS $$
DECLARE
    user_data jsonb;
    user_id uuid;
BEGIN
    -- Check for existing email
    IF EXISTS (SELECT 1 FROM users WHERE email = input->>'email') THEN
        RETURN mutation_validation_error('Email already exists', 'email');
    END IF;

    -- Create user
    user_id := gen_random_uuid();
    INSERT INTO users (id, name, email, created_at)
    VALUES (user_id, input->>'name', input->>'email', now());

    -- Build response data
    user_data := jsonb_build_object(
        'id', user_id,
        'name', input->>'name',
        'email', input->>'email',
        'created_at', now()
    );

    RETURN mutation_created(
        'User created successfully',
        user_data,
        'User'
    );
END;
$$ LANGUAGE plpgsql;
```

**Benefits of V2 Format:**
- Structured error handling with HTTP status codes
- Built-in helper functions for common operations
- Automatic cascade data construction
- Better type safety and consistency

See [Mutation Result Reference](mutation-result-reference.md) for complete v2 format documentation.

---

## Ultra-Direct Path Compatibility

FraiseQL's Ultra-Direct Path (see [ADR-002](../architecture/decisions/002-ultra-direct-mutation-path.md)) provides 10-80x performance improvement by skipping Python parsing and using Rust transformation directly.

### Requirements for Ultra-Direct Path

Your PostgreSQL functions **automatically work** with the ultra-direct path if they:

1. ✅ Return JSONB type (or `mutation_response`)
2. ✅ Follow either format: legacy (`success`/`data`/`error`) or v2 (`mutation_response`)
3. ✅ Use snake_case field names (Rust transforms to camelCase automatically)

### Example: Ultra-Direct Compatible Function (Legacy Format)

```sql
CREATE OR REPLACE FUNCTION app.update_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
    v_post_data jsonb;
BEGIN
    v_post_id := (input->>'post_id')::uuid;

    -- Update post
    UPDATE app.tb_post
    SET
        title = COALESCE(input->>'title', title),
        content = COALESCE(input->>'content', content),
        updated_at = now()
    WHERE id = v_post_id
    RETURNING
        jsonb_build_object(
            'id', id,
            'title', title,
            'content', content,
            'author_id', author_id,  -- ← snake_case (Rust converts to authorId)
            'created_at', created_at, -- ← snake_case (Rust converts to createdAt)
            'updated_at', updated_at
        ) INTO v_post_data;

    -- Return with complete post data from view
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'post', v_post_data,
            'message', 'Post updated successfully'
        )
    );
END;
$$ LANGUAGE plpgsql;
```

**Rust Transformation Pipeline:**
```
PostgreSQL Output (snake_case):
{
  "success": true,
  "data": {
    "post": {
      "id": "...",
      "author_id": "...",     ← snake_case
      "created_at": "..."
    }
  }
}

↓ Rust Transformer (automatic)

GraphQL Response (camelCase):
{
  "success": true,
  "data": {
    "post": {
      "__typename": "Post",  ← injected by Rust
      "id": "...",
      "authorId": "...",     ← camelCase
      "createdAt": "..."
    }
  }
}
```

### V2 Format Ultra-Direct Path

The v2 format also works with the ultra-direct path and provides richer error handling:

```sql
-- V2 format function (also ultra-direct compatible)
CREATE OR REPLACE FUNCTION graphql.update_user(user_id uuid, input jsonb)
RETURNS mutation_response AS $$
-- Uses structured error handling and helper functions
-- See Mutation Result Reference for details
$$ LANGUAGE plpgsql;
```

---

## GraphQL Cascade Support

GraphQL Cascade enables automatic cache updates in GraphQL clients (Apollo, Relay, URQL) by returning information about all entities affected by a mutation.

### Cascade Structure

Add a `_cascade` field to your JSONB return to enable cascade:

```sql
RETURN jsonb_build_object(
    'success', true,
    'data', jsonb_build_object(...),
    '_cascade', jsonb_build_object(
        'updated', jsonb_build_array(...),
        'deleted', jsonb_build_array(...),
        'invalidations', jsonb_build_array(...),
        'metadata', jsonb_build_object(...)
    )
);
```

### Cascade Field Definitions

#### `updated` (array of objects)

Entities that were **created or updated** by this mutation:

```sql
jsonb_build_array(
    jsonb_build_object(
        '__typename', 'Post',              -- GraphQL type name
        'id', v_post_id,                   -- Entity ID
        'operation', 'CREATED',            -- 'CREATED' or 'UPDATED'
        'entity', (SELECT data FROM v_post WHERE id = v_post_id)  -- Full entity
    ),
    jsonb_build_object(
        '__typename', 'User',
        'id', v_author_id,
        'operation', 'UPDATED',
        'entity', (SELECT data FROM v_user WHERE id = v_author_id)
    )
)
```

**Fields:**
- `__typename` (text): GraphQL type name for cache normalization
- `id` (uuid): Entity identifier
- `operation` (text): `"CREATED"` or `"UPDATED"`
- `entity` (jsonb): Complete entity data from the view

#### `deleted` (array of objects)

Entities that were **deleted** by this mutation:

```sql
jsonb_build_array(
    jsonb_build_object(
        '__typename', 'Comment',
        'id', v_comment_id,
        'operation', 'DELETED'
        -- No 'entity' field for deleted items
    )
)
```

#### `invalidations` (array of objects)

Cache invalidation hints for query results:

```sql
jsonb_build_array(
    jsonb_build_object(
        'queryName', 'posts',              -- Query field name to invalidate
        'strategy', 'INVALIDATE',          -- 'INVALIDATE', 'UPDATE', or 'EVICT'
        'scope', 'PREFIX'                  -- 'PREFIX' or 'EXACT'
    ),
    jsonb_build_object(
        'queryName', 'userPosts',
        'strategy', 'INVALIDATE',
        'scope', 'PREFIX'
    )
)
```

**Strategy Options:**
- `INVALIDATE`: Mark cached queries as stale (refetch on next access)
- `UPDATE`: Update cached data directly
- `EVICT`: Remove from cache completely

**Scope Options:**
- `PREFIX`: Invalidate all queries starting with this name (e.g., `posts:*`)
- `EXACT`: Only invalidate exact query match

#### `metadata` (object)

Additional metadata about the mutation:

```sql
jsonb_build_object(
    'timestamp', now(),                    -- When mutation occurred
    'affected_count', 2,                   -- Number of entities affected
    'depth', 1,                            -- Relationship depth traversed
    'transaction_id', txid_current()::text -- Optional: for debugging
)
```

> **Note**: Use snake_case in PostgreSQL (`affected_count`, `transaction_id`). FraiseQL's Rust layer automatically converts to camelCase (`affectedCount`, `transactionId`) in GraphQL responses.

---

## Complete Examples

### Example 1: Create Post with Author Update

This example creates a post and updates the author's post count, returning cascade data for both:

```sql
CREATE OR REPLACE FUNCTION blog.fn_create_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
    v_author_id uuid;
    v_post_data jsonb;
    v_author_data jsonb;
BEGIN
    v_author_id := (input->>'author_id')::uuid;

    -- Create post
    INSERT INTO blog.tb_post (title, content, author_id, created_at)
    VALUES (
        input->>'title',
        input->>'content',
        v_author_id,
        now()
    )
    RETURNING id INTO v_post_id;

    -- Update author stats (side effect)
    UPDATE blog.tb_user
    SET
        post_count = post_count + 1,
        updated_at = now()
    WHERE id = v_author_id;

    -- Fetch complete post data from view
    SELECT data INTO v_post_data
    FROM blog.v_post
    WHERE id = v_post_id;

    -- Fetch complete author data from view
    SELECT data INTO v_author_data
    FROM blog.v_user
    WHERE id = v_author_id;

    -- Return with cascade data
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'id', v_post_id,
            'message', 'Post created successfully'
        ),
        '_cascade', jsonb_build_object(
            'updated', jsonb_build_array(
                -- The created post
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', v_post_id,
                    'operation', 'CREATED',
                    'entity', v_post_data
                ),
                -- The updated author
                jsonb_build_object(
                    '__typename', 'User',
                    'id', v_author_id,
                    'operation', 'UPDATED',
                    'entity', v_author_data
                )
            ),
            'deleted', '[]'::jsonb,
            'invalidations', jsonb_build_array(
                jsonb_build_object(
                    'queryName', 'posts',
                    'strategy', 'INVALIDATE',
                    'scope', 'PREFIX'
                ),
                jsonb_build_object(
                    'queryName', 'userPosts',
                    'strategy', 'INVALIDATE',
                    'scope', 'PREFIX'
                )
            ),
            'metadata', jsonb_build_object(
                'timestamp', now(),
                'affected_count', 2,
                'depth', 1,
                'transaction_id', txid_current()::text
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

**GraphQL Response (after Rust transformation):**

```json
{
  "data": {
    "createPost": {
      "__typename": "CreatePostSuccess",
      "success": true,
      "message": "Post created successfully",
      "cascade": {
        "updated": [
          {
            "__typename": "Post",
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "operation": "CREATED",
            "entity": {
              "id": "550e8400-e29b-41d4-a716-446655440000",
              "title": "My New Post",
              "content": "Post content here",
              "authorId": "660e8400-e29b-41d4-a716-446655440001",
              "createdAt": "2025-11-11T10:30:00Z"
            }
          },
          {
            "__typename": "User",
            "id": "660e8400-e29b-41d4-a716-446655440001",
            "operation": "UPDATED",
            "entity": {
              "id": "660e8400-e29b-41d4-a716-446655440001",
              "name": "John Doe",
              "email": "john@example.com",
              "postCount": 6
            }
          }
        ],
        "deleted": [],
        "invalidations": [
          {
            "queryName": "posts",
            "strategy": "INVALIDATE",
            "scope": "PREFIX"
          },
          {
            "queryName": "userPosts",
            "strategy": "INVALIDATE",
            "scope": "PREFIX"
          }
        ],
        "metadata": {
          "timestamp": "2025-11-11T10:30:00Z",
          "affectedCount": 2,
          "depth": 1,
          "transactionId": "123456789"
        }
      }
    }
  }
}
```

### Example 2: Delete Post with Cascade Deletes

Soft-deleting a post and all its comments:

```sql
CREATE OR REPLACE FUNCTION blog.fn_delete_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
    v_author_id uuid;
    v_deleted_comment_ids uuid[];
    v_author_data jsonb;
BEGIN
    v_post_id := (input->>'post_id')::uuid;

    -- Get author ID before deleting
    SELECT author_id INTO v_author_id
    FROM blog.tb_post
    WHERE id = v_post_id;

    -- Soft delete all comments (cascade)
    UPDATE blog.tb_comment
    SET deleted_at = now()
    WHERE post_id = v_post_id AND deleted_at IS NULL
    RETURNING id INTO v_deleted_comment_ids;

    -- Soft delete the post
    UPDATE blog.tb_post
    SET deleted_at = now()
    WHERE id = v_post_id;

    -- Update author post count
    UPDATE blog.tb_user
    SET post_count = post_count - 1
    WHERE id = v_author_id;

    -- Fetch updated author
    SELECT data INTO v_author_data
    FROM blog.v_user
    WHERE id = v_author_id;

    -- Return with cascade data
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'message', 'Post and related comments deleted',
            'deleted_post_id', v_post_id,
            'deleted_comment_count', array_length(v_deleted_comment_ids, 1)
        ),
        '_cascade', jsonb_build_object(
            'updated', jsonb_build_array(
                -- Author was updated
                jsonb_build_object(
                    '__typename', 'User',
                    'id', v_author_id,
                    'operation', 'UPDATED',
                    'entity', v_author_data
                )
            ),
            'deleted',
                -- Post was deleted
                jsonb_build_array(
                    jsonb_build_object(
                        '__typename', 'Post',
                        'id', v_post_id,
                        'operation', 'DELETED'
                    )
                ) ||
                -- All comments were deleted
                (
                    SELECT jsonb_agg(
                        jsonb_build_object(
                            '__typename', 'Comment',
                            'id', comment_id,
                            'operation', 'DELETED'
                        )
                    )
                    FROM unnest(v_deleted_comment_ids) AS comment_id
                ),
            'invalidations', jsonb_build_array(
                jsonb_build_object(
                    'queryName', 'posts',
                    'strategy', 'INVALIDATE',
                    'scope', 'PREFIX'
                ),
                jsonb_build_object(
                    'queryName', 'comments',
                    'strategy', 'INVALIDATE',
                    'scope', 'PREFIX'
                )
            ),
            'metadata', jsonb_build_object(
                'timestamp', now(),
                'affected_count', 1 + array_length(v_deleted_comment_ids, 1),
                'depth', 2,
                'transaction_id', txid_current()::text
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### Example 3: Simple Update Without Cascade

For mutations that don't need cascade data, simply omit the `_cascade` field:

```sql
CREATE OR REPLACE FUNCTION app.update_user_preferences(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_user_id uuid;
BEGIN
    v_user_id := (input->>'user_id')::uuid;

    UPDATE app.tb_user
    SET preferences = input->'preferences'
    WHERE id = v_user_id;

    -- No cascade field = no automatic cache updates
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'message', 'Preferences updated'
        )
    );
END;
$$ LANGUAGE plpgsql;
```

---

## Best Practices

### 1. Always Use Views for Entity Data

Don't construct entity JSON manually. Use your views:

```sql
-- ❌ BAD: Manual JSON construction
'entity', jsonb_build_object(
    'id', v_post_id,
    'title', v_title,
    'content', v_content
    -- Easy to forget fields!
)

-- ✅ GOOD: Use view data
'entity', (SELECT data FROM v_post WHERE id = v_post_id)
```

### 2. Include All Affected Entities

Track all entities modified by the mutation, not just the primary one:

```sql
-- ✅ GOOD: Include all affected entities
'updated', jsonb_build_array(
    -- Primary entity
    jsonb_build_object('__typename', 'Order', 'id', v_order_id, ...),
    -- Updated product inventory
    jsonb_build_object('__typename', 'Product', 'id', v_product_id, ...),
    -- Updated user stats
    jsonb_build_object('__typename', 'User', 'id', v_user_id, ...)
)
```

### 3. Use Correct Operation Types

Be precise about whether entities were created or updated:

```sql
-- For new entities
'operation', 'CREATED'

-- For existing entities
'operation', 'UPDATED'

-- For deleted entities
'operation', 'DELETED'
```

### 4. Add Appropriate Invalidations

Include invalidation hints for affected queries:

```sql
'invalidations', jsonb_build_array(
    -- Invalidate list queries
    jsonb_build_object(
        'queryName', 'posts',
        'strategy', 'INVALIDATE',
        'scope', 'PREFIX'
    ),
    -- Invalidate filtered queries
    jsonb_build_object(
        'queryName', 'postsByAuthor',
        'strategy', 'INVALIDATE',
        'scope', 'PREFIX'
    )
)
```

### 5. Keep Metadata Accurate

Provide accurate counts and timestamps:

```sql
'metadata', jsonb_build_object(
    'timestamp', now(),                           -- Current timestamp
    'affected_count',
        array_length(v_updated_ids, 1) +          -- Count all affected
        array_length(v_deleted_ids, 1)
)
```

### 6. Use snake_case for Field Names

FraiseQL's Rust transformer automatically converts snake_case to camelCase:

```sql
-- ✅ GOOD: snake_case (Rust converts to camelCase)
'author_id', v_author_id
'created_at', now()
'post_count', v_count

-- ❌ BAD: camelCase (will NOT be converted)
'authorId', v_author_id  -- Don't do this!
```

### 7. Error Handling with CASCADE

Even in error cases, you can include cascade data if some operations succeeded:

```sql
EXCEPTION
    WHEN OTHERS THEN
        -- Some entities might have been modified before the error
        RETURN jsonb_build_object(
            'success', false,
            'error', jsonb_build_object(
                'code', 'PARTIAL_FAILURE',
                'message', SQLERRM
            ),
            '_cascade', jsonb_build_object(
                'updated', jsonb_build_array(
                    -- Include entities that were successfully updated
                    ...
                ),
                'metadata', jsonb_build_object(
                    'partial', true
                )
            )
        );
```

---

## FraiseQL Decorator Configuration

To enable cascade support in your FraiseQL mutation, use the `enable_cascade` parameter:

```python
from fraiseql import mutation, input, type

@input
class CreatePostInput:
    title: str
    content: str
    author_id: str

@fraiseql.type
class CreatePostSuccess:
    id: str
    message: str
    # Cascade data automatically added to response

@fraiseql.type
class CreatePostError:
    code: str
    message: str

@mutation(enable_cascade=True)  # ← Enable cascade
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    failure: CreatePostError
```

Without `enable_cascade=True`, the `_cascade` field is ignored.

---

## Performance Considerations

### Cascade Data is Optional

Only include cascade data when it's beneficial for client cache updates:

- ✅ Include cascade for mutations that affect multiple entities
- ✅ Include cascade for list invalidations
- ❌ Skip cascade for single-entity updates without side effects
- ❌ Skip cascade for preference/settings updates

### Use Efficient Queries

When fetching entity data for cascade:

```sql
-- ✅ GOOD: Single query using view
SELECT data INTO v_entity_data
FROM v_post
WHERE id = v_post_id;

-- ❌ BAD: Multiple queries
SELECT id, title, content, author_id, created_at, ...
INTO v_id, v_title, v_content, ...
```

### Batch Cascade Entries

For multiple entities of the same type:

```sql
-- ✅ GOOD: Batch query
SELECT jsonb_agg(
    jsonb_build_object(
        '__typename', 'Comment',
        'id', id,
        'operation', 'DELETED'
    )
)
FROM unnest(v_deleted_comment_ids) AS id

-- ❌ BAD: Loop through IDs
FOREACH comment_id IN ARRAY v_deleted_comment_ids LOOP
    -- Build individual entries
END LOOP;
```

---

## See Also

- [Mutation Result Reference](mutation-result-reference.md) - Complete format specifications (v1.7+)
- [Queries and Mutations](../core/queries-and-mutations.md) - FraiseQL mutation decorator
- [GraphQL Cascade](graphql-cascade.md) - Full cascade specification
- [ADR-002: Ultra-Direct Mutation Path](../architecture/decisions/002-ultra-direct-mutation-path.md) - Performance optimization
- [PostgreSQL Extensions](../core/postgresql-extensions.md) - Database setup

---

**Document Status**: Updated for v1.7
**Last Updated**: 2025-11-25
**Applies To**: FraiseQL v1.4+ (legacy), v1.7+ (v2 format)
