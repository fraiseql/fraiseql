# Phase 3: REFACTOR - Clean Implementation

**Status**: Ready for Implementation
**Effort**: 1-2 hours
**Type**: REFACTOR

---

## Objective

Improve code quality and maintainability without changing behavior. Focus on cleaning up the implementation from Phase 2 while keeping all tests passing.

---

## Context

**Phase 2 Results**:
- ✅ Custom scalar WHERE filtering works
- ✅ All unit tests pass
- ✅ 4/6 integration tests pass
- ✅ Basic functionality implemented

**Current Issues**:
- Code is functional but could be cleaner
- Some duplication with existing filter patterns
- Type hints could be improved
- Documentation could be better

---

## Implementation Steps

### Step 1: Review Current Implementation
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Examine the current `_create_custom_scalar_filter()` function and identify areas for improvement.

**Current Issues**:
- Manual class creation instead of using patterns from existing filters
- Field definitions are duplicated from StringFilter
- No clear documentation
- Type hints could be more specific

---

### Step 2: Extract Common Filter Field Definitions
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Create a reusable function for standard filter fields.

**Code Changes**:

```python
def _get_standard_filter_fields(scalar_type: type) -> dict[str, Any]:
    """Get standard filter fields for a scalar type.

    Returns a dict of field_name -> (field_type, default_value, graphql_name)
    suitable for use with make_dataclass or manual class creation.
    """
    return {
        "eq": (Optional[scalar_type], None, None),
        "ne": (Optional[scalar_type], None, None),
        "in_": (Optional[list[scalar_type]], fraise_field(default=None, graphql_name="in"), "in"),
        "not_in": (Optional[list[scalar_type]], fraise_field(default=None, graphql_name="notIn"), "notIn"),
        "contains": (Optional[scalar_type], None, None),
        "starts_with": (Optional[scalar_type], fraise_field(default=None, graphql_name="startsWith"), "startsWith"),
        "ends_with": (Optional[scalar_type], fraise_field(default=None, graphql_name="endsWith"), "endsWith"),
    }
```

---

### Step 3: Refactor Custom Scalar Filter Creation
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Simplify `_create_custom_scalar_filter()` to use the common field definitions.

**Improved Implementation**:

```python
def _create_custom_scalar_filter(scalar_type: GraphQLScalarType) -> type:
    """Create a filter type for a custom GraphQL scalar.

    Generates a filter with standard operators (eq, ne, in, notIn, contains,
    startsWith, endsWith) that accept the scalar type instead of String.

    Args:
        scalar_type: The GraphQL scalar type to create a filter for

    Returns:
        A new dataclass decorated with @fraise_input for GraphQL input types
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

    # Get standard filter fields
    filter_fields = _get_standard_filter_fields(scalar_type)

    # Create the filter class using make_dataclass
    field_definitions = []
    for field_name, (field_type, default, graphql_name) in filter_fields.items():
        if graphql_name:
            # Use fraise_field for GraphQL name mapping
            field_definitions.append((field_name, field_type, default))
        else:
            field_definitions.append((field_name, field_type, default))

    filter_class = make_dataclass(
        filter_name,
        field_definitions,
        bases=(),
        frozen=False,
    )

    # Mark as FraiseQL input type
    filter_class = fraise_input(filter_class)

    # Cache it
    _custom_scalar_filter_cache[scalar_type] = filter_class

    return filter_class
```

---

### Step 4: Add Comprehensive Documentation
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Add detailed docstrings and comments.

**Documentation**:

```python
"""
Custom Scalar WHERE Filter Support

This module extends FraiseQL's WHERE clause generation to support custom
GraphQL scalar types. Previously, all custom scalars defaulted to StringFilter,
causing type mismatches in GraphQL queries.

Key Features:
- Automatic detection of GraphQLScalarType instances
- Generation of type-specific filters (CIDRFilter, EmailFilter, etc.)
- Standard operators: eq, ne, in, notIn, contains, startsWith, endsWith
- Caching to prevent duplicate filter generation
- Full GraphQL schema integration

Example:
    @fraise_type
    class NetworkDevice:
        ip_address: CIDRScalar

    # Generates:
    input NetworkDeviceWhereInput {
        ipAddress: CIDRFilter
    }

    input CIDRFilter {
        eq: CIDR
        ne: CIDR
        in: [CIDR!]
        notIn: [CIDR!]
        contains: CIDR
        startsWith: CIDR
        endsWith: CIDR
    }
"""
```

---

### Step 5: Improve Type Hints
**File**: `src/fraiseql/sql/graphql_where_generator.py`

**Action**: Add better type hints throughout the implementation.

**Type Improvements**:
- Add proper imports for `GraphQLScalarType`
- Use `TypeAlias` for filter cache types
- Add return type annotations
- Use `Union` types where appropriate

---

### Step 6: Test Refactored Implementation
**Action**: Run all tests to ensure refactoring didn't break anything.

**Commands**:
```bash
# Run unit tests
uv run pytest tests/unit/sql/test_custom_scalar_where_filters.py -v

# Run integration tests
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_where_clause -v

# Run broader test suite to check for regressions
uv run pytest tests/unit/sql/ -x
```

**Expected**: All tests should still pass.

---

## Acceptance Criteria

- [ ] All 8 unit tests PASS
- [ ] All 4 working integration tests still PASS
- [ ] No regressions in existing functionality
- [ ] Code is more maintainable and follows FraiseQL patterns
- [ ] Clear documentation and type hints
- [ ] No duplicated code
- [ ] Implementation is easier to understand and extend

---

## Expected Code Quality Improvements

**Before (Phase 2)**:
- Manual class creation with hardcoded fields
- Duplicated field definitions
- Minimal documentation
- Basic type hints

**After (Phase 3)**:
- Reusable field definition functions
- Consistent with existing filter patterns
- Comprehensive docstrings
- Better type hints and error handling
- Clear separation of concerns

---

## Commit Message

```
refactor(where): clean up custom scalar filter generation [REFACTOR]

Improve code quality and maintainability of custom scalar WHERE support:

- Extract common filter field definitions into reusable function
- Add comprehensive documentation and type hints
- Simplify filter creation logic
- Follow existing FraiseQL patterns more closely
- Maintain all existing functionality while improving readability

All tests still pass. Implementation is now more maintainable and extensible.

Related: custom-scalar-where-support phase 3
```

---

## DO NOT

- ❌ Change any behavior or functionality
- ❌ Add new features or operators
- ❌ Modify test expectations
- ❌ Break existing APIs

## DO

- ✅ Improve code readability and maintainability
- ✅ Add documentation and type hints
- ✅ Extract duplicated code
- ✅ Follow FraiseQL conventions
- ✅ Ensure all tests still pass

---

**Next Phase**: Phase 4 - QA (comprehensive validation)
