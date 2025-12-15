# Phase 2: GREEN - Minimal Implementation

**Status**: Ready for Implementation
**Effort**: 3-4 hours
**Type**: TDD - Make Tests Pass

---

## Objective

Implement the minimal code changes needed to make all the RED tests from Phase 1 pass. Focus on getting the feature working, not perfection.

---

## Context

**Phase 1 Results**:
- ✅ 8 unit tests FAIL (expect CIDRFilter, get StringFilter)
- ✅ 6 integration tests FAIL (GraphQL type mismatch)
- ✅ Root cause identified: `_get_filter_type_for_field()` doesn't detect custom scalars

**Goal**: Make all 14 tests pass with minimal, focused changes.

---

## Implementation Steps

### Step 1: Understand Current Filter Generation
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Review the `_get_filter_type_for_field()` function to understand how it currently works.

**Key Points**:
- Function takes `field_type` and returns appropriate filter class
- Has special handling for built-in types (UUID, DateTime, etc.)
- Defaults to `StringFilter` for unknown types
- **Missing**: Detection of custom GraphQL scalars

---

### Step 2: Add Custom Scalar Detection
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Modify `_get_filter_type_for_field()` to detect custom scalars.

**Code Changes**:

```python
# Add this check BEFORE the type_mapping.get() call
from graphql import GraphQLScalarType

# Check for custom GraphQL scalars
if isinstance(field_type, GraphQLScalarType):
    # This is a custom scalar - create a filter for it
    return _create_custom_scalar_filter(field_type)
```

**Location**: Around line 528, before the `return type_mapping.get(field_type, StringFilter)` line.

---

### Step 3: Implement Custom Scalar Filter Creation
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Add `_create_custom_scalar_filter()` function.

**Implementation**:

```python
def _create_custom_scalar_filter(scalar_type: GraphQLScalarType) -> type:
    """Create a filter type for a custom GraphQL scalar.

    Generates a filter with the same operators as StringFilter,
    but using the scalar type instead of String.
    """
    # Check cache first
    if scalar_type in _custom_scalar_filter_cache:
        return _custom_scalar_filter_cache[scalar_type]

    # Generate filter name (e.g., CIDRScalar -> CIDRFilter)
    scalar_name = scalar_type.name
    if scalar_name.endswith('Scalar'):
        filter_name = scalar_name.replace('Scalar', 'Filter')
    else:
        filter_name = f"{scalar_name}Filter"

    # Create filter fields using same structure as StringFilter
    # but with scalar_type instead of str
    filter_fields = [
        ("eq", Optional[scalar_type], None),
        ("ne", Optional[scalar_type], None),
        ("in", Optional[list[scalar_type]], None),
        ("notIn", Optional[list[scalar_type]], None),
        ("contains", Optional[scalar_type], None),
        ("startsWith", Optional[scalar_type], None),
        ("endsWith", Optional[scalar_type], None),
    ]

    # Create the filter class
    filter_class = make_dataclass(
        filter_name,
        filter_fields,
        bases=(),
        frozen=False,
    )

    # Mark as FraiseQL input type
    filter_class = fraise_input(filter_class)

    # Cache it
    _custom_scalar_filter_cache[scalar_type] = filter_class

    return filter_class
```

**Dependencies**:
- Import `GraphQLScalarType` from graphql
- Import `make_dataclass` from dataclasses
- Import `fraise_input` from fraiseql
- Add global cache: `_custom_scalar_filter_cache = {}`

---

### Step 4: Add Global Cache
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Add the cache variable at module level.

**Code**:
```python
# Add near the top with other global variables
_custom_scalar_filter_cache: dict[GraphQLScalarType, type] = {}
```

---

### Step 5: Test the Implementation
**Action**: Run the tests to see if they pass.

**Commands**:
```bash
# Run unit tests
uv run pytest tests/unit/sql/test_custom_scalar_where_filters.py -v

# Run integration tests
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_where_clause -v
```

**Expected**: All tests should now pass.

---

## Acceptance Criteria

- [ ] All 8 unit tests PASS
- [ ] All 6 integration tests PASS
- [ ] No regressions in existing tests
- [ ] Custom scalars generate appropriate filter types (CIDRFilter, CUSIPFilter, etc.)
- [ ] Filters are cached (same scalar type reuses same filter)
- [ ] GraphQL queries with WHERE clauses work for all custom scalars

---

## Expected Test Output

```bash
$ uv run pytest tests/unit/sql/test_custom_scalar_where_filters.py -v
test_custom_scalar_filter_is_generated PASSED
test_custom_scalar_filter_has_standard_operators PASSED
test_custom_scalar_filter_uses_scalar_type PASSED
test_filter_type_is_cached PASSED
test_nullable_custom_scalar_filter PASSED
test_list_of_custom_scalars PASSED
test_mixed_field_types PASSED
test_built_in_scalar_types_unchanged PASSED

8 passed
```

```bash
$ uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_where_clause -v
test_scalar_in_where_clause[CIDRScalar] PASSED
test_scalar_in_where_clause[CUSIPScalar] PASSED
test_scalar_in_where_clause[DateScalar] PASSED
test_scalar_in_where_clause[IpAddressScalar] PASSED
test_scalar_in_where_clause[JSONScalar] PASSED
test_scalar_in_where_clause[UUIDScalar] PASSED

6 passed
```

---

## Commit Message

```
feat(where): implement custom scalar WHERE filter generation [GREEN]

Add detection and filter generation for custom GraphQL scalar types
in WHERE clauses. The filter generator now recognizes GraphQLScalarType
instances and creates appropriate filter types (CIDRFilter, CUSIPFilter, etc.)
with standard operators that accept the scalar type instead of String.

Key changes:
- Modified _get_filter_type_for_field() to detect custom scalars
- Added _create_custom_scalar_filter() function
- Added caching to avoid regenerating filters
- All 14 failing tests from Phase 1 now pass

Related: custom-scalar-where-support phase 2
```

---

## DO NOT

- ❌ Add complex logic or edge cases
- ❌ Change existing behavior for built-in types
- ❌ Add performance optimizations beyond basic caching
- ❌ Modify SQL generation (should work as-is)
- ❌ Add documentation or tests beyond making existing ones pass

## DO

- ✅ Make all RED tests from Phase 1 pass
- ✅ Keep implementation minimal and focused
- ✅ Follow existing code patterns
- ✅ Add basic caching to prevent performance issues
- ✅ Test thoroughly before committing

---

**Next Phase**: Phase 3 - REFACTOR (clean up the implementation)
