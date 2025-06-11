# View Composition Architecture: Eliminating N+1 Through Database Views

## Core Architecture Principle

FraiseQL eliminates the N+1 query problem by using PostgreSQL views that pre-assemble related entities into JSONB structures. Instead of fetching parents and then making separate queries for children, a single view returns the complete object graph.

## Current Implementation

### Basic View Pattern
```sql
-- Instead of normalized queries requiring joins
CREATE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'posts', (
            SELECT COALESCE(jsonb_agg(
                jsonb_build_object(
                    'id', p.id,
                    'title', p.title,
                    'content', p.content,
                    'comments', (
                        SELECT COALESCE(jsonb_agg(
                            jsonb_build_object(
                                'id', c.id,
                                'content', c.content,
                                'author_name', c.author_name
                            )
                        ), '[]'::jsonb)
                        FROM comments c
                        WHERE c.post_id = p.id
                    )
                )
            ), '[]'::jsonb)
            FROM posts p
            WHERE p.user_id = users.id
        )
    ) AS data
FROM users;
```

### Key Benefits
1. **Single Query**: Any GraphQL query depth executes as one database query
2. **No N+1**: Related entities are pre-assembled in the view
3. **Type Safety**: JSONB structure matches GraphQL schema exactly
4. **Performance**: Database handles optimization, uses indexes efficiently

## Enhancement Opportunities

### 1. Automatic View Generation from Type Definitions

```python
@fraise_type
@generate_view(
    name="v_users_full",
    include_relations=["posts", "posts.comments", "profile"],
    indexes=["email", "created_at"]
)
class User:
    id: int
    name: str
    email: str
    posts: List['Post']
    profile: Optional['Profile']

# Automatically generates optimal view with all relations
```

### 2. Smart View Composition Strategies

```python
class ViewComposer:
    """Intelligently compose views based on query patterns"""

    def generate_view(self, entity: Type, strategy: ViewStrategy) -> str:
        """
        Strategies:
        - FULL: Include all relations (for detail views)
        - LIST: Minimal fields + counts (for list views)
        - CUSTOM: Based on actual query usage patterns
        """
        if strategy == ViewStrategy.LIST:
            # Generate lightweight view with counts instead of full data
            return self._generate_list_view(entity)
        elif strategy == ViewStrategy.FULL:
            # Generate complete view with all nested relations
            return self._generate_full_view(entity)
```

### 3. Materialized View Management

```python
@fraise_type
@materialized_view(
    refresh_strategy="CONCURRENT",
    refresh_interval="5 minutes",
    indexes=["user_id", "created_at"]
)
class UserActivitySummary:
    """For expensive aggregations, use materialized views"""
    user_id: int
    total_posts: int
    total_comments: int
    last_activity: datetime
    engagement_score: float
```

### 4. View Dependency Graph

```python
class ViewDependencyManager:
    """Track and manage view dependencies"""

    def analyze_dependencies(self) -> DependencyGraph:
        """
        - Detect circular dependencies
        - Order view creation/updates
        - Generate migration scripts
        """
        pass

    def refresh_cascade(self, base_table: str) -> List[str]:
        """
        When base table changes, refresh dependent views
        in correct order
        """
        pass
```

### 5. Adaptive View Generation

```python
class AdaptiveViewGenerator:
    """Generate views based on actual query patterns"""

    def analyze_query_logs(self, days: int = 30) -> QueryPatterns:
        """Analyze which fields are actually queried together"""
        pass

    def suggest_view_optimizations(self) -> List[ViewOptimization]:
        """
        - Suggest new views for common query patterns
        - Identify unused views
        - Recommend view consolidation
        """
        pass
```

### 6. Conditional Inclusion in Views

```sql
-- Support conditional data inclusion
CREATE VIEW v_posts_with_permissions AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', CASE
            WHEN is_public OR current_user_id() = author_id
            THEN content
            ELSE NULL
        END,
        'comments', CASE
            WHEN comments_enabled THEN (
                SELECT jsonb_agg(...) FROM comments WHERE ...
            )
            ELSE '[]'::jsonb
        END
    ) AS data
FROM posts;
```

### 7. View Performance Monitoring

```python
@dataclass
class ViewMetrics:
    view_name: str
    avg_query_time: float
    cache_hit_ratio: float
    size_mb: float
    last_refreshed: datetime

class ViewMonitor:
    """Monitor view performance and suggest optimizations"""

    async def collect_metrics(self) -> List[ViewMetrics]:
        """Track view usage and performance"""
        pass

    def suggest_indexes(self, view: str) -> List[str]:
        """Suggest indexes based on query patterns"""
        pass
```

### 8. Incremental View Updates

```python
class IncrementalViewUpdater:
    """Update views incrementally instead of full refresh"""

    def generate_incremental_refresh(self, view: str, changed_ids: List[int]) -> str:
        """
        Instead of refreshing entire materialized view,
        update only affected rows
        """
        pass
```

### 9. GraphQL-Aware View Generation

```python
class GraphQLViewGenerator:
    """Generate views optimized for specific GraphQL queries"""

    def from_graphql_schema(self, schema: GraphQLSchema) -> List[str]:
        """
        Analyze GraphQL schema and generate views for:
        - Each type's common query patterns
        - Relay connection patterns
        - Aggregate queries
        """
        pass

    def optimize_for_query(self, query: str) -> str:
        """Generate specialized view for frequently-used query"""
        pass
```

### 10. View Composition Patterns Library

```python
# Common patterns for view composition

class ViewPatterns:
    @staticmethod
    def one_to_many_aggregation(
        parent_table: str,
        child_table: str,
        foreign_key: str,
        fields: List[str]
    ) -> str:
        """Standard pattern for 1:N relationships"""
        pass

    @staticmethod
    def many_to_many_bridge(
        left_table: str,
        right_table: str,
        junction_table: str
    ) -> str:
        """Pattern for M:N relationships through junction table"""
        pass

    @staticmethod
    def recursive_tree(
        table: str,
        parent_field: str,
        max_depth: int = 5
    ) -> str:
        """Pattern for hierarchical data (e.g., comment threads)"""
        pass
```

## Migration Strategy

### 1. Automated View Migration System

```python
class ViewMigration:
    """Manage view versions and migrations"""

    def generate_migration(self,
        old_schema: Dict[str, Type],
        new_schema: Dict[str, Type]
    ) -> str:
        """
        Generate SQL migration script that:
        1. Creates new views with temporary names
        2. Validates data integrity
        3. Atomically swaps views
        4. Cleans up old views
        """
        pass
```

## Performance Optimizations

### 1. View Layering Strategy
```sql
-- Base views: Simple JSONB transformation
CREATE VIEW v_users_base AS
SELECT id, jsonb_build_object('id', id, 'name', name) AS data FROM users;

-- Aggregation views: Add computed fields
CREATE VIEW v_users_stats AS
SELECT id, data || jsonb_build_object(
    'post_count', (SELECT COUNT(*) FROM posts WHERE user_id = u.id)
) AS data FROM v_users_base u;

-- Full views: Complete object graph
CREATE VIEW v_users_full AS
SELECT id, data || jsonb_build_object(
    'posts', (SELECT jsonb_agg(...) FROM v_posts_base WHERE ...)
) AS data FROM v_users_stats;
```

### 2. Partial Materialization
```python
@fraise_type
@partial_materialization(
    materialized_fields=["stats", "computed_score"],
    fresh_fields=["name", "email", "status"]
)
class UserProfile:
    """Mix materialized aggregates with fresh data"""
    pass
```

## Developer Experience

### 1. View Development Tools

```bash
# CLI commands for view management
fraiseql-views generate --type User --strategy full
fraiseql-views analyze --performance
fraiseql-views refresh --materialized
fraiseql-views debug "SELECT * FROM v_users_full WHERE id = 1"
```

### 2. Visual View Designer

Create a web interface for:
- Visualizing view dependencies
- Testing view performance
- Designing custom views
- Monitoring view health

## Expected Benefits

1. **Zero N+1 Queries**: Guaranteed by architecture
2. **Predictable Performance**: One query = one database round trip
3. **Optimal Caching**: Views can be cached at database level
4. **Flexibility**: Different views for different use cases
5. **Maintainability**: Views are declarative and versioned

## Implementation Priority

1. **Phase 1**: Automatic view generation from type definitions
2. **Phase 2**: Smart view composition strategies
3. **Phase 3**: Performance monitoring and optimization
4. **Phase 4**: Advanced patterns and incremental updates

This view-based architecture is FraiseQL's secret weapon for performance and simplicity!
