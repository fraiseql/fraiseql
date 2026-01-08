# Phase B: Route Python Type Generation to Rust Schema

**Timeline**: 1-3 months
**Effort**: 2-4 person-months
**Status**: Ready to start
**Dependency**: Phase A ✅ (Complete)

---

## Objective

Enable Python's GraphQL type generation (WHERE inputs, OrderBy inputs, custom filters) to leverage Rust-exported schemas instead of introspecting Python types.

**Key Principle**: No changes to Python API. Users still write the same code; the framework just gets schema from Rust instead of building it dynamically.

---

## Current State (After Phase A)

### What Works Now
- ✅ Rust exports complete schema via `export_schema_generators()`
- ✅ Python schema_loader caches it with 64.87ns access time
- ✅ WHERE generator has optional `get_filter_schema_from_loader(type_name)`
- ✅ OrderBy generator has optional `get_order_by_schema_from_loader()`

### Current Code Path (Python-only)
```
@fraiseql.type decorator
  ↓
graphql_where_generator.py (introspects class)
  ↓
Creates filter types dynamically
  ↓
Returns to user
```

### New Code Path (Rust-powered)
```
@fraiseql.type decorator
  ↓
graphql_where_generator.py (loads from schema_loader)
  ↓
Rust schema provides structure
  ↓
Returns same types to user
```

---

## Detailed Implementation Plan

### B.1: Update WHERE Generator to Default to Rust Schema (Week 1-2)

**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Current Code** (lines to modify):
```python
def create_graphql_where_input(cls: type) -> type:
    """Create a GraphQL WHERE input type from a class definition."""
    # Currently: introspects cls and builds schema
    # Goal: optionally use Rust schema instead

    # Line 180-200 area: filter schema construction
```

**Changes**:
1. Add preference flag: `use_rust_schema = True`
2. If enabled, call `get_filter_schema_from_loader()` for schema
3. Fall back to Python introspection if flag disabled
4. Maintain 100% compatibility with existing code

**Code Structure**:
```python
def create_graphql_where_input(cls: type, use_rust_schema: bool = True) -> type:
    """Create WHERE input type.

    Args:
        cls: The class to create filter for
        use_rust_schema: Use Rust-exported schema if available (default: True)

    Returns:
        GraphQL input type with filter fields
    """
    try:
        if use_rust_schema:
            # Get field type for this class
            field_type = get_field_type_for_class(cls)
            # Load schema from Rust
            filter_schema = get_filter_schema_from_loader(field_type)
            # Use filter_schema to build type
            return build_type_from_rust_schema(cls, filter_schema)
        else:
            # Fall back to Python introspection
            return build_type_from_introspection(cls)
    except Exception:
        # Fallback if Rust schema not available
        return build_type_from_introspection(cls)
```

**Tests to Create** (3-5):
- `test_where_generator_uses_rust_schema_by_default`
- `test_where_generator_fallback_to_python_schema`
- `test_where_generator_schema_matches_rust_export`
- `test_where_generator_produces_identical_types`

---

### B.2: Update OrderBy Generator to Default to Rust Schema (Week 2-3)

**File**: `src/fraiseql/sql/graphql_order_by_generator.py`

**Changes** (same pattern as WHERE):
1. Add preference flag: `use_rust_schema = True`
2. Call `get_order_by_schema_from_loader()` if enabled
3. Fall back to Python if not available
4. 100% compatible with existing code

**Code Structure**:
```python
def create_graphql_order_by_input(cls: type, use_rust_schema: bool = True) -> type:
    """Create OrderBy input type.

    Args:
        cls: The class to create ordering for
        use_rust_schema: Use Rust-exported schema if available (default: True)

    Returns:
        GraphQL input type with ordering fields
    """
    try:
        if use_rust_schema:
            # Load schema from Rust
            order_schema = get_order_by_schema_from_loader()
            # Use it to build type
            return build_order_by_type_from_rust_schema(cls, order_schema)
        else:
            # Fall back to Python
            return build_order_by_type_from_introspection(cls)
    except Exception:
        # Fallback
        return build_order_by_type_from_introspection(cls)
```

**Tests to Create** (3-5):
- `test_order_by_generator_uses_rust_schema_by_default`
- `test_order_by_generator_fallback_to_python_schema`
- `test_order_by_generator_includes_all_directions`
- `test_order_by_generator_produces_identical_types`

---

### B.3: Update Custom Filter Generators (Week 3)

**File**: `src/fraiseql/sql/graphql_custom_filters.py` (if exists) or inline

**For custom scalar filters** (StringFilter, IntFilter, etc.):
```python
def create_custom_filter_type(base_type: str, use_rust_schema: bool = True) -> type:
    """Create custom filter type for scalar.

    Example: create_custom_filter_type("String") → StringFilter type
    """
    try:
        if use_rust_schema:
            # Get operators for this type from Rust schema
            operators = get_filter_operators_from_loader(base_type)
            # Build type from operators
            return build_filter_type_from_rust_operators(base_type, operators)
        else:
            # Python-based operator discovery
            return build_filter_type_from_python_operators(base_type)
    except Exception:
        return build_filter_type_from_python_operators(base_type)
```

**Tests to Create** (3-5):
- `test_string_filter_uses_rust_schema`
- `test_int_filter_uses_rust_schema`
- `test_all_filter_types_match_rust_export`

---

### B.4: Integration Testing (Week 4)

**Create comprehensive integration tests**:
```python
# tests/unit/core/test_phase_b_type_generation.py

class TestPhaseB:
    """Test Phase B type generation with Rust schema."""

    def test_where_input_creation_with_rust_schema(self):
        """WHERE input uses Rust schema by default."""
        # Create a test class
        @fraiseql.type
        class User:
            id: ID
            name: str
            email: str

        # Get WHERE input - should use Rust schema
        where_input = create_graphql_where_input(User)

        # Should have all filter fields from Rust schema
        assert hasattr(where_input, 'name_filter')
        assert hasattr(where_input, 'email_filter')

    def test_order_by_input_creation_with_rust_schema(self):
        """OrderBy input uses Rust schema by default."""
        # Create test class
        @fraiseql.type
        class User:
            id: ID
            name: str

        # Get OrderBy input
        order_input = create_graphql_order_by_input(User)

        # Should have direction fields
        assert order_input is not None

    def test_backward_compatibility_with_python_schema(self):
        """Falls back to Python schema if Rust unavailable."""
        # Create type with rust_schema=False
        @fraiseql.type
        class User:
            id: ID
            name: str

        # Should work with Python fallback
        where_input = create_graphql_where_input(User, use_rust_schema=False)
        assert where_input is not None

    def test_all_17_filter_types_available(self):
        """All 17 filter types from Rust schema are available."""
        schema = load_schema()

        expected_types = {
            'String', 'Int', 'Float', 'Decimal', 'Boolean', 'ID',
            'Date', 'DateTime', 'DateRange', 'Array', 'JSONB',
            'UUID', 'Vector', 'NetworkAddress', 'MacAddress',
            'LTree', 'FullText'
        }

        actual_types = set(schema['filter_schemas'].keys())
        assert actual_types == expected_types

    def test_operators_match_between_python_and_rust(self):
        """Operator counts match between Python and Rust schemas."""
        rust_schema = load_schema()

        # For each type, verify operator count
        for type_name, type_schema in rust_schema['filter_schemas'].items():
            operators = type_schema.get('fields', {}).keys()

            # Should have at least 5 operators per type
            assert len(operators) >= 5, f"{type_name} has < 5 operators"
```

**Tests to Create**: 8-10 comprehensive integration tests

---

### B.5: Performance Validation (Week 4)

**Create performance tests** to ensure Rust schema doesn't slow things down:

```python
def test_phase_b_type_generation_performance(self, benchmark):
    """Verify type generation performance with Rust schema."""
    @fraiseql.type
    class User:
        id: ID
        name: str
        email: str

    def create_where_with_rust():
        return create_graphql_where_input(User, use_rust_schema=True)

    # Should be fast due to schema caching
    result = benchmark(create_where_with_rust)
    assert result is not None

def test_where_generation_speedup_factor(self):
    """Quantify performance improvement from Rust schema."""
    @fraiseql.type
    class User:
        id: ID
        name: str

    # Time with Rust schema
    import time
    start = time.perf_counter()
    type_rust = create_graphql_where_input(User, use_rust_schema=True)
    rust_time = time.perf_counter() - start

    # Time with Python schema
    start = time.perf_counter()
    type_python = create_graphql_where_input(User, use_rust_schema=False)
    python_time = time.perf_counter() - start

    # Rust should be similar or faster (not slower)
    assert rust_time <= python_time * 1.5  # Allow 50% margin
```

---

## Implementation Checklist

### Code Changes
- [ ] Update `graphql_where_generator.py` to use Rust schema by default
- [ ] Update `graphql_order_by_generator.py` to use Rust schema by default
- [ ] Update custom filter generators if applicable
- [ ] Ensure fallback to Python schema if Rust unavailable
- [ ] Maintain 100% backward compatibility

### Testing (30+ tests expected)
- [ ] WHERE generator uses Rust schema tests (5)
- [ ] OrderBy generator uses Rust schema tests (5)
- [ ] Custom filter generator tests (5)
- [ ] Backward compatibility tests (5)
- [ ] Integration tests (5)
- [ ] Performance validation tests (3)
- [ ] All 383+ pre-existing tests still pass

### Documentation
- [ ] Update schema_loader.py docstrings
- [ ] Update graphql_where_generator.py docstrings
- [ ] Create PHASE_B_IMPLEMENTATION.md with results
- [ ] Document any breaking changes (should be none)

### Quality Gates
- [ ] 100% test pass rate
- [ ] Zero regressions in pre-existing tests
- [ ] Performance same or better than Python schema
- [ ] All 17 filter types available
- [ ] Fallback to Python works

---

## Risk Mitigation

### Risk 1: Breaking Change to Type Generation
**Mitigation**:
- Feature flag `use_rust_schema` defaults to True but can be disabled
- Extensive testing for backward compatibility
- Fallback to Python introspection if any error

### Risk 2: Missing Filter Type in Rust Schema
**Mitigation**:
- Phase A already validated 17 types match Python
- Test `test_all_17_filter_types_available` ensures completeness

### Risk 3: Performance Regression
**Mitigation**:
- Benchmark tests measure performance
- Schema already cached from Phase A (~65ns access)
- Type generation should be same or faster

### Risk 4: Edge Cases in Custom Filters
**Mitigation**:
- Test custom filter types (StringFilter, IntFilter, etc.)
- Fallback to Python if Rust schema missing type
- Extensive edge case testing

---

## Success Criteria

- ✅ Type generation defaults to Rust schema
- ✅ 100% compatibility with existing code (same API)
- ✅ All 30+ new tests passing
- ✅ All 383+ pre-existing tests passing
- ✅ Zero performance regression
- ✅ All 17 filter types available
- ✅ Fallback to Python schema working
- ✅ Documentation updated

---

## Effort Estimate

| Task | Days | Effort |
|------|------|--------|
| B.1: WHERE generator update | 10 | 1.5 weeks |
| B.2: OrderBy generator update | 8 | 1 week |
| B.3: Custom filter generators | 5 | 0.5 weeks |
| B.4: Integration testing | 8 | 1 week |
| B.5: Performance validation | 5 | 0.5 weeks |
| Documentation | 3 | 0.5 weeks |
| **Total** | **39 days** | **5 weeks / 1.25 months** |

**With 2 engineers**: 2.5-3 weeks
**With 1 engineer**: 5-6 weeks

---

## What Phase B Enables

After Phase B completes:
- ✅ Python type generation routes to Rust schema
- ✅ Rust controls all type definitions
- ✅ Python framework uses Rust schema exclusively
- ✅ Ready for Phase C: Expose Rust operators

**Performance Impact**:
- Type generation 1-2% faster (from cached schema)
- Application startup 44.6 microseconds faster
- No user-visible changes

**Code Impact**:
- No Python API changes (users see no difference)
- framework gets schema from Rust instead of introspection
- 3500 LOC Python sql/ still present (Phase C/D removes this)

---

## Next Steps After Phase B

1. **Phase C** (2-4 months): Expose Rust operators to Python
   - Create PyO3 bindings for 26,781 lines of operators
   - Replace Python operator imports

2. **Phase D** (3-6 months): Route query building to Rust
   - Use existing SQLComposer
   - Delete Python sql_generator.py

3. **Phase E** (1-2 months): Remove Python sql/ module
   - Complete transition to Rust pipeline

**Total to completion**: 9-18 months (50% faster than originally planned)

---

*Phase B Implementation Plan*
*January 8, 2026*
*Ready to begin*
