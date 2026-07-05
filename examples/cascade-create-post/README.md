# CASCADE Create Post Example

Creating a post that also updates the author's post count and emits cache-invalidation
hints — all reported in one typed `cascade`, so clients update their caches without a
follow-up query.

## Overview

When a user creates a post, we:

1. Create the post entity
2. Increment the author's `post_count`
3. Return a `cascade` listing every affected entity + client cache-invalidation hints

The affected entities (`Post`, `User`) are projected and field-authorized by FraiseQL
just like a queried entity, and read from their RLS-protected views so cascade
row-visibility matches a query's.

## Schema

```sql
CREATE TABLE tb_user (id UUID PRIMARY KEY, name TEXT, post_count INT DEFAULT 0);
CREATE TABLE tb_post (id UUID PRIMARY KEY, title TEXT, author_id UUID REFERENCES tb_user(id));

-- Read views (RLS-protected in a real deployment) — cascade entities MUST be read
-- from these, never from tb_* directly, and the views MUST be `security_invoker`
-- so base-table RLS applies (a default view runs as the owner and leaks cross-tenant
-- rows). The `data` JSONB uses snake_case keys; FraiseQL projects them to camelCase.
CREATE VIEW v_user WITH (security_invoker = true) AS SELECT id, jsonb_build_object('id', id, 'name', name, 'post_count', post_count) AS data FROM tb_user;
CREATE VIEW v_post WITH (security_invoker = true) AS SELECT id, jsonb_build_object('id', id, 'title', title, 'author_id', author_id) AS data FROM tb_post;
```

## PostgreSQL Function

Uses the shipped builders (`fraiseql setup` installs them): `fraiseql.mutation_ok`,
`fraiseql.build_cascade`, `fraiseql.cascade_entity`, `fraiseql.cascade_invalidation`.

```sql
CREATE OR REPLACE FUNCTION graphql.create_post(input jsonb)
RETURNS SETOF app.mutation_response AS $$
DECLARE
    v_post_id   uuid;
    v_author_id uuid := (input->>'author_id')::uuid;
BEGIN
    INSERT INTO tb_post (title, author_id)
    VALUES (input->>'title', v_author_id)
    RETURNING id INTO v_post_id;

    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        p_entity      := (SELECT data FROM v_post WHERE id = v_post_id),
        p_entity_id   := v_post_id,
        p_entity_type := 'Post',
        p_cascade     := fraiseql.build_cascade(
            p_updated := jsonb_build_array(
                fraiseql.cascade_entity('Post', v_post_id,  'CREATED', 'v_post'),
                fraiseql.cascade_entity('User', v_author_id, 'UPDATED', 'v_user')
            ),
            p_invalidations := jsonb_build_array(
                fraiseql.cascade_invalidation('posts', 'INVALIDATE', 'PREFIX')
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

## FraiseQL Implementation

Opt the mutation into the typed cascade surface with the `cascade=True` flag:

```python
@fraiseql.type(crud=True, cascade=True)   # crud=True generates create/update/delete
class Post:
    id: int
    title: str
    author_id: int
```

The compiler then rewrites `createPost` to return a **payload wrapper**
`CreatePostPayload { entity: Post, cascade: CascadeUpdates, updatedFields: [String!] }`
— cascade lives on the payload, not on `Post`, so normalized caches stay clean. There
is no `enable_cascade` argument and no bare `Cascade` type; the flag is `cascade=True`
and the field is typed `CascadeUpdates`.

## GraphQL Response

```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    entity { id title authorId }
    cascade {
      updated { __typename id operation entity { ... on Post { title } ... on User { postCount } } }
      invalidations { queryName strategy scope }
      metadata { affectedCount truncated }
    }
    updatedFields
  }
}
```

```json
{
  "data": {
    "createPost": {
      "entity": { "id": "post-123", "title": "My New Post", "authorId": "user-456" },
      "cascade": {
        "updated": [
          { "__typename": "Post", "id": "post-123", "operation": "CREATED", "entity": { "title": "My New Post" } },
          { "__typename": "User", "id": "user-456", "operation": "UPDATED", "entity": { "postCount": 5 } }
        ],
        "invalidations": [ { "queryName": "posts", "strategy": "INVALIDATE", "scope": "PREFIX" } ],
        "metadata": { "affectedCount": 2, "truncated": false }
      },
      "updatedFields": []
    }
  }
}
```

## Client Integration (Apollo)

Because each `cascade.updated` entry carries `__typename` + `id` + the full `entity`,
a normalized cache can write it directly:

```typescript
function applyCascade(cache: ApolloCache<unknown>, cascade: CascadeUpdates) {
  cascade.updated?.forEach(u => {
    cache.writeFragment({
      id: cache.identify({ __typename: u.__typename, id: u.id }),
      fragment: gql`fragment _ on ${u.__typename} { id }`,
      data: u.entity,
    });
  });
  cascade.invalidations?.forEach(inv => {
    if (inv.strategy === 'INVALIDATE') cache.evict({ fieldName: inv.queryName });
  });
}
```

## Running the Example

```bash
createdb cascade_create_example
fraiseql setup --database-url postgres:///cascade_create_example   # installs fraiseql.* helpers
psql -d cascade_create_example -f schema.sql
```

## Key Learning Points

1. **Opt in with `cascade=True`** — the mutation then returns `<Name>Payload { entity, cascade, updatedFields }`.
2. **Nested entry shape** — each `updated` entry is `{ __typename, id, operation, entity }`; `deleted` entries are `{ __typename, id, deletedAt }` (no body).
3. **Read from views** — assemble cascade entities from `v_*` (RLS-protected), never `tb_*`.
4. **Enforced like a query** — cascade entities are projected to camelCase and field-authorized; the response is bounded by `cascade_limits`.
5. **Cache-clean** — cascade is on the payload, not the entity, so normalized client caches never store a `cascade` key on `Post`.
