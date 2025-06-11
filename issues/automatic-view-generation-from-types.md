# Automatic View Generation from FraiseQL Types

## Summary

Implement automatic generation of PostgreSQL views from FraiseQL type definitions while keeping table DDLs manual. This maintains framework simplicity while automating the most repetitive and error-prone aspects of CQRS read models.

## Motivation

Writing PostgreSQL views with complex JSONB aggregations is:
- Repetitive and error-prone
- Difficult to maintain as types evolve
- Following predictable patterns that can be automated

Meanwhile, table DDLs should remain manual because:
- Storage optimization is project-specific
- Write-side concerns vary greatly between applications
- Developers need full control over indexes, partitioning, and constraints

## Proposed Solution

### Basic Usage

```python
# Developer writes table DDL manually
"""
CREATE TABLE tb_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tb_posts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
"""

# Developer defines FraiseQL types
@fraise_type
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID
    created_at: datetime

@fraise_type
class User:
    id: UUID
    email: str
    name: str
    posts: List['Post'] = fraise_field(description="User's posts")
    created_at: datetime

# Framework generates COMPOSED views automatically
"""
-- First, generate the child view
CREATE OR REPLACE VIEW v_posts AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', id,
        'title', data->>'title',
        'content', data->>'content',
        'authorId', data->>'author_id',
        'createdAt', created_at
    ) AS data,
    -- Include foreign key for efficient joins
    data->>'author_id' AS author_id
FROM tb_posts;

-- Then, compose the parent view using the child view
CREATE OR REPLACE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'User',
        'id', id,
        'email', data->>'email',
        'name', data->>'name',
        'createdAt', created_at,
        'posts', (
            SELECT COALESCE(jsonb_agg(
                p.data ORDER BY p.data->>'createdAt' DESC
            ), '[]'::jsonb)
            FROM v_posts p  -- Use the view, not the table!
            WHERE p.author_id = u.id::text
        )
    ) AS data
FROM tb_users u;
"""
```

### Multiple Views per Type

```python
@fraise_type
@generate_views([
    # Minimal view for list queries
    ViewSpec(
        name="v_users_list",
        fields=["id", "email", "name"],
        include_counts=["posts"]  # COUNT instead of full data
    ),
    # Full view for detail queries
    ViewSpec(
        name="v_users_detail",
        include_relations=["posts", "comments", "profile"]
    ),
    # Materialized view for stats
    ViewSpec(
        name="mv_users_stats",
        type="materialized",
        fields=["id", "post_count", "comment_count", "last_active"]
    )
])
class User:
    id: UUID
    email: str
    name: str
    posts: List['Post']
    comments: List['Comment']
    profile: Optional['UserProfile']
```

### Relation Inference

```python
@fraise_type
class Post:
    id: UUID
    title: str
    # Framework infers: tb_users.id = tb_posts.data->>'user_id'
    author: User = fraise_field(foreign_key="user_id")

    # Or explicit relation
    comments: List['Comment'] = fraise_field(
        source="tb_comments",
        join_condition="data->>'post_id' = posts.id::text"
    )
```

### CLI Commands

```bash
# Generate all views
fraiseql-views generate

# Preview SQL without executing
fraiseql-views preview --type User

# Generate specific view strategy
fraiseql-views generate --type User --strategy list

# Update views after type changes
fraiseql-views update

# Generate migration for view changes
fraiseql-views diff > migrations/002_update_user_views.sql
```

## Technical Implementation

### 1. View Generator Core

```python
class ViewGenerator:
    def generate_view(self, type_cls: Type[T], spec: ViewSpec) -> str:
        """Generate SQL for a view from a FraiseQL type"""
        # Always generates self-contained views with foreign keys exposed

    def generate_view_with_dependencies(self, schema: Dict[str, Type]) -> List[str]:
        """Generate all views in dependency order"""
        # 1. Build dependency graph
        # 2. Topological sort
        # 3. Generate child views before parent views

    def generate_composed_relation(self, parent: Type, child: Type, field: Field) -> str:
        """Generate jsonb_agg from child view, not table"""
        # Always: FROM v_children, never FROM tb_children

    def expose_foreign_keys(self, type_cls: Type[T]) -> List[str]:
        """Expose FKs as top-level columns for efficient joins"""
        # data->>'user_id' AS user_id
```

### 2. View Composition Principles

```python
# WRONG - Direct table access
"""
'posts', (
    SELECT jsonb_agg(jsonb_build_object(...))
    FROM tb_posts WHERE ...
)
"""

# CORRECT - View composition
"""
'posts', (
    SELECT jsonb_agg(p.data)
    FROM v_posts p WHERE p.user_id = u.id::text
)
"""
```

### 2. View Specifications

```python
@dataclass
class ViewSpec:
    name: str
    type: Literal["regular", "materialized"] = "regular"
    fields: Optional[List[str]] = None  # None means all fields
    include_relations: Optional[List[str]] = None
    exclude_relations: Optional[List[str]] = None
    include_counts: Optional[List[str]] = None  # Relations to count only
    where_clause: Optional[str] = None  # Filter rows
    order_by: Optional[str] = None
    limit: Optional[int] = None
```

### 3. Smart Defaults

- View name: `v_{snake_case(type_name)}`
- Table name: `tb_{snake_case(type_name)}s`
- Foreign keys: `{parent_type}_id` in child's JSONB
- Relations: Include by default unless marked with `view_exclude=True`
- **Composition Rule**: Always aggregate from child views (`v_*`), not tables (`tb_*`)

### 4. View Dependency Management

```python
class ViewDependencyResolver:
    """Ensure views are created in correct order"""

    def resolve_dependencies(self, types: List[Type]) -> List[Type]:
        """
        Order types so that:
        1. Leaf types (no relations) are created first
        2. Parent types are created after their children
        3. Circular dependencies are detected and reported
        """
        graph = self.build_dependency_graph(types)
        return self.topological_sort(graph)
```

### 4. Performance Optimizations

```python
@fraise_type
class User:
    # Limit collection size in views
    posts: List['Post'] = fraise_field(
        view_limit=100,
        view_order="created_at DESC"
    )

    # Exclude from default views
    audit_logs: List['AuditLog'] = fraise_field(
        view_exclude=True
    )

    # Conditional inclusion
    private_notes: str = fraise_field(
        view_condition="current_user_role() = 'admin'"
    )
```

## Migration Strategy

### Phase 1: Basic View Generation
- Single view per type
- Simple relation mapping
- CLI command for generation

### Phase 2: Multiple View Strategies
- List/detail/stats views
- Materialized view support
- Performance optimizations

### Phase 3: Advanced Features
- View inheritance
- Conditional fields
- Custom SQL expressions
- Migration tracking

## Benefits

1. **Eliminates Boilerplate**: No more hand-writing complex JSONB queries
2. **Type Safety**: Views always match Python types
3. **Performance**: Generate optimized views for different use cases
4. **Maintainability**: Single source of truth for schema
5. **Flexibility**: Manual table control, automatic view generation
6. **Composability**: Views build on other views, creating clean layers
7. **Reusability**: Child views can be queried independently

## Non-Goals

- **Not** generating table DDLs (remains manual)
- **Not** managing table migrations (use existing tools)
- **Not** enforcing specific storage patterns
- **Not** hiding PostgreSQL complexity (views remain inspectable)

## Success Criteria

1. 80% reduction in hand-written view SQL
2. Zero runtime overhead (views are plain PostgreSQL)
3. Easy to override when needed
4. Clear, readable generated SQL
5. Seamless integration with existing FraiseQL types

## View Composition Example

```sql
-- Level 0: Leaf views (no dependencies)
CREATE VIEW v_comments AS
SELECT id,
       jsonb_build_object(
           'id', id,
           'content', data->>'content',
           'authorId', data->>'author_id',
           'postId', data->>'post_id'
       ) AS data,
       data->>'post_id' AS post_id,
       data->>'author_id' AS author_id
FROM tb_comments;

-- Level 1: Views that depend on leaf views
CREATE VIEW v_posts AS
SELECT id,
       jsonb_build_object(
           'id', id,
           'title', data->>'title',
           'content', data->>'content',
           'authorId', data->>'author_id',
           'comments', (
               SELECT jsonb_agg(c.data)
               FROM v_comments c  -- Compose from view!
               WHERE c.post_id = p.id::text
           )
       ) AS data,
       data->>'author_id' AS author_id
FROM tb_posts p;

-- Level 2: Views that depend on level 1 views
CREATE VIEW v_users AS
SELECT id,
       jsonb_build_object(
           'id', id,
           'name', data->>'name',
           'posts', (
               SELECT jsonb_agg(p.data)
               FROM v_posts p  -- Compose from view!
               WHERE p.author_id = u.id::text
           )
       ) AS data
FROM tb_users u;
```

## Open Questions

1. How to handle circular dependencies between types?
2. How to handle polymorphic relations?
3. Should materialized view refresh be automated?
4. How to version generated views?
5. Integration with existing migration tools?
6. Should we expose all foreign keys or only used ones?
7. How to optimize view queries (indexes on view FKs)?

## References

- Current manual view examples: `examples/blog_api/db/views/`
- CQRS pattern: `docs/advanced/domain-driven-database.md`
- View composition: `issues/view-composition-architecture-enhancements.md`
