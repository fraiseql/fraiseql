# CASCADE Create Post Example

This example demonstrates the basic CASCADE pattern: creating a post that updates the author's post count and provides cache invalidation hints.

## Overview

**Business Logic**: When a user creates a post, we need to:
1. Create the post entity
2. Update the author's post count
3. Provide CASCADE data so clients can update their caches

**CASCADE Benefits**:
- Client caches are automatically updated with the new post
- Author's post count is updated in cache
- Post list queries are invalidated for refetching
- No additional network requests needed

## Schema

```sql
-- Users and posts with CASCADE views
CREATE TABLE tb_user (id UUID PRIMARY KEY, name TEXT, post_count INT DEFAULT 0);
CREATE TABLE tb_post (id UUID PRIMARY KEY, title TEXT, author_id UUID REFERENCES tb_user(id));

-- CASCADE entity views
CREATE VIEW v_user AS SELECT id, jsonb_build_object('id', id, 'name', name, 'post_count', post_count) as data FROM tb_user;
CREATE VIEW v_post AS SELECT id, jsonb_build_object('id', id, 'title', title, 'author_id', author_id) as data FROM tb_post;
```

## PostgreSQL Function

```sql
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
    v_author_id uuid;
BEGIN
    -- Create post
    INSERT INTO tb_post (title, author_id)
    VALUES (input->>'title', (input->>'author_id')::uuid)
    RETURNING id INTO v_post_id;

    v_author_id := (input->>'author_id')::uuid;

    -- Update author stats
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    -- Return with CASCADE
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', v_post_id, 'message', 'Post created'),
        '_cascade', jsonb_build_object(
            'updated', jsonb_build_array(
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', v_post_id,
                    'operation', 'CREATED',
                    'entity', (SELECT data FROM v_post WHERE id = v_post_id)
                ),
                jsonb_build_object(
                    '__typename', 'User',
                    'id', v_author_id,
                    'operation', 'UPDATED',
                    'entity', (SELECT data FROM v_user WHERE id = v_author_id)
                )
            ),
            'invalidations', jsonb_build_array(
                jsonb_build_object('queryName', 'posts', 'strategy', 'INVALIDATE', 'scope', 'PREFIX')
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

## FraiseQL Implementation

```python
@fraiseql.type
class CreatePostSuccess:
    post: Post          # Entity field - will contain the created post
    message: str        # Standard success message
    cascade: Cascade    # CASCADE metadata (added automatically)

@fraiseql.mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError
```

## GraphQL Response

```json
{
  "data": {
    "createPost": {
      "post": {
        "id": "post-123",
        "title": "My New Post",
        "author_id": "user-456"
      },
      "message": "Post created successfully",
      "cascade": {
        "updated": [
          {
            "__typename": "Post",
            "id": "post-123",
            "operation": "CREATED",
            "entity": {
              "id": "post-123",
              "title": "My New Post",
              "author_id": "user-456"
            }
          },
          {
            "__typename": "User",
            "id": "user-456",
            "operation": "UPDATED",
            "entity": {
              "id": "user-456",
              "name": "John Doe",
              "post_count": 5
            }
          }
        ],
        "invalidations": [
          {
            "queryName": "posts",
            "strategy": "INVALIDATE",
            "scope": "PREFIX"
          }
        ]
      }
    }
  }
}
```

## Client Integration

### Apollo Client (Automatic)

```typescript
const CREATE_POST = gql`
  mutation CreatePost($input: CreatePostInput!) {
    createPost(input: $input) {
      post {
        id
        title
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
`;

// Apollo automatically applies CASCADE updates to cache
const [createPost] = useMutation(CREATE_POST);
```

### Manual Cache Updates

```typescript
function applyCascadeToCache(cache: ApolloCache, cascade: any) {
  // Update entities in cache
  cascade.updated?.forEach(update => {
    const id = cache.identify(update);
    cache.writeFragment({
      id,
      fragment: gql`fragment _ on ${update.__typename} { id }`,
      data: update.entity
    });
  });

  // Apply invalidations
  cascade.invalidations?.forEach(invalidation => {
    if (invalidation.strategy === 'INVALIDATE') {
      // Invalidate queries matching the pattern
      cache.evict({ fieldName: invalidation.queryName });
    }
  });
}
```

## Running the Example

```bash
# Set up database
createdb cascade_create_example
psql -d cascade_create_example -f schema.sql

# Run application
python main.py

# Test mutation
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation CreatePost($input: CreatePostInput!) { createPost(input: $input) { post { id title } message cascade { updated { __typename id operation } invalidations { queryName strategy scope } } } }",
    "variables": { "input": { "title": "Hello World", "author_id": "user-123" } }
  }'
```

## Key Learning Points

1. **Entity Fields**: Success types must have explicit entity fields (e.g., `post: Post`)
2. **Case Insensitive Matching**: `entity_type` from PostgreSQL matches field names case-insensitively
3. **CASCADE Structure**: Updated entities + invalidations for comprehensive cache management
4. **Automatic Application**: Modern GraphQL clients can apply CASCADE updates automatically
5. **Performance**: CASCADE eliminates follow-up queries for cache updates</content>
</xai:function_call<parameter name="filePath">examples/cascade-create-post/main.py
