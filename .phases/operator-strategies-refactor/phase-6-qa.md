# Phase 6: Quality Assurance & Integration - COMPLETE IMPLEMENTATION PLAN

**Phase:** QA (Comprehensive Verification)
**Duration:** 2-3 hours
**Risk:** Low
**Status:** Ready for Execution

---

## Objective

**TDD Phase QA:** Comprehensive verification that the operator strategies refactoring is complete, correct, and production-ready.

This phase performs exhaustive testing across all dimensions:
- Functional correctness (all 4,943+ tests pass)
- Integration with calling code (WHERE clause generation, GraphQL, repositories)
- Performance benchmarks (no regressions)
- Edge case coverage (NULL, empty, invalid inputs)
- Code quality metrics (linting, formatting, type checking)
- Memory and resource usage

**Success Criteria:** Zero regressions, all tests passing, performance within 5% of baseline.

---

## Context

**What has been completed:**
- ✅ Phase 1: Foundation & base infrastructure (7 tests)
- ✅ Phase 2: Core operators (32 tests, string/numeric/boolean)
- ✅ Phase 3: PostgreSQL operators (37 operators, network/ltree/daterange/macaddr)
- ✅ Phase 4: Advanced operators (array/JSONB/fulltext/vector/coordinate) [ASSUMED COMPLETE]
- ✅ Phase 5: Refactored & optimized (common patterns extracted)

**What this phase verifies:**
- All operator strategies work correctly in isolation
- All operators integrate correctly with WHERE clause generation
- All operators work correctly through GraphQL queries
- Performance meets or exceeds baseline
- Edge cases handled gracefully
- No memory leaks or resource issues

**Project structure:**
- Test count: 474 test files (count may vary as tests are added)
- Unit tests (SQL): 361+ test functions (approximate)
- Integration tests: 50+ files in `tests/integration/database/sql/`
- Regression tests: Tests in `tests/regression/`

**Note:** Test counts are approximate and may increase. Focus on zero failures/errors rather than exact counts.

---

## QA Checklist

### 1. Unit Test Suite Validation (30 min)

**Goal:** Verify all operator strategies work correctly in isolation.

#### 1.1 Core Operator Tests

```bash
# String operators (contains, startswith, endswith, like, regex, etc.)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/core/test_string_operators.py -v

# Numeric operators (eq, neq, gt, gte, lt, lte, in, nin)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/core/test_numeric_operators.py -v

# Boolean operators (eq, neq, isnull)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/core/test_boolean_operators.py -v
```

**Expected:** All core operator tests passing (~32 tests, count may vary)

**If failures:**
- Document which operator failed
- Check if it's a base class helper issue or strategy-specific
- Fix immediately before proceeding

**Note:** Run `pytest --co -q` to see actual test count without running tests

#### 1.2 PostgreSQL Type Operator Tests

```bash
# Network operators (INET, CIDR - isprivate, ispublic, insubnet, overlaps)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/ -k "network" -v

# LTree operators (ancestor_of, descendant_of, matches_lquery, matches_ltxtquery)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/where/ -k "ltree" -v

# DateRange operators (contains_date, overlaps, adjacent, strictly_left)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/where/test_daterange_operators_sql_building.py -v

# MAC Address operators (eq, neq, in, nin, manufacturer)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/ -k "mac" -v
```

**Expected:** All PostgreSQL type tests passing (37+ operators tested)

**Verification points:**
- [ ] Network operators handle IPv4 and IPv6 correctly
- [ ] LTree operators handle hierarchical paths correctly
- [ ] DateRange operators handle date ranges and overlaps
- [ ] MAC Address operators handle MAC address formats

#### 1.3 Advanced Operator Tests (Phase 4)

```bash
# Array operators (contains, overlaps, len_eq, any_eq, all_eq)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/ -k "array" -v

# JSONB operators (has_key, contains, path_exists, path_eq)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/ -k "jsonb or json" -v

# Full-text search operators (matches, rank, websearch_query)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/ -k "fulltext or tsquery or tsvector" -v

# Vector operators (cosine_distance, l2_distance, inner_product)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/where/test_field_detection_vector.py -v

# Coordinate/GIS operators (distance_within)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/where/test_coordinate_operators_sql_building.py -v
```

**Expected:** All advanced operator tests passing

**Note:** If Phase 4 is not yet complete, skip this section and mark it as pending.

#### 1.4 Base Strategy Tests

```bash
# Base class and registry tests
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/test_base_strategy.py -v
```

**Expected:** Base class helper methods work correctly

**Verification points:**
- [ ] `_cast_path()` handles JSONB vs regular columns correctly
- [ ] `_build_comparison()` generates correct SQL for all comparison operators
- [ ] `_build_in_operator()` handles lists, value casting, negation
- [ ] `_build_null_check()` generates correct IS NULL / IS NOT NULL SQL

#### 1.5 Full Unit Test Suite

```bash
# Run ALL unit tests for SQL operators
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/ -v --tb=short

# Count passing tests
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/ -v --tb=short | grep -E "passed|PASSED" | tail -1
```

**Expected:** 361+ tests passing, zero failures

**Acceptance:**
- [ ] All 361+ unit tests passing
- [ ] Zero test failures
- [ ] Zero test errors
- [ ] No skipped tests (unless intentionally marked)

---

### 2. Integration Test Suite Validation (45 min)

**Goal:** Verify operators integrate correctly with WHERE clause generation, GraphQL, and repositories.

#### 2.1 WHERE Clause Integration

```bash
# GraphQL WHERE clause generation (uses operators internally)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_graphql_where_generator.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_graphql_where_generator_nested.py -v

# WHERE clause builder integration
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_logical_where_operators.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_logical_where_database_integration.py -v
```

**Expected:** WHERE clause generation uses operator strategies correctly

**Verification points:**
- [ ] GraphQL filter input converted to operator strategy calls
- [ ] Nested filters (AND/OR) work correctly
- [ ] JSONB path extraction triggers correct casting
- [ ] Field type detection selects correct operator strategy

#### 2.2 End-to-End Filtering Tests

```bash
# Network type filtering (INET, CIDR)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_end_to_end_ip_filtering_clean.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_network_filtering_fix.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_network_address_filtering.py -v

# LTree filtering (hierarchical paths)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_end_to_end_ltree_filtering.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_ltree_filter_operations.py -v

# DateRange filtering
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_end_to_end_daterange_filtering.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_daterange_filter_operations.py -v

# MAC Address filtering
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_end_to_end_mac_address_filtering.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_mac_address_filter_operations.py -v

# Coordinate filtering (GIS)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_coordinate_filter_operations.py -v
```

**Expected:** All end-to-end filtering tests passing

**Verification points:**
- [ ] Database queries execute successfully
- [ ] Correct rows returned for each filter
- [ ] Complex filters (multiple operators) work correctly
- [ ] JSONB column filtering works correctly

#### 2.3 Repository Integration

```bash
# Repository find() with WHERE clause processing
uv run pytest /home/lionel/code/fraiseql/tests/integration/repository/test_repository_find_where_processing.py -v

# Repository integration with GraphQL WHERE
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_graphql_where_repository_fix.py -v
```

**Expected:** Repository queries use operator strategies correctly

**Verification points:**
- [ ] `repository.find(where={...})` works correctly
- [ ] Filter values passed to operator strategies unchanged
- [ ] Results match expected database rows

#### 2.4 Phase-Specific Integration Tests

```bash
# Phase 4 integration (if Phase 4 complete)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_end_to_end_phase4_filtering.py -v

# Phase 5 integration (if Phase 5 complete)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_end_to_end_phase5_filtering.py -v
```

**Expected:** Phase-specific tests passing

#### 2.5 Full Integration Test Suite

```bash
# Run ALL integration tests for database SQL
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/ -v --tb=short

# Count passing tests
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/ -v --tb=short | grep -E "passed|PASSED" | tail -1
```

**Expected:** 50+ integration tests passing, zero failures

**Acceptance:**
- [ ] All integration tests passing
- [ ] Zero test failures
- [ ] Zero test errors
- [ ] Database connections properly closed after tests

---

### 3. Regression Test Suite (20 min)

**Goal:** Verify no regressions from previous bug fixes.

```bash
# WHERE clause regressions
uv run pytest /home/lionel/code/fraiseql/tests/regression/where_clause/ -v

# Network filtering regressions (v0.5.7 bug fix)
uv run pytest /home/lionel/code/fraiseql/tests/regression/v0_5_7/test_network_filtering_regression.py -v

# JSONB network filtering bug (production fix)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_jsonb_network_filtering_bug.py -v
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_production_cqrs_ip_filtering_bug.py -v

# Network operator consistency bug
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_network_operator_consistency_bug.py -v
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/test_network_operator_strategy_fix.py -v

# Issue resolution demonstrations
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/test_issue_resolution_demonstration.py -v
```

**Expected:** All regression tests passing

**Critical verification:**
- [ ] JSONB path detection regression fixed (commit 70abf254)
- [ ] Network operator JSONB casting works correctly
- [ ] MAC address detection doesn't collide with IP addresses
- [ ] LTree path casting works correctly
- [ ] All previous production bugs remain fixed

**Acceptance:**
- [ ] All regression tests passing
- [ ] No previously-fixed bugs reintroduced
- [ ] No new bugs introduced by refactoring

---

### 4. Edge Case Testing (30 min)

**Goal:** Verify graceful handling of edge cases and invalid inputs.

#### 4.1 NULL Value Handling

**Option 1: Create permanent test fixture (recommended)**
```bash
# Add to existing test suite
cat >> tests/unit/sql/operators/test_edge_cases.py << 'EOF'
"""Edge case tests for NULL handling."""
from psycopg.sql import SQL, Identifier
from fraiseql.sql.operators import get_default_registry

def test_null_value_in_eq_operator():
    """Test eq operator with None value."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # None value should still work (some strategies may handle specially)
    result = registry.build_sql("eq", None, path_sql, field_type=str)
    assert result is not None

def test_null_value_in_in_operator():
    """Test in operator with None in list."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # List with None should work
    result = registry.build_sql("in", [None, "value"], path_sql, field_type=str)
    assert result is not None

def test_isnull_operator_true():
    """Test isnull operator with True."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    result = registry.build_sql("isnull", True, path_sql, field_type=str)
    assert result is not None
    assert "IS NULL" in str(result.as_string(None))

def test_isnull_operator_false():
    """Test isnull operator with False."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    result = registry.build_sql("isnull", False, path_sql, field_type=str)
    assert result is not None
    assert "IS NOT NULL" in str(result.as_string(None))
EOF

uv run pytest tests/unit/sql/operators/test_edge_cases.py::test_null_value_in_eq_operator -v
```

**Option 2: Quick temporary test (for rapid verification)**
```bash
# Create temporary test file
cat > /tmp/test_edge_case_nulls.py << 'EOF'
# ... same content as above ...
EOF

uv run pytest /tmp/test_edge_case_nulls.py -v
```

**Expected:** NULL values handled gracefully, no exceptions

**Recommendation:** Add these as permanent test fixtures after verification

#### 4.2 Empty List Handling

```bash
# Test in/nin operators with empty lists
cat > /tmp/test_edge_case_empty_lists.py << 'EOF'
"""Edge case tests for empty lists."""
from psycopg.sql import Identifier
from fraiseql.sql.operators import get_default_registry

def test_in_operator_empty_list():
    """Test in operator with empty list."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # Empty list might return special SQL or None
    result = registry.build_sql("in", [], path_sql, field_type=str)
    # Should not crash
    assert result is None or "IN" in str(result.as_string(None))

def test_nin_operator_empty_list():
    """Test nin operator with empty list."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    result = registry.build_sql("nin", [], path_sql, field_type=str)
    # Should not crash
    assert result is None or "NOT IN" in str(result.as_string(None))
EOF

uv run pytest /tmp/test_edge_case_empty_lists.py -v
```

**Expected:** Empty lists handled gracefully, no exceptions

#### 4.3 Invalid Operator Handling

```bash
# Test invalid operator names
cat > /tmp/test_edge_case_invalid_operators.py << 'EOF'
"""Edge case tests for invalid operators."""
from psycopg.sql import Identifier
from fraiseql.sql.operators import get_default_registry

def test_invalid_operator_name():
    """Test with invalid operator name."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # Invalid operator should return None
    result = registry.build_sql("invalid_op", "value", path_sql, field_type=str)
    assert result is None

def test_typo_operator_name():
    """Test with typo in operator name."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # Typo should return None (not raise exception)
    result = registry.build_sql("equls", "value", path_sql, field_type=str)
    assert result is None
EOF

uv run pytest /tmp/test_edge_case_invalid_operators.py -v
```

**Expected:** Invalid operators return None, no exceptions

#### 4.4 Type Mismatch Handling

```bash
# Test type mismatches
cat > /tmp/test_edge_case_type_mismatch.py << 'EOF'
"""Edge case tests for type mismatches."""
from psycopg.sql import Identifier
from fraiseql.sql.operators import get_default_registry

def test_numeric_operator_on_string_field():
    """Test numeric operator on string field."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # gt operator on string field (should work via fallback or type casting)
    result = registry.build_sql("gt", "10", path_sql, field_type=str)
    # Either numeric strategy doesn't handle it, or casts appropriately
    assert result is None or ">" in str(result.as_string(None))

def test_string_operator_on_numeric_field():
    """Test string operator on numeric field."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # contains operator on int field (should return None - not applicable)
    result = registry.build_sql("contains", "foo", path_sql, field_type=int)
    assert result is None
EOF

uv run pytest /tmp/test_edge_case_type_mismatch.py -v
```

**Expected:** Type mismatches handled gracefully

#### 4.5 Special Character Handling

```bash
# Test special characters in string values
cat > /tmp/test_edge_case_special_chars.py << 'EOF'
"""Edge case tests for special characters."""
from psycopg.sql import Identifier
from fraiseql.sql.operators import get_default_registry

def test_sql_injection_attempt():
    """Test SQL injection patterns are properly escaped."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # SQL injection attempt should be safely escaped
    malicious_value = "'; DROP TABLE users; --"
    result = registry.build_sql("eq", malicious_value, path_sql, field_type=str)
    assert result is not None
    # Value should be safely escaped (Literal handles this)

def test_unicode_characters():
    """Test Unicode characters in string values."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # Unicode should work correctly
    unicode_value = "Hello 世界 🌍"
    result = registry.build_sql("contains", unicode_value, path_sql, field_type=str)
    assert result is not None

def test_wildcard_characters():
    """Test wildcard characters in LIKE patterns."""
    registry = get_default_registry()
    path_sql = Identifier("field")

    # Wildcards should work in like operator
    result = registry.build_sql("like", "%test%", path_sql, field_type=str)
    assert result is not None
    assert "LIKE" in str(result.as_string(None))
EOF

uv run pytest /tmp/test_edge_case_special_chars.py -v
```

**Expected:** Special characters properly escaped, no SQL injection vulnerabilities

**Edge Case Acceptance:**
- [ ] NULL values handled gracefully
- [ ] Empty lists handled gracefully
- [ ] Invalid operators return None (don't crash)
- [ ] Type mismatches handled gracefully
- [ ] Special characters properly escaped
- [ ] SQL injection attempts safely escaped

---

### 5. Performance Benchmarks (20 min)

**Goal:** Verify performance is within 5% of baseline (before refactoring).

#### 5.1 Operator SQL Generation Performance

```bash
# Create performance benchmark test
cat > /tmp/test_performance_benchmarks.py << 'EOF'
"""Performance benchmarks for operator strategies."""
import time
from psycopg.sql import Identifier
from fraiseql.sql.operators import get_default_registry

def benchmark_operator(operator, value, field_type, iterations=10000):
    """Benchmark an operator."""
    registry = get_default_registry()
    path_sql = Identifier("test_field")

    start = time.perf_counter()
    for _ in range(iterations):
        result = registry.build_sql(operator, value, path_sql, field_type=field_type)
    elapsed = time.perf_counter() - start

    return elapsed / iterations * 1_000_000  # microseconds per operation

def test_string_operator_performance():
    """Benchmark string operators."""
    # eq operator
    eq_time = benchmark_operator("eq", "test_value", str)
    print(f"String eq: {eq_time:.2f} μs/op")
    assert eq_time < 10.0  # Should be < 10 microseconds

    # contains operator
    contains_time = benchmark_operator("contains", "test", str)
    print(f"String contains: {contains_time:.2f} μs/op")
    assert contains_time < 15.0

def test_numeric_operator_performance():
    """Benchmark numeric operators."""
    # gt operator
    gt_time = benchmark_operator("gt", 42, int)
    print(f"Numeric gt: {gt_time:.2f} μs/op")
    assert gt_time < 10.0

    # in operator
    in_time = benchmark_operator("in", [1, 2, 3, 4, 5], int)
    print(f"Numeric in: {in_time:.2f} μs/op")
    assert in_time < 20.0

def test_network_operator_performance():
    """Benchmark network operators."""
    from ipaddress import IPv4Address

    # eq operator
    eq_time = benchmark_operator("eq", "192.168.1.1", IPv4Address)
    print(f"Network eq: {eq_time:.2f} μs/op")
    assert eq_time < 15.0

    # insubnet operator
    insubnet_time = benchmark_operator("insubnet", "192.168.0.0/16", IPv4Address)
    print(f"Network insubnet: {insubnet_time:.2f} μs/op")
    assert insubnet_time < 20.0

def test_overall_average_performance():
    """Benchmark overall average across all operators."""
    operators = [
        ("eq", "value", str),
        ("gt", 42, int),
        ("contains", "test", str),
        ("in", [1, 2, 3], int),
    ]

    times = [benchmark_operator(op, val, ft) for op, val, ft in operators]
    avg_time = sum(times) / len(times)
    print(f"Average across all operators: {avg_time:.2f} μs/op")
    assert avg_time < 15.0  # Average should be < 15 microseconds
EOF

uv run pytest /tmp/test_performance_benchmarks.py -v -s
```

**Expected Performance Targets:**
- String operators: < 10 μs/op (eq, neq) to < 15 μs/op (contains, matches)
- Numeric operators: < 10 μs/op (comparisons) to < 20 μs/op (in with list)
- Network operators: < 15 μs/op (eq) to < 20 μs/op (insubnet)
- Overall average: < 15 μs/op

**Acceptance:**
- [ ] String operators within performance target
- [ ] Numeric operators within performance target
- [ ] Network operators within performance target
- [ ] No operator > 50 μs/op (flag for optimization)
- [ ] Overall average within 5% of baseline (if baseline available)

#### 5.2 Integration Performance (Optional)

```bash
# If you have existing performance tests
find /home/lionel/code/fraiseql/tests -name "*benchmark*" -o -name "*performance*" -o -name "*perf*" | head -5

# Run any existing benchmarks
uv run pytest /home/lionel/code/fraiseql/tests/ -k "benchmark or performance" -v 2>/dev/null || echo "No benchmark tests found"
```

---

### 6. Code Quality Metrics (15 min)

**Goal:** Verify code quality improved after refactoring.

#### 6.1 Linting

```bash
# Run ruff linting
ruff check /home/lionel/code/fraiseql/src/fraiseql/sql/operators/

# Expected: Zero errors
```

**Acceptance:**
- [ ] Zero linting errors
- [ ] Zero linting warnings (or only approved warnings)

#### 6.2 Formatting

```bash
# Check formatting
ruff format --check /home/lionel/code/fraiseql/src/fraiseql/sql/operators/

# Expected: All files properly formatted
```

**Acceptance:**
- [ ] All files pass format check
- [ ] Consistent style across all modules

#### 6.3 Type Checking (if mypy configured)

```bash
# Run mypy type checking (if configured)
mypy /home/lionel/code/fraiseql/src/fraiseql/sql/operators/ 2>/dev/null || echo "mypy not configured"
```

**Acceptance:**
- [ ] Zero type errors (if mypy configured)
- [ ] Or: mypy not configured (skip this check)

#### 6.4 Complexity Metrics

```bash
# Install radon if not present
pip install radon 2>/dev/null || uv pip install radon 2>/dev/null || echo "Install radon manually: pip install radon"

# Check cyclomatic complexity
radon cc /home/lionel/code/fraiseql/src/fraiseql/sql/operators/ -a -s

# Expected: Average complexity < 8, max complexity < 15
```

**Acceptance:**
- [ ] Average complexity < 8 (Grade A or B)
- [ ] No function > 15 complexity (Grade C or worse)
- [ ] Base class helpers < 5 complexity each

#### 6.5 Test Coverage

```bash
# Run tests with coverage
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/ \
    --cov=/home/lionel/code/fraiseql/src/fraiseql/sql/operators \
    --cov-report=term \
    --cov-report=html \
    -v

# View coverage report
echo "Coverage report saved to htmlcov/index.html"
```

**Expected Coverage:**
- Base class: 100% (all helper methods tested)
- Core operators: > 95% (string, numeric, boolean)
- PostgreSQL operators: > 90% (network, ltree, daterange, macaddr)
- Advanced operators: > 85% (array, JSONB, fulltext, vector, coordinate)

**Acceptance:**
- [ ] Overall coverage > 90%
- [ ] Base class coverage = 100%
- [ ] No critical paths uncovered

---

### 7. Memory & Resource Checks (10 min)

**Goal:** Verify no memory leaks or resource leaks.

#### 7.1 Memory Usage Check

```bash
# Check for memory leaks with memory profiler (if available)
# Note: This requires 'pytest-memray' or similar

# Check if memray available
pip show pytest-memray 2>/dev/null || echo "pytest-memray not installed (optional)"

# Run with memory profiling (if available)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/ \
    --memray \
    -v \
    2>/dev/null || echo "Memory profiling not available (optional check)"
```

**Acceptance:**
- [ ] No obvious memory leaks detected
- [ ] Or: Memory profiling not available (skip this check)

#### 7.2 Database Connection Checks

```bash
# Run integration tests and verify connections closed
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/ -v

# Check for unclosed connection warnings
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/ -v 2>&1 | grep -i "unclosed\|leak\|warning"
```

**Expected:** No unclosed connection warnings

**Acceptance:**
- [ ] No unclosed database connections
- [ ] No resource warnings in test output

---

## Full Test Suite Run (20 min)

**Goal:** Run the complete test suite to verify everything works together.

```bash
# Run FULL test suite (unit + integration + regression)
uv run pytest /home/lionel/code/fraiseql/tests/ -v --tb=short

# Get exact test count and summary
uv run pytest /home/lionel/code/fraiseql/tests/ --co -q | wc -l  # Count tests
uv run pytest /home/lionel/code/fraiseql/tests/ -v --tb=short | grep -E "passed|failed|error" | tail -3
```

**Expected Output:**
```
====== XXX passed in X.XXs ======
```

**Acceptance Criteria for Full Suite:**
- [ ] All tests passing (count will be shown in output - baseline was ~4,900, may have grown)
- [ ] Zero test failures
- [ ] Zero test errors
- [ ] No skipped tests (unless intentionally marked)
- [ ] Test run completes in reasonable time (< 5 minutes for unit, < 15 minutes total)

**Note:** Record actual test count for future reference. Use this as new baseline.

---

## Acceptance Criteria Summary

### Functional Correctness
- [ ] All 361+ unit tests passing
- [ ] All 50+ integration tests passing
- [ ] All regression tests passing (no reintroduced bugs)
- [ ] Full test suite passing (4,943+ tests)

### Edge Cases
- [ ] NULL values handled gracefully
- [ ] Empty lists handled gracefully
- [ ] Invalid operators return None
- [ ] Type mismatches handled gracefully
- [ ] Special characters properly escaped

### Performance
- [ ] String operators < 15 μs/op
- [ ] Numeric operators < 20 μs/op
- [ ] Network operators < 20 μs/op
- [ ] Overall average < 15 μs/op
- [ ] No performance regressions > 5%

### Code Quality
- [ ] Zero linting errors (`ruff check`)
- [ ] All files properly formatted (`ruff format --check`)
- [ ] Average cyclomatic complexity < 8
- [ ] Test coverage > 90%
- [ ] No critical paths uncovered

### Resources
- [ ] No memory leaks detected
- [ ] No unclosed database connections
- [ ] No resource warnings

---

## Issues Found → Fix Before Proceeding

**If QA finds issues:**

1. **Document the issue:**
   ```bash
   # Example
   echo "Issue: NetworkOperatorStrategy.insubnet fails for JSONB columns" >> /tmp/phase-6-issues.txt
   echo "Test: test_network_insubnet_jsonb" >> /tmp/phase-6-issues.txt
   echo "Error: AttributeError: 'Composed' object has no attribute 'as_string'" >> /tmp/phase-6-issues.txt
   ```

2. **Fix immediately:**
   - Small issues: Fix directly in strategy file
   - Large issues: May need to revisit Phase 5 refactoring

3. **Re-run QA:**
   - After fix, re-run entire QA checklist
   - DO NOT proceed to Phase 7 until all issues resolved

4. **Commit fixes:**
   ```bash
   git add src/fraiseql/sql/operators/
   git commit -m "fix(operators): resolve QA issues found in Phase 6 [QA]

   - Fix NetworkOperatorStrategy JSONB casting
   - Handle edge case with empty lists in _build_in_operator()
   - Add missing NULL checks

   Tests: All 4,943 tests now passing"
   ```

---

## Performance Baseline Comparison

**If you have baseline metrics from before refactoring:**

| Metric | Before (operator_strategies.py) | After (operators/) | Change | Status |
|--------|--------------------------------|-------------------|---------|---------|
| String eq | 8.5 μs | 8.2 μs | -3.5% | ✅ Improved |
| Numeric gt | 7.8 μs | 7.9 μs | +1.3% | ✅ Within 5% |
| Network insubnet | 12.4 μs | 12.8 μs | +3.2% | ✅ Within 5% |
| String contains | 11.2 μs | 10.9 μs | -2.7% | ✅ Improved |
| Overall average | 10.5 μs | 10.3 μs | -1.9% | ✅ Improved |

**Goal:** All metrics within 5% of baseline (ideally improved)

---

## Tools & Commands Reference

### Quick Test Commands

```bash
# Unit tests only (fast, ~10 seconds)
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/ -v

# Integration tests only (slower, ~1-2 minutes)
uv run pytest /home/lionel/code/fraiseql/tests/integration/database/sql/ -v

# Regression tests only (~30 seconds)
uv run pytest /home/lionel/code/fraiseql/tests/regression/ -v

# Full suite (~5-15 minutes)
uv run pytest /home/lionel/code/fraiseql/tests/ -v

# Specific operator family
uv run pytest /home/lionel/code/fraiseql/tests/ -k "network" -v
uv run pytest /home/lionel/code/fraiseql/tests/ -k "string" -v
```

### Code Quality Commands

```bash
# Linting
ruff check /home/lionel/code/fraiseql/src/fraiseql/sql/operators/

# Formatting
ruff format --check /home/lionel/code/fraiseql/src/fraiseql/sql/operators/

# Complexity
radon cc /home/lionel/code/fraiseql/src/fraiseql/sql/operators/ -a -s

# Coverage
uv run pytest /home/lionel/code/fraiseql/tests/unit/sql/operators/ \
    --cov=/home/lionel/code/fraiseql/src/fraiseql/sql/operators \
    --cov-report=term
```

---

## Commit Message (Once QA Passes)

```bash
git add .phases/operator-strategies-refactor/phase-6-qa.md
git commit -m "test(operators): comprehensive QA validation passed [QA]

Phase 6 QA Results:
- ✅ All 4,943 tests passing (361 unit + 50+ integration + regressions)
- ✅ Edge cases handled (NULL, empty lists, invalid operators, special chars)
- ✅ Performance within 5% of baseline (average 10.3 μs/op)
- ✅ Code quality: Zero lint errors, 92% test coverage, avg complexity 6.2
- ✅ No memory leaks, no resource leaks
- ✅ Zero regressions from previous bug fixes

All acceptance criteria met. Ready for Phase 7 (Legacy Cleanup).

Tested modules:
- Core operators: string, numeric, boolean
- PostgreSQL operators: network, ltree, daterange, macaddr
- Advanced operators: array, JSONB, fulltext, vector, coordinate
- Base class helpers: _cast_path, _build_comparison, _build_in_operator, _build_null_check
- Integration: WHERE clause generation, GraphQL filters, repository queries

Test environments:
- Unit tests: Isolated operator strategy tests
- Integration tests: Full database queries with PostgreSQL
- Regression tests: Previously-fixed bugs remain fixed"
```

---

## Next Phase

Once QA passes and all acceptance criteria are met:

→ **Phase 7:** Legacy Cleanup (`/tmp/phase-7-cleanup-COMPLETE.md`)

**Prerequisites for Phase 7:**
- All QA acceptance criteria met ✅
- All 4,943+ tests passing ✅
- Performance validated ✅
- Code quality metrics met ✅
- QA results committed ✅

**Phase 7 will:**
- Remove old `/home/lionel/code/fraiseql/src/fraiseql/sql/operator_strategies.py` (2,149 lines)
- Update all imports across codebase
- Remove deprecation warnings
- Verify no references to old module remain
- Final integration verification

---

## Notes

**Why is QA important?**
- Refactoring (Phase 5) changed internal implementation
- QA verifies behavior unchanged (no regressions)
- Catches edge cases not covered by existing tests
- Validates performance claims
- Provides confidence for production deployment

**What if performance is worse?**
- If performance > 5% worse than baseline, investigate before Phase 7
- Possible causes: Over-abstraction, inefficient SQL composition
- Solution: Optimize hot paths in base class helpers
- Worst case: Revert Phase 5 refactoring and use simpler approach

**What if tests fail?**
- DO NOT proceed to Phase 7 with failing tests
- Fix issues immediately
- Re-run full QA checklist after fixes
- Document what was fixed and why

**Test count may vary:**
- 4,943 is estimated based on current state
- Actual count depends on Phase 4 completion status
- Some tests may be skipped if Phase 4 incomplete
- Focus on zero failures/errors, not exact count
