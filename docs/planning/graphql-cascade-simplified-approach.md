# GraphQL Cascade: Simplified PostgreSQL-First Approach

**Status**: Analysis - Simplified Implementation Based on SpecQL Pattern
**Created**: 2025-11-11
**Version**: 2.0 (Simplified)

---

## Executive Summary

After analyzing the SpecQL implementation, we can **dramatically simplify** the GraphQL Cascade implementation by **leveraging the existing PostgreSQL function return structure** that FraiseQL already uses.

**Key Insight**: SpecQL (and FraiseQL) already return a structured `mutation_result` type with an `extra_metadata` JSONB field. We can **reuse this field** for cascade data instead of creating an entirely separate tracking system.

---

## Current FraiseQL Mutation Response Structure

FraiseQL mutations already return this structure from PostgreSQL:

```json
{
  "success": true/false,
  "data": {
    "id": "uuid",
    "field1": "value",
    ...
  },
  "error": {
    "code": "ERROR_CODE",
    "message": "Error message"
  }
}
```

---

## SpecQL's `mutation_result` Pattern

SpecQL uses a standardized PostgreSQL composite type:

```sql
CREATE TYPE app.mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB  -- ← WE CAN USE THIS!
);
```

The `extra_metadata` field is **already designed** for:
- Impact information
- Side effects
- Cache invalidation hints
- Custom metadata

---

## Simplified Cascade Implementation

### Phase 1: Extend PostgreSQL Return Structure

Instead of creating a separate tracking system, we **extend the existing response pattern**:

#### Current FraiseQL Pattern:
```sql
CREATE OR REPLACE FUNCTION fn_create_post(input jsonb)
RETURNS jsonb AS $$
BEGIN
    -- Business logic...

    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'id', v_post_id,
            'title', input->>'title'
        )
    );
END;
$$ LANGUAGE plpgsql;
```

#### **Cascade-Enhanced Pattern:**
```sql
CREATE OR REPLACE FUNCTION fn_create_post(input jsonb)
RETURNS jsonb AS $$
DECLARE
    v_post_id uuid;
    v_author_id uuid;
BEGIN
    -- Business logic
    INSERT INTO tb_post (title, content, author_id)
    VALUES (
        input->>'title',
        input->>'content',
        (input->>'author_id')::uuid
    )
    RETURNING id INTO v_post_id;

    v_author_id := (input->>'author_id')::uuid;

    -- Update author stats
    UPDATE tb_user
    SET post_count = post_count + 1
    WHERE id = v_author_id;

    -- Return with cascade metadata
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'id', v_post_id,
            'title', input->>'title'
        ),

        -- ← ADD CASCADE HERE (optional, PostgreSQL decides what to include)
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

---

## Key Simplifications

### 1. No Python Tracking Required

**Original Plan**: Complex Python `CascadeTracker` class with context variables.

**Simplified**: PostgreSQL functions **directly build** the cascade structure.

```python
# NO NEED FOR THIS:
# tracker = CascadeTracker()
# tracker.track_create("Post", entity)
# tracker.track_update("User", user)

# PostgreSQL does it all in the function!
```

### 2. No Separate Cascade Types

**Original Plan**: Create 7+ new GraphQL types (`CascadeData`, `CascadeEntityUpdate`, etc.)

**Simplified**: Reuse existing response structure. The `_cascade` field is **optional JSONB** that clients can parse.

```python
@fraise_type
class CreatePostSuccess:
    post: Post
    message: str
    # NO NEW FIELDS NEEDED - _cascade is in the raw JSONB response
```

### 3. Decorator Enhancement is Minimal

**Original Plan**: Major changes to `MutationDefinition` class, complex resolver logic.

**Simplified**: Just check if `_cascade` exists in response and **pass it through**:

```python
async def resolver(info: GraphQLResolveInfo, input: dict[str, Any]) -> Any:
    db = info.context.get("db")

    # Execute function (unchanged)
    result = await db.execute_function(
        f"{self.schema}.{self.function_name}",
        input_data
    )

    # Parse result (unchanged)
    parsed_result = parse_mutation_result(
        result,
        self.success_type,
        self.error_type,
        self.error_config,
    )

    # ← SIMPLE: If cascade data exists, attach it
    if "_cascade" in result:
        parsed_result.__cascade__ = result["_cascade"]

    return parsed_result
```

---

## Comparison: Original vs Simplified

| Aspect | Original Plan | Simplified Approach |
|--------|--------------|---------------------|
| **Python Code** | ~2000 lines (tracker, builder, types) | ~50 lines (check for `_cascade`) |
| **PostgreSQL** | Convention for `_cascade_entities` array | Same convention (already standard) |
| **GraphQL Types** | 7 new types | 0 new types (JSONB passthrough) |
| **Decorator Changes** | Major refactor | Minimal addition |
| **Manual Tracking API** | Full API with context manager | Not needed (PostgreSQL does it) |
| **Configuration** | 8 new config options | 1 option: `enable_cascade` |
| **Performance** | Tracking overhead + deduplication | Zero overhead (PostgreSQL native) |

---

## Implementation Plan (Simplified)

### Phase 1: Response Passthrough (1 day)

1. **Modify Mutation Resolver** (`src/fraiseql/mutations/mutation_decorator.py`):

```python
async def resolver(info: GraphQLResolveInfo, input: dict[str, Any]) -> Any:
    # ... existing code ...

    # Parse result
    parsed_result = parse_mutation_result(result, ...)

    # ← ADD: Check for cascade data
    if "_cascade" in result and self.enable_cascade:
        parsed_result.__cascade__ = result["_cascade"]

    return parsed_result
```

2. **Add Decorator Parameter**:

```python
def mutation(
    _cls: type[T] | None = None,
    *,
    enable_cascade: bool = False,  # ← ADD THIS
    ...
):
    # ...
```

**Tests**: Verify `_cascade` is passed through when present.

---

### Phase 2: Response Formatting (1 day)

Update the GraphQL response builder to include cascade in the response:

```python
def format_mutation_response(result: Any, field_name: str) -> dict[str, Any]:
    response = {
        "data": {
            field_name: serialize_result(result)
        }
    }

    # Include cascade if present
    if hasattr(result, "__cascade__"):
        # Option A: Add to result object
        response["data"][field_name]["cascade"] = result.__cascade__

        # Option B: Add to extensions (GraphQL standard location)
        response["extensions"] = {
            "cascade": result.__cascade__
        }

    return response
```

**Tests**: Verify cascade appears in GraphQL response.

---

### Phase 3: PostgreSQL Convention Documentation (1 day)

Document the PostgreSQL function pattern:

**Location**: `docs/features/graphql-cascade.md`

```markdown
# GraphQL Cascade

## PostgreSQL Function Pattern

To enable cascade for a mutation, include a `_cascade` field in the return JSONB:

```sql
CREATE OR REPLACE FUNCTION fn_your_mutation(input jsonb)
RETURNS jsonb AS $$
BEGIN
    -- Your mutation logic...

    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(...),
        '_cascade', jsonb_build_object(
            'updated', jsonb_build_array(
                jsonb_build_object(
                    '__typename', 'YourType',
                    'id', entity_id,
                    'operation', 'CREATED',  -- or 'UPDATED', 'DELETED'
                    'entity', (SELECT data FROM v_your_type WHERE id = entity_id)
                )
            ),
            'deleted', '[]'::jsonb,
            'invalidations', jsonb_build_array(...),
            'metadata', jsonb_build_object(
                'timestamp', now(),
                'affectedCount', 1
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

## Cascade Structure

The `_cascade` object includes:

- `updated`: Array of created/updated entities
- `deleted`: Array of deleted entity IDs
- `invalidations`: Query invalidation hints
- `metadata`: Timestamp and count
```

---

### Phase 4: Helper Functions (Optional, 1 day)

Create PostgreSQL helper functions to make cascade building easier:

```sql
-- Helper: Build cascade entity
CREATE OR REPLACE FUNCTION app.cascade_entity(
    p_typename TEXT,
    p_id UUID,
    p_operation TEXT,  -- 'CREATED', 'UPDATED', 'DELETED'
    p_view_name TEXT
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        '__typename', p_typename,
        'id', p_id,
        'operation', p_operation,
        'entity', (
            EXECUTE format('SELECT data FROM %I WHERE id = $1', p_view_name)
            USING p_id
        )
    );
END;
$$ LANGUAGE plpgsql;

-- Usage in mutations:
-- app.cascade_entity('Post', v_post_id, 'CREATED', 'v_post')
```

---

### Phase 5: Example & Tests (1 day)

Create complete example application showing:
- PostgreSQL function with cascade
- FraiseQL mutation decorator
- Client-side cache updates

---

## Benefits of Simplified Approach

### 1. **PostgreSQL-Native**
Cascade data is built where the business logic lives - no Python tracking overhead.

### 2. **Zero Abstraction**
No complex tracker classes, no context variables, no deduplication logic.

### 3. **Flexible**
PostgreSQL functions decide **exactly** what to include in cascade - full control.

### 4. **Backward Compatible**
Mutations without `_cascade` work unchanged. It's **purely additive**.

### 5. **Performance**
Zero Python overhead. Cascade construction happens in PostgreSQL with native JSONB functions.

### 6. **Testable**
Test PostgreSQL functions directly. No complex Python mocking needed.

---

## Migration from Original Plan

If you've already started implementing the original plan:

### What to Keep:
- PostgreSQL convention for `_cascade` field ✅
- Documentation structure ✅
- Client integration guides ✅

### What to Remove:
- `CascadeTracker` class ❌
- `CascadeBuilder` class ❌
- Context variable management ❌
- Deduplication logic ❌
- 7 new GraphQL types ❌
- Manual tracking API ❌

### What to Simplify:
- Mutation resolver: Just check for `_cascade` in result
- Decorator: Add `enable_cascade` parameter
- Response: Pass through `_cascade` field

---

## Example: Complete Flow

### 1. PostgreSQL Function
```sql
CREATE OR REPLACE FUNCTION fn_create_post(input jsonb)
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

    -- Update author
    UPDATE tb_user SET post_count = post_count + 1 WHERE id = v_author_id;

    -- Return with cascade
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
                jsonb_build_object('queryName', 'posts', 'strategy', 'INVALIDATE', 'scope', 'PREFIX')
            ),
            'metadata', jsonb_build_object('timestamp', now(), 'affectedCount', 2)
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### 2. FraiseQL Mutation
```python
@mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    failure: CreatePostError
```

### 3. GraphQL Response
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

### 4. Client Integration (Apollo)
```typescript
const result = await client.mutate({ mutation: CREATE_POST, variables: input });
const cascade = result.data.createPost.cascade;

// Apply cascade updates to cache
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
```

---

## Timeline: Simplified Implementation

| Phase | Time | Description |
|-------|------|-------------|
| Phase 1 | 1 day | Add cascade passthrough in resolver |
| Phase 2 | 1 day | Update response formatting |
| Phase 3 | 1 day | Document PostgreSQL pattern |
| Phase 4 | 1 day | (Optional) Helper functions |
| Phase 5 | 1 day | Examples and tests |
| **Total** | **3-5 days** | vs. **4 weeks** in original plan |

---

## Conclusion

By **leveraging the existing PostgreSQL response structure** that SpecQL uses (and FraiseQL can adopt), we can implement GraphQL Cascade in **3-5 days** instead of 4 weeks.

**Key Advantages**:
- ✅ **90% less code** (50 lines vs 2000+ lines)
- ✅ **PostgreSQL-native** (no Python tracking overhead)
- ✅ **100% flexible** (PostgreSQL decides what to cascade)
- ✅ **Backward compatible** (opt-in, additive)
- ✅ **Zero performance overhead** (native JSONB operations)

The simplified approach is:
- **Easier to implement**
- **Easier to test**
- **Easier to maintain**
- **More performant**
- **More flexible**

**Recommendation**: Use this simplified approach instead of the original 10-phase plan.

---

**Document Version**: 2.0 (Simplified)
**Last Updated**: 2025-11-11
**Status**: Ready for Implementation
