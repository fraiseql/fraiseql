# Performance Tuning for High-Scale Deployments

Comprehensive guide for optimizing FraiseQL performance in high-traffic, production environments.

## Table of Contents
- [Overview](#overview)
- [Database Optimization](#database-optimization)
- [Application Tuning](#application-tuning)
- [Connection Pooling](#connection-pooling)
- [Caching Strategies](#caching-strategies)
- [Query Optimization](#query-optimization)
- [Horizontal Scaling](#horizontal-scaling)
- [Load Testing](#load-testing)
- [Monitoring & Profiling](#monitoring--profiling)

## Overview

### Performance Targets

| Metric | Target | Critical Threshold |
|--------|--------|--------------------|
| **Response Time P95** | < 100ms | < 500ms |
| **Response Time P99** | < 200ms | < 1000ms |
| **Throughput** | > 10,000 RPS | > 5,000 RPS |
| **Error Rate** | < 0.1% | < 1% |
| **CPU Utilization** | < 70% | < 90% |
| **Memory Usage** | < 80% | < 95% |
| **Database Connections** | < 70% of pool | < 90% of pool |

### Optimization Strategy

```
┌─────────────────────────────────────────────────────────────┐
│                    Performance Optimization                 │
├─────────────────┬─────────────────┬─────────────────────────┤
│   Database      │   Application   │      Infrastructure     │
│                 │                 │                         │
│ • Connection    │ • Query Caching │ • Load Balancing       │
│   Pooling       │ • DataLoader    │ • Auto Scaling         │
│ • Indexing      │ • Async I/O     │ • CDN Integration       │
│ • Query Plans   │ • Memory Mgmt   │ • Edge Computing        │
│ • Partitioning  │ • GC Tuning     │ • Network Optimization │
└─────────────────┴─────────────────┴─────────────────────────┘
```

## Database Optimization

### 1. PostgreSQL Configuration

```ini
# postgresql.conf - Production optimizations
# Memory Configuration
shared_buffers = 25% of RAM                    # e.g., 8GB for 32GB system
effective_cache_size = 75% of RAM              # e.g., 24GB for 32GB system
work_mem = 256MB                               # Per operation memory
maintenance_work_mem = 2GB                     # Maintenance operations
max_connections = 500                          # Adjust based on connection pooling

# Query Optimization
random_page_cost = 1.1                         # For SSD storage
effective_io_concurrency = 200                 # For SSD storage
default_statistics_target = 500                # More detailed statistics

# Write-Ahead Logging
wal_buffers = 64MB                             # WAL buffer size
checkpoint_completion_target = 0.9             # Spread checkpoints
checkpoint_timeout = 15min                     # Checkpoint frequency
max_wal_size = 16GB                           # Maximum WAL size
min_wal_size = 4GB                            # Minimum WAL size

# Background Writer
bgwriter_delay = 200ms                         # Background writer delay
bgwriter_lru_maxpages = 100                    # Pages to write per round
bgwriter_lru_multiplier = 2.0                 # LRU scan multiplier

# Auto Vacuum
autovacuum = on                                # Enable auto vacuum
autovacuum_max_workers = 6                     # Max vacuum workers
autovacuum_naptime = 30s                       # Vacuum check interval
autovacuum_vacuum_scale_factor = 0.1           # Vacuum when 10% dead tuples
autovacuum_analyze_scale_factor = 0.05         # Analyze when 5% changed

# Parallel Query
max_parallel_workers = 16                      # Total parallel workers
max_parallel_workers_per_gather = 4            # Per query parallel workers
parallel_tuple_cost = 0.1                     # Cost of transferring tuple
parallel_setup_cost = 1000.0                  # Cost of setting up parallel query

# JIT Compilation (PostgreSQL 11+)
jit = on                                       # Enable JIT compilation
jit_above_cost = 100000                        # JIT threshold
jit_optimize_above_cost = 500000               # Expensive JIT optimizations
```

### 2. Advanced Indexing Strategies

```sql
-- High-performance indexing for FraiseQL JSONB data

-- 1. GIN Indexes for JSONB queries
CREATE INDEX CONCURRENTLY idx_users_data_gin 
ON users USING GIN (data);

-- 2. Partial indexes for common filters
CREATE INDEX CONCURRENTLY idx_users_active 
ON users USING GIN (data) 
WHERE (data->>'status') = 'active';

-- 3. Expression indexes for computed values
CREATE INDEX CONCURRENTLY idx_users_full_name 
ON users ((data->>'first_name' || ' ' || data->>'last_name'));

-- 4. Composite indexes for complex queries
CREATE INDEX CONCURRENTLY idx_posts_user_published 
ON posts USING GIN (data) 
WHERE (data->>'published')::boolean = true;

-- 5. BRIN indexes for time-series data
CREATE INDEX CONCURRENTLY idx_users_created_at_brin 
ON users USING BRIN (created_at);

-- 6. Hash indexes for equality queries
CREATE INDEX CONCURRENTLY idx_users_email_hash 
ON users USING HASH ((data->>'email'));

-- 7. Covering indexes to avoid heap lookups
CREATE INDEX CONCURRENTLY idx_users_lookup 
ON users (id) INCLUDE (data, created_at, updated_at);
```

### 3. Table Partitioning

```sql
-- Partition large tables by date for better performance
CREATE TABLE posts (
    id UUID NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
) PARTITION BY RANGE (created_at);

-- Create monthly partitions
CREATE TABLE posts_2024_01 PARTITION OF posts
FOR VALUES FROM ('2024-01-01') TO ('2024-02-01');

CREATE TABLE posts_2024_02 PARTITION OF posts
FOR VALUES FROM ('2024-02-01') TO ('2024-03-01');

-- Create indexes on each partition
CREATE INDEX ON posts_2024_01 USING GIN (data);
CREATE INDEX ON posts_2024_01 (created_at);

-- Automated partition management
CREATE OR REPLACE FUNCTION create_monthly_partition()
RETURNS void AS $$
DECLARE
    start_date date;
    end_date date;
    partition_name text;
BEGIN
    start_date := date_trunc('month', CURRENT_DATE + interval '1 month');
    end_date := start_date + interval '1 month';
    partition_name := 'posts_' || to_char(start_date, 'YYYY_MM');
    
    EXECUTE format('CREATE TABLE %I PARTITION OF posts FOR VALUES FROM (%L) TO (%L)',
                   partition_name, start_date, end_date);
    
    EXECUTE format('CREATE INDEX ON %I USING GIN (data)', partition_name);
    EXECUTE format('CREATE INDEX ON %I (created_at)', partition_name);
END;
$$ LANGUAGE plpgsql;

-- Schedule partition creation
SELECT cron.schedule('create-monthly-partition', '0 0 25 * *', 'SELECT create_monthly_partition();');
```

### 4. Query Plan Optimization

```sql
-- Analyze query performance
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) 
SELECT u.data->>'name' as name, count(p.id) as post_count
FROM users u
LEFT JOIN posts p ON u.id = (p.data->>'user_id')::uuid
WHERE u.data->>'status' = 'active'
GROUP BY u.id, u.data->>'name';

-- Create custom statistics for better query planning
CREATE STATISTICS user_status_correlation ON (data->>'status'), created_at FROM users;
CREATE STATISTICS post_user_correlation ON (data->>'user_id'), created_at FROM posts;

-- Analyze the statistics
ANALYZE users;
ANALYZE posts;

-- Monitor slow queries
SELECT query, calls, total_time, rows, 100.0 * shared_blks_hit /
       nullif(shared_blks_hit + shared_blks_read, 0) AS hit_percent
FROM pg_stat_statements 
ORDER BY total_time DESC 
LIMIT 20;
```

## Application Tuning

### 1. AsyncIO Optimization

```python
# app_config.py - Optimized async configuration
import asyncio
import uvloop
from fraiseql import create_fraiseql_app
from fraiseql.repository import FraiseQLRepository

# Use uvloop for better performance
asyncio.set_event_loop_policy(uvloop.EventLoopPolicy())

# Application configuration
class ProductionConfig:
    """Production-optimized configuration."""
    
    # Database connection pool
    DATABASE_POOL_MIN_SIZE = 10
    DATABASE_POOL_MAX_SIZE = 100
    DATABASE_POOL_MAX_QUERIES = 50000
    DATABASE_POOL_MAX_INACTIVE_CONNECTION_LIFETIME = 300
    
    # HTTP server settings
    UVICORN_WORKERS = 4  # Number of worker processes
    UVICORN_BACKLOG = 2048
    UVICORN_KEEPALIVE = 5
    UVICORN_MAX_REQUESTS = 10000
    UVICORN_MAX_REQUESTS_JITTER = 1000
    
    # Query optimization
    DEFAULT_QUERY_TIMEOUT = 30
    MAX_QUERY_COMPLEXITY = 1000
    ENABLE_QUERY_CACHE = True
    QUERY_CACHE_TTL = 300
    
    # Memory management
    MAX_QUERY_RESULT_SIZE = 1024 * 1024  # 1MB
    MEMORY_LIMIT_PER_REQUEST = 50 * 1024 * 1024  # 50MB

async def create_optimized_app():
    """Create application with performance optimizations."""
    
    # Create database connection pool
    import asyncpg
    
    pool = await asyncpg.create_pool(
        dsn=DATABASE_URL,
        min_size=ProductionConfig.DATABASE_POOL_MIN_SIZE,
        max_size=ProductionConfig.DATABASE_POOL_MAX_SIZE,
        max_queries=ProductionConfig.DATABASE_POOL_MAX_QUERIES,
        max_inactive_connection_lifetime=ProductionConfig.DATABASE_POOL_MAX_INACTIVE_CONNECTION_LIFETIME,
        command_timeout=60,
        server_settings={
            'application_name': 'fraiseql_production',
            'jit': 'off',  # Disable JIT for short queries
        }
    )
    
    # Create FraiseQL app
    app = create_fraiseql_app(
        database_pool=pool,
        production=True,
        enable_playground=False,
        enable_introspection=False,
        query_cache_enabled=ProductionConfig.ENABLE_QUERY_CACHE,
        query_cache_ttl=ProductionConfig.QUERY_CACHE_TTL,
        max_query_complexity=ProductionConfig.MAX_QUERY_COMPLEXITY,
        query_timeout=ProductionConfig.DEFAULT_QUERY_TIMEOUT
    )
    
    return app

# Optimized repository with caching
class OptimizedRepository(FraiseQLRepository):
    """Repository with performance optimizations."""
    
    def __init__(self, pool: asyncpg.Pool):
        super().__init__(pool)
        self._query_cache = {}
        self._statement_cache = {}
    
    async def execute_optimized_query(self, query: str, *args):
        """Execute query with statement caching."""
        
        # Use prepared statements for repeated queries
        if query in self._statement_cache:
            stmt = self._statement_cache[query]
        else:
            async with self.pool.acquire() as conn:
                stmt = await conn.prepare(query)
                self._statement_cache[query] = stmt
        
        async with self.pool.acquire() as conn:
            return await stmt.fetch(*args)
    
    async def get_with_cache(self, cache_key: str, query_func, ttl: int = 300):
        """Get data with caching."""
        
        # Check cache first
        if cache_key in self._query_cache:
            cached_data, expiry = self._query_cache[cache_key]
            if time.time() < expiry:
                return cached_data
        
        # Execute query
        result = await query_func()
        
        # Cache result
        self._query_cache[cache_key] = (result, time.time() + ttl)
        
        return result
```

### 2. Memory Management

```python
# memory_optimization.py - Memory usage optimization
import gc
import psutil
import asyncio
from typing import Optional
from dataclasses import dataclass

@dataclass
class MemoryLimits:
    """Memory limits configuration."""
    max_heap_size: int = 512 * 1024 * 1024  # 512MB
    max_query_result_size: int = 50 * 1024 * 1024  # 50MB
    gc_threshold: float = 0.8  # Trigger GC at 80% memory usage
    max_concurrent_queries: int = 100

class MemoryManager:
    """Memory management for high-scale deployments."""
    
    def __init__(self, limits: MemoryLimits):
        self.limits = limits
        self.active_queries = 0
        self._memory_monitor_task: Optional[asyncio.Task] = None
    
    def start_monitoring(self):
        """Start memory monitoring task."""
        self._memory_monitor_task = asyncio.create_task(self._monitor_memory())
    
    def stop_monitoring(self):
        """Stop memory monitoring."""
        if self._memory_monitor_task:
            self._memory_monitor_task.cancel()
    
    async def _monitor_memory(self):
        """Monitor memory usage and trigger cleanup."""
        while True:
            try:
                memory_percent = psutil.virtual_memory().percent / 100
                
                if memory_percent > self.limits.gc_threshold:
                    # Force garbage collection
                    collected = gc.collect()
                    print(f"Memory usage {memory_percent:.1%}, collected {collected} objects")
                
                await asyncio.sleep(10)  # Check every 10 seconds
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                print(f"Memory monitoring error: {e}")
                await asyncio.sleep(60)
    
    async def acquire_query_slot(self):
        """Acquire slot for query execution."""
        while self.active_queries >= self.limits.max_concurrent_queries:
            await asyncio.sleep(0.1)
        
        self.active_queries += 1
    
    def release_query_slot(self):
        """Release query execution slot."""
        self.active_queries = max(0, self.active_queries - 1)
    
    def check_result_size(self, result_size: int):
        """Check if result size is within limits."""
        if result_size > self.limits.max_query_result_size:
            raise MemoryError(f"Query result too large: {result_size} bytes")

# Integration with FraiseQL
memory_manager = MemoryManager(MemoryLimits())

@query
async def get_users_optimized(info, limit: int = 100) -> list[User]:
    """Memory-optimized user query."""
    
    # Acquire memory slot
    await memory_manager.acquire_query_slot()
    
    try:
        repository = OptimizedRepository(info.context["db"])
        
        # Limit result size
        safe_limit = min(limit, 1000)  # Maximum 1000 results
        
        users = await repository.get_many(User, limit=safe_limit)
        
        # Check memory usage
        estimated_size = len(users) * 1024  # Rough estimate
        memory_manager.check_result_size(estimated_size)
        
        return users
        
    finally:
        memory_manager.release_query_slot()
```

### 3. Query Complexity Analysis

```python
# complexity_analyzer.py - Query complexity analysis and limiting
from typing import Dict, Any
from graphql import DocumentNode, FieldNode, FragmentSpreadNode, InlineFragmentNode

class QueryComplexityAnalyzer:
    """Analyze and limit GraphQL query complexity."""
    
    def __init__(self, max_complexity: int = 1000):
        self.max_complexity = max_complexity
        self.type_complexities = {
            'User': 1,
            'Post': 1,
            'Comment': 1,
            'UserConnection': 10,  # Connections are more expensive
            'PostConnection': 10,
        }
        self.field_complexities = {
            'users': 5,
            'posts': 5,
            'comments': 3,
            'search': 20,  # Search is expensive
            'recommendations': 50,  # ML recommendations are very expensive
        }
    
    def analyze_complexity(self, document: DocumentNode, variables: Dict[str, Any] = None) -> int:
        """Calculate query complexity score."""
        total_complexity = 0
        
        for definition in document.definitions:
            if hasattr(definition, 'selection_set'):
                total_complexity += self._calculate_selection_set_complexity(
                    definition.selection_set, variables or {}, depth=0
                )
        
        return total_complexity
    
    def _calculate_selection_set_complexity(self, selection_set, variables: Dict[str, Any], depth: int) -> int:
        """Calculate complexity for a selection set."""
        if depth > 10:  # Prevent infinite recursion
            return 1000000  # Very high complexity to trigger limit
        
        complexity = 0
        
        for selection in selection_set.selections:
            if isinstance(selection, FieldNode):
                field_complexity = self._calculate_field_complexity(selection, variables, depth)
                complexity += field_complexity
            elif isinstance(selection, FragmentSpreadNode):
                # Handle fragment spreads
                complexity += 10  # Base cost for fragments
            elif isinstance(selection, InlineFragmentNode):
                # Handle inline fragments
                complexity += self._calculate_selection_set_complexity(
                    selection.selection_set, variables, depth + 1
                )
        
        return complexity
    
    def _calculate_field_complexity(self, field: FieldNode, variables: Dict[str, Any], depth: int) -> int:
        """Calculate complexity for a single field."""
        field_name = field.name.value
        base_complexity = self.field_complexities.get(field_name, 1)
        
        # Apply multipliers based on arguments
        multiplier = 1
        if field.arguments:
            for arg in field.arguments:
                if arg.name.value in ['first', 'last', 'limit']:
                    # Get the value (handle variables)
                    value = self._get_argument_value(arg.value, variables)
                    if isinstance(value, int):
                        multiplier *= max(1, value / 10)  # Every 10 items adds complexity
        
        # Calculate nested complexity
        nested_complexity = 0
        if field.selection_set:
            nested_complexity = self._calculate_selection_set_complexity(
                field.selection_set, variables, depth + 1
            )
        
        return int(base_complexity * multiplier + nested_complexity)
    
    def _get_argument_value(self, value_node, variables: Dict[str, Any]):
        """Extract value from argument node."""
        if hasattr(value_node, 'value'):
            return value_node.value
        elif hasattr(value_node, 'name') and value_node.name.value in variables:
            return variables[value_node.name.value]
        return 1

# Middleware for complexity limiting
from fraiseql.middleware import GraphQLMiddleware

class ComplexityLimitingMiddleware(GraphQLMiddleware):
    """Middleware to limit query complexity."""
    
    def __init__(self, max_complexity: int = 1000):
        self.analyzer = QueryComplexityAnalyzer(max_complexity)
    
    async def process_request(self, info, **kwargs):
        """Process request and check complexity."""
        query_complexity = self.analyzer.analyze_complexity(
            info.document, 
            info.variable_values
        )
        
        if query_complexity > self.analyzer.max_complexity:
            raise GraphQLError(
                f"Query too complex: {query_complexity} (max: {self.analyzer.max_complexity})"
            )
        
        # Add complexity to context for monitoring
        info.context['query_complexity'] = query_complexity
        
        return await super().process_request(info, **kwargs)
```

## Connection Pooling

### 1. Advanced Connection Pool Configuration

```python
# connection_pool.py - Advanced connection pooling
import asyncio
import asyncpg
import time
from typing import Optional, Dict, Any
from dataclasses import dataclass

@dataclass
class PoolConfig:
    """Connection pool configuration."""
    min_size: int = 10
    max_size: int = 100
    max_queries: int = 50000
    max_inactive_connection_lifetime: float = 300.0
    timeout: float = 60.0
    command_timeout: float = 30.0
    setup_timeout: float = 60.0
    
    # Advanced settings
    retry_attempts: int = 3
    retry_delay: float = 1.0
    health_check_interval: float = 30.0
    connection_validation_query: str = "SELECT 1"

class AdvancedConnectionPool:
    """Advanced connection pool with health monitoring and failover."""
    
    def __init__(self, dsn: str, config: PoolConfig):
        self.dsn = dsn
        self.config = config
        self.pool: Optional[asyncpg.Pool] = None
        self._health_check_task: Optional[asyncio.Task] = None
        self._stats = {
            'connections_created': 0,
            'connections_closed': 0,
            'queries_executed': 0,
            'connection_errors': 0,
            'health_checks': 0,
            'health_check_failures': 0,
        }
    
    async def initialize(self):
        """Initialize the connection pool."""
        for attempt in range(self.config.retry_attempts):
            try:
                self.pool = await asyncpg.create_pool(
                    dsn=self.dsn,
                    min_size=self.config.min_size,
                    max_size=self.config.max_size,
                    max_queries=self.config.max_queries,
                    max_inactive_connection_lifetime=self.config.max_inactive_connection_lifetime,
                    timeout=self.config.timeout,
                    command_timeout=self.config.command_timeout,
                    setup=self._setup_connection,
                    init=self._init_connection,
                    server_settings={
                        'application_name': 'fraiseql_production',
                        'statement_timeout': '30s',
                        'idle_in_transaction_session_timeout': '60s',
                    }
                )
                
                # Start health monitoring
                self._health_check_task = asyncio.create_task(self._health_check_loop())
                
                print(f"Connection pool initialized with {self.config.min_size}-{self.config.max_size} connections")
                return
                
            except Exception as e:
                self._stats['connection_errors'] += 1
                print(f"Failed to create connection pool (attempt {attempt + 1}): {e}")
                
                if attempt < self.config.retry_attempts - 1:
                    await asyncio.sleep(self.config.retry_delay * (2 ** attempt))
                else:
                    raise
    
    async def _setup_connection(self, connection):
        """Setup connection with optimizations."""
        self._stats['connections_created'] += 1
        
        # Set connection-level optimizations
        await connection.execute("SET synchronous_commit = 'off'")
        await connection.execute("SET statement_timeout = '30s'")
        await connection.execute("SET lock_timeout = '10s'")
    
    async def _init_connection(self, connection):
        """Initialize connection after creation."""
        # Warm up the connection
        await connection.fetchval("SELECT 1")
    
    async def _health_check_loop(self):
        """Periodic health check for the pool."""
        while True:
            try:
                await asyncio.sleep(self.config.health_check_interval)
                await self._perform_health_check()
            except asyncio.CancelledError:
                break
            except Exception as e:
                print(f"Health check error: {e}")
    
    async def _perform_health_check(self):
        """Perform health check on the pool."""
        if not self.pool:
            return
        
        self._stats['health_checks'] += 1
        
        try:
            async with self.pool.acquire() as conn:
                await conn.fetchval(self.config.connection_validation_query)
            
            print(f"Pool health check passed. Active: {self.pool.get_size()}, Idle: {self.pool.get_idle_size()}")
            
        except Exception as e:
            self._stats['health_check_failures'] += 1
            print(f"Pool health check failed: {e}")
    
    async def acquire(self):
        """Acquire connection with retry logic."""
        if not self.pool:
            raise RuntimeError("Pool not initialized")
        
        for attempt in range(self.config.retry_attempts):
            try:
                connection = await self.pool.acquire()
                return connection
            except Exception as e:
                if attempt < self.config.retry_attempts - 1:
                    await asyncio.sleep(self.config.retry_delay)
                else:
                    raise
    
    async def execute_with_retry(self, query: str, *args):
        """Execute query with connection retry."""
        for attempt in range(self.config.retry_attempts):
            try:
                async with self.pool.acquire() as conn:
                    result = await conn.fetch(query, *args)
                    self._stats['queries_executed'] += 1
                    return result
            except Exception as e:
                if attempt < self.config.retry_attempts - 1:
                    print(f"Query retry {attempt + 1}: {e}")
                    await asyncio.sleep(self.config.retry_delay)
                else:
                    raise
    
    def get_stats(self) -> Dict[str, Any]:
        """Get pool statistics."""
        pool_stats = {}
        if self.pool:
            pool_stats.update({
                'pool_size': self.pool.get_size(),
                'idle_connections': self.pool.get_idle_size(),
                'pool_utilization': (self.pool.get_size() - self.pool.get_idle_size()) / self.pool.get_max_size()
            })
        
        return {**self._stats, **pool_stats}
    
    async def close(self):
        """Close the connection pool."""
        if self._health_check_task:
            self._health_check_task.cancel()
            
        if self.pool:
            await self.pool.close()
            self._stats['connections_closed'] += self.pool.get_size()

# Usage in application
async def create_production_pool():
    """Create production-ready connection pool."""
    config = PoolConfig(
        min_size=20,
        max_size=200,
        max_queries=100000,
        max_inactive_connection_lifetime=600,
        timeout=30,
        command_timeout=30,
        retry_attempts=3,
        health_check_interval=30
    )
    
    pool = AdvancedConnectionPool(DATABASE_URL, config)
    await pool.initialize()
    
    return pool
```

## Caching Strategies

### 1. Multi-Level Caching

```python
# caching.py - Multi-level caching implementation
import asyncio
import json
import time
import redis.asyncio as redis
from typing import Any, Optional, Union, Dict
from dataclasses import dataclass
from abc import ABC, abstractmethod

@dataclass
class CacheConfig:
    """Cache configuration."""
    l1_ttl: int = 60  # In-memory cache TTL
    l2_ttl: int = 300  # Redis cache TTL
    l3_ttl: int = 3600  # Database cache TTL
    max_l1_size: int = 10000  # Maximum L1 cache entries
    compression_threshold: int = 1024  # Compress data larger than this

class CacheLevel(ABC):
    """Abstract base class for cache levels."""
    
    @abstractmethod
    async def get(self, key: str) -> Optional[Any]:
        pass
    
    @abstractmethod
    async def set(self, key: str, value: Any, ttl: int) -> None:
        pass
    
    @abstractmethod
    async def delete(self, key: str) -> None:
        pass

class L1Cache(CacheLevel):
    """In-memory cache (L1)."""
    
    def __init__(self, max_size: int = 10000):
        self.cache: Dict[str, tuple[Any, float]] = {}
        self.max_size = max_size
        self.access_count: Dict[str, int] = {}
    
    async def get(self, key: str) -> Optional[Any]:
        if key in self.cache:
            value, expiry = self.cache[key]
            if time.time() < expiry:
                self.access_count[key] = self.access_count.get(key, 0) + 1
                return value
            else:
                del self.cache[key]
                self.access_count.pop(key, None)
        return None
    
    async def set(self, key: str, value: Any, ttl: int) -> None:
        if len(self.cache) >= self.max_size:
            # Evict least frequently used item
            lfu_key = min(self.access_count, key=self.access_count.get)
            del self.cache[lfu_key]
            del self.access_count[lfu_key]
        
        self.cache[key] = (value, time.time() + ttl)
        self.access_count[key] = 1
    
    async def delete(self, key: str) -> None:
        self.cache.pop(key, None)
        self.access_count.pop(key, None)

class L2Cache(CacheLevel):
    """Redis cache (L2)."""
    
    def __init__(self, redis_client: redis.Redis, compression_threshold: int = 1024):
        self.redis = redis_client
        self.compression_threshold = compression_threshold
    
    async def get(self, key: str) -> Optional[Any]:
        try:
            data = await self.redis.get(key)
            if data:
                return self._deserialize(data)
        except Exception as e:
            print(f"L2 cache get error: {e}")
        return None
    
    async def set(self, key: str, value: Any, ttl: int) -> None:
        try:
            data = self._serialize(value)
            await self.redis.setex(key, ttl, data)
        except Exception as e:
            print(f"L2 cache set error: {e}")
    
    async def delete(self, key: str) -> None:
        try:
            await self.redis.delete(key)
        except Exception as e:
            print(f"L2 cache delete error: {e}")
    
    def _serialize(self, value: Any) -> bytes:
        """Serialize value with optional compression."""
        json_data = json.dumps(value, default=str).encode('utf-8')
        
        if len(json_data) > self.compression_threshold:
            import gzip
            return gzip.compress(json_data)
        
        return json_data
    
    def _deserialize(self, data: bytes) -> Any:
        """Deserialize value with decompression."""
        try:
            # Try to decompress first
            import gzip
            decompressed = gzip.decompress(data)
            return json.loads(decompressed.decode('utf-8'))
        except:
            # Not compressed
            return json.loads(data.decode('utf-8'))

class MultiLevelCache:
    """Multi-level cache implementation."""
    
    def __init__(self, config: CacheConfig, redis_client: redis.Redis):
        self.config = config
        self.l1 = L1Cache(config.max_l1_size)
        self.l2 = L2Cache(redis_client, config.compression_threshold)
        self._stats = {
            'l1_hits': 0,
            'l1_misses': 0,
            'l2_hits': 0,
            'l2_misses': 0,
            'sets': 0,
            'deletes': 0,
        }
    
    async def get(self, key: str) -> Optional[Any]:
        """Get value from cache hierarchy."""
        
        # Try L1 cache first
        value = await self.l1.get(key)
        if value is not None:
            self._stats['l1_hits'] += 1
            return value
        
        self._stats['l1_misses'] += 1
        
        # Try L2 cache
        value = await self.l2.get(key)
        if value is not None:
            self._stats['l2_hits'] += 1
            # Store in L1 for faster future access
            await self.l1.set(key, value, self.config.l1_ttl)
            return value
        
        self._stats['l2_misses'] += 1
        return None
    
    async def set(self, key: str, value: Any, ttl: Optional[int] = None) -> None:
        """Set value in all cache levels."""
        self._stats['sets'] += 1
        
        l1_ttl = min(ttl or self.config.l1_ttl, self.config.l1_ttl)
        l2_ttl = ttl or self.config.l2_ttl
        
        # Set in both levels
        await asyncio.gather(
            self.l1.set(key, value, l1_ttl),
            self.l2.set(key, value, l2_ttl),
            return_exceptions=True
        )
    
    async def delete(self, key: str) -> None:
        """Delete from all cache levels."""
        self._stats['deletes'] += 1
        
        await asyncio.gather(
            self.l1.delete(key),
            self.l2.delete(key),
            return_exceptions=True
        )
    
    async def get_or_set(self, key: str, factory_func, ttl: Optional[int] = None) -> Any:
        """Get value or compute and cache it."""
        
        # Try to get from cache
        value = await self.get(key)
        if value is not None:
            return value
        
        # Compute value
        value = await factory_func()
        
        # Cache the result
        await self.set(key, value, ttl)
        
        return value
    
    def get_stats(self) -> Dict[str, Any]:
        """Get cache statistics."""
        total_requests = self._stats['l1_hits'] + self._stats['l1_misses']
        l1_hit_rate = self._stats['l1_hits'] / max(total_requests, 1)
        
        l2_requests = self._stats['l2_hits'] + self._stats['l2_misses']
        l2_hit_rate = self._stats['l2_hits'] / max(l2_requests, 1)
        
        return {
            **self._stats,
            'l1_hit_rate': l1_hit_rate,
            'l2_hit_rate': l2_hit_rate,
            'overall_hit_rate': (self._stats['l1_hits'] + self._stats['l2_hits']) / max(total_requests, 1)
        }

# Integration with FraiseQL resolvers
cache_instance = None

async def init_cache():
    """Initialize multi-level cache."""
    global cache_instance
    
    redis_client = redis.Redis.from_url(REDIS_URL)
    config = CacheConfig(
        l1_ttl=60,
        l2_ttl=300,
        max_l1_size=10000,
        compression_threshold=1024
    )
    
    cache_instance = MultiLevelCache(config, redis_client)

@query
async def get_user_cached(info, user_id: str) -> Optional[User]:
    """Get user with multi-level caching."""
    
    cache_key = f"user:{user_id}"
    
    async def fetch_user():
        repository = OptimizedRepository(info.context["db"])
        return await repository.get_by_id(User, user_id)
    
    return await cache_instance.get_or_set(cache_key, fetch_user, ttl=300)
```

## Query Optimization

### 1. DataLoader Implementation

```python
# dataloader.py - Optimized DataLoader for FraiseQL
import asyncio
from typing import Dict, List, Any, Optional, Callable, TypeVar, Generic
from dataclasses import dataclass
from collections import defaultdict

T = TypeVar('T')
K = TypeVar('K')

@dataclass
class LoaderStats:
    """DataLoader statistics."""
    loads: int = 0
    cache_hits: int = 0
    cache_misses: int = 0
    batch_loads: int = 0
    max_batch_size: int = 0
    total_items_loaded: int = 0

class DataLoader(Generic[K, T]):
    """High-performance DataLoader implementation."""
    
    def __init__(
        self,
        batch_load_fn: Callable[[List[K]], List[Optional[T]]],
        batch_size: int = 100,
        cache: bool = True,
        cache_ttl: Optional[int] = None
    ):
        self.batch_load_fn = batch_load_fn
        self.batch_size = batch_size
        self.cache_enabled = cache
        self.cache_ttl = cache_ttl
        
        self._cache: Dict[K, T] = {}
        self._cache_timestamps: Dict[K, float] = {}
        self._pending: Dict[K, asyncio.Future] = {}
        self._queue: List[K] = []
        self._dispatch_task: Optional[asyncio.Task] = None
        self.stats = LoaderStats()
    
    async def load(self, key: K) -> Optional[T]:
        """Load a single item by key."""
        self.stats.loads += 1
        
        # Check cache first
        if self.cache_enabled and self._is_cached(key):
            self.stats.cache_hits += 1
            return self._cache[key]
        
        self.stats.cache_misses += 1
        
        # Check if already pending
        if key in self._pending:
            return await self._pending[key]
        
        # Create future for this key
        future = asyncio.Future()
        self._pending[key] = future
        self._queue.append(key)
        
        # Schedule dispatch if not already scheduled
        if not self._dispatch_task or self._dispatch_task.done():
            self._dispatch_task = asyncio.create_task(self._dispatch())
        
        return await future
    
    async def load_many(self, keys: List[K]) -> List[Optional[T]]:
        """Load multiple items by keys."""
        tasks = [self.load(key) for key in keys]
        return await asyncio.gather(*tasks)
    
    def _is_cached(self, key: K) -> bool:
        """Check if key is cached and not expired."""
        if key not in self._cache:
            return False
        
        if self.cache_ttl is None:
            return True
        
        timestamp = self._cache_timestamps.get(key, 0)
        return time.time() - timestamp < self.cache_ttl
    
    async def _dispatch(self):
        """Dispatch pending loads in batches."""
        await asyncio.sleep(0)  # Allow other coroutines to add to queue
        
        while self._queue:
            # Take a batch from the queue
            batch_keys = self._queue[:self.batch_size]
            self._queue = self._queue[self.batch_size:]
            
            if not batch_keys:
                break
            
            # Update stats
            self.stats.batch_loads += 1
            self.stats.max_batch_size = max(self.stats.max_batch_size, len(batch_keys))
            self.stats.total_items_loaded += len(batch_keys)
            
            try:
                # Execute batch load
                results = await self.batch_load_fn(batch_keys)
                
                # Resolve futures and update cache
                for key, result in zip(batch_keys, results):
                    if key in self._pending:
                        future = self._pending.pop(key)
                        if not future.cancelled():
                            future.set_result(result)
                        
                        # Update cache
                        if self.cache_enabled and result is not None:
                            self._cache[key] = result
                            if self.cache_ttl:
                                self._cache_timestamps[key] = time.time()
                
            except Exception as e:
                # Resolve all futures with the exception
                for key in batch_keys:
                    if key in self._pending:
                        future = self._pending.pop(key)
                        if not future.cancelled():
                            future.set_exception(e)
    
    def clear_cache(self, key: Optional[K] = None):
        """Clear cache for specific key or all keys."""
        if key is None:
            self._cache.clear()
            self._cache_timestamps.clear()
        else:
            self._cache.pop(key, None)
            self._cache_timestamps.pop(key, None)

# FraiseQL DataLoader factory
class FraiseQLDataLoaders:
    """Factory for FraiseQL DataLoaders."""
    
    def __init__(self, repository: OptimizedRepository):
        self.repository = repository
        self.loaders: Dict[str, DataLoader] = {}
    
    def get_user_loader(self) -> DataLoader[str, User]:
        """Get or create user DataLoader."""
        if 'users' not in self.loaders:
            async def batch_load_users(user_ids: List[str]) -> List[Optional[User]]:
                users = await self.repository.get_many(
                    User,
                    where={"id": {"_in": user_ids}}
                )
                user_map = {user.id: user for user in users}
                return [user_map.get(user_id) for user_id in user_ids]
            
            self.loaders['users'] = DataLoader(
                batch_load_users,
                batch_size=100,
                cache_ttl=300
            )
        
        return self.loaders['users']
    
    def get_posts_by_user_loader(self) -> DataLoader[str, List[Post]]:
        """Get or create posts by user DataLoader."""
        if 'posts_by_user' not in self.loaders:
            async def batch_load_posts_by_user(user_ids: List[str]) -> List[List[Post]]:
                all_posts = await self.repository.get_many(
                    Post,
                    where={"user_id": {"_in": user_ids}}
                )
                
                posts_by_user = defaultdict(list)
                for post in all_posts:
                    posts_by_user[post.user_id].append(post)
                
                return [posts_by_user[user_id] for user_id in user_ids]
            
            self.loaders['posts_by_user'] = DataLoader(
                batch_load_posts_by_user,
                batch_size=50,
                cache_ttl=180
            )
        
        return self.loaders['posts_by_user']
    
    def get_stats(self) -> Dict[str, Any]:
        """Get statistics for all loaders."""
        stats = {}
        for name, loader in self.loaders.items():
            stats[name] = {
                'loads': loader.stats.loads,
                'cache_hits': loader.stats.cache_hits,
                'cache_misses': loader.stats.cache_misses,
                'cache_hit_rate': loader.stats.cache_hits / max(loader.stats.loads, 1),
                'batch_loads': loader.stats.batch_loads,
                'max_batch_size': loader.stats.max_batch_size,
                'avg_batch_size': loader.stats.total_items_loaded / max(loader.stats.batch_loads, 1)
            }
        return stats

# Usage in resolvers
@query
async def get_user_with_posts(info, user_id: str) -> Optional[User]:
    """Get user with optimized post loading."""
    data_loaders = FraiseQLDataLoaders(info.context["repository"])
    
    # Load user and posts in parallel
    user_task = data_loaders.get_user_loader().load(user_id)
    posts_task = data_loaders.get_posts_by_user_loader().load(user_id)
    
    user, posts = await asyncio.gather(user_task, posts_task)
    
    if user:
        user.posts = posts
    
    return user
```

## Horizontal Scaling

### 1. Load Balancing Configuration

```yaml
# load-balancer.yaml - Production load balancer setup
apiVersion: v1
kind: ConfigMap
metadata:
  name: nginx-config
data:
  nginx.conf: |
    upstream fraiseql_backend {
        least_conn;
        server fraiseql-0.fraiseql:8000 max_fails=3 fail_timeout=30s;
        server fraiseql-1.fraiseql:8000 max_fails=3 fail_timeout=30s;
        server fraiseql-2.fraiseql:8000 max_fails=3 fail_timeout=30s;
        server fraiseql-3.fraiseql:8000 max_fails=3 fail_timeout=30s;
        server fraiseql-4.fraiseql:8000 max_fails=3 fail_timeout=30s;
        
        # Health check
        server fraiseql-5.fraiseql:8000 backup;
    }
    
    server {
        listen 80;
        server_name api.company.com;
        
        # Security headers
        add_header X-Frame-Options DENY;
        add_header X-Content-Type-Options nosniff;
        add_header X-XSS-Protection "1; mode=block";
        
        # Compression
        gzip on;
        gzip_vary on;
        gzip_types
            application/json
            application/javascript
            text/css
            text/javascript
            text/plain;
        
        # Rate limiting
        limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
        limit_req zone=api burst=20 nodelay;
        
        location /graphql {
            proxy_pass http://fraiseql_backend;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            
            # Timeouts
            proxy_connect_timeout 30s;
            proxy_send_timeout 60s;
            proxy_read_timeout 60s;
            
            # Buffering
            proxy_buffering on;
            proxy_buffer_size 4k;
            proxy_buffers 8 4k;
            
            # Keep alive
            proxy_http_version 1.1;
            proxy_set_header Connection "";
        }
        
        location /health {
            access_log off;
            proxy_pass http://fraiseql_backend;
        }
        
        location /metrics {
            proxy_pass http://fraiseql_backend;
            allow 10.0.0.0/8;
            deny all;
        }
    }
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nginx-lb
spec:
  replicas: 3
  selector:
    matchLabels:
      app: nginx-lb
  template:
    metadata:
      labels:
        app: nginx-lb
    spec:
      containers:
      - name: nginx
        image: nginx:1.21
        ports:
        - containerPort: 80
        volumeMounts:
        - name: nginx-config
          mountPath: /etc/nginx/nginx.conf
          subPath: nginx.conf
        resources:
          requests:
            cpu: 100m
            memory: 128Mi
          limits:
            cpu: 500m
            memory: 512Mi
      volumes:
      - name: nginx-config
        configMap:
          name: nginx-config
```

### 2. Auto-scaling Configuration

```yaml
# autoscaling.yaml - Horizontal Pod Autoscaler
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: fraiseql-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql
  minReplicas: 5
  maxReplicas: 50
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
  - type: Pods
    pods:
      metric:
        name: graphql_queries_per_second
      target:
        type: AverageValue
        averageValue: "100"
  behavior:
    scaleUp:
      stabilizationWindowSeconds: 60
      policies:
      - type: Percent
        value: 50
        periodSeconds: 60
      - type: Pods
        value: 2
        periodSeconds: 60
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Percent
        value: 10
        periodSeconds: 60
---
# Vertical Pod Autoscaler (optional)
apiVersion: autoscaling.k8s.io/v1
kind: VerticalPodAutoscaler
metadata:
  name: fraiseql-vpa
spec:
  targetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql
  updatePolicy:
    updateMode: "Auto"
  resourcePolicy:
    containerPolicies:
    - containerName: fraiseql
      maxAllowed:
        cpu: 2
        memory: 4Gi
      minAllowed:
        cpu: 100m
        memory: 256Mi
```

## Load Testing

### 1. GraphQL Load Testing

```python
# load_test.py - Comprehensive load testing for FraiseQL
import asyncio
import aiohttp
import time
import json
import random
from typing import Dict, List, Any
from dataclasses import dataclass
from concurrent.futures import ThreadPoolExecutor

@dataclass
class TestConfig:
    """Load test configuration."""
    base_url: str
    concurrent_users: int = 100
    duration: int = 300  # 5 minutes
    ramp_up_time: int = 60  # 1 minute
    queries_per_user: int = 1000
    query_weights: Dict[str, float] = None

class LoadTestResults:
    """Load test results aggregator."""
    
    def __init__(self):
        self.response_times: List[float] = []
        self.error_count = 0
        self.success_count = 0
        self.query_counts: Dict[str, int] = {}
        self.error_types: Dict[str, int] = {}
        self.start_time = time.time()
    
    def add_result(self, query_name: str, response_time: float, success: bool, error_type: str = None):
        """Add a test result."""
        if success:
            self.success_count += 1
            self.response_times.append(response_time)
        else:
            self.error_count += 1
            if error_type:
                self.error_types[error_type] = self.error_types.get(error_type, 0) + 1
        
        self.query_counts[query_name] = self.query_counts.get(query_name, 0) + 1
    
    def get_percentile(self, percentile: float) -> float:
        """Calculate response time percentile."""
        if not self.response_times:
            return 0
        
        sorted_times = sorted(self.response_times)
        index = int(len(sorted_times) * percentile / 100) - 1
        return sorted_times[max(0, index)]
    
    def get_summary(self) -> Dict[str, Any]:
        """Get test summary."""
        total_requests = self.success_count + self.error_count
        duration = time.time() - self.start_time
        
        return {
            'duration': duration,
            'total_requests': total_requests,
            'success_count': self.success_count,
            'error_count': self.error_count,
            'error_rate': self.error_count / max(total_requests, 1) * 100,
            'requests_per_second': total_requests / max(duration, 1),
            'avg_response_time': sum(self.response_times) / max(len(self.response_times), 1),
            'p50_response_time': self.get_percentile(50),
            'p95_response_time': self.get_percentile(95),
            'p99_response_time': self.get_percentile(99),
            'query_distribution': self.query_counts,
            'error_types': self.error_types
        }

class GraphQLLoadTester:
    """GraphQL load testing framework."""
    
    def __init__(self, config: TestConfig):
        self.config = config
        self.results = LoadTestResults()
        
        # Default query weights if not provided
        if config.query_weights is None:
            config.query_weights = {
                'simple_user_query': 0.4,
                'user_with_posts': 0.3,
                'search_query': 0.2,
                'complex_aggregation': 0.1
            }
        
        self.queries = {
            'simple_user_query': {
                'query': '''
                    query GetUser($id: ID!) {
                        user(id: $id) {
                            id
                            name
                            email
                        }
                    }
                ''',
                'variables': lambda: {'id': str(random.randint(1, 10000))}
            },
            'user_with_posts': {
                'query': '''
                    query GetUserWithPosts($id: ID!) {
                        user(id: $id) {
                            id
                            name
                            posts(first: 10) {
                                id
                                title
                                content
                                createdAt
                            }
                        }
                    }
                ''',
                'variables': lambda: {'id': str(random.randint(1, 1000))}
            },
            'search_query': {
                'query': '''
                    query SearchUsers($query: String!) {
                        searchUsers(query: $query, first: 20) {
                            id
                            name
                            email
                            posts(first: 3) {
                                title
                            }
                        }
                    }
                ''',
                'variables': lambda: {'query': random.choice(['john', 'jane', 'bob', 'alice', 'test'])}
            },
            'complex_aggregation': {
                'query': '''
                    query GetUserStats {
                        userStats {
                            totalUsers
                            activeUsers
                            averagePostsPerUser
                            topPosters(limit: 10) {
                                user {
                                    name
                                }
                                postCount
                            }
                        }
                    }
                ''',
                'variables': lambda: {}
            }
        }
    
    async def execute_query(self, session: aiohttp.ClientSession, query_name: str) -> None:
        """Execute a single GraphQL query."""
        query_info = self.queries[query_name]
        
        payload = {
            'query': query_info['query'],
            'variables': query_info['variables']()
        }
        
        start_time = time.time()
        
        try:
            async with session.post(
                f"{self.config.base_url}/graphql",
                json=payload,
                timeout=aiohttp.ClientTimeout(total=30)
            ) as response:
                await response.json()
                
                response_time = time.time() - start_time
                success = response.status == 200
                
                if not success:
                    error_type = f"HTTP_{response.status}"
                else:
                    error_type = None
                
                self.results.add_result(query_name, response_time, success, error_type)
                
        except asyncio.TimeoutError:
            response_time = time.time() - start_time
            self.results.add_result(query_name, response_time, False, "TIMEOUT")
        except Exception as e:
            response_time = time.time() - start_time
            self.results.add_result(query_name, response_time, False, type(e).__name__)
    
    def select_query(self) -> str:
        """Select a query based on weights."""
        weights = list(self.config.query_weights.values())
        queries = list(self.config.query_weights.keys())
        return random.choices(queries, weights=weights)[0]
    
    async def user_session(self, user_id: int) -> None:
        """Simulate a single user session."""
        
        # Calculate delay for ramp-up
        ramp_delay = (user_id / self.config.concurrent_users) * self.config.ramp_up_time
        await asyncio.sleep(ramp_delay)
        
        # Create HTTP session
        connector = aiohttp.TCPConnector(limit=10, limit_per_host=10)
        async with aiohttp.ClientSession(connector=connector) as session:
            
            # Execute queries for the duration
            end_time = time.time() + self.config.duration
            query_count = 0
            
            while time.time() < end_time and query_count < self.config.queries_per_user:
                query_name = self.select_query()
                await self.execute_query(session, query_name)
                query_count += 1
                
                # Add some realistic delay between queries
                await asyncio.sleep(random.uniform(0.1, 2.0))
    
    async def run_load_test(self) -> Dict[str, Any]:
        """Run the complete load test."""
        print(f"Starting load test with {self.config.concurrent_users} users for {self.config.duration}s")
        
        # Create user sessions
        tasks = [
            self.user_session(user_id) 
            for user_id in range(self.config.concurrent_users)
        ]
        
        # Run all sessions concurrently
        await asyncio.gather(*tasks, return_exceptions=True)
        
        # Return results
        return self.results.get_summary()

async def run_performance_test():
    """Run comprehensive performance test."""
    
    # Test configurations for different scenarios
    test_scenarios = [
        {
            'name': 'Light Load',
            'config': TestConfig(
                base_url='http://localhost:8000',
                concurrent_users=10,
                duration=60,
                ramp_up_time=10
            )
        },
        {
            'name': 'Medium Load',
            'config': TestConfig(
                base_url='http://localhost:8000',
                concurrent_users=50,
                duration=180,
                ramp_up_time=30
            )
        },
        {
            'name': 'High Load',
            'config': TestConfig(
                base_url='http://localhost:8000',
                concurrent_users=200,
                duration=300,
                ramp_up_time=60
            )
        },
        {
            'name': 'Stress Test',
            'config': TestConfig(
                base_url='http://localhost:8000',
                concurrent_users=500,
                duration=300,
                ramp_up_time=60
            )
        }
    ]
    
    results = {}
    
    for scenario in test_scenarios:
        print(f"\n{'='*50}")
        print(f"Running {scenario['name']}")
        print(f"{'='*50}")
        
        tester = GraphQLLoadTester(scenario['config'])
        scenario_results = await tester.run_load_test()
        results[scenario['name']] = scenario_results
        
        # Print results
        print(f"Duration: {scenario_results['duration']:.1f}s")
        print(f"Total Requests: {scenario_results['total_requests']}")
        print(f"Requests/sec: {scenario_results['requests_per_second']:.1f}")
        print(f"Error Rate: {scenario_results['error_rate']:.2f}%")
        print(f"Avg Response Time: {scenario_results['avg_response_time']:.3f}s")
        print(f"P95 Response Time: {scenario_results['p95_response_time']:.3f}s")
        print(f"P99 Response Time: {scenario_results['p99_response_time']:.3f}s")
        
        # Wait between scenarios
        if scenario != test_scenarios[-1]:
            print("Waiting 60s before next scenario...")
            await asyncio.sleep(60)
    
    return results

if __name__ == "__main__":
    results = asyncio.run(run_performance_test())
    
    # Save results to file
    with open('load_test_results.json', 'w') as f:
        json.dump(results, f, indent=2)
    
    print("\nLoad test completed. Results saved to load_test_results.json")
```

## Monitoring & Profiling

### 1. Performance Monitoring

```python
# performance_monitor.py - Real-time performance monitoring
import asyncio
import psutil
import time
import gc
from typing import Dict, Any, Optional
from dataclasses import dataclass
from prometheus_client import Gauge, Counter, Histogram

# Metrics
cpu_usage_gauge = Gauge('fraiseql_cpu_usage_percent', 'CPU usage percentage')
memory_usage_gauge = Gauge('fraiseql_memory_usage_bytes', 'Memory usage in bytes')
gc_collections_counter = Counter('fraiseql_gc_collections_total', 'GC collections', ['generation'])
gc_duration_histogram = Histogram('fraiseql_gc_duration_seconds', 'GC duration')
query_duration_histogram = Histogram('fraiseql_query_duration_seconds', 'Query duration', ['operation'])
active_connections_gauge = Gauge('fraiseql_active_connections', 'Active database connections')

@dataclass
class PerformanceMetrics:
    """Performance metrics snapshot."""
    timestamp: float
    cpu_percent: float
    memory_bytes: int
    memory_percent: float
    active_connections: int
    gc_stats: Dict[str, Any]
    query_stats: Dict[str, Any]

class PerformanceMonitor:
    """Real-time performance monitoring."""
    
    def __init__(self, sample_interval: int = 10):
        self.sample_interval = sample_interval
        self.monitoring = False
        self._monitor_task: Optional[asyncio.Task] = None
        self._metrics_history: List[PerformanceMetrics] = []
        self._max_history = 1000  # Keep last 1000 samples
    
    def start_monitoring(self):
        """Start performance monitoring."""
        if not self.monitoring:
            self.monitoring = True
            self._monitor_task = asyncio.create_task(self._monitoring_loop())
            print("Performance monitoring started")
    
    def stop_monitoring(self):
        """Stop performance monitoring."""
        self.monitoring = False
        if self._monitor_task:
            self._monitor_task.cancel()
    
    async def _monitoring_loop(self):
        """Main monitoring loop."""
        while self.monitoring:
            try:
                metrics = self._collect_metrics()
                self._update_prometheus_metrics(metrics)
                self._store_metrics(metrics)
                
                await asyncio.sleep(self.sample_interval)
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                print(f"Monitoring error: {e}")
                await asyncio.sleep(self.sample_interval)
    
    def _collect_metrics(self) -> PerformanceMetrics:
        """Collect current performance metrics."""
        
        # System metrics
        cpu_percent = psutil.cpu_percent()
        memory = psutil.virtual_memory()
        
        # GC metrics
        gc_stats = {}
        for generation in range(3):
            gc_stats[f'generation_{generation}'] = gc.get_count()[generation]
        
        # Database connection metrics (would need to be implemented)
        active_connections = self._get_active_connections()
        
        return PerformanceMetrics(
            timestamp=time.time(),
            cpu_percent=cpu_percent,
            memory_bytes=memory.used,
            memory_percent=memory.percent,
            active_connections=active_connections,
            gc_stats=gc_stats,
            query_stats={}  # Would be populated by query monitoring
        )
    
    def _update_prometheus_metrics(self, metrics: PerformanceMetrics):
        """Update Prometheus metrics."""
        cpu_usage_gauge.set(metrics.cpu_percent)
        memory_usage_gauge.set(metrics.memory_bytes)
        active_connections_gauge.set(metrics.active_connections)
        
        for generation, count in metrics.gc_stats.items():
            gc_collections_counter.labels(generation=generation)._value._value = count
    
    def _store_metrics(self, metrics: PerformanceMetrics):
        """Store metrics in history."""
        self._metrics_history.append(metrics)
        
        # Trim history if too long
        if len(self._metrics_history) > self._max_history:
            self._metrics_history = self._metrics_history[-self._max_history:]
    
    def _get_active_connections(self) -> int:
        """Get active database connections count."""
        # This would need to be implemented based on your connection pool
        return 0
    
    def get_performance_summary(self) -> Dict[str, Any]:
        """Get performance summary."""
        if not self._metrics_history:
            return {}
        
        recent_metrics = self._metrics_history[-10:]  # Last 10 samples
        
        avg_cpu = sum(m.cpu_percent for m in recent_metrics) / len(recent_metrics)
        avg_memory = sum(m.memory_bytes for m in recent_metrics) / len(recent_metrics)
        max_connections = max(m.active_connections for m in recent_metrics)
        
        return {
            'avg_cpu_percent': avg_cpu,
            'avg_memory_bytes': avg_memory,
            'avg_memory_mb': avg_memory / (1024 * 1024),
            'max_active_connections': max_connections,
            'sample_count': len(self._metrics_history),
            'monitoring_duration': self._metrics_history[-1].timestamp - self._metrics_history[0].timestamp if len(self._metrics_history) > 1 else 0
        }

# Query profiling decorator
def profile_query(operation_name: str):
    """Decorator to profile GraphQL queries."""
    def decorator(func):
        async def wrapper(*args, **kwargs):
            start_time = time.time()
            
            try:
                result = await func(*args, **kwargs)
                duration = time.time() - start_time
                query_duration_histogram.labels(operation=operation_name).observe(duration)
                
                if duration > 1.0:  # Log slow queries
                    print(f"Slow query detected: {operation_name} took {duration:.3f}s")
                
                return result
                
            except Exception as e:
                duration = time.time() - start_time
                query_duration_histogram.labels(operation=f"{operation_name}_error").observe(duration)
                raise
        
        return wrapper
    return decorator

# Usage example
performance_monitor = PerformanceMonitor(sample_interval=5)

@query
@profile_query("get_user")
async def get_user_profiled(info, user_id: str) -> Optional[User]:
    """Profiled user query."""
    repository = OptimizedRepository(info.context["db"])
    return await repository.get_by_id(User, user_id)

# Application startup
async def start_monitoring():
    """Start all monitoring components."""
    performance_monitor.start_monitoring()
    
    # Initialize other monitoring components
    memory_manager.start_monitoring()
    
    print("All monitoring systems started")

# Application shutdown
async def stop_monitoring():
    """Stop all monitoring components."""
    performance_monitor.stop_monitoring()
    memory_manager.stop_monitoring()
    
    print("All monitoring systems stopped")
```

## Best Practices Summary

### 1. Database Optimization
- **Connection Pooling**: Use optimized pool sizes (10-100 connections)
- **Indexing**: Create appropriate GIN indexes for JSONB queries
- **Query Planning**: Analyze and optimize slow queries regularly
- **Partitioning**: Partition large tables by date or other logical boundaries

### 2. Application Performance
- **Async I/O**: Use uvloop and optimize async patterns
- **Memory Management**: Monitor memory usage and implement cleanup
- **Query Complexity**: Limit query complexity and depth
- **Caching**: Implement multi-level caching strategies

### 3. Scaling Strategies
- **Horizontal Scaling**: Use auto-scaling with appropriate metrics
- **Load Balancing**: Implement proper load balancing with health checks
- **Connection Management**: Use connection pooling and monitoring
- **Resource Limits**: Set appropriate CPU and memory limits

### 4. Monitoring & Testing
- **Performance Monitoring**: Real-time monitoring of key metrics
- **Load Testing**: Regular load testing with realistic scenarios
- **Profiling**: Profile queries and identify bottlenecks
- **Alerting**: Set up alerts for performance degradation

### 5. Production Deployment
- **Configuration**: Use production-optimized settings
- **Security**: Implement proper security measures
- **Backup**: Ensure reliable backup and recovery procedures
- **Documentation**: Maintain runbooks and procedures

## Next Steps

- [Disaster Recovery](./disaster-recovery.md) - Backup and recovery strategies
- [Security Guide](./security.md) - Comprehensive security implementation
- [Monitoring](./monitoring.md) - Production monitoring and observability