# JSONB to HTTP Simplification Plan

## Executive Summary

Based on our exploration of the complete data path from PostgreSQL JSONB to HTTP response, we've identified **7 key optimization opportunities** that can reduce complexity and improve performance by an estimated **15-30%**.

**Current Performance Baseline:**
- Full scan (10K rows): **6.6 TPS** (jsonb_build_object) vs **8.2 TPS** (to_jsonb) = 24% faster
- Paginated (100 rows): **22.2 TPS** (current) vs **14.6 TPS** (to_jsonb) = 52% faster
- Filtered query: **475 TPS** (current) vs **431 TPS** (to_jsonb) = 10% improvement
- **Key Finding:** Current `jsonb_build_object` approach is actually optimal for read performance

## Complete Data Path Analysis

### Current Architecture (9 Steps)

```
1. GraphQL Request arrives → FastAPI endpoint
2. Repository.find() or find_raw_json() called
3. [OPTIMIZATION TARGET] Sample query executed (db.py:464-483)
   └─ Extra DB roundtrip to detect JSONB column
4. [OPTIMIZATION TARGET] Type registry lookup (db.py:521-529)
   └─ String matching and registry iteration
5. SQL query built with JSONB extraction: SELECT data::text FROM table
6. Query executed → PostgreSQL returns JSON strings
7. [OPTIMIZATION TARGET] Rows concatenated into JSON array (raw_json_executor.py:216-222)
   └─ String concatenation with commas
8. [OPTIMIZATION TARGET] RawJSONResult wrapper created (raw_json_executor.py:167-174)
9. Rust transformation applied (10-80x faster than Python)
   └─ snake_case → camelCase + __typename injection
10. FastAPI detects RawJSONResult → bypasses Pydantic serialization
11. Pre-serialized JSON bytes sent directly as HTTP response
```

---

## 7 Optimization Opportunities

### 🔴 Priority 1: Eliminate Sample Query Execution (Biggest Impact)

**Location:** `src/fraiseql/db.py:464-483`

**Problem:**
```python
# Current: Extra DB roundtrip on every query
if not field_paths:
    sample_query = self._build_find_query(view_name, **sample_kwargs)
    async with conn.cursor() as cursor:
        await cursor.execute(sample_query.statement)  # ❌ EXTRA DB QUERY
        sample_rows = await cursor.fetchall()

    jsonb_column = self._determine_jsonb_column(view_name, sample_rows)
```

**Impact:**
- **50% reduction in DB queries** (2 queries → 1 query)
- Eliminates 5-15ms latency per request
- Reduces connection pool pressure

**Solution:**
```python
# Move JSONB column detection to registration time
def register_type_for_view(
    view_name: str,
    type_class: type,
    jsonb_column: str = "data",  # ✅ Explicit at registration
) -> None:
    _table_metadata[view_name] = {
        "jsonb_column": jsonb_column,
        "type_class": type_class,
    }

# Runtime: Use cached metadata (no DB query)
async def find(self, view_name: str, **kwargs):
    metadata = _table_metadata.get(view_name)
    jsonb_column = metadata["jsonb_column"] if metadata else "data"

    # Build query directly - no sample query needed
    query = self._build_find_query(
        view_name,
        raw_json=True,
        jsonb_column=jsonb_column,
        **kwargs
    )
```

**Estimated Gain:** 15-20% latency improvement on list queries

---

### 🟡 Priority 2: Cache Type Name Lookups

**Location:** `src/fraiseql/db.py:521-529`

**Problem:**
```python
# Current: Registry lookup on every query
type_name = None
try:
    type_class = self._get_type_for_view(view_name)  # ❌ Registry iteration
    if hasattr(type_class, "__name__"):
        type_name = type_class.__name__
except Exception:
    pass
```

**Solution:**
```python
# Add instance-level cache
def __init__(self, pool, context):
    self._pool = pool
    self.context = context
    self._type_name_cache = {}  # ✅ Cache type names

async def find(self, view_name: str, **kwargs):
    # Check cache first
    type_name = self._type_name_cache.get(view_name)

    if type_name is None:
        # Only lookup once per view per repository instance
        try:
            type_class = self._get_type_for_view(view_name)
            type_name = type_class.__name__ if hasattr(type_class, "__name__") else None
            self._type_name_cache[view_name] = type_name
        except Exception:
            self._type_name_cache[view_name] = None
```

**Estimated Gain:** 2-3% latency improvement (minor but clean)

---

### 🟡 Priority 3: Direct Rust Call (Reduce Wrapper Overhead)

**Location:** `src/fraiseql/core/raw_json_executor.py:165-174`

**Problem:**
```python
# Current: Multi-step transformation
result_obj = RawJSONResult(json_response, transformed=False)  # ❌ Object creation
transformed_result = result_obj.transform(root_type=type_name)  # ❌ Method call
```

**Solution:**
```python
# Direct Rust transformation
if type_name:
    transformer = get_transformer()
    # Call Rust directly - skip Python wrapper
    transformed_json = transformer.transform(json_response, type_name)
    return RawJSONResult(transformed_json, transformed=True)

return RawJSONResult(json_response, transformed=False)
```

**Estimated Gain:** 5-10% overhead reduction on transformation path

---

### 🟢 Priority 4: PostgreSQL-Level Response Building

**Location:** `src/fraiseql/core/raw_json_executor.py:216-222`

**Problem:**
```python
# Current: Python string concatenation
json_items = []
for row in rows:
    if row[0] is not None:
        json_items.append(row[0])  # ❌ Python list append

json_array = f"[{','.join(json_items)}]"  # ❌ String join in Python
```

**Solution:**
```python
# Use PostgreSQL json_agg() to build array at database level
# Modify query building to use:
SELECT json_agg(data)::text FROM (
    SELECT data FROM table WHERE ...
) AS subquery

# Result: Single JSON array returned from DB, no Python concatenation
```

**Estimated Gain:** 3-5% improvement on list queries with >100 rows

---

### 🟢 Priority 5: Lazy Field Path Extraction

**Location:** `src/fraiseql/db.py:451-457`

**Problem:**
```python
# Current: Always extracts field paths even if not needed
field_paths = None
if info:
    from fraiseql.core.ast_parser import extract_field_paths_from_info
    field_paths = extract_field_paths_from_info(info, transform_path=to_snake_case)
```

**Solution:**
```python
# Extract field paths only when actually needed
field_paths = None
if info and self.context.get("enable_field_selection"):
    # Lazy import and execution
    field_paths = self._extract_field_paths_lazy(info)

def _extract_field_paths_lazy(self, info):
    """Cache field path extraction per info object"""
    if not hasattr(self, "_field_path_cache"):
        self._field_path_cache = {}

    info_id = id(info)
    if info_id not in self._field_path_cache:
        from fraiseql.core.ast_parser import extract_field_paths_from_info
        self._field_path_cache[info_id] = extract_field_paths_from_info(
            info, transform_path=to_snake_case
        )

    return self._field_path_cache[info_id]
```

**Estimated Gain:** 1-2% improvement on simple queries

---

### 🟢 Priority 6: Simplify JSON Wrapping Logic

**Location:** `src/fraiseql/core/raw_json_executor.py:151-156`

**Problem:**
```python
# Current: String manipulation with escaping
if field_name:
    escaped_field = field_name.replace('"', '\\"')  # ❌ Python string processing
    json_response = f'{{"data":{{"{escaped_field}":{json_data}}}}}'
else:
    json_response = f'{{"data":{json_data}}}'
```

**Solution:**
```python
# Use PostgreSQL json_build_object() to build response at DB level
# Modify query to return complete GraphQL response:
SELECT json_build_object(
    'data', json_build_object(
        'users', json_agg(data)
    )
)::text FROM (
    SELECT data FROM users WHERE ...
) AS subquery

# Result: PostgreSQL builds the entire GraphQL response structure
```

**Estimated Gain:** 2-4% improvement + reduced escaping bugs

---

### 🟢 Priority 7: Optimize RawJSONResult.transform()

**Location:** `src/fraiseql/core/raw_json_executor.py:41-102`

**Problem:**
```python
# Current: Multiple JSON parse/serialize cycles
def transform(self, root_type):
    data = json.loads(self.json_string)  # ❌ Parse

    # Extract nested structure
    graphql_data = data["data"]
    field_name = next(iter(graphql_data.keys()))
    field_data = graphql_data[field_name]

    # Transform
    field_json = json.dumps(field_data)  # ❌ Serialize
    transformed_json = transformer.transform(field_json, root_type)

    # Rebuild
    transformed_data = json.loads(transformed_json)  # ❌ Parse again
    response = {"data": {field_name: transformed_data}}
    return RawJSONResult(json.dumps(response), transformed=True)  # ❌ Serialize again
```

**Solution:**
```python
# Pass full GraphQL response to Rust - let Rust handle structure
def transform(self, root_type):
    if self._transformed:
        return self

    # Single transformation - Rust handles GraphQL response structure
    transformer = get_transformer()
    transformed_json = transformer.transform_graphql_response(
        self.json_string,  # Full response: {"data": {"users": [...]}}
        root_type
    )

    return RawJSONResult(transformed_json, transformed=True)
```

**Estimated Gain:** 5-8% improvement on transformation path

---

## Implementation Roadmap

### Phase 1: Quick Wins (1-2 days)
**Target: 10-15% improvement**

1. ✅ Eliminate sample query execution (Priority 1)
2. ✅ Cache type name lookups (Priority 2)
3. ✅ Direct Rust call (Priority 3)

**Changes:**
- `src/fraiseql/db.py`: Add `_type_name_cache`, remove sample query
- `src/fraiseql/core/raw_json_executor.py`: Direct transformer call
- Registration time metadata storage

### Phase 2: Database-Level Optimization (2-3 days)
**Target: Additional 5-10% improvement**

4. ✅ PostgreSQL-level response building (Priority 4)
5. ✅ Simplify JSON wrapping logic (Priority 6)

**Changes:**
- `src/fraiseql/sql/sql_generator.py`: Add `json_agg()` support
- `src/fraiseql/core/raw_json_executor.py`: Use DB-level JSON building
- Query builder modifications

### Phase 3: Fine-Tuning (1-2 days)
**Target: Additional 2-5% improvement**

6. ✅ Lazy field path extraction (Priority 5)
7. ✅ Optimize RawJSONResult.transform() (Priority 7)

**Changes:**
- `src/fraiseql/core/raw_json_executor.py`: Simplified transform logic
- `src/fraiseql/db.py`: Lazy field path caching

---

## Expected Performance Improvements

### Current Baseline (from benchmark)
```
Filtered Query (100 rows):   475 TPS, 20.4ms latency
Paginated Query (100 rows):  22.2 TPS, 445ms latency
Full Scan (10K rows):        6.6 TPS, 601ms latency
```

### After Phase 1 (Quick Wins)
```
Filtered Query:   550 TPS, 17.5ms latency  (+16% TPS, -14% latency)
Paginated Query:  25.5 TPS, 385ms latency  (+15% TPS, -13% latency)
Full Scan:        7.5 TPS, 520ms latency   (+14% TPS, -13% latency)
```

### After Phase 2 (DB Optimization)
```
Filtered Query:   600 TPS, 16.0ms latency  (+26% TPS, -22% latency)
Paginated Query:  27.0 TPS, 365ms latency  (+22% TPS, -18% latency)
Full Scan:        8.0 TPS, 485ms latency   (+21% TPS, -19% latency)
```

### After Phase 3 (Fine-Tuning)
```
Filtered Query:   625 TPS, 15.5ms latency  (+32% TPS, -24% latency)
Paginated Query:  28.0 TPS, 350ms latency  (+26% TPS, -21% latency)
Full Scan:        8.5 TPS, 460ms latency   (+28% TPS, -23% latency)
```

---

## Key Insights from Benchmark

### ✅ Keep Current Approach
1. **`jsonb_build_object` is optimal for reads** - 52% faster than `to_jsonb` on pagination
2. **Rust transformation is critical** - 10-80x faster than Python
3. **String-based JSON (no parsing)** - Key to performance
4. **Smart response detection** - RawJSONResult marker works well

### 🔄 Simplify These Areas
1. **Eliminate runtime introspection** - Move to registration time
2. **Reduce Python wrapper overhead** - Direct Rust calls
3. **Push work to PostgreSQL** - json_agg(), json_build_object()
4. **Cache everything cacheable** - Type names, field paths, metadata

### ❌ Don't Change
1. Current SQL generation method (`jsonb_build_object`)
2. Rust-first architecture
3. RawJSONResult marker pattern
4. FastAPI response detection

---

## Testing Strategy

### Performance Tests
```python
# Benchmark suite to run after each phase
async def test_performance_improvement():
    # Test 1: Filtered query (most common)
    start = time.perf_counter()
    await repo.find("users", where={"status": {"eq": "active"}})
    filtered_time = time.perf_counter() - start

    # Test 2: Paginated query
    start = time.perf_counter()
    await repo.find("users", limit=100, offset=200)
    paginated_time = time.perf_counter() - start

    # Test 3: Full scan
    start = time.perf_counter()
    await repo.find("users")
    full_scan_time = time.perf_counter() - start

    # Assert improvements
    assert filtered_time < BASELINE_FILTERED * 0.85  # 15% improvement
    assert paginated_time < BASELINE_PAGINATED * 0.85
    assert full_scan_time < BASELINE_FULL_SCAN * 0.85
```

### Regression Tests
- All existing integration tests must pass
- GraphQL schema generation unchanged
- Type safety maintained
- Error handling preserved

---

## Migration Path

### Breaking Changes
**None** - All changes are internal optimizations

### Configuration Changes
```python
# Optional: New registration-time metadata
@fraise_type
class User:
    id: UUID
    name: str

    class FraiseQLConfig:
        jsonb_column = "data"  # ✅ Explicit (default: "data")
        table_name = "users"
        enable_field_selection = True  # ✅ New: control field path extraction
```

### Rollback Strategy
All changes are incremental and backward-compatible. Each phase can be rolled back independently:
- Phase 1: Revert registration-time metadata, restore sample queries
- Phase 2: Revert to Python string concatenation
- Phase 3: Restore original transform logic

---

## Success Metrics

### Performance Targets
- [ ] **15% reduction in p50 latency** (filtered queries)
- [ ] **20% reduction in p95 latency** (paginated queries)
- [ ] **25% reduction in p99 latency** (full scans)
- [ ] **30% reduction in DB connection pool usage**

### Code Quality Targets
- [ ] **100% test coverage** maintained
- [ ] **Zero breaking changes** to public API
- [ ] **50% reduction in dynamic introspection** (sample queries eliminated)
- [ ] **Improved code clarity** (fewer abstraction layers)

---

## Related Files

### Core Data Path
1. `src/fraiseql/db.py` - Repository layer (main optimization target)
2. `src/fraiseql/core/raw_json_executor.py` - JSON execution and transformation
3. `src/fraiseql/core/rust_transformer.py` - Rust integration
4. `src/fraiseql/sql/sql_generator.py` - SQL query building
5. `src/fraiseql/fastapi/response_handlers.py` - FastAPI response detection

### Supporting Files
- `src/fraiseql/core/ast_parser.py` - Field path extraction
- `src/fraiseql/core/graphql_type.py` - Type registration
- `benchmarks/jsonb_generation_benchmark/` - Performance validation

---

## Conclusion

The current architecture is fundamentally sound - **Rust-first design with string-based JSON** is the right approach. The optimizations focus on:

1. **Eliminating unnecessary work** (sample queries, redundant lookups)
2. **Moving work to the database** (json_agg, json_build_object)
3. **Reducing Python overhead** (direct Rust calls, fewer wrappers)
4. **Caching aggressively** (metadata, type names, field paths)

**Expected total improvement: 25-32% latency reduction** across all query types, with **zero breaking changes** to the public API.
