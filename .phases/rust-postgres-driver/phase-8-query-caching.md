# Phase 8: Query Plan Caching

**Phase**: 8 of 9
**Effort**: 6-8 hours
**Status**: Ready to implement (after Phase 7 complete)
**Prerequisite**: Phase 7 - Query Building complete

---

## Objective

Implement query plan caching at the Rust level to eliminate repeated query building for identical GraphQL patterns:

1. Cache compiled query plans by query signature
2. Detect repeated queries (same structure, different variables)
3. Return cached plan when signature matches
4. Automatic cache invalidation on schema changes
5. Performance: 5-10x speedup for repeated queries

**Success Criteria**:
- ✅ Identical queries return pre-compiled plan (< 1µs)
- ✅ Different queries bypass cache properly
- ✅ Cache hit rate 60-80% in typical workloads
- ✅ Schema changes invalidate cache
- ✅ Memory usage reasonable (< 100MB for 5000 cached plans)
- ✅ Benchmarks show 5-10x improvement for repeated queries

---

## Architecture Overview

### Caching Strategy

```
Query String: "query { users(where: {status: $status}) { id } }"
    ↓
Generate Signature: "query::users::parameterized"
    ↓
Check Cache[signature]
    ├─ MISS: Build plan, store in cache
    └─ HIT: Return cached plan
    ↓
Execute Plan
    └─ Bind parameters to cached query
```

### Cache Entry

```rust
pub struct CachedQueryPlan {
    pub signature: String,          // Unique key
    pub sql_template: String,       // SELECT ... WHERE ... (with $1, $2 placeholders)
    pub param_positions: Vec<ParamInfo>,  // Position of each parameter
    pub parameter_schema: Vec<ParamSchema>,  // Type of each parameter
    pub created_at: Instant,        // For LRU eviction
    pub hit_count: u64,             // Statistics
}

pub struct ParamInfo {
    pub name: String,
    pub position: usize,            // Position in SQL ($1, $2, etc)
    pub expected_type: String,      // "string", "int", "float", "bool"
}
```

### LRU Cache

```rust
pub struct QueryPlanCache {
    cache: LruCache<String, CachedQueryPlan>,  // signature → plan
    max_size: usize,                           // 5000 plans max
    hits: u64,
    misses: u64,
}
```

---

## Implementation Steps

### Step 1: Add Cache Dependencies

**File**: `fraiseql_rs/Cargo.toml`

```toml
[dependencies]
# ... existing dependencies ...

# LRU cache
lru = "0.12"
linked-hash-map = "0.5"

# Hashing
sha2 = "0.10"
hex = "0.4"

# Metrics
prometheus = "0.13"
```

---

### Step 2: Create Cache Structures

**File**: `fraiseql_rs/src/cache/mod.rs` (NEW)

```rust
//! Query plan caching module.

use std::time::Instant;
use std::sync::{Arc, Mutex};
use lru::LruCache;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedQueryPlan {
    pub signature: String,
    pub sql_template: String,
    pub parameters: Vec<ParamInfo>,
    pub created_at: u64,  // Unix timestamp
    pub hit_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamInfo {
    pub name: String,
    pub position: usize,
    pub expected_type: String,  // "string", "int", "float", "bool", "json"
}

/// Thread-safe query plan cache.
pub struct QueryPlanCache {
    cache: Arc<Mutex<LruCache<String, CachedQueryPlan>>>,
    max_size: usize,
    hits: Arc<Mutex<u64>>,
    misses: Arc<Mutex<u64>>,
}

impl QueryPlanCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(
                std::num::NonZeroUsize::new(max_size).unwrap()
            ))),
            max_size,
            hits: Arc::new(Mutex::new(0)),
            misses: Arc::new(Mutex::new(0)),
        }
    }

    pub fn get(&self, signature: &str) -> Result<Option<CachedQueryPlan>> {
        let mut cache = self.cache.lock().map_err(|e| {
            anyhow::anyhow!("Cache lock error: {}", e)
        })?;

        if let Some(plan) = cache.get_mut(signature) {
            plan.hit_count += 1;
            *self.hits.lock().unwrap() += 1;
            Ok(Some(plan.clone()))
        } else {
            *self.misses.lock().unwrap() += 1;
            Ok(None)
        }
    }

    pub fn put(&self, signature: String, plan: CachedQueryPlan) -> Result<()> {
        let mut cache = self.cache.lock().map_err(|e| {
            anyhow::anyhow!("Cache lock error: {}", e)
        })?;
        cache.put(signature, plan);
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        let mut cache = self.cache.lock().map_err(|e| {
            anyhow::anyhow!("Cache lock error: {}", e)
        })?;
        cache.clear();
        Ok(())
    }

    pub fn stats(&self) -> Result<CacheStats> {
        let hits = *self.hits.lock().unwrap();
        let misses = *self.misses.lock().unwrap();
        let size = self.cache.lock().map_err(|e| {
            anyhow::anyhow!("Cache lock error: {}", e)
        })?.len();

        Ok(CacheStats {
            hits,
            misses,
            hit_rate: if hits + misses > 0 {
                hits as f64 / (hits + misses) as f64
            } else {
                0.0
            },
            size,
            max_size: self.max_size,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub size: usize,
    pub max_size: usize,
}

impl Default for QueryPlanCache {
    fn default() -> Self {
        Self::new(5000)  // 5000 cached plans by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_put_get() {
        let cache = QueryPlanCache::new(100);
        let plan = CachedQueryPlan {
            signature: "test_query".to_string(),
            sql_template: "SELECT * FROM users".to_string(),
            parameters: vec![],
            created_at: 0,
            hit_count: 0,
        };

        cache.put("test_query".to_string(), plan.clone()).unwrap();
        let retrieved = cache.get("test_query").unwrap().unwrap();

        assert_eq!(retrieved.signature, "test_query");
    }

    #[test]
    fn test_cache_hit_counting() {
        let cache = QueryPlanCache::new(100);
        let plan = CachedQueryPlan {
            signature: "test".to_string(),
            sql_template: "SELECT *".to_string(),
            parameters: vec![],
            created_at: 0,
            hit_count: 0,
        };

        cache.put("test".to_string(), plan).unwrap();

        // Access 5 times
        for _ in 0..5 {
            cache.get("test").unwrap();
        }

        let stats = cache.stats().unwrap();
        assert_eq!(stats.hits, 5);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = QueryPlanCache::new(3);

        for i in 0..5 {
            let plan = CachedQueryPlan {
                signature: format!("query_{}", i),
                sql_template: "SELECT *".to_string(),
                parameters: vec![],
                created_at: 0,
                hit_count: 0,
            };
            cache.put(format!("query_{}", i), plan).unwrap();
        }

        let stats = cache.stats().unwrap();
        assert_eq!(stats.size, 3);  // Only 3 entries (LRU eviction)
    }
}
```

---

### Step 3: Create Query Signature Generator

**File**: `fraiseql_rs/src/cache/signature.rs` (NEW)

```rust
//! Query signature generation for caching.

use crate::graphql::types::{ParsedQuery, FieldSelection};
use sha2::{Sha256, Digest};

/// Generate cache key from GraphQL query.
pub fn generate_signature(parsed_query: &ParsedQuery) -> String {
    // Create string representation of query structure (ignoring variables and literals)
    let structure = build_query_structure(parsed_query);

    // Hash the structure to get a short signature
    let mut hasher = Sha256::new();
    hasher.update(&structure);
    let hash = hasher.finalize();

    format!("{:x}", hash)
}

/// Build structural representation (variables → placeholders).
fn build_query_structure(parsed_query: &ParsedQuery) -> String {
    let mut parts = vec![];

    parts.push(format!("op:{}", parsed_query.operation_type));
    parts.push(format!("root:{}", parsed_query.root_field));

    // Include field structure (nested fields)
    for selection in &parsed_query.selections {
        parts.push(build_selection_structure(selection));
    }

    // Include variable names (not values)
    for variable in &parsed_query.variables {
        parts.push(format!("var:{}", variable.name));
    }

    parts.join("|")
}

fn build_selection_structure(selection: &FieldSelection) -> String {
    let mut parts = vec![format!("field:{}", selection.name)];

    // Include argument names (not values)
    for arg in &selection.arguments {
        parts.push(format!("arg:{}", arg.name));
    }

    // Recurse for nested fields
    for nested in &selection.nested_fields {
        parts.push(build_selection_structure(nested));
    }

    format!("({})", parts.join("|"))
}

/// Check if query is suitable for caching.
pub fn is_cacheable(parsed_query: &ParsedQuery) -> bool {
    // Cacheable if:
    // 1. No variables (fully static query)
    // 2. All arguments are literal values (not variables)

    // For now, simple heuristic: cache if no variables defined
    parsed_query.variables.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_generation() {
        // Create two identical queries
        let query1 = create_test_query("query { users { id } }");
        let query2 = create_test_query("query { users { id } }");

        let sig1 = generate_signature(&query1);
        let sig2 = generate_signature(&query2);

        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_different_signatures() {
        let query1 = create_test_query("query { users { id } }");
        let query2 = create_test_query("query { posts { id } }");

        let sig1 = generate_signature(&query1);
        let sig2 = generate_signature(&query2);

        assert_ne!(sig1, sig2);
    }

    fn create_test_query(query_str: &str) -> ParsedQuery {
        // Simplified for testing
        ParsedQuery {
            operation_type: "query".to_string(),
            operation_name: None,
            root_field: "users".to_string(),
            selections: vec![],
            variables: vec![],
            source: query_str.to_string(),
        }
    }
}
```

---

### Step 4: Integrate Cache into Query Builder

**File**: `fraiseql_rs/src/query/mod.rs` (MODIFY)

```rust
// Add to module
pub mod cache;
pub mod signature;

use crate::cache::QueryPlanCache;
use lazy_static::lazy_static;

lazy_static! {
    static ref QUERY_PLAN_CACHE: QueryPlanCache = QueryPlanCache::new(5000);
}

/// Build SQL query with caching.
#[pyfunction]
pub fn build_sql_query_cached(
    py: Python,
    parsed_query: ParsedQuery,
    schema_json: String,
) -> PyResult<Py<PyAny>> {
    use pyo3_asyncio::tokio;

    tokio::future_into_py(py, async move {
        // Generate query signature
        let signature = crate::cache::signature::generate_signature(&parsed_query);

        // Check cache
        if let Ok(Some(cached_plan)) = QUERY_PLAN_CACHE.get(&signature) {
            // Cache hit - return cached plan
            return Ok(GeneratedQuery {
                sql: cached_plan.sql_template,
                parameters: Vec::new(),  // Parameters already bound
            });
        }

        // Cache miss - build query normally
        let schema: SchemaMetadata = serde_json::from_str(&schema_json)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        let composer = SQLComposer::new(schema);
        let composed = composer.compose(&parsed_query)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let result = GeneratedQuery {
            sql: composed.sql.clone(),
            parameters: composed.parameters.into_iter()
                .map(|(name, value)| {
                    let value_str = match value {
                        where_builder::ParameterValue::String(s) => s,
                        where_builder::ParameterValue::Integer(i) => i.to_string(),
                        where_builder::ParameterValue::Float(f) => f.to_string(),
                        where_builder::ParameterValue::Boolean(b) => b.to_string(),
                        where_builder::ParameterValue::JsonObject(s) => s,
                        where_builder::ParameterValue::Array(_) => "[]".to_string(),
                    };
                    (name, value_str)
                })
                .collect(),
        };

        // Store in cache
        let _ = QUERY_PLAN_CACHE.put(
            signature,
            crate::cache::CachedQueryPlan {
                signature: signature.clone(),
                sql_template: composed.sql,
                parameters: vec![],
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                hit_count: 0,
            },
        );

        Ok(result)
    })
}

/// Get cache statistics.
#[pyfunction]
pub fn get_cache_stats(py: Python) -> PyResult<Py<PyAny>> {
    let stats = QUERY_PLAN_CACHE.stats()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("hits", stats.hits)?;
    dict.set_item("misses", stats.misses)?;
    dict.set_item("hit_rate", stats.hit_rate)?;
    dict.set_item("cached_plans", stats.size)?;
    dict.set_item("max_cached_plans", stats.max_size)?;

    Ok(dict.into())
}

/// Clear cache (for schema changes).
#[pyfunction]
pub fn clear_cache() -> PyResult<()> {
    QUERY_PLAN_CACHE.clear()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))
}
```

---

### Step 5: Python Integration

**File**: `src/fraiseql/core/query_builder.py` (MODIFY)

```python
"""Rust-based SQL query builder with caching."""

from fraiseql._fraiseql_rs import (
    build_sql_query_cached,
    get_cache_stats,
    clear_cache as rust_clear_cache,
)


class RustQueryBuilder:
    """SQL query builder with caching."""

    async def build(
        self,
        parsed_query: ParsedQuery,
        schema_metadata: dict,
    ) -> GeneratedQuery:
        """Build query with caching."""
        schema_json = self._serialize_schema(schema_metadata)
        return await build_sql_query_cached(parsed_query, schema_json)

    @staticmethod
    def get_stats() -> dict:
        """Get cache statistics."""
        return get_cache_stats()

    @staticmethod
    def clear_cache():
        """Clear query plan cache."""
        return rust_clear_cache()

    @staticmethod
    def _serialize_schema(metadata: dict) -> str:
        import json
        return json.dumps(metadata)
```

---

### Step 6: Cache Invalidation Hook

**File**: `src/fraiseql/fastapi/app.py` (MODIFY)

```python
# When schema is updated, clear cache
def update_schema(new_schema):
    """Update schema and clear query plan cache."""
    # ... update schema ...

    # Clear Rust query cache
    from fraiseql.core.query_builder import RustQueryBuilder
    RustQueryBuilder.clear_cache()
```

---

### Step 7: Monitoring Middleware

**File**: `src/fraiseql/fastapi/middleware.py` (NEW)

```python
"""Middleware for cache statistics."""

from starlette.middleware.base import BaseHTTPMiddleware
from fraiseql.core.query_builder import RustQueryBuilder
import logging

logger = logging.getLogger(__name__)


class CacheStatsMiddleware(BaseHTTPMiddleware):
    """Log cache statistics periodically."""

    async def dispatch(self, request, call_next):
        response = await call_next(request)

        # Log stats every 100 requests
        if hasattr(self, 'request_count'):
            self.request_count += 1
        else:
            self.request_count = 1

        if self.request_count % 100 == 0:
            stats = RustQueryBuilder.get_stats()
            logger.info(
                f"Query cache stats: "
                f"hits={stats['hits']}, "
                f"misses={stats['misses']}, "
                f"hit_rate={stats['hit_rate']:.1%}, "
                f"cached={stats['cached_plans']}/{stats['max_cached_plans']}"
            )

        return response
```

---

### Step 8: Tests

**File**: `tests/test_query_caching.py` (NEW)

```python
"""Tests for query plan caching."""

import pytest
from fraiseql.core.graphql_parser import RustGraphQLParser
from fraiseql.core.query_builder import RustQueryBuilder


@pytest.fixture
def parser():
    return RustGraphQLParser()


@pytest.fixture
def builder():
    RustQueryBuilder.clear_cache()  # Clean slate
    return RustQueryBuilder()


@pytest.fixture
def test_schema():
    return {
        "tables": {
            "v_users": {
                "view_name": "v_users",
                "sql_columns": ["id", "email"],
                "jsonb_column": "data",
                "fk_mappings": {},
                "has_jsonb_data": True
            }
        },
        "types": {}
    }


@pytest.mark.asyncio
async def test_cache_hit(parser, builder, test_schema):
    """Test that identical queries hit cache."""
    query = "query { users { id } }"

    # First query - cache miss
    parsed1 = await parser.parse(query)
    result1 = await builder.build(parsed1, test_schema)

    stats_before = RustQueryBuilder.get_stats()
    misses_before = stats_before['misses']

    # Second identical query - cache hit
    parsed2 = await parser.parse(query)
    result2 = await builder.build(parsed2, test_schema)

    stats_after = RustQueryBuilder.get_stats()

    # Verify cache hit
    assert stats_after['hits'] > 0
    assert result1.sql == result2.sql


@pytest.mark.asyncio
async def test_cache_miss_different_query(parser, builder, test_schema):
    """Test that different queries are not cached together."""
    query1 = "query { users { id } }"
    query2 = "query { posts { id } }"

    parsed1 = await parser.parse(query1)
    result1 = await builder.build(parsed1, test_schema)

    parsed2 = await parser.parse(query2)
    result2 = await builder.build(parsed2, test_schema)

    # Different queries should generate different SQL
    assert result1.sql != result2.sql


@pytest.mark.asyncio
async def test_cache_clear(parser, builder, test_schema):
    """Test cache invalidation."""
    query = "query { users { id } }"
    parsed = await parser.parse(query)

    # Build and cache
    await builder.build(parsed, test_schema)

    stats_before = RustQueryBuilder.get_stats()
    initial_cached = stats_before['cached_plans']

    # Clear cache
    RustQueryBuilder.clear_cache()

    stats_after = RustQueryBuilder.get_stats()

    assert stats_after['cached_plans'] == 0
    assert stats_after['hits'] == 0


@pytest.mark.asyncio
async def test_cache_stats(parser, builder, test_schema):
    """Test cache statistics."""
    query = "query { users { id } }"

    for _ in range(5):
        parsed = await parser.parse(query)
        await builder.build(parsed, test_schema)

    stats = RustQueryBuilder.get_stats()

    assert stats['hits'] == 4  # 5 queries - 1 first miss
    assert stats['hit_rate'] > 0.7
```

---

## Performance Analysis

### Before Caching
```
Query 1: Parse (40µs) + Build (150µs) = 190µs
Query 2: Parse (40µs) + Build (150µs) = 190µs
Query 3: Parse (40µs) + Build (150µs) = 190µs
Total for identical queries: 570µs
```

### After Caching
```
Query 1: Parse (40µs) + Build (150µs) + Cache store (5µs) = 195µs
Query 2: Parse (40µs) + Cache lookup (1µs) = 41µs  ✓ 4.6x faster
Query 3: Parse (40µs) + Cache lookup (1µs) = 41µs  ✓ 4.6x faster
Total for identical queries: 277µs  ✓ 2x faster overall
```

### Real-World Workload (Typical SaaS App)

Assuming 60% query pattern repetition:
- 100 requests/second
- 40 repeated patterns (cache hits)
- 60 unique patterns (cache misses)

**Without cache**: 100 × 190µs = 19ms total
**With cache**: (40 × 41µs) + (60 × 195µs) = 12.8ms total
**Improvement**: 1.5x

With higher repetition (80%):
**Improvement**: 3-4x

---

## Verification Checklist

- [ ] Cache stores/retrieves plans correctly
- [ ] Hit rate measured and > 60%
- [ ] LRU eviction works (max 5000 plans)
- [ ] Cache cleared on schema update
- [ ] Statistics endpoint works
- [ ] Memory usage reasonable (< 100MB)
- [ ] All 5991+ tests pass
- [ ] Performance tests confirm 5-10x gain
- [ ] Thread-safe under concurrent access

---

## Success Metrics

**Cache Hit Rate**: Target 60-80% in typical workloads

**Memory Usage**: < 100MB for 5000 cached plans

**Performance**: Cached lookup < 1µs vs. building from scratch 150µs

**Impact**: 1.5-4x overall speedup depending on query pattern repetition

---

## Files Created/Modified

| File | Status | Purpose |
|------|--------|---------|
| `fraiseql_rs/Cargo.toml` | Modified | Add cache dependencies |
| `fraiseql_rs/src/cache/mod.rs` | New | Cache implementation |
| `fraiseql_rs/src/cache/signature.rs` | New | Signature generation |
| `fraiseql_rs/src/query/mod.rs` | Modified | Integrate caching |
| `src/fraiseql/core/query_builder.py` | Modified | Cache interface |
| `src/fraiseql/fastapi/middleware.py` | New | Stats monitoring |
| `src/fraiseql/fastapi/app.py` | Modified | Cache invalidation |
| `tests/test_query_caching.py` | New | Cache tests |

---

## Next Steps

- **Phase 9**: Full integration - simplify Python interface to single Rust call

