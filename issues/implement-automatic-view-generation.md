# Implement Automatic View Generation

## Summary

Create a system that automatically generates PostgreSQL views from FraiseQL type definitions using view composition patterns. Views should be layered, with complex views aggregating data from simpler views rather than directly from tables.

## Background

Currently, developers must manually write PostgreSQL views with complex JSONB aggregations. This is error-prone and repetitive. Since views follow predictable patterns based on type definitions, they can be automated while keeping table DDLs manual for flexibility.

## Requirements

### 1. Core View Generation

Generate a view for each `@fraise_type` decorated class:

```python
@fraise_type
class Comment:
    id: UUID
    content: str
    author_id: UUID
    post_id: UUID
    created_at: datetime

# Should generate:
CREATE OR REPLACE VIEW v_comments AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Comment',
        'id', id,
        'content', data->>'content',
        'authorId', data->>'author_id',
        'postId', data->>'post_id',
        'createdAt', created_at
    ) AS data,
    -- Expose foreign keys for efficient joins
    data->>'author_id' AS author_id,
    data->>'post_id' AS post_id
FROM tb_comments;
```

### 2. View Composition for Relations

When types have relations, compose from child views:

```python
@fraise_type
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID
    comments: List[Comment]  # Relation to Comment type

# Should generate:
CREATE OR REPLACE VIEW v_posts AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', id,
        'title', data->>'title',
        'content', data->>'content',
        'authorId', data->>'author_id',
        'comments', (
            SELECT COALESCE(jsonb_agg(
                c.data ORDER BY c.data->>'createdAt' DESC
            ), '[]'::jsonb)
            FROM v_comments c  -- FROM view, not table!
            WHERE c.post_id = p.id::text
        )
    ) AS data,
    data->>'author_id' AS author_id
FROM tb_posts p;
```

### 3. Dependency Resolution

Views must be created in dependency order:

```python
# Given types: User -> Post -> Comment
# Generate in order: v_comments, v_posts, v_users

class ViewDependencyResolver:
    def resolve_order(self, types: List[Type]) -> List[Type]:
        """Return types in order they should be created"""
        # 1. Build dependency graph
        # 2. Topological sort
        # 3. Detect circular dependencies
```

### 4. Foreign Key Inference

Automatically infer relationships:

```python
# Inference rules:
# 1. Field named {type}_id -> foreign key to that type
# 2. List[Type] -> one-to-many relationship
# 3. Optional[Type] -> nullable one-to-one

# Allow explicit override:
comments: List[Comment] = fraise_field(
    foreign_key="post_id",  # Explicit FK in Comment
    order_by="created_at DESC"
)
```

### 5. CLI Interface

```bash
# Generate all views
fraiseql-views generate

# Generate specific type and dependencies
fraiseql-views generate --type Post

# Preview without executing
fraiseql-views generate --dry-run

# Generate to file
fraiseql-views generate --output views.sql

# Show dependency graph
fraiseql-views deps --graph
```

### 6. Configuration Options

```python
@fraise_type
@view_config(
    name="v_posts_custom",  # Override default name
    exclude_fields=["internal_notes"],  # Don't expose
    limit_collections={"comments": 100}  # Prevent huge responses
)
class Post:
    pass

# Or globally in settings:
FRAISEQL_VIEW_CONFIG = {
    "default_collection_limit": 1000,
    "expose_foreign_keys": True,
    "use_camel_case": True,
    "view_prefix": "v_",
    "table_prefix": "tb_"
}
```

## Implementation Plan

### Phase 1: Basic Generation (Week 1-2)
- [ ] Create `ViewGenerator` class
- [ ] Implement basic view generation for simple types
- [ ] Add foreign key detection and exposure
- [ ] Create CLI command structure

### Phase 2: Composition (Week 3-4)
- [ ] Implement dependency resolver
- [ ] Add relation detection (List, Optional types)
- [ ] Generate composed views with jsonb_agg
- [ ] Handle ordering and limits

### Phase 3: Configuration (Week 5)
- [ ] Add @view_config decorator
- [ ] Implement field exclusion/inclusion
- [ ] Add collection limits
- [ ] Support custom SQL expressions

### Phase 4: Production Features (Week 6)
- [ ] Add --dry-run mode
- [ ] Implement view diffing
- [ ] Add migration generation
- [ ] Create comprehensive tests

## Technical Design

### Core Classes

```python
@dataclass
class ViewDefinition:
    name: str
    source_table: str
    fields: List[ViewField]
    relations: List[ViewRelation]
    exposed_keys: List[str]
    dependencies: List[str]

class ViewGenerator:
    def generate(self, type_def: Type) -> ViewDefinition:
        """Generate view definition from type"""

    def to_sql(self, view_def: ViewDefinition) -> str:
        """Convert definition to SQL"""

class ViewComposer:
    def compose_all(self, types: Dict[str, Type]) -> List[str]:
        """Generate all views in correct order"""
```

### Relation Detection

```python
def detect_relation(field_type: Type) -> RelationType:
    if is_list_type(field_type):
        return RelationType.ONE_TO_MANY
    elif is_optional_type(field_type):
        return RelationType.OPTIONAL_ONE_TO_ONE
    elif is_fraise_type(field_type):
        return RelationType.ONE_TO_ONE
    else:
        return RelationType.NONE
```

## Testing Strategy

1. **Unit Tests**: Test each component in isolation
2. **Integration Tests**: Test view generation with real PostgreSQL
3. **Snapshot Tests**: Compare generated SQL against expected output
4. **Performance Tests**: Ensure composed views perform well

## Success Metrics

1. Zero manual view writing for 90% of use cases
2. Generated views perform within 10% of hand-optimized views
3. Clear error messages for unsupported patterns
4. Easy override mechanism when needed

## Example Output

Given a blog schema with Users, Posts, and Comments:

```sql
-- Generated by fraiseql-views on 2024-01-20
-- Dependencies: None
CREATE OR REPLACE VIEW v_comments AS
SELECT id, jsonb_build_object(
    '__typename', 'Comment',
    'id', id,
    'content', data->>'content',
    'authorId', data->>'author_id',
    'postId', data->>'post_id'
) AS data,
data->>'author_id' AS author_id,
data->>'post_id' AS post_id
FROM tb_comments;

-- Dependencies: v_comments
CREATE OR REPLACE VIEW v_posts AS
SELECT id, jsonb_build_object(
    '__typename', 'Post',
    'id', id,
    'title', data->>'title',
    'authorId', data->>'author_id',
    'comments', (
        SELECT COALESCE(jsonb_agg(c.data), '[]'::jsonb)
        FROM v_comments c
        WHERE c.post_id = p.id::text
    )
) AS data,
data->>'author_id' AS author_id
FROM tb_posts p;

-- Dependencies: v_posts
CREATE OR REPLACE VIEW v_users AS
SELECT id, jsonb_build_object(
    '__typename', 'User',
    'id', id,
    'name', data->>'name',
    'email', data->>'email',
    'posts', (
        SELECT COALESCE(jsonb_agg(p.data), '[]'::jsonb)
        FROM v_posts p
        WHERE p.author_id = u.id::text
    )
) AS data
FROM tb_users u;
```

## Future Enhancements

1. **Materialized Views**: Support for `@materialized_view` decorator
2. **View Strategies**: Different views for list vs detail queries
3. **Conditional Fields**: Fields visible based on context
4. **Performance Analysis**: Suggest indexes based on view usage
5. **View Inheritance**: Share common field definitions

## References

- Design discussion: `issues/automatic-view-generation-from-types.md`
- Manual view examples: `examples/blog_api/db/views/`
- View composition principles: `issues/view-composition-architecture-enhancements.md`
