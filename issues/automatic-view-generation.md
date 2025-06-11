# Automatic View Generation for FraiseQL

## Overview

Automate the generation of PostgreSQL views (regular, materialized, and projected) from FraiseQL type definitions, while keeping table DDLs manual for maximum flexibility and framework simplicity.

## Core Principle

Developers write table DDLs manually (maintaining full control over storage optimization), while FraiseQL automatically generates the view layer that bridges storage with GraphQL queries.

## View Generation Strategy

### 1. Basic View Generation

```python
@fraise_type
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime
    posts: List['Post'] = fraise_field(description="User's posts")

# Automatically generates:
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
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', p.id,
                    'title', p.data->>'title',
                    'content', p.data->>'content'
                ) ORDER BY p.created_at DESC
            ), '[]'::jsonb)
            FROM tb_posts p
            WHERE p.data->>'user_id' = u.id::text
        )
    ) AS data
FROM tb_users u;
```

### 2. View Strategies via Decorators

```python
@fraise_type
@view_config(
    type="regular",  # regular | materialized | projected
    name="v_users_full",  # custom view name
    include_relations=["posts", "comments"],  # explicit relation inclusion
    exclude_relations=["audit_logs"],  # exclude heavy relations
)
class User:
    id: UUID
    email: str
    name: str
    posts: List['Post']
    comments: List['Comment']
    audit_logs: List['AuditLog']  # Excluded from default view
```

### 3. Multiple View Generation

```python
@fraise_type
@generate_views([
    ViewSpec(
        name="v_users_list",
        type="regular",
        fields=["id", "email", "name"],
        include_counts=["posts", "comments"]  # Just counts, not full data
    ),
    ViewSpec(
        name="v_users_detail",
        type="regular",
        include_relations=["posts", "profile", "settings"]
    ),
    ViewSpec(
        name="mv_users_stats",
        type="materialized",
        fields=["id", "email", "post_count", "last_active"],
        refresh_interval="1 hour"
    )
])
class User:
    # One type, multiple optimized views
    pass
```

### 4. Projected Tables (tv_) Generation

```python
@fraise_type
@projection(
    table="tv_user_activity",
    source_events=["UserCreated", "UserUpdated", "PostCreated"],
    update_strategy="append"  # append | upsert | replace
)
class UserActivity:
    user_id: UUID
    email: str
    total_posts: int = fraise_field(
        compute="COUNT(*) FROM tb_posts WHERE data->>'user_id' = user_id::text"
    )
    last_post_date: Optional[datetime]
    activity_score: float

# Generates projected table and update triggers/functions
```

### 5. Smart Relation Detection

```python
class ViewGenerator:
    """Automatically detect and generate relation subqueries"""

    def generate_relation_subquery(self,
        parent_type: Type,
        field_name: str,
        field_type: Type
    ) -> str:
        # Detect relation type
        if is_list_type(field_type):
            # One-to-many: generate jsonb_agg subquery
            return self._generate_one_to_many(parent_type, field_name, field_type)
        else:
            # One-to-one: generate single object subquery
            return self._generate_one_to_one(parent_type, field_name, field_type)

    def _infer_foreign_key(self, parent_type: Type, child_type: Type) -> str:
        """
        Smart foreign key inference:
        1. Look for explicit back_populates
        2. Check for {parent}_id pattern
        3. Use naming conventions
        """
        # Default: tb_children.data->>'parent_id' = parent.id::text
        pass
```

### 6. Conditional Field Inclusion

```python
@fraise_type
class Post:
    id: UUID
    title: str
    content: str = fraise_field(
        # Content only included for authenticated users
        view_condition="current_setting('app.user_id', true) IS NOT NULL"
    )
    draft_notes: str = fraise_field(
        # Only visible to post author
        view_condition="data->>'author_id' = current_setting('app.user_id', true)"
    )

# Generates views with conditional fields
CREATE VIEW v_posts AS
SELECT id, jsonb_build_object(
    'id', id,
    'title', data->>'title',
    'content', CASE
        WHEN current_setting('app.user_id', true) IS NOT NULL
        THEN data->>'content'
        ELSE NULL
    END,
    'draftNotes', CASE
        WHEN data->>'author_id' = current_setting('app.user_id', true)
        THEN data->>'draft_notes'
        ELSE NULL
    END
) AS data FROM tb_posts;
```

### 7. Materialized View Management

```python
@fraise_type
@materialized_view(
    refresh_strategy="concurrent",  # concurrent | blocking
    indexes=["user_id", "created_at"],
    refresh_on_commit=False,  # For development
    depends_on=["tb_users", "tb_posts"]  # Track dependencies
)
class UserPostStats:
    user_id: UUID
    total_posts: int
    avg_post_length: float
    most_recent_post: datetime

# Generates:
# 1. Materialized view with aggregations
# 2. Indexes for performance
# 3. Refresh function
# 4. Optional triggers for auto-refresh
```

### 8. View Generation CLI

```bash
# Generate all views from type definitions
fraiseql-views generate

# Generate specific view
fraiseql-views generate --type User --strategy detail

# Preview SQL without executing
fraiseql-views preview --type User

# Refresh materialized views
fraiseql-views refresh --materialized

# Analyze view performance
fraiseql-views analyze --view v_users_full

# Generate migration for view changes
fraiseql-views migrate --from-version 1.0.0 --to-version 1.1.0
```

### 9. Performance Optimizations

```python
@fraise_type
@view_optimizations(
    # Limit collections to prevent huge responses
    collection_limits={"posts": 100, "comments": 50},

    # Use EXISTS instead of COUNT for booleans
    boolean_aggregates={"has_posts": "EXISTS(SELECT 1 FROM tb_posts WHERE ...)"},

    # Parallel-safe for read replicas
    parallel_safe=True
)
class User:
    posts: List['Post'] = fraise_field(
        description="Recent posts",
        view_limit=100,
        view_order="created_at DESC"
    )
```

### 10. View Inheritance and Composition

```python
# Base view configuration
@view_interface
class Timestamped:
    created_at: datetime
    updated_at: datetime

@view_interface
class Authorable:
    author: 'User' = fraise_field(
        source="(SELECT jsonb_build_object(...) FROM tb_users WHERE id = $.data->>'author_id')"
    )

# Composed view
@fraise_type
@implements(Timestamped, Authorable)
class Post:
    id: UUID
    title: str
    content: str
    # Inherits created_at, updated_at, author field definitions
```

## Implementation Details

### View Naming Conventions

- `v_<type>` - Default query view
- `v_<type>_list` - Optimized for lists (minimal fields)
- `v_<type>_detail` - Full detail view with all relations
- `mv_<type>_<purpose>` - Materialized views
- `tv_<type>_<projection>` - Projected tables

### Automatic Index Suggestions

```python
class ViewAnalyzer:
    """Suggest indexes based on view definitions"""

    def suggest_indexes(self, view_sql: str) -> List[str]:
        # Analyze JOIN conditions
        # Find WHERE clauses in subqueries
        # Suggest GIN indexes for JSONB operations
        # Return CREATE INDEX statements
        pass
```

### Migration Support

```python
class ViewMigrator:
    """Handle view evolution"""

    def generate_migration(self, old_type: Type, new_type: Type) -> str:
        """
        1. CREATE OR REPLACE for compatible changes
        2. DROP and CREATE for breaking changes
        3. Handle materialized view refresh
        4. Update dependent views
        """
        pass
```

## Benefits

1. **Thin Framework**: No magic in storage layer
2. **Automation Where It Counts**: Views are repetitive and error-prone
3. **Flexibility**: Multiple view strategies per type
4. **Performance**: Optimized views for different use cases
5. **Maintainability**: Views generated from single source of truth

## Example Workflow

```python
# 1. Developer writes table DDL (full control)
CREATE TABLE tb_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_users_email ON tb_users((data->>'email'));

# 2. Developer defines GraphQL type
@fraise_type
@generate_views([
    ViewSpec("v_users", type="regular"),
    ViewSpec("v_users_list", fields=["id", "email", "name"]),
    ViewSpec("mv_users_with_stats", type="materialized")
])
class User:
    id: UUID
    email: str
    name: str
    posts: List['Post']

# 3. Framework generates all views automatically
fraiseql-views generate

# 4. Developer deploys and uses
```

This approach keeps FraiseQL thin while automating the most tedious and error-prone part of the CQRS read side!
