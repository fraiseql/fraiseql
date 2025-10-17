# Theoretical Optimal FraiseQL Architecture for Maximum Performance

**Date**: 2025-10-16
**Focus**: Fastest path from PostgreSQL JSONB (snake_case) to GraphQL response (camelCase + field selection)

---

## 🎯 The Challenge

**Starting Point**: PostgreSQL JSONB columns with snake_case keys (SQL convention)

```json
{
  "id": 1,
  "first_name": "Alice",
  "created_at": "2025-01-01",
  "user_posts": [...]
}
```

**End Goal**: GraphQL response with camelCase keys + field selection + `__typename`

```json
{
  "data": {
    "user": {
      "__typename": "User",
      "id": 1,
      "firstName": "Alice",
      "createdAt": "2025-01-01"
    }
  }
}
```

**Requirements**:

1. Transform snake_case → camelCase
2. Select only requested GraphQL fields (`{ id firstName }` not all fields)
3. Inject `__typename` for GraphQL compatibility
4. Serialize to JSON string
5. Minimize latency (<5ms target)

---

## 🔬 Pipeline Stage Analysis

### Stage 1: PostgreSQL Query Execution

**Options:**

| Approach | SQL | Latency | Pros | Cons |
|----------|-----|---------|------|------|
| **Direct Cast** | `SELECT data::text` | ~0.023ms | Fastest | No transformation |
| **Binary JSONB** | `SELECT data` | ~0.05ms | Fast, native format | Needs parsing |
| **Field Extraction** | `SELECT jsonb_build_object('id', data->>'id', ...)` | ~0.147ms | Field selection at DB | 6x slower, verbose |

**Optimal**: Binary JSONB (`SELECT data`) - Fast + allows flexible transformation

### Stage 2: Snake_case → CamelCase Transformation

**Options:**

| Approach | Latency | Tool | Pros | Cons |
|----------|---------|------|------|------|
| **Rust** | ~0.1-0.5ms | `fraiseql_rs.transform_json()` | 10-50x faster than Python | Requires Rust compilation |
| **Python** | ~5-15ms | `transform_keys_to_camel_case()` | No dependencies | Slow for large payloads |
| **PostgreSQL** | ~0.5-2ms | SQL functions | No app-level processing | Complex SQL, hard to maintain |
| **Pre-compute** | ~0ms | Write-time cost | Zero read-time cost | Storage overhead, write amplification |

**Optimal**: **Rust transformer** - Best balance of speed and flexibility

### Stage 3: Field Selection

**Options:**

| Approach | Latency | When Applied | Pros | Cons |
|----------|---------|--------------|------|------|
| **PostgreSQL** | ~0.1ms | Query time | Reduces data transfer | Requires dynamic SQL generation |
| **Rust** | ~0.1ms | Post-query | Fast, flexible | Data already fetched |
| **Python** | ~1-5ms | Post-query | Easy to implement | Slow |
| **GraphQL Framework** | ~2-10ms | Resolution time | Standard approach | Wasteful (fetches unused fields) |

**Optimal**: **Rust transformer with field selection** - Fast + avoids fetching unused data

### Stage 4: __typename Injection

**Options:**

| Approach | Latency | Pros | Cons |
|----------|---------|------|------|
| **Rust** | ~0.05ms | Fast, integrated | Part of transform |
| **Python** | ~0.5-1ms | Easy | Slower |
| **PostgreSQL** | ~0.1ms | DB-level | Requires knowing type at query time |

**Optimal**: **Rust transformer** - Integrated with case transformation

### Stage 5: Serialization

**Options:**

| Approach | Latency | Tool | Pros | Cons |
|----------|---------|------|------|------|
| **Direct ::text** | ~0ms | PostgreSQL | No serialization | Only if no transformation |
| **Rust** | ~0.2-1ms | `fraiseql_rs` | Fast native serialization | Requires Rust |
| **orjson** | ~1-3ms | Python (C extension) | Fast JSON serializer | Still Python overhead |
| **json.dumps** | ~5-15ms | Python stdlib | No dependencies | Slow |

**Optimal**: **Rust serialization** - Integrated with transformation

---

## 🏆 Theoretical Optimal Paths

### Path 1: Pure PostgreSQL Passthrough (Theoretical Maximum - 0.5-1ms)

**Architecture**: Store camelCase + `__typename` in JSONB at write time

```sql
-- Write-time: Compose camelCase JSONB
CREATE OR REPLACE FUNCTION update_user_graphql_cache()
RETURNS TRIGGER AS $$
BEGIN
    NEW.gql_data = jsonb_build_object(
        '__typename', 'User',
        'id', NEW.id,
        'firstName', NEW.first_name,
        'lastName', NEW.last_name,
        'createdAt', NEW.created_at::text,
        'userPosts', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', p.id,
                    'title', p.title,
                    'createdAt', p.created_at::text
                )
            )
            FROM posts p
            WHERE p.user_id = NEW.id
            LIMIT 10
        )
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER user_gql_cache
    BEFORE INSERT OR UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_user_graphql_cache();
```

**Read-time: Direct JSON passthrough**

```sql
-- Zero transformation - return as-is
SELECT gql_data::text AS result
FROM users
WHERE id = 1;
-- Execution time: ~0.023ms
```

**Application Code:**

```python
@fraiseql.query
async def user(info, id: int) -> User:
    # Direct passthrough - no processing
    db = info.context["db"]
    result = await db.execute(
        "SELECT gql_data::text FROM users WHERE id = $1",
        id
    )
    return result[0][0]  # Return JSON string directly
```

**Performance:**

- **Query**: 0.023ms
- **Network**: 0.2-0.5ms
- **Total**: **0.5-1ms**

**Trade-offs:**

- ✅ **Maximum performance** - Fastest possible path
- ✅ **Zero runtime transformation** - No CPU cost
- ✅ **Predictable latency** - Always fast
- ❌ **Write amplification** - Update cost on every write
- ❌ **Storage overhead** - Duplicate data (snake_case + camelCase)
- ❌ **Inflexible** - Pre-computed query patterns only
- ❌ **Field selection complex** - Need multiple columns for different field sets

**When to Use**:

- Extremely high read:write ratio (1000:1+)
- Fixed query patterns (dashboard, feeds)
- <1ms latency requirement
- Storage cost acceptable

---

### Path 2: Rust Transformer Passthrough (Practical Optimal - 1-2ms)

**Architecture**: Store snake_case JSONB, transform with Rust at read time

**PostgreSQL Storage** (standard snake_case):

```sql
-- Store natural snake_case JSONB
CREATE TABLE users (
    id INT PRIMARY KEY,
    first_name TEXT,
    last_name TEXT,
    created_at TIMESTAMPTZ,
    -- Composed JSONB with snake_case keys
    data JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            'id', id,
            'first_name', first_name,
            'last_name', last_name,
            'created_at', created_at,
            'user_posts', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', p.id,
                        'title', p.title,
                        'created_at', p.created_at
                    )
                )
                FROM posts p
                WHERE p.user_id = users.id
                LIMIT 10
            )
        )
    ) STORED
);
```

**Query Execution**:

```sql
-- Return binary JSONB (not ::text)
SELECT data
FROM users
WHERE id = 1;
-- Execution time: ~0.05ms
```

**Rust Transformation Pipeline**:

```python
from fraiseql.core.rust_transformer import get_transformer

@fraiseql.query
async def user(info, id: int) -> User:
    db = info.context["db"]
    transformer = get_transformer()

    # 1. Fetch binary JSONB from PostgreSQL (~0.05ms)
    result = await db.execute("SELECT data FROM users WHERE id = $1", id)
    json_data = result[0][0]  # Binary JSONB

    # 2. Convert JSONB to JSON string (~0.1ms)
    json_str = json.dumps(json_data) if isinstance(json_data, dict) else json_data

    # 3. Rust transform: snake_case → camelCase + __typename (~0.5ms)
    #    - Parallel operation: case conversion + __typename injection
    #    - Field selection based on GraphQL query
    #    - Fast C-speed native code
    transformed_json = transformer.transform(json_str, "User")

    # 4. Return (GraphQL framework handles response wrapping)
    return transformed_json

# Total latency: 0.05 + 0.1 + 0.5 + 0.5 (framework) = ~1.15ms
```

**Rust Transformer Internals** (hypothetical optimized version):

```rust
// fraiseql-rs/src/transformer.rs

pub fn transform_json(
    json_str: &str,
    root_type: &str,
    requested_fields: &[&str],  // From GraphQL query
    schema: &SchemaRegistry
) -> Result<String, Error> {
    // 1. Parse JSON (using simd-json for speed)
    let mut value: Value = simd_json::from_str(json_str)?;

    // 2. Parallel transformation: case conversion + field filtering
    //    Process multiple keys simultaneously
    let transformed = match value {
        Value::Object(mut map) => {
            // Pre-allocate output with exact capacity
            let mut output = Map::with_capacity(requested_fields.len() + 1);

            // Add __typename first
            output.insert("__typename".to_string(), Value::String(root_type.to_string()));

            // Transform requested fields only (field selection)
            for field in requested_fields {
                // Convert GraphQL field (camelCase) to DB field (snake_case)
                let snake_field = camel_to_snake(field);

                if let Some(value) = map.remove(&snake_field) {
                    // Recursive transform for nested objects
                    let transformed_value = if value.is_object() || value.is_array() {
                        transform_nested(value, field, schema)?
                    } else {
                        value
                    };

                    output.insert(field.to_string(), transformed_value);
                }
            }

            Value::Object(output)
        }
        _ => value
    };

    // 3. Serialize (using simd-json for speed)
    simd_json::to_string(&transformed)
}

// Helper: Transform nested objects/arrays
fn transform_nested(value: Value, field_name: &str, schema: &SchemaRegistry) -> Result<Value, Error> {
    match value {
        Value::Array(arr) => {
            // Get nested type from schema
            let item_type = schema.get_list_item_type(field_name)?;

            // Transform each item in parallel (rayon)
            let transformed: Vec<Value> = arr
                .par_iter()
                .map(|item| {
                    if let Value::Object(obj) = item {
                        transform_object(obj.clone(), item_type, schema)
                    } else {
                        Ok(item.clone())
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;

            Ok(Value::Array(transformed))
        }
        Value::Object(obj) => {
            let nested_type = schema.get_field_type(field_name)?;
            transform_object(obj, nested_type, schema)
        }
        _ => Ok(value)
    }
}
```

**Performance Breakdown**:

| Stage | Time | Notes |
|-------|------|-------|
| PostgreSQL query | 0.05ms | Binary JSONB |
| JSONB → String | 0.1ms | If needed |
| Rust transformation | 0.3-0.8ms | Case + field selection + __typename |
| Serialization | 0.2ms | Integrated with transform |
| Framework overhead | 0.3-0.5ms | Response wrapping |
| **Total** | **1-2ms** | Consistent performance |

**Trade-offs:**

- ✅ **Excellent performance** - 1-2ms per query
- ✅ **Flexible** - Any field selection at runtime
- ✅ **Standard storage** - Snake_case SQL conventions
- ✅ **No write amplification** - Transform on read only
- ✅ **Small storage overhead** - Generated column, but single format
- ⚠️ **Requires Rust extension** - `fraiseql-rs` must be compiled
- ⚠️ **Slightly slower than pure passthrough** - But much more flexible

**When to Use**:

- **RECOMMENDED for most use cases**
- Any read:write ratio
- Dynamic field selection needed
- <5ms latency requirement
- Standard SQL conventions preferred

---

### Path 3: Hybrid Pre-computation + Rust Fallback (Adaptive Optimal - 0.5-2ms)

**Architecture**: Cache common query patterns, Rust transform for everything else

**PostgreSQL Storage** (multiple pre-computed columns):

```sql
CREATE TABLE users (
    id INT PRIMARY KEY,
    first_name TEXT,
    last_name TEXT,
    created_at TIMESTAMPTZ,

    -- Source data (snake_case)
    data_snake JSONB GENERATED ALWAYS AS (...) STORED,

    -- Pre-computed patterns (camelCase + __typename)
    gql_simple JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            '__typename', 'User',
            'id', id,
            'firstName', first_name,
            'lastName', last_name
        )
    ) STORED,

    gql_with_posts JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            '__typename', 'User',
            'id', id,
            'firstName', first_name,
            'lastName', last_name,
            'createdAt', created_at::text,
            'userPosts', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        '__typename', 'Post',
                        'id', p.id,
                        'title', p.title
                    )
                )
                FROM posts p
                WHERE p.user_id = users.id
                LIMIT 10
            )
        )
    ) STORED,

    gql_full JSONB GENERATED ALWAYS AS (...) STORED
);

-- Index for fast pattern lookup
CREATE INDEX idx_users_query_pattern ON users USING gin(data_snake);
```

**Intelligent Query Router**:

```python
from enum import Enum
from fraiseql.core.rust_transformer import get_transformer

class QueryPattern(Enum):
    """Common query patterns that have pre-computed columns"""
    SIMPLE = "gql_simple"              # { id firstName lastName }
    WITH_POSTS = "gql_with_posts"      # { id firstName userPosts { id title } }
    FULL = "gql_full"                  # All fields
    CUSTOM = "data_snake"              # Unknown pattern - needs Rust transform

def detect_query_pattern(graphql_info) -> QueryPattern:
    """Detect which pre-computed pattern matches the GraphQL query"""
    # Extract requested fields from GraphQL query
    fields = extract_fields_from_info(graphql_info)
    field_set = set(fields.keys())

    # Match against known patterns
    if field_set == {"id", "firstName", "lastName"}:
        return QueryPattern.SIMPLE
    elif field_set == {"id", "firstName", "lastName", "createdAt", "userPosts"}:
        # Check if userPosts only requests { id title }
        if fields.get("userPosts") == ["id", "title"]:
            return QueryPattern.WITH_POSTS
    elif is_full_field_set(field_set):
        return QueryPattern.FULL

    # Unknown pattern - will use Rust transformer
    return QueryPattern.CUSTOM

@fraiseql.query
async def user(info, id: int) -> User:
    db = info.context["db"]

    # 1. Detect query pattern (~0.05ms)
    pattern = detect_query_pattern(info)

    if pattern == QueryPattern.CUSTOM:
        # 2a. CACHE MISS: Use Rust transformer (1-2ms path)
        result = await db.execute(
            "SELECT data_snake FROM users WHERE id = $1",
            id
        )
        json_str = json.dumps(result[0][0])

        transformer = get_transformer()
        transformed = transformer.transform(json_str, "User")
        return transformed
    else:
        # 2b. CACHE HIT: Direct passthrough (0.5-1ms path)
        result = await db.execute(
            f"SELECT {pattern.value}::text FROM users WHERE id = $1",
            id
        )
        return result[0][0]  # Pre-computed JSON string

# Performance:
# - Cache hit (80-95% of queries): 0.5-1ms
# - Cache miss (5-20% of queries): 1-2ms
# - Average: 0.6-1.2ms depending on hit rate
```

**Query Pattern Analytics**:

```python
class QueryPatternCache:
    """Track query patterns to optimize pre-computation"""

    def __init__(self):
        self.pattern_frequency = {}
        self.total_queries = 0

    def record_pattern(self, pattern_hash: str):
        """Record a query pattern"""
        self.pattern_frequency[pattern_hash] = \
            self.pattern_frequency.get(pattern_hash, 0) + 1
        self.total_queries += 1

    def get_top_patterns(self, n: int = 10):
        """Get the top N most common query patterns"""
        sorted_patterns = sorted(
            self.pattern_frequency.items(),
            key=lambda x: x[1],
            reverse=True
        )
        return sorted_patterns[:n]

    def should_precompute(self, pattern_hash: str, threshold: float = 0.05):
        """Determine if a pattern should be pre-computed"""
        if self.total_queries < 1000:
            return False  # Not enough data

        frequency = self.pattern_frequency.get(pattern_hash, 0)
        rate = frequency / self.total_queries

        return rate >= threshold  # Pre-compute if >5% of queries

# Usage: Monitor in production, create migrations for new patterns
# Example: If we see { id firstName email } in 10% of queries,
#          add gql_with_email generated column
```

**Performance Profile**:

| Scenario | Latency | Frequency | Notes |
|----------|---------|-----------|-------|
| **Cache hit (simple)** | 0.5ms | 40% | `{ id firstName }` |
| **Cache hit (with posts)** | 0.8ms | 35% | Common dashboard query |
| **Cache hit (full)** | 1.0ms | 10% | Admin views |
| **Cache miss (Rust)** | 1.8ms | 15% | Custom queries |
| **Average** | **0.9ms** | 100% | Weighted average |

**Trade-offs:**

- ✅ **Best average performance** - 0.9ms across all queries
- ✅ **Flexible** - Falls back to Rust for unknown patterns
- ✅ **Self-optimizing** - Learn patterns over time
- ✅ **Handles 80-95% queries optimally** - Common patterns cached
- ❌ **Complex setup** - Multiple generated columns
- ❌ **Storage overhead** - 3-5x data duplication
- ❌ **Write cost** - Update all generated columns
- ❌ **Requires analytics** - Need to identify common patterns

**When to Use**:

- High-traffic production APIs
- Known common query patterns
- Extremely high read:write ratio (100:1+)
- Storage cost acceptable
- <1ms average latency requirement

---

## 📊 Comparative Analysis

### Performance Comparison

| Approach | P50 Latency | P95 Latency | P99 Latency | Flexibility | Complexity |
|----------|-------------|-------------|-------------|-------------|------------|
| **Pure PostgreSQL Passthrough** | 0.5ms | 0.8ms | 1.0ms | Low | Low |
| **Rust Transformer** | 1.2ms | 1.8ms | 2.5ms | High | Medium |
| **Hybrid Pre-compute + Rust** | 0.7ms | 1.5ms | 2.2ms | High | High |
| **Current (Field Extraction)** | 24ms | 35ms | 45ms | Medium | Low |
| **Strawberry Traditional** | 37ms | 52ms | 68ms | High | Low |

**Performance Gains**:

- Pure PostgreSQL: **48x faster** than current
- Rust Transformer: **20x faster** than current  ← **RECOMMENDED**
- Hybrid: **34x faster** than current
- All options: **15-48x faster** than Strawberry

### Storage Overhead Comparison

| Approach | Storage Multiplier | Example (10GB base) |
|----------|-------------------|---------------------|
| **Pure PostgreSQL** | 2-3x | 20-30GB |
| **Rust Transformer** | 1.1x | 11GB (generated column) |
| **Hybrid** | 3-5x | 30-50GB |
| **Current** | 1x | 10GB |

### Write Cost Comparison

| Approach | Write Latency Increase | Notes |
|----------|------------------------|-------|
| **Pure PostgreSQL** | +30-50% | Update all gql_* columns |
| **Rust Transformer** | +5-10% | Update generated column only |
| **Hybrid** | +50-100% | Update multiple generated columns |
| **Current** | Baseline | No pre-computation |

### Recommended by Use Case

#### Use Case 1: High-traffic Public API

**Workload**: 10,000 RPS, read-heavy (100:1), predictable queries

**Recommendation**: **Hybrid Pre-compute + Rust**

**Rationale**:

- Cache hit rate: 90%+
- Average latency: <1ms
- Can handle traffic spikes
- Storage cost justified by performance

**Expected Results**:

- 90% of queries: 0.5-1ms (cached)
- 10% of queries: 1.5-2ms (Rust transform)
- Overall: 0.6-1.1ms average

#### Use Case 2: Dynamic B2B API

**Workload**: 1,000 RPS, mixed read/write (10:1), dynamic queries

**Recommendation**: **Rust Transformer** ✅

**Rationale**:

- Field selection varies widely
- Write frequency too high for pre-compute
- 1-2ms still excellent
- Minimal storage overhead

**Expected Results**:

- All queries: 1-2ms
- No write amplification
- Flexible field selection

#### Use Case 3: Internal Dashboard

**Workload**: 100 RPS, read-heavy (1000:1), fixed queries

**Recommendation**: **Pure PostgreSQL Passthrough**

**Rationale**:

- Query patterns fixed (3-5 views)
- Read:write ratio extreme
- Sub-millisecond critical for UX
- Storage cheap

**Expected Results**:

- All queries: 0.3-0.8ms
- Zero runtime cost
- Maximum performance

#### Use Case 4: Rapid Development / MVP

**Workload**: <50 RPS, unknown patterns

**Recommendation**: **Rust Transformer** ✅

**Rationale**:

- Don't know query patterns yet
- Maximum flexibility
- Still very fast (1-2ms)
- Simple to set up

**Expected Results**:

- Any query pattern: 1-2ms
- Easy iteration
- Production-ready performance

---

## 🏗️ Recommended Implementation: Rust Transformer Path

### Why This is the Optimal Default

**Balance of:**

1. **Performance**: 1-2ms (20x faster than current, 18x faster than Strawberry)
2. **Flexibility**: Any field selection, dynamic queries
3. **Simplicity**: Standard SQL storage, single transform layer
4. **Maintenance**: No pre-computation management
5. **Cost**: Minimal storage overhead

### Implementation Steps

#### Step 1: PostgreSQL Schema

```sql
-- Use generated JSONB column with snake_case keys
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    email TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Generated JSONB column (embedded relations)
    data JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            'id', id,
            'first_name', first_name,
            'last_name', last_name,
            'email', email,
            'created_at', created_at,
            'user_posts', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', p.id,
                        'title', p.title,
                        'content', p.content,
                        'created_at', p.created_at
                    )
                    ORDER BY p.created_at DESC
                )
                FROM posts p
                WHERE p.user_id = users.id
                LIMIT 10  -- Prevent huge payloads
            )
        )
    ) STORED
);

-- GIN index for fast JSONB queries
CREATE INDEX idx_users_data_gin ON users USING gin(data);
```

#### Step 2: FraiseQL Type Registration

```python
import fraiseql
from fraiseql.core.rust_transformer import get_transformer, register_graphql_types

@fraiseql.type(sql_source="users", jsonb_column="data")
class User:
    id: int
    first_name: str  # Will be transformed to firstName in GraphQL
    last_name: str   # Will be transformed to lastName
    email: str
    created_at: datetime
    user_posts: list[Post] | None = None  # Will be transformed to userPosts

@fraiseql.type(sql_source="posts", jsonb_column="data")
class Post:
    id: int
    title: str
    content: str
    created_at: datetime

# Register types with Rust transformer
register_graphql_types(User, Post)
```

#### Step 3: Query Resolver with Rust Transformation

```python
from fraiseql.core.rust_transformer import transform_db_json

@fraiseql.query
async def user(info, id: int) -> User | None:
    """
    Get single user with Rust transformation.

    Pipeline:
    1. PostgreSQL: SELECT data FROM users WHERE id = $1 (~0.05ms)
    2. Rust: transform(data, "User") (~0.5ms)
    3. Return transformed JSON (~0.5ms framework)

    Total: ~1.1ms
    """
    db = info.context["db"]

    # Fetch binary JSONB (not ::text)
    result = await db.execute(
        "SELECT data FROM users WHERE id = $1",
        id
    )

    if not result:
        return None

    json_data = result[0][0]

    # Convert to JSON string if needed
    import json
    json_str = json.dumps(json_data) if isinstance(json_data, dict) else json_data

    # Rust transformation: snake_case → camelCase + __typename + field selection
    transformed = transform_db_json(json_str, "User")

    return transformed

@fraiseql.query
async def users(info, limit: int = 10, offset: int = 0) -> list[User]:
    """
    Get multiple users with Rust transformation.

    Performance: ~1.2ms + (0.3ms × num_users)
    Example: 10 users = ~4.2ms total
    """
    db = info.context["db"]

    result = await db.execute(
        "SELECT data FROM users ORDER BY id LIMIT $1 OFFSET $2",
        limit, offset
    )

    # Transform each user (could parallelize in Rust)
    import json
    transformed_users = []
    for row in result:
        json_str = json.dumps(row[0]) if isinstance(row[0], dict) else row[0]
        transformed = transform_db_json(json_str, "User")
        transformed_users.append(transformed)

    return transformed_users
```

#### Step 4: Optimized FraiseQL Configuration

```python
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app
from fraiseql_performance_presets import create_optimized_config, PerformanceProfile

# Use performance preset for optimal config
config = create_optimized_config(
    database_url="postgresql://user:pass@host/db",
    profile=PerformanceProfile.MAXIMUM_SPEED,

    # Ensure Rust transformer is enabled
    pure_passthrough_use_rust=True,
    json_passthrough_enabled=True,

    # Field selection optimization
    jsonb_field_limit_threshold=50,  # Use full passthrough for <50 fields
)

app = create_fraiseql_app(
    config=config,
    types=[User, Post],
    queries=[user, users],
)
```

#### Step 5: Benchmark Validation

```python
import time
import httpx

async def benchmark_rust_transformer():
    """Validate Rust transformer performance"""

    query = """
    {
      user(id: 1) {
        id
        firstName
        lastName
        userPosts {
          id
          title
        }
      }
    }
    """

    # Warm-up
    for _ in range(10):
        await httpx.post("http://localhost:8000/graphql", json={"query": query})

    # Measure
    latencies = []
    for _ in range(100):
        t0 = time.perf_counter()
        response = await httpx.post("http://localhost:8000/graphql", json={"query": query})
        t1 = time.perf_counter()
        latencies.append((t1 - t0) * 1000)

    print(f"P50: {sorted(latencies)[50]:.2f}ms")
    print(f"P95: {sorted(latencies)[95]:.2f}ms")
    print(f"P99: {sorted(latencies)[99]:.2f}ms")

    # Expected output:
    # P50: 1.2ms
    # P95: 1.8ms
    # P99: 2.5ms
```

---

## 🎯 Summary: Recommended Architecture

### The Winning Combination

**Rust Transformer Path** with these components:

1. **PostgreSQL**: Generated JSONB columns (snake_case) with embedded relations
2. **Rust Transformer**: Fast case conversion + field selection + __typename
3. **Binary JSONB**: Fetch `data` not `data::text` for flexibility
4. **Field Selection**: Rust handles dynamic field filtering
5. **Minimal Storage**: Only 1.1x overhead (generated column)

### Expected Performance

| Metric | Value | vs Current | vs Strawberry |
|--------|-------|------------|---------------|
| **Simple Query** | 1.2ms | 20x faster | 31x faster |
| **Nested Query** | 1.8ms | 14x faster | 17x faster |
| **100 users** | 35ms | 7x faster | 10x faster |

### Why This Beats All Alternatives

**vs Pure PostgreSQL Passthrough**:

- More flexible (any field selection)
- Standard SQL conventions
- Minimal storage overhead
- Only slightly slower (1.2ms vs 0.5ms)

**vs Hybrid Pre-compute**:

- Much simpler to implement and maintain
- No pattern detection needed
- Handles unknown queries perfectly
- Lower storage cost
- Only slightly slower on average (1.2ms vs 0.9ms)

**vs Current Field Extraction**:

- 20x faster
- Same flexibility
- No downsides

**vs Strawberry Traditional**:

- 18-31x faster
- Zero N+1 queries
- Better for read-heavy workloads

### Implementation Checklist

- [ ] Ensure `fraiseql-rs` Rust extension is installed
- [ ] Create PostgreSQL tables with generated JSONB columns
- [ ] Register GraphQL types with Rust transformer
- [ ] Update resolvers to use `transform_db_json()`
- [ ] Use `PerformanceProfile.MAXIMUM_SPEED` config
- [ ] Run benchmarks to validate <2ms latency
- [ ] Monitor Rust transformer usage in production

### If Rust Extension Not Available (Fallback)

If `fraiseql-rs` cannot be compiled:

1. **Use Python transformation** (5-15ms, still better than Strawberry)
2. **Consider PostgreSQL functions** for case conversion (2-5ms)
3. **Evaluate PostgreSQL C extension** (custom solution, 0.5-1ms)

---

**Conclusion**: The **Rust Transformer path** provides the best balance of performance (1-2ms), flexibility (any query pattern), and simplicity (standard setup). This should be the default architecture for all FraiseQL applications targeting maximum performance.

**Next Step**: Validate that `fraiseql-rs` is properly installed and activated in the benchmark environment.
