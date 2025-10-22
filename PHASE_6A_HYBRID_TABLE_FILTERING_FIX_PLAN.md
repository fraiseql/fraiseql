# Phase 6A: Hybrid Table Filtering Fix - Detailed Phased Plan

**Date**: 2025-10-22
**Status**: 🔍 INVESTIGATION
**Tests Affected**: 5 failing tests (final blocker!)
**Estimated Time**: 4-8 hours

---

## 📊 Problem Summary

All 5 remaining test failures share the **exact same root cause**:

```python
KeyError: 'brand'       # test_hybrid_table_filtering_generic
KeyError: 'hostname'    # test_industrial_where_clause (4 tests)
```

**Pattern**: Tests are querying hybrid tables (tables with both regular SQL columns + JSONB `data` column), but the **JSONB fields are missing from the response**.

---

## 🔍 Root Cause Analysis

### What is a Hybrid Table?

A **hybrid table** has two types of columns:

1. **Regular SQL columns** - Used for efficient filtering/indexing
   ```sql
   id UUID PRIMARY KEY,
   name TEXT,
   is_active BOOLEAN,  -- ← Can filter on this efficiently
   ```

2. **JSONB data column** - Contains flexible/additional fields
   ```sql
   data JSONB  -- ← Contains { "brand": "TechCorp", "color": "blue", ... }
   ```

**Example**:
```sql
CREATE TABLE products (
    -- Regular columns
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    is_active BOOLEAN,

    -- JSONB column with additional fields
    data JSONB  -- Contains: brand, color, specifications, etc.
);

INSERT INTO products VALUES (
    '123...',
    'Widget',
    true,
    '{"brand": "TechCorp", "color": "blue"}'::jsonb
);
```

### Why This Architecture?

**Benefits**:
- Fast filtering on indexed columns (`WHERE is_active = true`)
- Flexible schema in JSONB (`data->>'brand'`)
- No schema migrations for new JSONB fields
- Common pattern in multi-tenant SaaS apps

### The Bug

**Expected Behavior**:
```python
result = await repo.find("products", where={"brand": {"eq": "TechCorp"}})
# Should return: [{"id": "123", "name": "Widget", "isActive": true, "brand": "TechCorp"}]
```

**Actual Behavior**:
```python
result = await repo.find("products", where={"brand": {"eq": "TechCorp"}})
# Returns: [{"id": "123", "name": "Widget", "isActive": true}]  # Missing 'brand'!
# Accessing result["brand"] → KeyError: 'brand'
```

### Why Are JSONB Fields Missing?

Three possible root causes:

#### Hypothesis A: Missing View Definition ❌
**Issue**: FraiseQL isn't creating a view that exposes JSONB fields

**What should happen**:
```sql
CREATE VIEW products_view AS
SELECT
    id, name, is_active,  -- Regular columns
    data->>'brand' as brand,  -- Extract JSONB fields
    data->>'color' as color,
    data  -- Or just return full JSONB
FROM products;
```

**Evidence Against**: Tests are directly querying the table, not a view

---

#### Hypothesis B: Rust Pipeline Not Projecting JSONB Fields ✅ LIKELY
**Issue**: Rust pipeline queries the table but doesn't know which JSONB fields to extract/return

**What's happening**:
```sql
-- Rust pipeline generates:
SELECT data::text FROM products WHERE is_active = true;

-- Returns: {"id": "123", "name": "Widget", "is_active": true, "brand": "TechCorp"}
```

But then the Rust pipeline only extracts **registered fields** from the response, missing JSONB-only fields.

**Why this is likely**:
1. Tests register the table with `table_columns` parameter
2. `table_columns` lists SQL columns but may not include JSONB field names
3. Rust pipeline uses `table_columns` to know which fields to project

**Code Evidence**:
```python
# From test file:
register_type_for_view(
    "products",
    Product,
    table_columns={
        "id", "name", "status", "is_active", "is_featured",
        "is_available", "category_id", "created_date", "data"  # ← Only 'data', not 'brand'
    },
)
```

The `table_columns` set includes `"data"` but NOT `"brand"`, `"color"`, `"specifications"` - so Rust pipeline doesn't know to project these fields!

---

#### Hypothesis C: Type Registration Issue ✅ ALSO LIKELY
**Issue**: The Python type defines fields, but registration doesn't map them to JSONB paths

**The type definition**:
```python
@fraiseql.type
class Product:
    # SQL columns
    id: str
    name: str
    is_active: bool

    # JSONB fields (from data column)
    brand: str | None = None  # ← Should map to data->>'brand'
    color: str | None = None  # ← Should map to data->>'color'
```

**What might be missing**: FraiseQL doesn't know that `brand` should come from `data->>'brand'` in the JSONB column.

---

## 🎯 The Fix Strategy

Based on the analysis, the fix likely involves **one or both** of:

1. **Update `table_columns` registration** to include JSONB field names
2. **Enhance Rust pipeline** to extract JSONB fields from `data` column

---

## 📋 Phased Fix Plan

### Phase 6A.1: Reproduce and Confirm Root Cause (30-60 min)

**Objective**: Understand exactly what the Rust pipeline is returning

#### Steps:

1. **Add debug logging to failing test**
   ```python
   # In test_hybrid_table_filtering_generic.py
   result = await repo.find("products", where=where)
   results = extract_graphql_data(result, "products")

   # Add this debug:
   import json
   print("=== RAW RESULT ===")
   print(json.dumps(results, indent=2))
   print("=== AVAILABLE KEYS ===")
   if results:
       print(list(results[0].keys()))
   ```

2. **Run test with debug output**
   ```bash
   uv run pytest tests/integration/database/repository/test_hybrid_table_filtering_generic.py::TestHybridTableFiltering::test_mixed_regular_and_jsonb_filtering -xvs
   ```

3. **Analyze output**:
   - What fields ARE present?
   - Is `data` column included?
   - Are SQL columns present but JSONB fields missing?

4. **Check SQL query generated**
   - Add logging to see what SQL FraiseQL generates
   - Does it SELECT the `data` column?
   - Does it try to extract JSONB fields?

#### Deliverables:
- [ ] Exact list of fields returned in response
- [ ] SQL query that FraiseQL generates
- [ ] Confirmation of which hypothesis (B or C) is correct

---

### Phase 6A.2: Fix Type Registration (1-2 hours)

**Objective**: Ensure FraiseQL knows which fields come from JSONB

#### Option 1: Update table_columns to Include JSONB Fields

**Change**:
```python
# In test file (and possibly in db.py logic)
register_type_for_view(
    "products",
    Product,
    table_columns={
        "id", "name", "status", "is_active", "is_featured",
        "is_available", "category_id", "created_date",
        "data",  # Keep this
        # ADD JSONB fields:
        "brand", "color", "specifications"  # ← Tell system these exist in JSONB
    },
)
```

**Why this might work**: If Rust pipeline uses `table_columns` to know which fields to project, adding JSONB field names tells it to extract them from the response.

**Files to Modify**:
- Test files (immediate fix)
- `src/fraiseql/db.py` - `register_type_for_view()` logic
- Documentation for hybrid table setup

---

#### Option 2: Auto-detect JSONB Fields from Type Definition

**Change**: Make `register_type_for_view()` automatically include all fields from the type definition, not just what's in `table_columns`.

```python
# In src/fraiseql/db.py
def register_type_for_view(view_name, type_cls, table_columns=None):
    # Get all fields from type definition
    all_fields = set(get_type_fields(type_cls).keys())

    if table_columns:
        # table_columns lists SQL columns
        # Fields NOT in table_columns must come from JSONB
        jsonb_fields = all_fields - table_columns
    else:
        # If no table_columns specified, assume all fields are in JSONB 'data'
        jsonb_fields = all_fields

    # Store mapping for Rust pipeline to use
    _jsonb_field_mapping[view_name] = {
        "sql_columns": table_columns or set(),
        "jsonb_fields": jsonb_fields,
        "jsonb_column": "data"  # Convention: JSONB data is in 'data' column
    }
```

**Why this might work**: The system auto-detects that `brand`, `color`, etc. aren't SQL columns, so they must come from JSONB.

**Files to Modify**:
- `src/fraiseql/db.py` - `register_type_for_view()`
- `src/fraiseql/core/rust_pipeline.py` - Pass JSONB field info to Rust
- Rust crate (`fraiseql-rs`) - Extract JSONB fields if needed

---

### Phase 6A.3: Update Rust Pipeline to Project JSONB Fields (2-3 hours)

**Objective**: Ensure Rust pipeline extracts and includes JSONB fields in response

#### Current Rust Pipeline Behavior (Suspected):

```rust
// fraiseql-rs current behavior
fn build_response(rows: Vec<Row>) -> JsonValue {
    let mut results = vec![];
    for row in rows {
        let data_json: String = row.get("data");  // Get JSONB column
        let mut obj = serde_json::from_str(&data_json)?;

        // Problem: Only includes fields from schema,
        // doesn't merge in additional JSONB fields
        results.push(obj);
    }
    // ...
}
```

#### What We Need:

```rust
// fraiseql-rs desired behavior
fn build_response(rows: Vec<Row>, jsonb_fields: Vec<String>) -> JsonValue {
    let mut results = vec![];
    for row in rows {
        let data_json: String = row.get("data");  // Get JSONB column
        let mut obj = serde_json::from_str(&data_json)?;

        // Extract JSONB fields if they're in the type definition
        // but not in SQL columns
        for field in jsonb_fields {
            if let Some(value) = obj.get(field) {
                // Keep this field in response
                // (already present if data column contains it)
            }
        }

        results.push(obj);
    }
    // ...
}
```

#### Implementation:

1. **Pass JSONB field list to Rust pipeline**:
   ```python
   # In src/fraiseql/core/rust_pipeline.py
   response_bytes = fraiseql_rs.build_graphql_response(
       json_strings=json_strings,
       field_name=field_name,
       type_name=type_name,
       field_paths=field_paths,
       jsonb_fields=jsonb_fields,  # ← NEW parameter
   )
   ```

2. **Update Rust crate** (if needed):
   - Modify `build_graphql_response()` signature
   - Ensure JSONB fields are preserved in output
   - Handle field projection correctly

**Files to Modify**:
- `src/fraiseql/core/rust_pipeline.py` - Pass JSONB field info
- `fraiseql-rs` crate (if Rust changes needed)

---

### Phase 6A.4: Test and Validate (1-2 hours)

**Objective**: Verify all 5 tests pass with the fix

#### Test Strategy:

1. **Run the 5 failing tests**:
   ```bash
   # Test 1: Hybrid table filtering
   uv run pytest tests/integration/database/repository/test_hybrid_table_filtering_generic.py::TestHybridTableFiltering::test_mixed_regular_and_jsonb_filtering -xvs

   # Tests 2-5: Industrial WHERE clause generation
   uv run pytest tests/regression/where_clause/test_industrial_where_clause_generation.py::TestREDPhaseProductionScenarios -xvs
   ```

2. **Verify fields are present**:
   ```python
   # Each test should now pass with fields accessible:
   assert product["brand"] == "TechCorp"  # ← No longer KeyError!
   assert result["hostname"] == "printserver01.local"  # ← Works!
   ```

3. **Run full test suite**:
   ```bash
   uv run pytest --tb=line -q
   ```

4. **Check for regressions**:
   - Ensure other tests still pass
   - Verify non-hybrid-table tests unaffected

#### Success Criteria:
- [ ] All 5 tests passing
- [ ] No new test failures
- [ ] JSONB fields accessible in responses
- [ ] SQL filtering still works correctly

---

### Phase 6A.5: Document and Commit (30 min)

**Objective**: Document the fix and commit changes

#### Documentation:

1. **Update hybrid table guide** (create if doesn't exist):
   ```markdown
   # Hybrid Table Pattern in FraiseQL

   ## Overview
   Hybrid tables combine SQL columns (for filtering) with JSONB data (for flexibility).

   ## Setup
   \```python
   @fraiseql.type
   class Product:
       # SQL columns
       id: str
       is_active: bool

       # JSONB fields (from 'data' column)
       brand: str | None = None

   register_type_for_view(
       "products",
       Product,
       table_columns={"id", "is_active", "data"}  # List SQL columns
   )
   # FraiseQL auto-detects that 'brand' must come from JSONB
   \```

   ## How It Works
   - Fields in `table_columns` are queried as SQL columns
   - Fields NOT in `table_columns` are extracted from JSONB `data` column
   - Filtering works on both types transparently
   ```

2. **Update migration guide**:
   - Document changes to `register_type_for_view()`
   - Note any breaking changes

3. **Create completion report**:
   - `PHASE_6A_HYBRID_TABLE_FILTERING_FIXED.md`
   - Document root cause, solution, tests fixed

#### Commit:

```bash
git add -A
git commit -m "fix: enable JSONB field projection in hybrid table queries (Phase 6A)

Phase 6A: Hybrid Table Filtering Fix

Problem:
- 5 tests failing with KeyError when accessing JSONB fields
- Hybrid tables (SQL columns + JSONB data) weren't projecting JSONB fields

Root Cause:
- register_type_for_view() only projected SQL columns
- JSONB fields (brand, hostname, etc.) missing from responses
- Rust pipeline didn't know to extract/include these fields

Solution:
- Enhanced register_type_for_view() to auto-detect JSONB fields
- Fields in table_columns = SQL columns
- Fields NOT in table_columns = JSONB fields (extracted from 'data')
- Updated Rust pipeline to project JSONB fields in responses

Tests Fixed:
1. test_mixed_regular_and_jsonb_filtering (hybrid table)
2. test_production_hostname_filtering (industrial)
3. test_production_port_filtering (industrial)
4. test_production_boolean_filtering (industrial)
5. test_production_mixed_filtering_comprehensive (industrial)

Files Modified:
- src/fraiseql/db.py (register_type_for_view logic)
- src/fraiseql/core/rust_pipeline.py (JSONB field projection)
- tests/ (validation)

Result:
- All 5 tests now passing
- JSONB fields accessible in hybrid table queries
- 0 failing tests (100% test suite health!)

🎉 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## 🎯 Expected Outcome

### Before Phase 6A:
```
✅ 3,546 passing
⏭️ 0 skipped
❌ 5 failing (hybrid table filtering)
```

### After Phase 6A:
```
✅ 3,551 passing (all tests!)
⏭️ 0 skipped
❌ 0 failing (100% pass rate!)
```

---

## 🔍 Key Files to Investigate

### 1. Type Registration
- `src/fraiseql/db.py` - `register_type_for_view()`
- `src/fraiseql/db.py` - Type registry logic

### 2. Rust Pipeline
- `src/fraiseql/core/rust_pipeline.py` - `execute_via_rust_pipeline()`
- `src/fraiseql/core/rust_pipeline.py` - Field projection logic

### 3. Test Files
- `tests/integration/database/repository/test_hybrid_table_filtering_generic.py`
- `tests/regression/where_clause/test_industrial_where_clause_generation.py`

### 4. SQL Generation
- `src/fraiseql/sql/query_builder.py` - Query construction
- `src/fraiseql/sql/where_generator.py` - WHERE clause generation

---

## 🚨 Potential Pitfalls

### Pitfall 1: Rust Crate Changes Required
**Issue**: If Rust pipeline needs modification, this requires:
- Rust development environment
- Rebuilding Python bindings
- Potentially waiting for fraiseql-rs release

**Mitigation**: Try Python-side fixes first (option 2 in Phase 6A.2)

### Pitfall 2: Breaking Changes
**Issue**: Changing `register_type_for_view()` behavior might break existing code

**Mitigation**:
- Make changes backward compatible
- Add new optional parameter instead of changing behavior
- Document migration path

### Pitfall 3: Performance Impact
**Issue**: Extracting JSONB fields might be slower than SQL columns

**Mitigation**:
- Benchmark before/after
- Document performance characteristics
- Suggest SQL column migration for frequently filtered fields

---

## 🎯 Success Metrics

### Primary Goals:
- [ ] All 5 failing tests pass
- [ ] 100% test suite health (3,551 / 3,551)
- [ ] No regressions in other tests

### Secondary Goals:
- [ ] Hybrid table pattern documented
- [ ] Migration guide updated
- [ ] Performance acceptable (no >10% regression)

### Stretch Goals:
- [ ] Auto-detect hybrid tables from schema
- [ ] Generate optimal SQL based on field types
- [ ] Add warning for inefficient JSONB filtering

---

## ⏱️ Time Estimates

| Phase | Estimated Time | Dependencies |
|-------|----------------|--------------|
| 6A.1: Reproduce | 30-60 min | None |
| 6A.2: Fix Registration | 1-2 hours | Phase 6A.1 |
| 6A.3: Update Rust Pipeline | 2-3 hours | Phase 6A.2 (if needed) |
| 6A.4: Test & Validate | 1-2 hours | Phase 6A.2 or 6A.3 |
| 6A.5: Document & Commit | 30 min | Phase 6A.4 |
| **Total** | **5-8 hours** | Sequential |

**Optimistic**: 4-5 hours (if Python-side fix works)
**Realistic**: 5-8 hours (if Rust changes needed)
**Pessimistic**: 8-12 hours (if complex Rust refactoring)

---

## 🚀 Next Steps

1. **Start with Phase 6A.1** (Investigation)
   - Add debug logging
   - Run failing test
   - Confirm root cause

2. **Try Python-side fix first** (Phase 6A.2, Option 2)
   - Auto-detect JSONB fields
   - Update registration logic
   - See if this alone fixes tests

3. **Fall back to Rust changes** (Phase 6A.3)
   - Only if Python fix insufficient
   - Requires Rust expertise

4. **Validate thoroughly** (Phase 6A.4)
   - Run all 5 tests
   - Full test suite
   - Performance check

5. **Document and ship** (Phase 6A.5)
   - Write completion report
   - Update docs
   - Commit with detailed message

---

**Status**: 📋 PLAN COMPLETE - Ready for execution
**Priority**: 🔥 CRITICAL - Final blocker for 100% test health
**Impact**: HIGH - Enables hybrid table pattern for production use

---

*Created: 2025-10-22*
*Phase: 6A (Final)*
*Tests Remaining: 5 → 0 (after completion)*
