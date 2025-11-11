# GraphQL Cascade Example

This example demonstrates GraphQL Cascade functionality in FraiseQL. Cascade enables automatic cache updates and side effect tracking for mutations.

## Overview

When a mutation modifies data, it can include cascade information that clients use to update their caches without additional queries. This example shows:

- PostgreSQL function with cascade metadata
- FraiseQL mutation with `enable_cascade=True`
- Client-side cache updates

## Schema

```sql
-- Users table
CREATE TABLE tb_user (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    post_count INT DEFAULT 0
);

-- Posts table
CREATE TABLE tb_post (
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT,
    author_id UUID NOT NULL REFERENCES tb_user(id)
);

-- Views for cascade
CREATE VIEW v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'post_count', post_count
    ) as data
FROM tb_user;

CREATE VIEW v_post AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'author_id', author_id
    ) as data
FROM tb_post;
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

## FraiseQL Mutation

```python
@mutation(enable_cascade=True)
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

## Client Integration (Apollo)

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
}
```

## Running the Example

1. Set up the database:
```bash
psql -f schema.sql
```

2. Run the application:
```bash
python main.py
```

3. Test the mutation:
```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    post {
      id
      title
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
```

## Files

- `schema.sql` - Database schema and functions
- `main.py` - FraiseQL application
- `client.html` - Simple client-side example
