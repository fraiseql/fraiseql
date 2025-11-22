# Adopting GraphQL Cascade

This guide helps you adopt GraphQL Cascade in your existing FraiseQL applications. Cascade enables automatic cache updates and side effect tracking for mutations, improving performance by reducing unnecessary network requests.

## Quick Assessment: Should You Use Cascade?

**Use Cascade If:**
- ✅ Your mutations affect multiple entities (e.g., creating a post updates user stats)
- ✅ You want to reduce client-side network requests
- ✅ You're using Apollo Client, Relay, or similar caching GraphQL clients
- ✅ Performance is critical for your application

**Skip Cascade If:**
- ❌ Mutations only affect single entities without side effects
- ❌ You're using simple REST-style clients without caching
- ❌ Network requests are not a performance bottleneck
- ❌ You prefer explicit cache management

## Migration Steps

### Step 1: Enable Cascade on Mutations

Start with one mutation to test cascade functionality.

**Before:**
```python
@mutation
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError
```

**After:**
```python
@mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError
```

### Step 2: Update PostgreSQL Functions

Modify your PostgreSQL functions to return cascade metadata.

**Before:**
```sql
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS jsonb AS $$
BEGIN
    -- Create post logic...
    INSERT INTO tb_post (title, content, author_id)
    VALUES (input->>'title', input->>'content', (input->>'author_id')::uuid)
    RETURNING id INTO v_post_id;

    -- Update user stats
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id, 'message', 'Post created')
    );
END;
$$ LANGUAGE plpgsql;
```

**After:**
```sql
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS jsonb AS $$
BEGIN
    -- Create post logic...
    INSERT INTO tb_post (title, content, author_id)
    VALUES (input->>'title', input->>'content', (input->>'author_id')::uuid)
    RETURNING id INTO v_post_id;

    -- Update user stats
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id, 'message', 'Post created'),
        '_cascade', jsonb_build_object(
            'updated', jsonb_build_array(
                -- New post entity
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', v_post_id,
                    'operation', 'CREATED',
                    'entity', (SELECT data FROM v_post WHERE id = v_post_id)
                ),
                -- Updated user entity
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

### Step 3: Create Entity Views

Ensure you have views that expose entity data for cascade:

```sql
-- View for Post entities
CREATE VIEW v_post AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'author_id', author_id,
        'created_at', created_at
    ) as data
FROM tb_post;

-- View for User entities
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'post_count', post_count,
        'created_at', created_at
    ) as data
FROM tb_user;
```

### Step 4: Update Client Code

Modify your GraphQL clients to handle cascade data.

#### Apollo Client Integration

```typescript
import { gql, useMutation } from '@apollo/client';

const CREATE_POST = gql`
    mutation CreatePost($input: CreatePostInput!) {
        createPost(input: $input) {
            id
            message
            cascade {
                updated {
                    __typename
                    id
                    operation
                    entity
                }
                deleted {
                    __typename
                    id
                }
                invalidations {
                    queryName
                    strategy
                    scope
                }
            }
        }
    }
`;

function CreatePostComponent() {
    const [createPost, { loading, error }] = useMutation(CREATE_POST);

    const handleSubmit = async (input) => {
        const result = await createPost({ variables: { input } });
        const cascade = result.data.createPost.cascade;

        if (cascade) {
            // Apply entity updates to cache
            for (const update of cascade.updated) {
                client.cache.writeFragment({
                    id: client.cache.identify({
                        __typename: update.__typename,
                        id: update.id
                    }),
                    fragment: gql`fragment _ on ${update.__typename} { id }`,
                    data: update.entity
                });
            }

            // Apply invalidations
            for (const invalidation of cascade.invalidations) {
                if (invalidation.strategy === 'INVALIDATE') {
                    client.cache.evict({
                        fieldName: invalidation.queryName
                    });
                }
            }

            // Handle deletions
            for (const deletion of cascade.deleted) {
                client.cache.evict({
                    id: client.cache.identify({
                        __typename: deletion.__typename,
                        id: deletion.id
                    })
                });
            }
        }
    };

    // ... component JSX
}
```

#### Relay Integration

```javascript
import { commitMutation } from 'react-relay';

const mutation = graphql`
    mutation CreatePostMutation($input: CreatePostInput!) {
        createPost(input: $input) {
            id
            message
            cascade {
                updated {
                    __typename
                    id
                    operation
                    entity
                }
                invalidations {
                    queryName
                    strategy
                    scope
                }
            }
        }
    }
`;

function commitCreatePost(environment, input) {
    return commitMutation(environment, {
        mutation,
        variables: { input },
        updater: (store, data) => {
            const cascade = data.createPost.cascade;
            if (!cascade) return;

            // Apply entity updates
            cascade.updated.forEach(update => {
                const record = store.get(update.id);
                if (record) {
                    // Update existing record
                    Object.keys(update.entity).forEach(key => {
                        record.setValue(update.entity[key], key);
                    });
                } else {
                    // Create new record
                    const newRecord = store.create(update.id, update.__typename);
                    Object.keys(update.entity).forEach(key => {
                        newRecord.setValue(update.entity[key], key);
                    });
                }
            });

            // Apply invalidations
            cascade.invalidations.forEach(invalidation => {
                if (invalidation.strategy === 'INVALIDATE') {
                    store.invalidateStore();
                    // Or more selective invalidation based on queryName
                }
            });
        }
    });
}
```

#### Vanilla GraphQL Client

```javascript
const result = await client.mutate({
    mutation: CREATE_POST,
    variables: { input }
});

const cascade = result.data.createPost.cascade;
if (cascade) {
    // Handle cascade data according to your cache implementation
    handleCascadeUpdates(cascade);
}
```

## Helper Functions

Use these PostgreSQL helper functions to simplify cascade construction:

```sql
-- Create cascade entity update
CREATE OR REPLACE FUNCTION app.cascade_entity(
    p_typename TEXT,
    p_id UUID,
    p_operation TEXT,
    p_view_name TEXT
) RETURNS JSONB AS $$
DECLARE
    v_entity_data JSONB;
BEGIN
    EXECUTE format('SELECT data FROM %I WHERE id = $1', p_view_name)
    INTO v_entity_data
    USING p_id;

    RETURN jsonb_build_object(
        '__typename', p_typename,
        'id', p_id,
        'operation', p_operation,
        'entity', v_entity_data
    );
END;
$$ LANGUAGE plpgsql;

-- Create cache invalidation
CREATE OR REPLACE FUNCTION app.cascade_invalidation(
    p_query_name TEXT,
    p_strategy TEXT,
    p_scope TEXT
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        'queryName', p_query_name,
        'strategy', p_strategy,
        'scope', p_scope
    );
END;
$$ LANGUAGE plpgsql;

-- Build complete cascade object
CREATE OR REPLACE FUNCTION app.build_cascade(
    p_updated JSONB DEFAULT '[]'::jsonb,
    p_deleted JSONB DEFAULT '[]'::jsonb,
    p_invalidations JSONB DEFAULT '[]'::jsonb,
    p_metadata JSONB DEFAULT NULL
) RETURNS JSONB AS $$
DECLARE
    v_metadata JSONB;
BEGIN
    v_metadata := p_metadata;
    IF v_metadata IS NULL THEN
        v_metadata := jsonb_build_object(
            'timestamp', now(),
            'affectedCount', (jsonb_array_length(p_updated) + jsonb_array_length(p_deleted))
        );
    END IF;

    RETURN jsonb_build_object(
        'updated', p_updated,
        'deleted', p_deleted,
        'invalidations', p_invalidations,
        'metadata', v_metadata
    );
END;
$$ LANGUAGE plpgsql;
```

Simplified function using helpers:

```sql
-- Using helper functions
v_cascade := app.build_cascade(
    updated => jsonb_build_array(
        app.cascade_entity('Post', v_post_id, 'CREATED', 'v_post'),
        app.cascade_entity('User', v_author_id, 'UPDATED', 'v_user')
    ),
    invalidations => jsonb_build_array(
        app.cascade_invalidation('posts', 'INVALIDATE', 'PREFIX')
    )
);
```

## Testing Your Cascade Implementation

### 1. Unit Tests

```python
def test_cascade_data_structure():
    """Test that cascade data has correct structure."""
    # Test your PostgreSQL function returns valid cascade data
    pass

def test_cascade_serialization():
    """Test that cascade data serializes correctly in GraphQL responses."""
    # Test JSON encoding includes cascade field
    pass
```

### 2. Integration Tests

```python
async def test_cascade_end_to_end():
    """Test complete cascade flow."""
    # Execute mutation
    # Verify cascade data in response
    # Verify client cache updates
    pass
```

### 3. Client Tests

```typescript
it('should update cache with cascade data', () => {
    // Mock mutation response with cascade
    // Verify cache updates
    // Verify UI updates without additional queries
});
```

## Performance Considerations

### When Cascade Helps
- **Multiple Entity Updates**: Creating a post that updates user stats
- **List Invalidation**: New items require list cache invalidation
- **Complex Relationships**: Updates that affect related entities

### When Cascade May Not Help
- **Single Entity**: Simple CRUD without side effects
- **Frequent Updates**: Very high-frequency mutations
- **Large Payloads**: Cascade data larger than refetched data

### Monitoring Performance
```python
# Track cascade metrics
cascade_processing_time = Histogram('fraiseql_cascade_processing_time', 'Cascade processing time')
cascade_payload_size = Histogram('fraiseql_cascade_payload_size', 'Cascade payload size')
cache_hit_rate = Gauge('fraiseql_cache_hit_rate', 'Client cache hit rate')
```

## Troubleshooting

### Common Issues

**Cascade data not appearing in response:**
- Check `enable_cascade=True` on mutation decorator
- Verify PostgreSQL function returns `_cascade` field
- Check for JSON serialization errors

**Client cache not updating:**
- Verify cascade data structure matches client expectations
- Check for typos in `__typename` and field names
- Ensure entity IDs match cache keys

**Performance degradation:**
- Review cascade payload size
- Consider selective cascade (only essential entities)
- Monitor cache hit rates

### Debugging

Enable debug logging to see cascade processing:

```python
import logging
logging.getLogger('fraiseql.mutations').setLevel(logging.DEBUG)
```

Check PostgreSQL function logs for cascade building errors.

## Rollback Plan

If cascade causes issues:

1. **Immediate**: Remove `enable_cascade=True` from mutation decorators
2. **Client**: Update clients to ignore cascade field
3. **Database**: PostgreSQL functions can keep `_cascade` (ignored when disabled)
4. **Full**: Remove cascade code entirely

## Examples by Use Case

### Blog Platform
- **Create Post**: Cascade updates post list, author stats
- **Delete Comment**: Cascade updates post comment count, comment list
- **Like Post**: Cascade updates post like count, user like history

### E-commerce
- **Add to Cart**: Cascade updates cart total, item counts
- **Place Order**: Cascade updates inventory, order history
- **Update Profile**: Cascade updates user preferences, order addresses

### Social Media
- **Create Tweet**: Cascade updates timeline, user tweet count
- **Follow User**: Cascade updates follower/following counts
- **Like Post**: Cascade updates like counts, notification feeds

## Next Steps

1. **Start Small**: Enable cascade on one mutation
2. **Test Thoroughly**: Verify client and server behavior
3. **Monitor Performance**: Track metrics and user experience
4. **Expand Gradually**: Add cascade to more mutations over time
5. **Gather Feedback**: Learn from real-world usage patterns

## Support

- **Documentation**: See `docs/features/graphql-cascade.md`
- **Examples**: Check `examples/graphql-cascade/`
- **Community**: GitHub Discussions for questions
- **Enterprise**: Priority support available

---

**Migration completed?** Update your application to use cascade and enjoy improved performance!</content>
</xai:function_call</xai:function_call name="write">
<parameter name="filePath">docs/guides/cascade-best-practices.md
