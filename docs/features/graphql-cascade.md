# GraphQL Cascade

**Navigation**: [← SQL Function Return Format](sql-function-return-format.md) • [Queries & Mutations →](../core/queries-and-mutations.md)

GraphQL Cascade enables automatic cache updates and side effect tracking for mutations in FraiseQL. When a mutation modifies data, it can include cascade information that clients use to update their caches without additional queries.

## Overview

Cascade works by having PostgreSQL functions return not just the mutation result, but also metadata about what changed. This metadata includes:

- **Updated entities**: Objects that were created, updated, or modified
- **Deleted entities**: IDs of objects that were deleted
- **Invalidations**: Query cache invalidation hints
- **Metadata**: Timestamps and operation counts

## Quick Start

For detailed information on SQL function return formats, see [SQL Function Return Format](sql-function-return-format.md).

## PostgreSQL Function Pattern

To enable cascade for a mutation, include a `_cascade` field in the function's JSONB return value:

```sql
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
    v_author_id uuid;
BEGIN
    -- Create post
    INSERT INTO tb_post (title, content, author_id)
    VALUES (input->>'title', input->>'content', (input->>'author_id')::uuid)
    RETURNING id INTO v_post_id;

    v_author_id := (input->>'author_id')::uuid;

    -- Update author stats
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    -- Return with cascade metadata
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id, 'message', 'Post created'),
        '_cascade', jsonb_build_object(
            'updated', jsonb_build_array(
                -- The created post
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', v_post_id,
                    'operation', 'CREATED',
                    'entity', (SELECT data FROM v_post WHERE id = v_post_id)
                ),
                -- The updated author
                jsonb_build_object(
                    '__typename', 'User',
                    'id', v_author_id,
                    'operation', 'UPDATED',
                    'entity', (SELECT data FROM v_user WHERE id = v_author_id)
                )
            ),
            'deleted', '[]'::jsonb,
            'invalidations', jsonb_build_array(
                jsonb_build_object(
                    'queryName', 'posts',
                    'strategy', 'INVALIDATE',
                    'scope', 'PREFIX'
                )
            ),
            'metadata', jsonb_build_object(
                'timestamp', now(),
                'affectedCount', 2
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

## FraiseQL Mutation Decorator

Enable cascade for a mutation by adding `enable_cascade=True` to the `@mutation` decorator:

```python
@mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError
```

## Cascade Structure

The `_cascade` object contains:

### `updated` (Array)
Array of entities that were created or updated:

```json
{
  "__typename": "Post",
  "id": "uuid",
  "operation": "CREATED" | "UPDATED",
  "entity": { /* full entity data */ }
}
```

### `deleted` (Array)
Array of entity IDs that were deleted:

```json
[
  {
    "__typename": "Post",
    "id": "uuid"
  }
]
```

### `invalidations` (Array)
Query cache invalidation hints:

```json
{
  "queryName": "posts",
  "strategy": "INVALIDATE" | "REFETCH",
  "scope": "PREFIX" | "EXACT" | "ALL"
}
```

### `metadata` (Object)
Operation metadata:

```json
{
  "timestamp": "2025-11-11T10:30:00Z",
  "affectedCount": 2
}
```

## GraphQL Response

Cascade data appears in the mutation response as a `cascade` field:

```json
{
  "data": {
    "createPost": {
      "post": { "id": "...", "title": "..." },
      "message": "Post created",
      "cascade": {
        "updated": [
          {
            "__typename": "Post",
            "id": "...",
            "operation": "CREATED",
            "entity": { "id": "...", "title": "...", ... }
          },
          {
            "__typename": "User",
            "id": "...",
            "operation": "UPDATED",
            "entity": { "id": "...", "name": "...", "post_count": 6 }
          }
        ],
        "deleted": [],
        "invalidations": [
          { "queryName": "posts", "strategy": "INVALIDATE", "scope": "PREFIX" }
        ],
        "metadata": {
          "timestamp": "2025-11-11T10:30:00Z",
          "affectedCount": 2
        }
      }
    }
  }
}
```

## Client Integration

### Apollo Client

```typescript
const result = await client.mutate({ mutation: CREATE_POST, variables: input });
const cascade = result.data.createPost.cascade;

if (cascade) {
  // Apply entity updates to cache
  for (const update of cascade.updated) {
    client.cache.writeFragment({
      id: client.cache.identify({ __typename: update.__typename, id: update.id }),
      fragment: gql`fragment _ on ${update.__typename} { id }`,
      data: update.entity
    });
  }

  // Apply invalidations
  for (const hint of cascade.invalidations) {
    if (hint.strategy === 'INVALIDATE') {
      client.cache.evict({ fieldName: hint.queryName });
    }
  }

  // Handle deletions
  for (const deleted of cascade.deleted) {
    client.cache.evict({
      id: client.cache.identify({ __typename: deleted.__typename, id: deleted.id })
    });
  }
}
```

### Relay

```javascript
commitMutation(environment, {
  mutation: CREATE_POST,
  variables: input,
  onCompleted: (response) => {
    const cascade = response.createPost.cascade;
    if (cascade) {
      // Update store with cascade data
      cascade.updated.forEach(update => {
        environment.getStore().publish({
          __typename: update.__typename,
          id: update.id
        }, update.entity);
      });
    }
  }
});
```

## Helper Functions

PostgreSQL helper functions are available to simplify cascade construction:

```sql
-- Build cascade entity
SELECT app.cascade_entity('Post', v_post_id, 'CREATED', 'v_post');

-- Build invalidation hint
SELECT app.cascade_invalidation('posts', 'INVALIDATE', 'PREFIX');
```

## Best Practices

### PostgreSQL Functions

1. **Include all side effects**: Any data modified by the mutation should be included in cascade
2. **Use appropriate operations**: `CREATED` for inserts, `UPDATED` for updates, `DELETED` for deletes
3. **Provide full entities**: Include complete entity data for cache updates
4. **Add invalidations**: Include query invalidation hints for list views

### Client Integration

1. **Apply updates first**: Update cache with new data before invalidations
2. **Handle all operations**: Support CREATE, UPDATE, and DELETE operations
3. **Respect invalidations**: Clear or refetch invalidated queries
4. **Error handling**: Gracefully handle missing cascade data

### Performance

1. **Minimal cascade data**: Only include necessary entities and invalidations
2. **Efficient queries**: Use indexed views for entity data retrieval
3. **Batch operations**: Group multiple cache operations when possible

## Migration

Mutations without cascade work unchanged. Add `enable_cascade=True` and `_cascade` return data incrementally.

## Examples

See `examples/cascade/` for complete working examples including:
- PostgreSQL functions with cascade
- FraiseQL mutations
- Client-side cache updates
- Testing patterns
