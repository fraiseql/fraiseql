# CASCADE Migration Guide

This guide provides step-by-step instructions for migrating existing FraiseQL mutations to use the CASCADE feature. The migration process is designed to be safe, incremental, and easily reversible.

## Quick Assessment: Should You Migrate?

### ✅ Good Candidates for CASCADE Migration

**High-Impact Mutations**:
- **Create Operations**: New entities with side effects (post creation → author stats update)
- **Complex Updates**: Multi-entity modifications (order fulfillment → inventory + user balance)
- **Social Features**: Follow/unfollow, like/unlike with counter updates
- **Content Management**: Article publishing with category/tag updates

**Performance Benefits**:
- **Network Reduction**: 60-80% fewer queries after mutations
- **Cache Consistency**: Automatic cache updates prevent stale data
- **User Experience**: Immediate UI updates without loading states

### ❌ Skip CASCADE for These Cases

**Low-Impact Mutations**:
- Simple preference updates (single entity, no side effects)
- Administrative operations (rarely used)
- Real-time features (cursor positions, typing indicators)

**When CASCADE Adds Little Value**:
- Mutations without client-side follow-up queries
- Single-entity updates with no dependent data
- Operations that trigger full page reloads

## Migration Prerequisites

### 1. Database Schema Updates

#### Create Entity Views

Create views for CASCADE entity data extraction:

```sql
-- Example: Post entity view
CREATE VIEW v_post AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'author_id', author_id,
        'created_at', created_at,
        'updated_at', updated_at,
        'like_count', like_count,
        'comment_count', comment_count
    ) as data
FROM tb_post;

-- Example: User entity view
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'post_count', post_count,
        'follower_count', follower_count,
        'updated_at', updated_at
    ) as data
FROM tb_user;
```

**Best Practices for Entity Views**:
- Include all fields clients typically access
- Use consistent naming: `v_entity_name`
- Add performance indexes on `id` column
- Keep views simple and focused

#### Add CASCADE Helper Functions

```sql
-- Entity cascade entry builder
CREATE OR REPLACE FUNCTION app.cascade_entity(
    entity_type text,
    entity_id uuid,
    operation text,
    view_name text
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        '__typename', entity_type,
        'id', entity_id,
        'operation', operation,
        'entity', (SELECT data FROM view_name WHERE id = entity_id)
    );
END;
$$ LANGUAGE plpgsql;

-- Invalidation entry builder
CREATE OR REPLACE FUNCTION app.cascade_invalidation(
    query_name text,
    strategy text,
    scope text
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'queryName', query_name,
        'strategy', strategy,
        'scope', scope
    );
END;
$$ LANGUAGE plpgsql;

-- Complete cascade builder
CREATE OR REPLACE FUNCTION app.build_cascade(
    updated_entities jsonb DEFAULT '[]'::jsonb,
    deleted_entities jsonb DEFAULT '[]'::jsonb,
    invalidations jsonb DEFAULT '[]'::jsonb,
    metadata jsonb DEFAULT NULL
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'updated', updated_entities,
        'deleted', deleted_entities,
        'invalidations', invalidations,
        'metadata', COALESCE(metadata, jsonb_build_object(
            'timestamp', now(),
            'affectedCount', jsonb_array_length(updated_entities) + jsonb_array_length(deleted_entities)
        ))
    );
END;
$$ LANGUAGE plpgsql;
```

### 2. Update PostgreSQL Functions

#### Before (Standard Mutation)

```sql
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
BEGIN
    -- Create post
    INSERT INTO tb_post (title, content, author_id)
    VALUES (input->>'title', input->>'content', (input->>'author_id')::uuid)
    RETURNING id INTO v_post_id;

    -- Update author stats
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = (input->>'author_id')::uuid;

    -- Return success
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id, 'message', 'Post created')
    );
END;
$$ LANGUAGE plpgsql;
```

#### After (With CASCADE)

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

    -- Return with cascade data
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id, 'message', 'Post created'),
        '_cascade', app.build_cascade(
            updated => jsonb_build_array(
                app.cascade_entity('Post', v_post_id, 'CREATED', 'v_post'),
                app.cascade_entity('User', v_author_id, 'UPDATED', 'v_user')
            ),
            invalidations => jsonb_build_array(
                app.cascade_invalidation('posts', 'INVALIDATE', 'PREFIX'),
                app.cascade_invalidation('userPosts', 'INVALIDATE', 'EXACT')
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

**Key Changes**:
1. Extract entity IDs into variables for reuse
2. Add `_cascade` field to return JSONB
3. Include all affected entities in `updated` array
4. Add query invalidations for cache management

## Phase 1: Application Code Migration

### Step 1.1: Update Mutation Decorators

#### Before
```python
@fraiseql.mutation
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError
```

#### After
```python
@fraiseql.mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError
```

### Step 1.2: Update Success Types

Ensure Success types have explicit field annotations:

#### Before (May work but not recommended)
```python
@fraiseql.type
class CreatePostSuccess:
    # Generic success - may not work with CASCADE
    pass
```

#### After (Required for CASCADE)
```python
@fraiseql.type
class CreatePostSuccess:
    post: Post           # Entity field (case-insensitive match with entity_type)
    message: str         # Standard field
    cascade: Cascade     # CASCADE metadata (added automatically)
```

**Field Mapping Rules**:
- **Entity Fields**: GraphQL object types (Post, User, etc.)
- **Standard Fields**: Primitive types (str, int, bool, etc.)
- **CASCADE Field**: Automatically added when `enable_cascade=True`

### Step 1.3: Update GraphQL Queries

#### Before (Client handles cache manually)
```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    id
    message
  }
}
# Client needs follow-up queries to refresh data
```

#### After (CASCADE provides cache updates)
```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    post {
      id
      title
      content
      author {
        id
        name
        post_count
      }
    }
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
# Client cache automatically updated
```

## Phase 2: Client Integration

### Apollo Client (Most Common)

#### Automatic Cache Updates

```typescript
const CREATE_POST = gql`
  mutation CreatePost($input: CreatePostInput!) {
    createPost(input: $input) {
      post {
        id
        title
        content
      }
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

function CreatePostComponent() {
  const [createPost, { loading, error }] = useMutation(CREATE_POST);

  const handleSubmit = async (input: CreatePostInput) => {
    const result = await createPost({ variables: { input } });

    // Cache automatically updated by Apollo Client
    // No manual cache operations needed!
  };

  return (
    // Your component JSX
  );
}
```

#### Custom Cache Update Logic (Advanced)

```typescript
import { useMutation, gql, ApolloCache } from '@apollo/client';

function applyCascadeToCache(cache: ApolloCache<any>, cascade: any) {
  if (!cascade) return;

  // Apply entity updates
  cascade.updated?.forEach((update: any) => {
    const id = cache.identify({ __typename: update.__typename, id: update.id });
    cache.writeFragment({
      id,
      fragment: gql`
        fragment CascadeUpdate on ${update.__typename} {
          id
        }
      `,
      data: update.entity
    });
  });

  // Apply invalidations
  cascade.invalidations?.forEach((invalidation: any) => {
    if (invalidation.strategy === 'INVALIDATE') {
      // Handle different scopes
      switch (invalidation.scope) {
        case 'PREFIX':
          // Invalidate queries starting with queryName
          break;
        case 'EXACT':
          // Invalidate exact query name match
          break;
        case 'SUFFIX':
          // Invalidate queries ending with queryName
          break;
      }
    }
  });
}

function CreatePostComponent() {
  const [createPost] = useMutation(CREATE_POST, {
    update: (cache, result) => {
      const cascade = result.data?.createPost?.cascade;
      if (cascade) {
        applyCascadeToCache(cache, cascade);
      }
    }
  });

  // ... component logic
}
```

### React Query Integration

```typescript
import { useMutation, useQueryClient } from '@tanstack/react-query';

function useCreatePost() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (input) => {
      const response = await fetch('/graphql', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          query: CREATE_POST_MUTATION,
          variables: { input }
        })
      });
      return response.json();
    },
    onSuccess: (data) => {
      const cascade = data.data?.createPost?.cascade;
      if (cascade) {
        // Apply entity updates
        cascade.updated?.forEach(update => {
          queryClient.setQueryData(
            [update.__typename.toLowerCase(), update.id],
            update.entity
          );
        });

        // Apply invalidations
        cascade.invalidations?.forEach(invalidation => {
          if (invalidation.strategy === 'INVALIDATE') {
            queryClient.invalidateQueries({
              queryKey: [invalidation.queryName],
              exact: invalidation.scope === 'EXACT'
            });
          }
        });
      }
    }
  });
}
```

### Relay Integration

```typescript
import { commitMutation, graphql } from 'react-relay';

const mutation = graphql`
  mutation CreatePostMutation($input: CreatePostInput!) {
    createPost(input: $input) {
      post {
        id
        title
        content
      }
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
    updater: (store) => {
      // Relay automatically handles basic cache updates
      // Add custom logic for complex cascade scenarios
    }
  });
}
```

## Phase 3: Testing & Validation

### Unit Tests

```python
import pytest
from your_app.mutations import CreatePost

def test_cascade_enabled():
    """Test that CASCADE is properly enabled."""
    mutation = CreatePost()
    assert mutation.enable_cascade is True

def test_success_type_fields():
    """Test Success type has required fields."""
    success_type = CreatePost.__annotations__['success']
    fields = success_type.__annotations__

    # Should have entity field, message, and cascade
    assert 'post' in fields
    assert 'message' in fields
    assert 'cascade' in fields  # Added automatically
```

### Integration Tests

```python
import pytest
from fastapi.testclient import TestClient

@pytest.mark.asyncio
async def test_cascade_end_to_end(client: TestClient, db_connection):
    """Test complete CASCADE flow."""
    # Setup test data
    await db_connection.execute("""
        INSERT INTO tb_user (id, name, post_count)
        VALUES ('user-123', 'Test User', 0)
    """)

    # Execute mutation
    response = client.post("/graphql", json={
        "query": """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    post {
                        id
                        title
                        content
                    }
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
        """,
        "variables": {
            "input": {
                "title": "Test Post",
                "content": "Test content",
                "author_id": "user-123"
            }
        }
    })

    assert response.status_code == 200
    data = response.json()

    # Verify response structure
    assert data["data"]["createPost"]["post"] is not None
    assert data["data"]["createPost"]["message"] == "Post created"

    # Verify CASCADE data
    cascade = data["data"]["createPost"]["cascade"]
    assert cascade is not None
    assert len(cascade["updated"]) == 2  # Post + User
    assert len(cascade["invalidations"]) >= 1

    # Verify entity data
    post_update = next(u for u in cascade["updated"] if u["__typename"] == "Post")
    assert post_update["entity"]["title"] == "Test Post"

    user_update = next(u for u in cascade["updated"] if u["__typename"] == "User")
    assert user_update["entity"]["post_count"] == 1  # Should be incremented
```

### Client-Side Tests

```typescript
// Apollo Client test
describe('CASCADE Integration', () => {
  it('applies cascade updates to cache', () => {
    const mockCache = createMockCache();
    const cascade = {
      updated: [
        {
          __typename: 'Post',
          id: 'post-123',
          operation: 'CREATED',
          entity: { id: 'post-123', title: 'Test Post' }
        }
      ],
      invalidations: [
        { queryName: 'posts', strategy: 'INVALIDATE', scope: 'PREFIX' }
      ]
    };

    applyCascadeToCache(mockCache, cascade);

    expect(mockCache.writeFragment).toHaveBeenCalledTimes(1);
    expect(mockCache.evict).toHaveBeenCalledTimes(1);
  });
});
```

## Phase 4: Deployment & Monitoring

### Feature Flags

#### Environment Variable Control

```bash
# Enable CASCADE globally
export FRAISEQL_ENABLE_CASCADE=true

# Or disable for safety
export FRAISEQL_ENABLE_CASCADE=false
```

#### Per-Mutation Control (Recommended)

```python
# Enable CASCADE for specific mutations
@fraiseql.mutation(enable_cascade=True)
class CreatePost:
    # This mutation uses CASCADE

@fraiseql.mutation(enable_cascade=False)
class UpdateProfile:
    # This mutation does not use CASCADE
```

### Monitoring Setup

#### Performance Metrics

```python
# Add to your monitoring setup
cascade_processing_duration = Histogram(
    'fraiseql_cascade_processing_duration_seconds',
    'Time spent processing cascade data'
)

cascade_payload_bytes = Histogram(
    'fraiseql_cascade_payload_bytes',
    'Size of cascade payloads in bytes'
)

cascade_entities_total = Counter(
    'fraiseql_cascade_entities_total',
    'Total entities processed via cascade',
    ['operation']  # CREATED, UPDATED, DELETED
)
```

#### Grafana Dashboard

Create dashboards tracking:
- CASCADE processing latency
- Payload size distribution
- Error rates
- Cache hit rate improvements
- Network request reduction

#### Alerting Rules

```yaml
groups:
  - name: cascade_performance
    rules:
      - alert: HighCascadeProcessingTime
        expr: histogram_quantile(0.95, rate(fraiseql_cascade_processing_duration_seconds_bucket[5m])) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "CASCADE processing time is too high"

      - alert: LargeCascadePayloads
        expr: histogram_quantile(0.95, fraiseql_cascade_payload_bytes) > 50000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "CASCADE payloads are getting large"

      - alert: CascadeProcessingErrors
        expr: rate(fraiseql_cascade_errors_total[5m]) > 0.05
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High rate of CASCADE processing errors"
```

### Rollback Strategy

#### Immediate Rollback (If Issues Arise)

1. **Set environment variable**: `FRAISEQL_ENABLE_CASCADE=false`
2. **No database changes needed**
3. **Clients ignore cascade field gracefully**
4. **Monitor for 24-48 hours**

#### Partial Rollback

1. **Keep CASCADE enabled but reduce scope**
2. **Remove complex cascades, keep simple ones**
3. **Adjust invalidation strategies**

#### Complete Rollback

1. **Remove `enable_cascade=True` from mutations**
2. **Update client code to remove CASCADE handling**
3. **Monitor for performance improvements**
4. **Document lessons learned**

## Troubleshooting Common Issues

### CASCADE Data Not Appearing

**Symptoms**: `cascade` field is `null` or missing in GraphQL response

**Checklist**:
1. ✅ Mutation has `enable_cascade=True`
2. ✅ PostgreSQL function returns `_cascade` field
3. ✅ Entity views exist and are accessible
4. ✅ Success type has proper field annotations

**Debug Commands**:
```sql
-- Check PostgreSQL function output
SELECT jsonb_pretty(_cascade) FROM graphql.create_post('{"title": "Test"}');

-- Validate entity view data
SELECT data FROM v_post WHERE id = 'post-123';
```

### Entity Fields Missing

**Symptoms**: Entity fields (like `post`) not appearing in response

**Common Causes**:
- Case sensitivity mismatch between `entity_type` and field names
- Missing fields in PostgreSQL entity structure
- Incorrect Success type field definitions

**Debug Steps**:
```python
# Check entity flattener logs
logger.debug(f"Entity type '{entity_type}' matched field '{field_name}'")

# Validate field mapping
expected_fields = list(success_type.__annotations__.keys())
print(f"Expected fields: {expected_fields}")
```

### Client Cache Not Updating

**Symptoms**: Client cache doesn't reflect CASCADE changes

**Checklist**:
1. ✅ Apollo Client version supports CASCADE
2. ✅ Cache updates applied correctly
3. ✅ Entity IDs match cache keys (`__typename` + `id`)
4. ✅ Fragment structure matches entity schema

**Debug Tips**:
```typescript
// Manually test cache operations
const id = cache.identify({ __typename: 'Post', id: 'post-123' });
console.log('Cache ID:', id);
```

### Performance Issues

**Symptoms**: CASCADE processing is slow or memory-intensive

**Optimization Steps**:
1. **Limit CASCADE scope**: Include only directly affected entities
2. **Optimize database views**: Add indexes for CASCADE view queries
3. **Batch updates**: Group related entity updates
4. **Monitor payload size**: Keep CASCADE data under 50KB

```sql
-- Monitor CASCADE payload sizes
SELECT
    pg_size_pretty(pg_column_size(_cascade)) as cascade_size,
    jsonb_array_length(_cascade->'updated') as entities_updated
FROM graphql.create_post('{"title": "Test"}');
```

## Migration Checklist

### Database Preparation
- [ ] Create entity views for CASCADE data extraction
- [ ] Add CASCADE helper functions to schema
- [ ] Update PostgreSQL functions to include `_cascade` field
- [ ] Test CASCADE data generation

### Application Code Changes
- [ ] Add `enable_cascade=True` to mutation decorators
- [ ] Update Success types with explicit field annotations
- [ ] Update GraphQL queries to request CASCADE field
- [ ] Implement client-side CASCADE processing logic
- [ ] Test CASCADE integration end-to-end

### Deployment Steps
- [ ] Enable feature flag in staging environment
- [ ] Deploy with CASCADE-enabled mutations
- [ ] Monitor performance and errors
- [ ] Gradually enable for production traffic

### Post-Deployment
- [ ] Monitor CASCADE performance metrics
- [ ] Collect user feedback on performance improvements
- [ ] Plan optimizations based on usage patterns
- [ ] Document lessons learned and best practices

## Best Practices Summary

### Database Design
- Use consistent entity view naming (`v_entity_name`)
- Include all fields clients typically need
- Add performance indexes on frequently accessed columns
- Validate view data completeness and accuracy

### Application Architecture
- Start with simple mutations (create operations)
- Use feature flags for gradual rollout
- Implement comprehensive error handling
- Monitor performance and payload sizes

### Client Integration
- Leverage automatic cache updates when possible
- Implement manual updates for complex scenarios
- Handle CASCADE processing errors gracefully
- Test thoroughly with real data patterns

### Performance Optimization
- Keep CASCADE payloads under 50KB
- Include only directly affected entities
- Use appropriate invalidation strategies
- Monitor and optimize based on real usage

## Support Resources

- **Architecture Documentation**: `docs/mutations/cascade-architecture.md`
- **Best Practices**: `docs/guides/cascade-best-practices.md`
- **Examples**: `examples/graphql-cascade/`
- **Troubleshooting**: `docs/guides/troubleshooting.md`

## Migration Effort Estimate

**Typical Application Migration**:
- **Small App** (1-5 mutations): 1-2 days
- **Medium App** (5-20 mutations): 3-5 days
- **Large App** (20+ mutations): 1-2 weeks

**Risk Level**: Low (opt-in feature with easy rollback)
**Performance Impact**: Minimal (typically < 5% overhead)
**User Experience Impact**: High (60-80% reduction in follow-up queries)
