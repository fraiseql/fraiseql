# Automatic SQL Generation in FraiseQL: Current State and Enhancement Opportunities

## Current Implementation

FraiseQL currently provides powerful automatic SQL generation through several components:

### 1. Core SQL Generation (`src/fraiseql/sql/sql_generator.py`)

The `SQLGenerator` class automatically builds PostgreSQL queries from GraphQL selections:

```python
# Current capabilities:
- JSONB field extraction using -> and ->> operators
- Nested field support (e.g., data->'profile'->'address'->>'city')
- Automatic camelCase to snake_case conversion
- Type-safe query composition using psycopg
- Single-query fetching with jsonb_build_object()
```

### 2. WHERE Clause Generation (`src/fraiseql/sql/where_generator.py`)

Supports comprehensive filtering:
- Basic operators: eq, neq, gt, gte, lt, lte
- JSONB operators: contains, contained_by, overlaps
- Pattern matching: matches, imatches, startswith, istartswith
- Array operations: in, notin, array_contains
- Special: isnull, date_range_adjacent, date_range_overlaps

### 3. ORDER BY Generation (`src/fraiseql/sql/order_by_generator.py`)

- Sort by nested JSONB fields
- Multiple sort criteria
- ASC/DESC support
- Null handling

### 4. Query Translation (`src/fraiseql/core/translate_query.py`)

Orchestrates the translation process:
1. Parses GraphQL query string to AST
2. Extracts field paths from selection sets
3. Resolves fragments
4. Generates optimized SQL

## Enhancement Opportunities

### 1. Query Optimization Engine

```python
@dataclass
class QueryOptimizer:
    """Optimize generated SQL queries"""

    def optimize_jsonb_paths(self, query: str) -> str:
        """
        - Detect repeated JSONB path extractions
        - Use CTEs for complex nested extractions
        - Optimize jsonb_build_object calls
        """
        pass

    def suggest_indexes(self, query: str, table: str) -> List[str]:
        """
        Analyze query patterns and suggest GIN/BTREE indexes:
        - CREATE INDEX idx_users_email ON users ((data->>'email'));
        - CREATE INDEX idx_users_data ON users USING gin (data);
        """
        pass
```

### 2. Advanced Aggregation Support

```python
@fraise_type
class UserStats:
    total_posts: int = fraise_field(
        sql_expression="(SELECT COUNT(*) FROM posts WHERE data->>'user_id' = users.data->>'id')"
    )
    avg_rating: float = fraise_field(
        sql_expression="(SELECT AVG((data->>'rating')::float) FROM reviews WHERE data->>'user_id' = users.data->>'id')"
    )

    # Generate efficient aggregation queries:
    # WITH stats AS (
    #   SELECT user_id, COUNT(*) as post_count, AVG(rating) as avg_rating
    #   FROM posts GROUP BY user_id
    # )
    # SELECT jsonb_build_object(...) FROM users LEFT JOIN stats ON ...
```

### 3. Batch Query Optimization

```python
class BatchSQLGenerator:
    """Generate efficient queries for N+1 scenarios"""

    def generate_dataloader_query(self, parent_ids: List[int], relation: str) -> str:
        """
        Instead of N queries:
        SELECT * FROM posts WHERE data->>'user_id' = '1'
        SELECT * FROM posts WHERE data->>'user_id' = '2'

        Generate one query:
        SELECT * FROM posts
        WHERE data->>'user_id' = ANY($1::text[])
        ORDER BY data->>'user_id'
        """
        pass
```

### 4. SQL Function Generation for Computed Fields

```python
@fraise_type
class Product:
    price: Decimal = fraise_field()
    tax_rate: Decimal = fraise_field()

    # Auto-generate SQL expression:
    total_price: Decimal = fraise_field(
        computed=True,
        sql_expression="(data->>'price')::numeric * (1 + (data->>'tax_rate')::numeric)"
    )
```

### 5. Query Plan Analysis Integration

```python
class QueryAnalyzer:
    """Analyze and optimize query performance"""

    async def analyze_query(self, sql: str, params: List[Any]) -> QueryPlan:
        """
        - Run EXPLAIN ANALYZE
        - Identify slow operations
        - Suggest optimizations
        """
        explain_sql = f"EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}"
        # Parse results and provide recommendations
```

### 6. Materialized View Support

```python
@fraise_type
@materialized_view(
    refresh_interval="1 hour",
    indexes=["email", "created_at"]
)
class UserSummary:
    """Auto-generate and maintain materialized views"""
    id: int
    email: str
    post_count: int = fraise_field(
        sql_expression="(SELECT COUNT(*) FROM posts WHERE data->>'user_id' = users.data->>'id')"
    )
```

### 7. Smart JOIN Detection

```python
class JoinOptimizer:
    """Detect when JOINs would be more efficient than subqueries"""

    def analyze_query_pattern(self, selections: List[str]) -> OptimizationHint:
        """
        If detecting multiple related entity fetches:
        - Suggest JOIN-based query
        - Generate optimized SQL with JOINs
        - Maintain JSONB structure in output
        """
        pass
```

### 8. PostgreSQL-Specific Optimizations

```python
class PostgreSQLOptimizer:
    """Leverage PostgreSQL-specific features"""

    def use_jsonb_path_ops(self, query: str) -> str:
        """Use @> operator for contains queries when possible"""
        pass

    def generate_partial_indexes(self, common_filters: List[Filter]) -> List[str]:
        """
        CREATE INDEX idx_active_users ON users ((data->>'status'))
        WHERE data->>'status' = 'active';
        """
        pass
```

### 9. Query Caching Layer

```python
@dataclass
class QueryCache:
    """Cache frequently used query patterns"""

    def get_cached_sql(self, query_ast: DocumentNode, table: str) -> Optional[str]:
        """Return cached SQL for identical query structures"""
        pass

    def should_cache(self, query_complexity: int, frequency: int) -> bool:
        """Determine if query pattern should be cached"""
        pass
```

### 10. Development Tools

```python
# CLI command for SQL inspection
$ fraiseql-sql inspect "{ users { id name posts { title } } }"

Generated SQL:
--------------
SELECT jsonb_build_object(
  'id', data->>'id',
  'name', data->>'name',
  'posts', (
    SELECT jsonb_agg(
      jsonb_build_object('title', data->>'title')
    )
    FROM posts WHERE data->>'user_id' = users.data->>'id'
  )
) AS result FROM users

Suggested Indexes:
------------------
1. CREATE INDEX idx_posts_user_id ON posts ((data->>'user_id'));
2. CREATE INDEX idx_users_data_gin ON users USING gin (data);

Query Plan Analysis:
--------------------
- Estimated cost: 142.5
- Potential N+1 issue detected
- Consider using DataLoader pattern
```

## Implementation Priority

1. **High Priority**
   - Query optimization engine
   - Batch query optimization for N+1 prevention
   - SQL function generation for computed fields

2. **Medium Priority**
   - Advanced aggregation support
   - Query plan analysis
   - PostgreSQL-specific optimizations

3. **Low Priority**
   - Materialized view support
   - Query caching layer
   - Development tools

## Benefits

1. **Performance**: 10-50x improvement for complex queries
2. **Developer Experience**: Automatic optimization suggestions
3. **Type Safety**: Computed fields with SQL validation
4. **Debugging**: SQL inspection and analysis tools
5. **Scalability**: Efficient handling of large datasets

## Next Steps

1. Implement QueryOptimizer class
2. Add batch query generation
3. Create SQL inspection CLI tool
4. Write comprehensive benchmarks
5. Document optimization patterns
