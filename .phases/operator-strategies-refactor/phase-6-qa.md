# Phase 6: Quality Assurance & Integration

**Phase:** QA (Comprehensive Verification)
**Duration:** 2-3 hours
**Risk:** Low

---

## Objective

**TDD Phase QA:** Comprehensive verification that refactoring is complete and correct.

Verify:
- All 4,943 tests passing
- Performance benchmarks meet or exceed baseline
- Edge cases handled correctly
- Integration with calling code works
- No memory leaks or resource issues
- Documentation accurate

---

## QA Checklist

### 1. Test Suite Validation

```bash
# Full test suite (all tests)
uv run pytest --tb=short -v

# Specific operator test suites
uv run pytest tests/unit/sql/operators/ -v
uv run pytest tests/unit/sql/where/ -v
uv run pytest tests/integration/database/repository/ -v
uv run pytest tests/integration/database/sql/ -v

# Edge case tests
uv run pytest tests/ -k "edge" -v

# Regression tests
uv run pytest tests/regression/ -v
```

**Acceptance:** All 4,943+ tests passing, zero failures

### 2. Performance Benchmarks

```bash
# Benchmark operator SQL generation
uv run pytest tests/benchmarks/test_operator_performance.py -v --benchmark-only

# Compare with baseline
# Baseline: operator_strategies.py (old)
# New: operators/ modules (new)
```

**Acceptance:** New implementation ≥ 95% of baseline performance

### 3. Integration Testing

Test integration with:
- `graphql_where_generator.py` - WHERE clause generation
- `where_clause.py` - WHERE clause objects
- `db.py` - Repository queries
- GraphQL resolvers - End-to-end queries

```bash
# Integration test suite
uv run pytest tests/integration/graphql/ -v
uv run pytest tests/integration/database/ -v

# End-to-end tests
uv run pytest tests/integration/examples/ -v
```

**Acceptance:** Zero integration failures

### 4. Edge Case Validation

Verify handling of:
- `None` values
- Empty lists
- Invalid operators
- Mixed types
- JSONB vs regular columns
- Special characters in strings
- Very large arrays
- Deeply nested JSONB
- NULL checks
- Type coercion edge cases

```bash
# Run edge case tests
uv run pytest tests/ -k "edge or null or empty or invalid" -v
```

**Acceptance:** All edge cases handled gracefully

### 5. Memory & Resource Checks

```bash
# Check for memory leaks
uv run pytest tests/integration/ --memray

# Check for unclosed resources
uv run pytest tests/integration/ --resource-check
```

**Acceptance:** No memory leaks, all resources properly closed

### 6. Code Quality Metrics

```bash
# Linting
ruff check src/fraiseql/sql/operators/

# Formatting
ruff format --check src/fraiseql/sql/operators/

# Type checking
mypy src/fraiseql/sql/operators/

# Complexity metrics
radon cc src/fraiseql/sql/operators/ -a

# Test coverage
uv run pytest --cov=src/fraiseql/sql/operators --cov-report=html
```

**Acceptance:**
- Zero linting errors
- 100% formatted
- Type checking passes
- Average complexity < 10
- Test coverage > 95%

---

## Performance Baseline Comparison

| Metric | Old (operator_strategies.py) | New (operators/) | Status |
|--------|----------------------------|------------------|---------|
| String operators | 100μs | ≤105μs | ✅ |
| Numeric operators | 80μs | ≤84μs | ✅ |
| Array operators | 150μs | ≤160μs | ✅ |
| Network operators | 120μs | ≤126μs | ✅ |
| Overall avg | 110μs | ≤116μs | ✅ |

**Goal:** < 5% performance regression acceptable

---

## Acceptance Criteria

- [ ] All 4,943 tests passing
- [ ] Performance within 5% of baseline
- [ ] All integration tests passing
- [ ] All edge cases handled
- [ ] No memory leaks
- [ ] Code quality metrics met
- [ ] Test coverage > 95%
- [ ] Zero regressions found

---

## Issues Found → Fix Before Proceeding

If QA finds issues:
1. Document the issue
2. Fix immediately
3. Re-run full QA
4. Do NOT proceed to next phase until all issues resolved

---

## Next Phase

Once QA passes:
→ **Phase 7:** Legacy Cleanup (remove operator_strategies.py)
