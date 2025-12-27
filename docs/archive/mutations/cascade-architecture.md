# FraiseQL CASCADE Architecture

This document provides a comprehensive overview of the CASCADE feature in FraiseQL, including its architecture, data flow, and implementation details.

## Overview

GraphQL CASCADE is a FraiseQL feature that enables automatic client cache updates after mutations. When `enable_cascade=True` is set on a mutation, the server returns additional metadata about entities that were affected by the mutation, allowing clients to update their caches without additional queries.

## Selection-Aware Behavior

**Important**: CASCADE data is only included in GraphQL responses when explicitly requested in the selection set. This follows GraphQL's fundamental principle that clients should only receive the data they request.

### Selection Filtering

**No CASCADE Requested**:
```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    ... on CreatePostSuccess {
      id
      message
      post { id title }
      # cascade NOT requested
    }
  }
}
```
**Response**: No `cascade` field in response (smaller payload)

**Full CASCADE Requested**:
```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    ... on CreatePostSuccess {
      id
      message
      cascade {
        updated { __typename id operation entity }
        deleted { __typename id }
        invalidations { queryName strategy scope }
        metadata { timestamp affectedCount }
      }
    }
  }
}
```
**Response**: Complete CASCADE data included

**Partial CASCADE Requested**:
```graphql
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    ... on CreatePostSuccess {
      id
      message
      cascade {
        metadata { affectedCount }
        # Only metadata requested
      }
    }
  }
}
```
**Response**: Only `metadata` field in CASCADE object

### Performance Benefits

Not requesting CASCADE can reduce response payload size by 2-10x for typical mutations:

- Simple mutation without CASCADE: ~200-500 bytes
- Same mutation with full CASCADE: ~1,500-5,000 bytes

Clients should only request CASCADE when they need the side effect information for cache updates or UI synchronization.

## Architecture Overview

### Three-Layer Data Flow

The CASCADE implementation uses a three-layer architecture:

```
┌─────────────────────────────────────────────────────────┐
│ Layer 1: PostgreSQL Response Normalization (Python)     │
│ ─────────────────────────────────────────────────────── │
│ Input:  mutation_response tuple from PostgreSQL         │
│ Output: Normalized dict with explicit field mapping     │
│ Responsibility: Convert various formats to canonical     │
└─────────────────────────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────┐
│ Layer 2: GraphQL Transformation (Rust)                  │
│ ─────────────────────────────────────────────────────── │
│ Input:  Normalized dict + Success type schema           │
│ Output: GraphQL-compliant JSON (RustResponseBytes)      │
│ Responsibility: Apply GraphQL conventions, build JSON   │
└─────────────────────────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────┐
│ Layer 3: Object Instantiation (Python)                  │
│ ─────────────────────────────────────────────────────── │
│ Input:  GraphQL JSON response                           │
│ Output: Typed Success/Error objects with __cascade__    │
│ Responsibility: Parse JSON, instantiate types           │
└─────────────────────────────────────────────────────────┘
```

### Key Components

#### 1. Rust Mutation Pipeline (`fraiseql_rs/src/mutation/`)

**Purpose**: Unified Rust pipeline that processes PostgreSQL mutation responses into GraphQL-compatible format.

**Key Components**:
- `parser.rs` - Parses JSON responses with format auto-detection
- `entity_processor.rs` - Handles entity extraction, __typename injection, and CASCADE processing
- `response_builder.rs` - Builds GraphQL-compliant responses

**Architecture**: Single 2-layer pipeline (PostgreSQL → Rust → JSON) replacing the previous 5-layer Python/Rust architecture.

#### 2. Mutation Decorator (`src/fraiseql/mutations/mutation_decorator.py`)

**Purpose**: Orchestrates mutation execution and result parsing.

**Key Features**:
- Derives entity field names from Success type annotations
- Passes schema information to Rust transformer
- Attaches CASCADE metadata to Success objects

#### 3. Rust Transformer (`fraiseql-rs/src/mutations.rs`)

**Purpose**: Applies GraphQL conventions and builds JSON responses.

**Key Features**:
- Validates field mapping against Success type schema
- Applies camelCase conversion
- Ensures all expected fields are present in response

#### 4. CASCADE Type System (`src/fraiseql/mutations/cascade_types.py`)

**Purpose**: Adds CASCADE field to GraphQL success types.

**Implementation**: Uses GraphQL field resolvers to dynamically add the `cascade` field to Success types when `enable_cascade=True`.

#### 5. JSON Encoder (`src/fraiseql/fastapi/json_encoder.py`)

**Purpose**: Serializes FraiseQL objects to JSON for HTTP responses.

**CASCADE Handling**: Renames `__cascade__` attribute to `cascade` field in JSON output.

## Data Flow Details

### PostgreSQL Function Response

Database functions return a `mutation_response` tuple with this structure:

```sql
-- Example PostgreSQL function return
RETURN ROW(
    'created',                          -- status: TEXT
    'Post created successfully',        -- message: TEXT
    v_post_id,                          -- entity_id: TEXT
    'Post',                             -- entity_type: TEXT (class name)
    jsonb_build_object(                 -- entity: JSONB
        'post', jsonb_build_object(...), -- nested entity data
        'message', 'Success'
    ),
    NULL,                               -- updated_fields: TEXT[]
    v_cascade_data,                     -- cascade: JSONB
    NULL                                -- metadata: JSONB
)::mutation_response;
```

### Python Normalization Layer

The entity flattener processes the PostgreSQL response:

1. **Case-Insensitive Matching**: Matches `entity_type` ("Post") with Success field names case-insensitively
2. **Field Validation**: Ensures all expected Success type fields are present
3. **Internal Field Removal**: Removes `entity_id`, `entity_type`, etc. from GraphQL response
4. **Flattening**: Converts nested entity structure to flat field structure

**Before Fix** (Broken):
```python
# entity_type = "Post" (from DB)
# expected_fields = {"post", "message", "cascade"}
# "Post" not in {"post", ...} → False → Wrong path taken
```

**After Fix** (Working):
```python
# Case-insensitive check: "post".lower() == "Post".lower()
# Match found → Skip flattening → Pass to Rust
```

### Rust Transformation Layer

The Rust transformer:
1. Receives flattened data and Success type schema
2. Validates all expected fields are present
3. Applies GraphQL conventions (camelCase, __typename)
4. Builds final JSON response

### Python Object Instantiation

Final layer:
1. Parses GraphQL JSON response
2. Instantiates typed Success/Error objects
3. Attaches `__cascade__` attribute for CASCADE metadata
4. Returns objects for JSON encoding

## CASCADE Data Structure

### Cascade Metadata Format

```json
{
  "updated": [
    {
      "__typename": "Post",
      "id": "post-123",
      "operation": "CREATED",
      "entity": {
        "id": "post-123",
        "title": "Test Post",
        "content": "Content",
        "author_id": "user-456"
      }
    },
    {
      "__typename": "User",
      "id": "user-456",
      "operation": "UPDATED",
      "entity": {
        "id": "user-456",
        "name": "Author",
        "post_count": 5
      }
    }
  ],
  "deleted": [
    {
      "__typename": "Comment",
      "id": "comment-789"
    }
  ],
  "invalidations": [
    {
      "queryName": "posts",
      "strategy": "INVALIDATE",
      "scope": "PREFIX"
    },
    {
      "queryName": "userPosts",
      "strategy": "INVALIDATE",
      "scope": "EXACT"
    }
  ],
  "metadata": {
    "timestamp": "2025-12-05T10:30:00Z",
    "affectedCount": 2,
    "depth": 1,
    "transactionId": "123456789"
  }
}
```

### Operations

- **CREATED**: New entity added to the system
- **UPDATED**: Existing entity modified
- **DELETED**: Entity removed

### Invalidation Strategies

- **INVALIDATE**: Mark queries for invalidation
- **Scopes**: PREFIX (matches query names starting with), EXACT (exact match), SUFFIX (ending with)

## Success Type Definition

### Basic Structure

```python
@fraiseql.type
class CreatePostSuccess:
    post: Post           # Entity field (matches entity_type case-insensitively)
    message: str         # Standard field
    cascade: Cascade     # CASCADE metadata (added automatically when enable_cascade=True)
```

### Field Mapping Rules

1. **Entity Fields**: Fields with GraphQL types (Post, User, etc.) are treated as entity fields
2. **Standard Fields**: Primitive types (str, int, bool) are standard fields
3. **CASCADE Field**: Automatically added when `enable_cascade=True`

## Mutation Decorator

### Configuration

```python
@fraiseql.mutation(
    function="create_post",
    schema="app",
    enable_cascade=True,  # Enables CASCADE feature
)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess  # Must have explicit fields
    failure: CreatePostError
```

### Key Parameters

- `enable_cascade`: Enables CASCADE metadata attachment
- `function`: PostgreSQL function name
- `schema`: Database schema name

## Database Function Requirements

### Entity Views

Create views for CASCADE entity data:

```sql
CREATE VIEW v_post AS
SELECT id, jsonb_build_object(
    'id', id,
    'title', title,
    'content', content,
    'author_id', author_id,
    'created_at', created_at,
    'updated_at', updated_at
) as data FROM tb_post;

CREATE VIEW v_user AS
SELECT id, jsonb_build_object(
    'id', id,
    'name', name,
    'email', email,
    'post_count', post_count
) as data FROM tb_user;
```

### Helper Functions

```sql
-- Entity cascade entry
CREATE OR REPLACE FUNCTION app.cascade_entity(
    entity_type text,
    entity_id uuid,
    operation text,
    view_name text
) RETURNS jsonb;

-- Invalidation entry
CREATE OR REPLACE FUNCTION app.cascade_invalidation(
    query_name text,
    strategy text,
    scope text
) RETURNS jsonb;

-- Build complete cascade object
CREATE OR REPLACE FUNCTION app.build_cascade(
    updated_entities jsonb DEFAULT '[]'::jsonb,
    deleted_entities jsonb DEFAULT '[]'::jsonb,
    invalidations jsonb DEFAULT '[]'::jsonb,
    metadata jsonb DEFAULT NULL
) RETURNS jsonb;
```

### Function Structure

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

    -- Return with cascade
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object(
            'id', v_post_id,
            'message', 'Post created successfully'
        ),
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

## Client Integration

### Apollo Client

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

// Automatic cache updates - no manual intervention needed
const [createPost] = useMutation(CREATE_POST);
```

### Manual Cache Updates (Advanced)

```typescript
function applyCascadeToCache(cache: ApolloCache, cascade: CascadeData) {
  // Apply entity updates
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
    // Implementation depends on invalidation strategy
  });
}
```

## Performance Characteristics

### Benchmarks

Based on `benchmarks/cascade_performance_benchmark.py`:

| Operation | Time (μs) | Notes |
|-----------|-----------|-------|
| Entity flattening (small) | ~150 | 10 fields |
| Entity flattening (large) | ~800 | 100 fields |
| Rust transformation | ~500 | Includes JSON parsing |
| Python parsing | ~200 | Object instantiation |
| CASCADE attachment | ~50 | Attribute assignment |
| **Total (with CASCADE)** | **~1700** | End-to-end |
| Total (without CASCADE) | ~1650 | Negligible overhead |

### Optimization Guidelines

- **Payload Size**: Keep CASCADE data under 50KB
- **Entity Count**: Limit to directly affected entities only
- **Validation**: Enable in development, optional in production
- **Caching**: Entity views should be indexed for performance

## Error Handling

### Validation Layers

1. **Python Layer**: Field presence validation, case-insensitive matching
2. **Rust Layer**: Schema validation, field mapping verification
3. **GraphQL Layer**: Type checking, resolver validation

### Common Issues

#### Missing Entity Fields
**Symptoms**: Entity fields not appearing in GraphQL response
**Causes**:
- Case sensitivity mismatch in entity type matching
- Missing fields in PostgreSQL entity structure
- Incorrect Success type field names

**Debugging**:
```python
# Check entity flattener logs
logger.debug(f"Entity type '{entity_type}' matched field '{field_name}'")

# Validate PostgreSQL response structure
SELECT jsonb_object_keys(entity) FROM graphql.function_call();
```

#### CASCADE Not Attached
**Symptoms**: `cascade` field is null in GraphQL response
**Causes**:
- `enable_cascade=False` on mutation
- Missing `_cascade` in PostgreSQL function return
- CASCADE attachment failure in mutation decorator

**Debugging**:
```python
# Check mutation decorator
assert mutation.enable_cascade is True

# Validate PostgreSQL function
SELECT _cascade FROM graphql.function_call();
```

## Migration Guide

See `docs/mutations/migration-guide.md` for detailed migration instructions.

## Testing

### Unit Tests

```python
def test_entity_type_case_insensitive_matching():
    """Test case-insensitive entity type matching."""
    mutation_result = {
        "entity_type": "Post",  # Uppercase from DB
        "entity": {"post": {...}, "message": "..."}
    }

    class TestSuccess:
        post: Post      # Lowercase field name
        message: str

    result = flatten_entity_wrapper(mutation_result, TestSuccess)
    assert "entity" in result  # Should skip flattening
```

### Integration Tests

```python
async def test_cascade_end_to_end():
    """Test complete CASCADE flow."""
    # Execute mutation with enable_cascade=True
    response = await client.execute(CREATE_POST_MUTATION)

    # Verify cascade data structure
    assert response.data.createPost.cascade is not None
    assert len(response.data.createPost.cascade.updated) > 0

    # Verify entity fields present
    assert response.data.createPost.post is not None
    assert response.data.createPost.post.id is not None
```

## Monitoring

### Key Metrics

```python
cascade_processing_duration = Histogram(
    'fraiseql_cascade_processing_duration_seconds',
    'Time spent processing cascade data'
)

cascade_payload_bytes = Histogram(
    'fraiseql_cascade_payload_bytes',
    'Size of cascade payloads in bytes'
)

cascade_errors_total = Counter(
    'fraiseql_cascade_errors_total',
    'Total cascade processing errors',
    ['error_type']
)
```

### Alerting

```yaml
- alert: HighCascadeProcessingTime
  expr: histogram_quantile(0.95, rate(fraiseql_cascade_processing_duration_seconds_bucket[5m])) > 0.1
  labels:
    severity: warning

- alert: LargeCascadePayloads
  expr: histogram_quantile(0.95, fraiseql_cascade_payload_bytes) > 50000
  for: 5m
  labels:
    severity: warning
```

## Best Practices

### Database Design
- Use consistent entity view naming (`v_entity_name`)
- Include all fields clients typically need
- Index views for performance
- Validate view data completeness

### Application Architecture
- Start with simple mutations (create operations)
- Use feature flags for gradual rollout
- Implement comprehensive error handling
- Monitor performance and payload sizes

### Client Integration
- Leverage automatic cache updates when possible
- Implement manual updates for complex scenarios
- Handle CASCADE processing errors gracefully
- Test thoroughly with real data

## Troubleshooting

See `docs/guides/troubleshooting.md` for detailed troubleshooting guides.

## Related Documentation

- `docs/guides/cascade-best-practices.md` - Usage best practices
- `docs/mutations/migration-guide.md` - Migration instructions
- `examples/graphql-cascade/` - Working examples
- `docs/guides/troubleshooting.md` - Troubleshooting guide
