# WHERE Clause Test Reorganization Plan

**Status**: LTree consolidation completed (proof-of-concept ✅)
**Date**: 2025-12-11

## Problem Statement

The WHERE clause tests have accumulated through phase-by-phase development, resulting in:
- **Duplicate coverage**: Same operators tested in `*_sql_building.py` AND `*_complete.py` files
- **Inconsistent naming**: Multiple naming patterns without clear organization
- **Scattered tests**: Related tests split across 4-5 files (e.g., LTree)
- **Flat structure**: 33 test files in one directory

## Proof-of-Concept: LTree Consolidation ✅

**Before**: 5 files, 263+188+150+192+141 = 934 lines total
- `test_ltree_operators_complete.py` (263 lines)
- `test_ltree_operators_sql_building.py` (188 lines)
- `test_ltree_array_operators.py` (150 lines)
- `test_ltree_path_analysis_operators.py` (192 lines)
- `test_ltree_path_manipulation_operators.py` (141 lines)

**After**: 1 file, 715 lines
- `tests/unit/sql/where/operators/specialized/test_ltree.py` (715 lines)
- **Result**: 61 passing tests, 7 expected failures (TDD RED phase)
- **Reduction**: 5 files → 1 file, ~24% fewer lines (no duplication)

## Target Directory Structure

```
tests/unit/sql/where/
├── __init__.py
├── core/                          # Core WHERE functionality
│   ├── __init__.py
│   ├── test_field_detection.py   # IP, vector, email detection logic
│   ├── test_where_builder.py     # build_where_clause, nested objects
│   └── test_error_handling.py    # Operator error handling
│
├── operators/                     # All operator tests
│   ├── __init__.py
│   ├── test_basic.py             # eq, neq, gt, gte, lt, lte
│   ├── test_logical.py           # and, or, not
│   ├── test_pattern.py           # like, ilike, regex
│   ├── test_array.py             # array_contains, array_overlap, etc.
│   ├── test_list.py              # in, notin (list membership)
│   ├── test_jsonb.py             # JSONB operators, nulls
│   │
│   ├── temporal/                 # Date/time operators
│   │   ├── __init__.py
│   │   ├── test_date.py          # Date operators (consolidated)
│   │   ├── test_datetime.py      # DateTime operators (consolidated)
│   │   └── test_daterange.py     # DateRange operators (consolidated)
│   │
│   ├── network/                  # Network-related types
│   │   ├── __init__.py
│   │   ├── test_ip.py            # IP address operators
│   │   ├── test_email.py         # Email operators
│   │   ├── test_hostname.py      # Hostname operators
│   │   ├── test_mac.py           # MAC address operators
│   │   └── test_port.py          # Port operators
│   │
│   ├── spatial/                  # Spatial/geometric types
│   │   ├── __init__.py
│   │   └── test_coordinate.py    # Point/coordinate operators
│   │
│   └── specialized/              # Specialized PostgreSQL types
│       ├── __init__.py
│       ├── test_ltree.py         # ✅ LTree operators (COMPLETED)
│       ├── test_vector.py        # Vector similarity operators
│       └── test_fulltext.py      # Full-text search operators
│
└── integration/                   # Integration tests (future)
    ├── __init__.py
    └── test_complex_queries.py   # Multi-operator, complex WHERE clauses
```

## Migration Plan by Category

### Phase 1: Core Functionality ✅ (Low Risk)

These are simple moves with minimal consolidation.

#### 1.1 Field Detection
**Action**: Merge
**Files**:
- `test_field_detection_ip_filtering.py` (119 lines)
- `test_field_detection_vector.py` (58 lines)

**Output**: `core/test_field_detection.py`
**Steps**:
```bash
mkdir -p tests/unit/sql/where/core
# Create consolidated file with sections:
# - IP Address Field Detection (from test_field_detection_ip_filtering.py)
# - Vector Field Detection (from test_field_detection_vector.py)
uv run pytest tests/unit/sql/where/core/test_field_detection.py -v
rm tests/unit/sql/where/test_field_detection_ip_filtering.py
rm tests/unit/sql/where/test_field_detection_vector.py
```

#### 1.2 WHERE Builder
**Action**: Merge
**Files**:
- `test_nested_object_where_builder.py` (90 lines)
- `test_base_builders_complete.py` (232 lines)

**Output**: `core/test_where_builder.py`
**Steps**:
```bash
# Merge both files:
# - Base builder tests (from test_base_builders_complete.py)
# - Nested object tests (from test_nested_object_where_builder.py)
uv run pytest tests/unit/sql/where/core/test_where_builder.py -v
rm tests/unit/sql/where/test_nested_object_where_builder.py
rm tests/unit/sql/where/test_base_builders_complete.py
```

#### 1.3 Error Handling
**Action**: Move only
**File**: `test_operator_error_handling.py` (171 lines)
**Output**: `core/test_error_handling.py`
**Steps**:
```bash
mv tests/unit/sql/where/test_operator_error_handling.py \
   tests/unit/sql/where/core/test_error_handling.py
uv run pytest tests/unit/sql/where/core/test_error_handling.py -v
```

### Phase 2: Simple Operators (Move Only)

These are complete, well-organized files that just need to be moved.

```bash
mkdir -p tests/unit/sql/where/operators

# Basic operators
mv tests/unit/sql/where/test_basic_operators_complete.py \
   tests/unit/sql/where/operators/test_basic.py

# Pattern operators
mv tests/unit/sql/where/test_pattern_operators_complete.py \
   tests/unit/sql/where/operators/test_pattern.py

# Array operators
mv tests/unit/sql/where/test_array_operators_complete.py \
   tests/unit/sql/where/operators/test_array.py

# List operators
mv tests/unit/sql/where/test_list_operators_complete.py \
   tests/unit/sql/where/operators/test_list.py

# JSONB operators
mv tests/unit/sql/where/test_jsonb_nulls_complete.py \
   tests/unit/sql/where/operators/test_jsonb.py

# Full-text search
mkdir -p tests/unit/sql/where/operators/specialized
mv tests/unit/sql/where/test_fulltext_operators_complete.py \
   tests/unit/sql/where/operators/specialized/test_fulltext.py

# Run verification
uv run pytest tests/unit/sql/where/operators/ -v --collect-only
```

### Phase 3: Logical Operators (Merge + Deduplicate)

**Action**: Merge and remove foundation file
**Files**:
- `test_logical_operators_complete.py` (301 lines) - KEEP
- `test_logical_operators_foundation.py` (116 lines) - DELETE (superseded)

**Output**: `operators/test_logical.py`
**Steps**:
```bash
# The "complete" file supersedes the "foundation" file
mv tests/unit/sql/where/test_logical_operators_complete.py \
   tests/unit/sql/where/operators/test_logical.py
rm tests/unit/sql/where/test_logical_operators_foundation.py
uv run pytest tests/unit/sql/where/operators/test_logical.py -v
```

### Phase 4: Temporal Operators (Consolidate + Deduplicate)

These have both `*_sql_building.py` (old phase) and `*_complete.py` (new phase), plus split files.

#### 4.1 Date Operators
**Action**: Merge and deduplicate
**Files**:
- `test_date_operators_sql_building.py` (261 lines) - OLD PHASE
- `test_date_datetime_port_complete.py` (242 lines) - EXTRACT date parts

**Output**: `operators/temporal/test_date.py`
**Strategy**:
1. Read both files
2. Use `*_complete.py` as base (newer, comprehensive)
3. Add any unique tests from `*_sql_building.py` if missing
4. Delete both old files

**Steps**:
```bash
mkdir -p tests/unit/sql/where/operators/temporal
# Create consolidated file (manual merge required)
# Extract date-related tests from test_date_datetime_port_complete.py
# Verify no duplication with test_date_operators_sql_building.py
uv run pytest tests/unit/sql/where/operators/temporal/test_date.py -v
rm tests/unit/sql/where/test_date_operators_sql_building.py
# Keep test_date_datetime_port_complete.py for now (has datetime + port parts)
```

#### 4.2 DateTime Operators
**Action**: Merge and deduplicate
**Files**:
- `test_datetime_operators_sql_building.py` (262 lines) - OLD PHASE
- `test_date_datetime_port_complete.py` (242 lines) - EXTRACT datetime parts

**Output**: `operators/temporal/test_datetime.py`
**Steps**:
```bash
# Extract datetime-related tests from test_date_datetime_port_complete.py
# Verify no duplication with test_datetime_operators_sql_building.py
uv run pytest tests/unit/sql/where/operators/temporal/test_datetime.py -v
rm tests/unit/sql/where/test_datetime_operators_sql_building.py
```

#### 4.3 DateRange Operators
**Action**: Merge and deduplicate
**Files**:
- `test_daterange_operators_sql_building.py` (245 lines) - OLD PHASE
- `test_daterange_operators_complete.py` (170 lines) - NEW PHASE

**Output**: `operators/temporal/test_daterange.py`
**Steps**:
```bash
# Use test_daterange_operators_complete.py as base
# Add any unique tests from test_daterange_operators_sql_building.py
uv run pytest tests/unit/sql/where/operators/temporal/test_daterange.py -v
rm tests/unit/sql/where/test_daterange_operators_sql_building.py
rm tests/unit/sql/where/test_daterange_operators_complete.py
```

### Phase 5: Network Operators (Split + Consolidate)

#### 5.1 IP Operators
**Action**: Merge network parts
**Files**:
- `test_ip_operators_sql_building.py` (139 lines)
- `test_network_operators_complete.py` (113 lines) - EXTRACT IP parts

**Output**: `operators/network/test_ip.py`
**Steps**:
```bash
mkdir -p tests/unit/sql/where/operators/network
# Consolidate IP-related tests
uv run pytest tests/unit/sql/where/operators/network/test_ip.py -v
rm tests/unit/sql/where/test_ip_operators_sql_building.py
rm tests/unit/sql/where/test_network_operators_complete.py
```

#### 5.2 Email/Hostname/MAC Operators
**Action**: Split one file into three
**File**: `test_email_hostname_mac_complete.py` (191 lines)

**Outputs**:
- `operators/network/test_email.py`
- `operators/network/test_hostname.py`
- `operators/network/test_mac.py`

**Steps**:
```bash
# Also merge with old sql_building files:
# - test_email_operators_sql_building.py (183 lines)
# - test_hostname_operators_sql_building.py (171 lines)
# - test_mac_address_operators_sql_building.py (124 lines)

# Split test_email_hostname_mac_complete.py by test class
# Extract EmailOperator tests → test_email.py
# Extract HostnameOperator tests → test_hostname.py
# Extract MACOperator tests → test_mac.py

uv run pytest tests/unit/sql/where/operators/network/test_email.py -v
uv run pytest tests/unit/sql/where/operators/network/test_hostname.py -v
uv run pytest tests/unit/sql/where/operators/network/test_mac.py -v

rm tests/unit/sql/where/test_email_hostname_mac_complete.py
rm tests/unit/sql/where/test_email_operators_sql_building.py
rm tests/unit/sql/where/test_hostname_operators_sql_building.py
rm tests/unit/sql/where/test_mac_address_operators_sql_building.py
```

#### 5.3 Port Operators
**Action**: Extract from combined file
**Files**:
- `test_port_operators_sql_building.py` (246 lines)
- `test_date_datetime_port_complete.py` (242 lines) - EXTRACT port parts

**Output**: `operators/network/test_port.py`
**Steps**:
```bash
# Extract port-related tests from test_date_datetime_port_complete.py
# Merge with test_port_operators_sql_building.py
uv run pytest tests/unit/sql/where/operators/network/test_port.py -v
rm tests/unit/sql/where/test_port_operators_sql_building.py
# After all temporal + port extraction:
rm tests/unit/sql/where/test_date_datetime_port_complete.py
```

### Phase 6: Spatial Operators (Consolidate)

**Action**: Merge and deduplicate
**Files**:
- `test_coordinate_operators_sql_building.py` (146 lines) - OLD PHASE
- `test_coordinate_operators_complete.py` (205 lines) - NEW PHASE

**Output**: `operators/spatial/test_coordinate.py`
**Steps**:
```bash
mkdir -p tests/unit/sql/where/operators/spatial
# Use test_coordinate_operators_complete.py as base
# Add any unique tests from test_coordinate_operators_sql_building.py
uv run pytest tests/unit/sql/where/operators/spatial/test_coordinate.py -v
rm tests/unit/sql/where/test_coordinate_operators_sql_building.py
rm tests/unit/sql/where/test_coordinate_operators_complete.py
```

### Phase 7: Specialized Operators

#### 7.1 LTree Operators ✅ COMPLETED
**Status**: Already completed as proof-of-concept
**Location**: `operators/specialized/test_ltree.py`
**Result**: 61 passing, 7 expected failures

#### 7.2 Vector Operators
**Action**: Merge
**Files**:
- `test_vector_operators_complete.py` (319 lines)
- `operators/test_vectors.py` (7162 lines in subdirectory)

**Output**: `operators/specialized/test_vector.py`
**Steps**:
```bash
# Consolidate vector tests
uv run pytest tests/unit/sql/where/operators/specialized/test_vector.py -v
rm tests/unit/sql/where/test_vector_operators_complete.py
rm -rf tests/unit/sql/where/operators/test_vectors.py
```

## Summary: Files to Delete After Migration

### Old Phase Files (`*_sql_building.py`)
- `test_date_operators_sql_building.py`
- `test_datetime_operators_sql_building.py`
- `test_daterange_operators_sql_building.py`
- `test_email_operators_sql_building.py`
- `test_hostname_operators_sql_building.py`
- `test_mac_address_operators_sql_building.py`
- `test_ip_operators_sql_building.py`
- `test_port_operators_sql_building.py`
- `test_coordinate_operators_sql_building.py`
- ~~`test_ltree_operators_sql_building.py`~~ ✅ DELETED

### Foundation/Intermediate Files
- `test_logical_operators_foundation.py` (superseded by complete)
- ~~`test_ltree_array_operators.py`~~ ✅ DELETED
- ~~`test_ltree_path_analysis_operators.py`~~ ✅ DELETED
- ~~`test_ltree_path_manipulation_operators.py`~~ ✅ DELETED

### Split Files (after extraction)
- `test_email_hostname_mac_complete.py` (split into 3 files)
- `test_date_datetime_port_complete.py` (split into 3 files)
- `test_network_operators_complete.py` (merged into IP tests)

### Already Consolidated
- ~~`test_ltree_operators_complete.py`~~ ✅ DELETED

**Total files to delete**: 18 files
**Total files after migration**: ~20 files (down from 33)

## Execution Strategy

### Option A: Incremental Migration (Recommended)
1. ✅ **LTree** (proof-of-concept) - COMPLETED
2. **Core** (field detection, where builder, error handling) - Low risk
3. **Simple operators** (basic, pattern, array, list, jsonb) - Low risk, just moves
4. **Logical operators** - Low risk, simple merge
5. **Temporal operators** - Medium risk, requires careful deduplication
6. **Network operators** - Medium risk, requires splitting
7. **Spatial operators** - Low risk, simple merge
8. **Vector operators** - Low risk, simple merge

After each phase:
- Run full test suite: `uv run pytest tests/unit/sql/where/ -v`
- Commit: `git add . && git commit -m "refactor(tests): consolidate [category] WHERE tests"`

### Option B: All-at-once Migration
Create script to execute all phases, then run full test suite.

**Recommendation**: Use **Option A** (incremental) to minimize risk.

## Verification Checklist

After each phase:
- [ ] All tests in new location pass
- [ ] Test count matches before/after (no lost tests)
- [ ] Old files deleted
- [ ] `__init__.py` files created in new directories
- [ ] Commit with descriptive message

Final verification:
- [ ] Run full WHERE test suite: `uv run pytest tests/unit/sql/where/ -v`
- [ ] Check test count: should be ~same total tests, just reorganized
- [ ] Verify git diff shows file moves/consolidations, not lost tests
- [ ] Update any documentation referencing old test locations

## Benefits

1. **Clarity**: Tests organized by category (core, operators, temporal, network, spatial, specialized)
2. **No duplication**: Eliminated `*_sql_building.py` vs `*_complete.py` redundancy
3. **Maintainability**: Related tests in one file (e.g., all LTree operators together)
4. **Discoverability**: Clear hierarchy makes finding tests easy
5. **Reduced clutter**: 33 files → ~20 files

## Rollback Plan

If issues arise:
```bash
git revert <commit-sha>
```

Each phase is a separate commit, allowing selective rollback.

## Next Steps

1. Review this plan
2. Execute Phase 2 (Core functionality) as next proof-of-concept
3. Continue through phases incrementally
4. Commit after each successful phase
5. Final verification and cleanup

---

**Last Updated**: 2025-12-11
**Status**: LTree proof-of-concept ✅ | Ready for Phase 2
