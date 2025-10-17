# JSONB to HTTP Data Flow - Summary

## Quick Overview

FraiseQL's data path from PostgreSQL JSONB to HTTP response is highly optimized, achieving **2.8-5x better performance** than traditional approaches through:

1. **Rust-first transformation** (10-80x faster than Python)
2. **String-based JSON** (zero parsing in Python)
3. **Smart response detection** (FastAPI bypasses Pydantic)
4. **Database-level optimization** (JSONB column passthrough)

## The 4 Main Layers

```
┌─────────────────────────────────────────────────────────────┐
│ 1. GraphQL Layer (FastAPI + Strawberry)                     │
│    - Receives GraphQL query                                  │
│    - Detects RawJSONResult → bypasses serialization         │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│ 2. Repository Layer (db.py)                                  │
│    - Builds optimized SQL: SELECT data::text                 │
│    - Extracts field paths for GraphQL compliance            │
│    - Caches metadata at registration time                   │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│ 3. Database Layer (PostgreSQL)                               │
│    - Returns raw JSON strings (not parsed!)                  │
│    - JSONB → text cast (zero serialization overhead)        │
│    - Fast JSONB operations at database level                │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│ 4. Transformation Layer (Rust)                               │
│    - snake_case → camelCase (10-80x faster)                 │
│    - __typename injection for GraphQL                        │
│    - Returns pre-serialized JSON bytes                      │
└─────────────────────────────────────────────────────────────┘
```

## Complete Step-by-Step Flow

### 1. **GraphQL Request Arrives**
   - FastAPI receives: `query { users { id name email } }`
   - Strawberry parses GraphQL AST
   - Info object contains field selection

### 2. **Repository.find() Called**
   - **File:** `src/fraiseql/db.py:430`
   - Mode detection: Production mode (passthrough enabled)
   - GraphQL info extracted from context

### 3. **Field Paths Extracted**
   - **File:** `src/fraiseql/db.py:454-457`
   - AST parsing: Extract `["id", "name", "email"]`
   - Transform to snake_case: `["id", "name", "email"]`

### 4. **JSONB Column Determined**
   - **File:** `src/fraiseql/db.py:464-486`
   - ⚠️ **OPTIMIZATION TARGET:** Sample query executed
   - Detects JSONB column name (usually `data`)

### 5. **SQL Query Built**
   - **File:** `src/fraiseql/db.py:677-858`
   - **Mode 1 (Pure passthrough):** `SELECT data::text FROM users`
   - **Mode 2 (Field extraction):** Complex JSONB path query
   - Uses psycopg's `Composed` SQL for safety

### 6. **Query Executed**
   - **File:** `src/fraiseql/db.py:92-103`
   - PostgreSQL returns JSON **as text** (no dict parsing!)
   - Result: `['{"id":"123","name":"Alice"}', '{"id":"456","name":"Bob"}']`

### 7. **Rows Concatenated**
   - **File:** `src/fraiseql/core/raw_json_executor.py:216-222`
   - ⚠️ **OPTIMIZATION TARGET:** String concatenation in Python
   - Result: `[{"id":"123"},{"id":"456"}]`

### 8. **JSON Wrapped for GraphQL**
   - **File:** `src/fraiseql/core/raw_json_executor.py:225-229`
   - ⚠️ **OPTIMIZATION TARGET:** String manipulation
   - Result: `{"data":{"users":[{"id":"123"},{"id":"456"}]}}`

### 9. **Rust Transformation Applied**
   - **File:** `src/fraiseql/core/rust_transformer.py:122-132`
   - fraiseql-rs transforms: `snake_case` → `camelCase`
   - Injects `__typename` for GraphQL clients
   - Result: `{"data":{"users":[{"id":"123","__typename":"User"}]}}`

### 10. **RawJSONResult Returned**
   - **File:** `src/fraiseql/core/raw_json_executor.py:240`
   - Marker object: Signals FastAPI to skip serialization
   - Contains: Pre-serialized JSON bytes

### 11. **FastAPI Response**
   - **File:** `src/fraiseql/fastapi/response_handlers.py`
   - Detects RawJSONResult → bypasses Pydantic
   - Sends JSON directly as HTTP bytes

## 7 Unnecessary Transformations (Optimization Targets)

### 🔴 Priority 1: Sample Query Execution
**Location:** `db.py:464-483`
**Impact:** 50% reduction in DB queries
**Fix:** Move to registration time

### 🟡 Priority 2: Type Name Registry Lookup
**Location:** `db.py:521-529`
**Impact:** 2-3% latency improvement
**Fix:** Add instance-level caching

### 🟡 Priority 3: RawJSONResult Wrapper Overhead
**Location:** `raw_json_executor.py:165-174`
**Impact:** 5-10% overhead reduction
**Fix:** Direct Rust transformer call

### 🟢 Priority 4: Double JSON Wrapping
**Location:** `raw_json_executor.py:151-156`
**Impact:** 2-4% improvement
**Fix:** Use PostgreSQL `json_build_object()`

### 🟢 Priority 5: Field Path Extraction
**Location:** `db.py:451-457`
**Impact:** 1-2% improvement
**Fix:** Lazy evaluation + caching

### 🟢 Priority 6: JSON Parsing After Raw Execution
**Location:** `db.py:539-542`
**Impact:** 3-5% improvement on large results
**Fix:** Keep as raw JSON, use `json_agg()`

### 🟢 Priority 7: RawJSONResult.transform() Complexity
**Location:** `raw_json_executor.py:41-102`
**Impact:** 5-8% improvement
**Fix:** Single-pass transformation in Rust

## Performance Baseline

### Current Results (from benchmark)
| Query Type | Method | TPS | Latency | Winner |
|------------|--------|-----|---------|--------|
| **Filtered (100 rows)** | jsonb_build_object | **475 TPS** | **20.4ms** | ✅ Current |
| | to_jsonb | 431 TPS | 22.3ms | |
| **Paginated (100 rows)** | jsonb_build_object | **22.2 TPS** | **445ms** | ✅ Current |
| | to_jsonb | 14.6 TPS | 680ms | |
| **Full Scan (10K rows)** | to_jsonb | **8.2 TPS** | **486ms** | ✅ Simpler |
| | jsonb_build_object | 6.6 TPS | 601ms | |

**Key Findings:**
- ✅ Current `jsonb_build_object` is **52% faster** on paginated queries
- ✅ Current approach is **10% faster** on filtered queries
- ⚠️ `to_jsonb` is **24% faster** on full scans (but rarely used)

## 5-Phase Simplification Roadmap

### Phase 1: Eliminate Sample Queries (Biggest Impact)
**Target:** 15-20% latency improvement
**Changes:** Registration-time metadata storage

### Phase 2: Cache Type Names (Quick Win)
**Target:** 2-3% latency improvement
**Changes:** Instance-level cache

### Phase 3: Direct Rust Calls (Reduce Overhead)
**Target:** 5-10% overhead reduction
**Changes:** Skip Python wrappers

### Phase 4: Database-Level Response Building
**Target:** 3-5% improvement on lists
**Changes:** Use `json_agg()` in SQL

### Phase 5: Lazy Field Path Extraction
**Target:** 1-2% improvement
**Changes:** Cache and lazy evaluation

## Implementation Notes

### What to Keep (Strengths)
✅ Rust-first design (all transformations in compiled code)
✅ String-based JSON (no unnecessary parsing)
✅ Smart response detection (RawJSONResult marker)
✅ Metadata caching at registration time
✅ Production-only optimization

### What to Simplify
🔄 Runtime introspection (move to registration)
🔄 Type name lookups (add caching)
🔄 Python wrapper layers (call Rust directly)
🔄 String concatenation (use PostgreSQL functions)
🔄 Multiple parse/serialize cycles (single-pass in Rust)

### Zero Breaking Changes
- All optimizations are internal
- Public API remains unchanged
- Backward compatibility maintained
- Existing tests continue to pass

## Expected Total Improvement

**Conservative Estimate:** 15-25% latency reduction
**Optimistic Estimate:** 25-35% latency reduction

**Key Metric Targets:**
- Filtered queries: 475 TPS → **600+ TPS** (+26%)
- Paginated queries: 22 TPS → **28+ TPS** (+27%)
- Full scans: 6.6 TPS → **8.5+ TPS** (+29%)

---

**Next Steps:** See `JSONB_TO_HTTP_SIMPLIFICATION_PLAN.md` for detailed implementation guide
