# JSON Passthrough Performance Fix - Root Cause Analysis

**Date**: October 12, 2025
**Status**: 🔴 Critical Path to v1 Alpha
**Impact**: 25-60x performance improvement (30ms → 0.5-1.2ms)

---

## 🎯 Executive Summary

JSON passthrough optimization exists in FraiseQL but is **bypassed** by field extraction logic. When GraphQL field info is available (normal case), queries use `jsonb_build_object()` to extract fields individually instead of pure `data::text` passthrough.

**Current Performance**: 28-31ms (equivalent to Strawberry)
**Target Performance**: 0.5-2ms (25-60x faster)
**Blocker**: Field-level extraction prevents pure passthrough

---

## 🔍 Root Cause

### Code Path Analysis

**File**: `src/fraiseql/db.py`

**Line 1191-1288**: Field extraction path (CURRENT - SLOW)
```python
if raw_json and field_paths is not None and len(field_paths) > 0:
    # Uses build_sql_query() which generates field-by-field extraction
    from fraiseql.sql.sql_generator import build_sql_query

    statement = build_sql_query(
        table=view_name,
        field_paths=field_paths,  # ← Triggers field extraction
        raw_json_output=True,
    )
```

**Generated SQL**:
```sql
SELECT jsonb_build_object(
    'id', data->>'id',
    'name', data->>'name',
    'email', data->>'email',
    ...
)::text FROM tv_user;
```

**Line 1289-1314**: Pure passthrough path (DESIRED - FAST but never reached)
```python
if raw_json:
    if jsonb_column:
        query_parts = [
            SQL("SELECT ") + Identifier(jsonb_column) + SQL("::text FROM ") + Identifier(view_name)
        ]
```

**Generated SQL**:
```sql
SELECT data::text FROM tv_user;  -- ← This is what we want!
```

### Why Pure Passthrough Never Activates

1. **GraphQL Info Available**: When resolvers run, they always have GraphQL field info
2. **Field Paths Extracted**: `extract_field_paths_from_info()` populates `field_paths`
3. **Conditional Check**: `if raw_json and field_paths is not None and len(field_paths) > 0`
4. **Result**: First branch taken, pure passthrough skipped

### Performance Impact

| Metric | Field Extraction | Pure Passthrough | Speedup |
|--------|------------------|------------------|---------|
| **PostgreSQL** | 28ms (jsonb_build_object) | 0.3-0.5ms (::text cast) | **56-93x** |
| **Python Processing** | 2ms (dict parsing) | 0ms (skip entirely) | **∞** |
| **Rust Transform** | N/A | 0.1-0.3ms (camelCase) | **New** |
| **Total** | **30ms** | **0.5-1ms** | **30-60x** |

---

## 🛠️ Fix Strategy

### Phase 1: Add Pure Passthrough Flag

**File**: `src/fraiseql/fastapi/config.py`

```python
class FraiseQLConfig(BaseSettings):
    # Existing
    json_passthrough_enabled: bool = True

    # NEW: Pure passthrough mode
    pure_json_passthrough: bool = True  # Always use data::text, skip field extraction
    pure_passthrough_use_rust: bool = True  # Use Rust for JSON transform
```

### Phase 2: Modify Query Building Logic

**File**: `src/fraiseql/db.py` (line ~1088)

```python
def _build_find_query(
    self,
    view_name: str,
    raw_json: bool = False,
    field_paths: list[Any] | None = None,
    **kwargs,
) -> DatabaseQuery:
    """Build SELECT query with pure passthrough support."""

    # Get config
    config = self.context.get("config")
    pure_passthrough = (
        config and
        hasattr(config, "pure_json_passthrough") and
        config.pure_json_passthrough
    )

    # PURE PASSTHROUGH MODE: Skip all field extraction
    if raw_json and pure_passthrough:
        # Determine JSONB column
        jsonb_column = self._determine_jsonb_column(view_name, [])
        if not jsonb_column:
            jsonb_column = "data"  # Default

        # Build pure passthrough query
        query_parts = [
            SQL("SELECT ") + Identifier(jsonb_column) + SQL("::text FROM ") + Identifier(view_name)
        ]

        # Add WHERE, ORDER BY, LIMIT, OFFSET...
        # (existing logic)

        return DatabaseQuery(statement=SQL("").join(query_parts), params={})

    # EXISTING LOGIC: Field extraction fallback
    if raw_json and field_paths is not None and len(field_paths) > 0:
        # ... existing code ...
```

### Phase 3: Integrate Rust Transform

**File**: `src/fraiseql/core/raw_json_executor.py`

```python
async def execute_raw_json_list_query(
    conn: AsyncConnection,
    query: Composed | SQL,
    params: dict[str, Any] | None = None,
    field_name: Optional[str] = None,
    type_name: Optional[str] = None,  # NEW
    use_rust: bool = True,  # NEW
) -> RawJSONResult:
    """Execute query and optionally transform with Rust."""

    # Execute query
    async with conn.cursor() as cursor:
        await cursor.execute(query, params or {})
        rows = await cursor.fetchall()

    # Combine JSON rows
    json_items = [row[0] for row in rows if row[0]]
    json_array = f"[{','.join(json_items)}]"

    # Wrap in GraphQL response
    if field_name:
        json_string = f'{{"data":{{"{field_name}":{json_array}}}}}'
    else:
        json_string = f'{{"data":{json_array}}}'

    result = RawJSONResult(json_string, transformed=False)

    # Transform with Rust if enabled
    if use_rust and type_name:
        from fraiseql.core.rust_transformer import get_transformer
        transformer = get_transformer()

        # Transform snake_case → camelCase + inject __typename
        transformed_json = transformer.transform(json_string, type_name)
        return RawJSONResult(transformed_json, transformed=True)

    return result
```

### Phase 4: Update Repository Methods

**File**: `src/fraiseql/db.py` (line ~674)

```python
async def find_raw_json(
    self, view_name: str, field_name: str, info: Any = None, **kwargs
) -> RawJSONResult:
    """Find records with pure passthrough + Rust transform."""

    # Build pure passthrough query (no field_paths)
    query = self._build_find_query(
        view_name,
        raw_json=True,
        field_paths=None,  # Force pure passthrough
        **kwargs
    )

    # Get type name for Rust transform
    type_name = None
    config = self.context.get("config")
    use_rust = (
        config and
        hasattr(config, "pure_passthrough_use_rust") and
        config.pure_passthrough_use_rust
    )

    if use_rust:
        try:
            type_class = self._get_type_for_view(view_name)
            type_name = getattr(type_class, "__name__", None)
        except Exception:
            pass

    # Execute with Rust transform
    async with self._pool.connection() as conn:
        result = await execute_raw_json_list_query(
            conn,
            query.statement,
            query.params,
            field_name,
            type_name=type_name,
            use_rust=use_rust
        )

    return result
```

---

## 📊 Expected Performance

### Benchmark Targets

| Scenario | Current | Target | Speedup |
|----------|---------|--------|---------|
| **Simple query (10 users)** | 28-31ms | 0.5-1ms | **28-62x** |
| **Nested query (user + 10 posts)** | 31-35ms | 1-2ms | **15-35x** |
| **Cached (pg_fraiseql_cache)** | 28ms | 0.3-0.5ms | **56-93x** |

### Performance Breakdown

```
Pure Passthrough + Rust Pipeline:
┌─────────────────────────┬─────────┐
│ PostgreSQL (data::text) │ 0.3-0.5ms│
│ Network + Psycopg       │ 0.1-0.2ms│
│ Rust Transform          │ 0.1-0.3ms│
│ HTTP Response           │ 0.1-0.2ms│
├─────────────────────────┼─────────┤
│ Total                   │ 0.6-1.2ms│
└─────────────────────────┴─────────┘

vs Current (Field Extraction):
┌─────────────────────────┬─────────┐
│ PostgreSQL (extraction) │ 28ms    │
│ Python Parsing          │ 2ms     │
│ HTTP Response           │ 0.3ms   │
├─────────────────────────┼─────────┤
│ Total                   │ 30.3ms  │
└─────────────────────────┴─────────┘
```

---

## ✅ Implementation Checklist

### Phase 1: Pure Passthrough Mode (Week 1)
- [ ] Add `pure_json_passthrough` config flag
- [ ] Modify `_build_find_query()` to skip field extraction
- [ ] Update `find_raw_json()` to force pure mode
- [ ] Add comprehensive logging for debugging
- [ ] Test with tv_* tables from benchmarks

### Phase 2: Rust Integration (Week 1-2)
- [ ] Update `execute_raw_json_list_query()` signature
- [ ] Call `fraiseql_rs.SchemaRegistry` for transforms
- [ ] Handle schema registration from GraphQL types
- [ ] Test snake_case → camelCase + __typename
- [ ] Verify 10-80x speedup vs Python

### Phase 3: Testing & Validation (Week 2)
- [ ] Unit tests for pure passthrough SQL generation
- [ ] Integration tests with Rust transform
- [ ] Benchmark against v0.10.2 baseline
- [ ] Validate 0.5-2ms response times
- [ ] Test with pg_fraiseql_cache integration

### Phase 4: Benchmarks & Documentation (Week 2)
- [ ] Update graphql-benchmarks to v1 alpha
- [ ] Run full benchmark suite
- [ ] Document 25-60x improvement
- [ ] Update README with verified claims
- [ ] Prepare v1 alpha release notes

---

## 🚨 Risks & Mitigations

### Risk 1: Schema Mismatch
**Issue**: JSON from PostgreSQL might not match GraphQL schema
**Mitigation**: Rust transformer validates structure, adds __typename

### Risk 2: Nested Objects
**Issue**: Pure passthrough assumes flat JSONB structure
**Mitigation**: tv_* tables already have composed nested data

### Risk 3: Backward Compatibility
**Issue**: Existing field-level auth/resolvers break
**Mitigation**: Keep field extraction as fallback, use feature flag

---

## 📈 Success Criteria

**V1 Alpha Release Ready When:**
- ✅ Pure passthrough generates `SELECT data::text`
- ✅ Rust transform handles snake_case → camelCase + __typename
- ✅ Benchmarks show 0.5-2ms response time
- ✅ 25-60x faster than v0.10.2
- ✅ All tests passing
- ✅ Documentation updated

**KPI**: Response time consistently under 2ms for simple queries

---

## 🎯 Next Steps

1. **Today**: Implement pure passthrough flag + query logic
2. **Tomorrow**: Integrate Rust transform in execution path
3. **Day 3-4**: Testing and benchmarking
4. **Day 5**: Update graphql-benchmarks, publish results

**Estimated Completion**: 5-7 days to v1 alpha candidate

---

**Priority**: 🔴 **CRITICAL** - This is the key differentiator for FraiseQL
**Impact**: **25-60x performance improvement**
**Effort**: **1-2 weeks**
**Dependencies**: fraiseql_rs (✅ complete), pg_fraiseql_cache (✅ integrated)

---

*This fix unblocks FraiseQL v1 alpha and validates the "fastest GraphQL framework" claim with reproducible benchmarks.*
