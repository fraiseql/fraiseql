# TurboRouter: Pre-Compiled Query Registry for Production Performance

## Summary

Implement a TurboRouter feature that pre-compiles and caches frequently-used GraphQL queries as direct SQL templates, bypassing GraphQL parsing and SQL generation overhead in production. This can improve performance by 15-40% for common queries while maintaining full compatibility and safety through automatic fallback.

## Background

Currently, every GraphQL query in FraiseQL goes through:
1. GraphQL parsing and validation (~0.5ms)
2. SQL generation from AST (~0.1ms)
3. PostgreSQL execution (10-50ms)
4. Result mapping (~0.2ms)

For high-frequency queries, the ~0.8ms overhead per request adds up:
- At 1,000 req/s: 800ms CPU time per second
- At 10,000 req/s: 8 seconds CPU time per second

## Proposed Solution

Create a query registry that stores pre-compiled SQL templates for known GraphQL queries, allowing direct execution without parsing overhead.

### 1. Database Schema

```sql
-- Query registry table
CREATE TABLE fraiseql_query_registry (
    query_hash           TEXT PRIMARY KEY,          -- SHA-256 of normalized query
    operation_name       TEXT,                      -- GraphQL operation name
    query_pattern        TEXT NOT NULL,             -- Normalized GraphQL query
    sql_template         TEXT NOT NULL,             -- Pre-compiled SQL with placeholders
    view_name            TEXT NOT NULL,             -- Target view/table
    required_variables   JSONB DEFAULT '[]',        -- Required GraphQL variables
    optional_variables   JSONB DEFAULT '[]',        -- Optional variables with defaults
    result_transformer   TEXT,                      -- Python code for result shaping
    use_fast_path        BOOLEAN DEFAULT TRUE,      -- Enable/disable flag
    hit_count           INTEGER DEFAULT 0,          -- Usage statistics
    last_used           TIMESTAMPTZ DEFAULT NOW(),  -- For cache management
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    created_by          TEXT,                       -- For audit trail

    -- Indexes for performance
    INDEX idx_query_registry_last_used ON fraiseql_query_registry(last_used),
    INDEX idx_query_registry_hit_count ON fraiseql_query_registry(hit_count DESC)
);

-- Audit log for changes
CREATE TABLE fraiseql_query_registry_log (
    id                  SERIAL PRIMARY KEY,
    query_hash          TEXT NOT NULL,
    action              TEXT NOT NULL,  -- 'created', 'updated', 'disabled'
    changed_at          TIMESTAMPTZ DEFAULT NOW(),
    changed_by          TEXT,
    previous_values     JSONB,
    new_values          JSONB
);
```

### 2. Query Registration Process

#### Automatic Detection (Development Mode)

```python
# fraiseql/turbo/detector.py
from typing import Dict, Set
import hashlib
import json

class QueryDetector:
    def __init__(self, threshold: int = 100):
        self.query_stats: Dict[str, int] = {}
        self.threshold = threshold

    async def track_query(self, query: str, variables: dict):
        """Track query usage in development/staging."""
        normalized = self._normalize_query(query)
        query_hash = self._hash_query(normalized)

        self.query_stats[query_hash] = self.query_stats.get(query_hash, 0) + 1

        if self.query_stats[query_hash] == self.threshold:
            await self._suggest_registration(query, variables, query_hash)

    def _normalize_query(self, query: str) -> str:
        """Normalize whitespace and formatting."""
        # Remove comments, normalize whitespace, etc.
        return " ".join(query.split())

    def _hash_query(self, query: str) -> str:
        """Generate stable hash for query."""
        return hashlib.sha256(query.encode('utf-8')).hexdigest()

    async def _suggest_registration(self, query: str, variables: dict, query_hash: str):
        """Log suggestion for query registration."""
        logger.info(
            f"Query {query_hash[:8]}... executed {self.threshold} times. "
            f"Consider registering for TurboRouter optimization."
        )
```

#### Manual Registration Tool

```python
# fraiseql/turbo/register.py
class QueryRegistrar:
    async def register_query(
        self,
        query: str,
        operation_name: str,
        view_name: str,
        description: str = None
    ) -> str:
        """Register a GraphQL query for turbo execution."""

        # Parse and validate query
        parsed = parse_graphql_query(query)

        # Generate SQL template
        sql_template = self._generate_sql_template(parsed, view_name)

        # Extract variable requirements
        required_vars, optional_vars = self._analyze_variables(parsed)

        # Store in registry
        query_hash = self._hash_query(query)

        await self.db.execute("""
            INSERT INTO fraiseql_query_registry
            (query_hash, operation_name, query_pattern, sql_template,
             view_name, required_variables, optional_variables)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (query_hash)
            DO UPDATE SET
                sql_template = EXCLUDED.sql_template,
                updated_at = NOW()
        """, query_hash, operation_name, query, sql_template,
            view_name, json.dumps(required_vars), json.dumps(optional_vars))

        return query_hash

    def _generate_sql_template(self, parsed_query, view_name: str) -> str:
        """Convert GraphQL query to parameterized SQL."""
        # Example output:
        # SELECT data FROM v_users
        # WHERE (data->>'id')::uuid = %(userId)s
        #   AND (data->>'status') = %(status)s

        selections = self._extract_selections(parsed_query)
        filters = self._extract_filters(parsed_query)

        sql = f"SELECT data FROM {view_name}"

        if filters:
            where_clauses = []
            for field, param in filters.items():
                where_clauses.append(f"(data->>'{field}') = %({param})s")
            sql += " WHERE " + " AND ".join(where_clauses)

        return sql
```

### 3. TurboRouter Executor

```python
# fraiseql/turbo/executor.py
from typing import Optional, Dict, Any
import asyncpg
import json

class TurboExecutor:
    def __init__(self, db_pool: asyncpg.Pool, cache_size: int = 1000):
        self.db_pool = db_pool
        self.registry_cache: Dict[str, Dict] = {}  # In-memory cache
        self.cache_size = cache_size

    async def execute(
        self,
        query: str,
        variables: Dict[str, Any],
        operation_name: Optional[str] = None
    ) -> Optional[Dict[str, Any]]:
        """Try to execute query via turbo path."""

        query_hash = self._hash_query(query)

        # Check cache first
        registry_entry = self.registry_cache.get(query_hash)

        if not registry_entry:
            # Load from database
            registry_entry = await self._load_registry_entry(query_hash)
            if not registry_entry:
                return None  # Not registered, fall back

            # Update cache (LRU-style)
            if len(self.registry_cache) >= self.cache_size:
                # Remove least recently used
                oldest = min(self.registry_cache.items(),
                           key=lambda x: x[1].get('last_access', 0))
                del self.registry_cache[oldest[0]]

            self.registry_cache[query_hash] = registry_entry

        # Validate required variables
        required = registry_entry.get('required_variables', [])
        for var in required:
            if var not in variables:
                logger.warning(f"Missing required variable {var} for turbo query")
                return None

        # Execute pre-compiled SQL
        sql_template = registry_entry['sql_template']

        try:
            async with self.db_pool.acquire() as conn:
                # Execute with variable substitution
                result = await conn.fetchval(sql_template, **variables)

                # Update statistics
                await self._update_stats(query_hash)

                # Apply result transformer if needed
                if registry_entry.get('result_transformer'):
                    result = self._apply_transformer(
                        result,
                        registry_entry['result_transformer']
                    )

                return result

        except Exception as e:
            logger.error(f"Turbo execution failed: {e}")
            # Disable problematic query
            await self._disable_query(query_hash)
            return None

    async def _load_registry_entry(self, query_hash: str) -> Optional[Dict]:
        """Load registry entry from database."""
        async with self.db_pool.acquire() as conn:
            row = await conn.fetchrow("""
                SELECT sql_template, required_variables, optional_variables,
                       result_transformer, view_name
                FROM fraiseql_query_registry
                WHERE query_hash = $1 AND use_fast_path = TRUE
            """, query_hash)

            if row:
                return {
                    'sql_template': row['sql_template'],
                    'required_variables': json.loads(row['required_variables']),
                    'optional_variables': json.loads(row['optional_variables']),
                    'result_transformer': row['result_transformer'],
                    'view_name': row['view_name'],
                    'last_access': time.time()
                }
            return None

    async def _update_stats(self, query_hash: str):
        """Update usage statistics."""
        async with self.db_pool.acquire() as conn:
            await conn.execute("""
                UPDATE fraiseql_query_registry
                SET hit_count = hit_count + 1,
                    last_used = NOW()
                WHERE query_hash = $1
            """, query_hash)
```

### 4. Integration with FraiseQL

```python
# fraiseql/fastapi/routers.py (modified)
class ProductionRouter:
    def __init__(self, config: FraiseQLConfig):
        self.config = config
        self.turbo_executor = TurboExecutor(config.db_pool) if config.enable_turbo else None

    async def handle_graphql_request(
        self,
        query: str,
        variables: Optional[Dict] = None,
        operation_name: Optional[str] = None
    ):
        # Try turbo path first if enabled
        if self.turbo_executor:
            start_time = time.time()

            turbo_result = await self.turbo_executor.execute(
                query, variables or {}, operation_name
            )

            if turbo_result is not None:
                # Success! Skip all GraphQL processing
                elapsed = time.time() - start_time
                logger.debug(f"Turbo execution completed in {elapsed*1000:.2f}ms")

                return {
                    "data": turbo_result,
                    "extensions": {
                        "turbo": True,
                        "executionTime": elapsed
                    }
                }

        # Fall back to normal GraphQL execution
        return await self.execute_graphql_standard(query, variables, operation_name)
```

### 5. Management Commands

```python
# fraiseql/cli/turbo.py
import click

@click.group()
def turbo():
    """TurboRouter management commands."""
    pass

@turbo.command()
@click.argument('query_file')
@click.option('--view', required=True, help='Target view name')
@click.option('--name', help='Operation name')
def register(query_file: str, view: str, name: str):
    """Register a GraphQL query for turbo execution."""
    with open(query_file) as f:
        query = f.read()

    registrar = QueryRegistrar()
    query_hash = asyncio.run(
        registrar.register_query(query, name or "Query", view)
    )

    click.echo(f"Query registered with hash: {query_hash}")

@turbo.command()
def stats():
    """Show turbo query statistics."""
    # Show most used queries, cache hit rates, etc.

@turbo.command()
@click.argument('query_hash')
def disable(query_hash: str):
    """Disable a turbo query."""
    # Set use_fast_path = FALSE

@turbo.command()
@click.option('--days', default=7, help='Days of inactivity')
def cleanup(days: int):
    """Clean up unused turbo queries."""
    # Remove queries not used in N days
```

### 6. Configuration

```python
# fraiseql/config.py
@dataclass
class TurboConfig:
    enabled: bool = True
    cache_size: int = 1000
    auto_detect: bool = False  # Enable in development
    detection_threshold: int = 100
    ttl_days: int = 30  # Auto-disable after N days

@dataclass
class FraiseQLConfig:
    # ... existing config ...
    turbo: TurboConfig = field(default_factory=TurboConfig)
```

## Implementation Phases

### Phase 1: Core Infrastructure (Week 1-2)
- [ ] Create database schema
- [ ] Implement QueryRegistrar
- [ ] Implement TurboExecutor
- [ ] Add basic integration to ProductionRouter

### Phase 2: Developer Tools (Week 3)
- [ ] Add QueryDetector for development mode
- [ ] Create CLI commands
- [ ] Add registration UI/endpoint
- [ ] Write documentation

### Phase 3: Production Features (Week 4)
- [ ] Add monitoring and statistics
- [ ] Implement cache warming
- [ ] Add automatic cleanup
- [ ] Performance benchmarking

### Phase 4: Advanced Features (Future)
- [ ] Automatic SQL optimization
- [ ] Query plan caching
- [ ] Batch query support
- [ ] GraphQL subscription support

## Benefits

1. **Performance**: 15-40% faster for cached queries
2. **Scalability**: Reduces CPU usage at high volume
3. **Predictability**: Consistent performance for known queries
4. **Flexibility**: Automatic fallback maintains compatibility
5. **Observability**: Built-in statistics and monitoring

## Risks and Mitigations

### Risk: Cache Invalidation
**Mitigation**: TTL-based expiration and manual invalidation commands

### Risk: Schema Changes
**Mitigation**: Version tracking and automatic validation on startup

### Risk: Security
**Mitigation**: Parameterized queries only, no dynamic SQL generation

### Risk: Debugging Complexity
**Mitigation**: Detailed logging and ability to disable per-query

## Success Metrics

- Reduction in p99 latency for common queries
- CPU usage reduction under load
- Cache hit rate > 80% for registered queries
- Zero security incidents from turbo path

## Migration Strategy

1. **Enable in production with no registered queries** (safe no-op)
2. **Register read-only queries first** (lowest risk)
3. **Monitor performance and errors**
4. **Gradually register more complex queries**
5. **Enable auto-detection in staging** to find candidates

This feature would make FraiseQL extremely competitive for production workloads while maintaining its excellent developer experience.
