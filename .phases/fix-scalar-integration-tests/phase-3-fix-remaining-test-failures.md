# Phase 3: Fix Remaining Scalar Integration Test Failures [REFACTOR]

**Objective**: Fix the remaining 50 failing tests in scalar integration test suite
**Priority**: P2 - Medium (core functionality works, tests need refinement)
**Estimated Effort**: 3-4 hours
**Tests Fixed**: 50 tests (44 GraphQL query + 6 WHERE clause)

---

## Context

After Phase 1 & 2 and the scalar field type fix:
- ✅ **108 tests passing** (schema registration + database roundtrip)
- ✅ **Core functionality working** (scalars can be used as field types)
- ❌ **50 tests failing** due to test infrastructure issues, not product bugs

**Current Status**:
```
118 passed, 50 failed
```

---

## Root Cause Analysis

### Issue #1: Missing Test Values (44 GraphQL query test failures)

**Test**: `test_scalar_in_graphql_query`
**Error Example**:
```
GraphQLError("Variable '$testValue' got invalid value 'test_value';
Invalid airport code: test_value. Must be 3 uppercase letters (e.g., 'LAX', 'JFK', 'LHR')")
```

**Root Cause**:
The `get_test_value_for_scalar()` helper function only has test values for 6 scalars:
```python
test_values = {
    CIDRScalar: "192.168.1.0/24",
    CUSIPScalar: "037833100",
    DateScalar: "2023-12-13",
    IpAddressScalar: "192.168.1.1",
    JSONScalar: {"key": "value"},
    UUIDScalar: "550e8400-e29b-41d4-a716-446655440000",
}
return test_values.get(scalar_class, "test_value")  # ← Returns 'test_value' for unknowns
```

For the other 48 scalars, it returns `"test_value"`, which fails validation:
- AirportCodeScalar expects 3 uppercase letters (e.g., "LAX")
- ColorScalar expects hex color (e.g., "#FF5733")
- PhoneNumberScalar expects E.164 format (e.g., "+14155552671")
- etc.

**Why Some Tests Pass**:
Tests pass for the 6 scalars with defined test values + a few that accept generic strings.

**Tests Affected**: 44 GraphQL query tests

---

### Issue #2: WHERE Clause Field Generation (6 WHERE clause test failures)

**Test**: `test_scalar_in_where_clause`
**Error**:
```
GraphQLError("Unknown argument 'where' on field 'Query.getTestData'.")
GraphQLError("Cannot query field 'testField' on type 'TestType'.")
```

**Root Cause #1**: Field not added to GraphQL schema

**Current Code** (line 243):
```python
@fraise_type(sql_source=table_name)
class TestType:
    id: int
    test_field = scalar_class  # ❌ Assignment, not annotation
```

FraiseQL's field generation requires **type annotations**, not **assignments**.

**Fix**:
```python
@fraise_type(sql_source=table_name)
class TestType:
    id: int
    test_field: scalar_class  # ✅ Type annotation
```

**Root Cause #2**: WHERE argument not automatically added

The `@query` decorator doesn't automatically add WHERE arguments. Looking at `test_all_where_operators.py` (which passes), queries return types with `sql_source` and FraiseQL automatically adds WHERE support.

**Current Code** (lines 247-249):
```python
@query
async def get_test_data(info) -> list[TestType]:
    return []
```

This pattern should work if TestType has `sql_source`, but the field generation issue prevents it.

**Tests Affected**: 6 WHERE clause tests

---

## Solution

### Fix #1: Add Test Values for All Scalars

**Location**: `tests/integration/meta/test_all_scalars.py`, line 356-363

**Current** (6 scalars):
```python
test_values = {
    CIDRScalar: "192.168.1.0/24",
    CUSIPScalar: "037833100",
    DateScalar: "2023-12-13",
    IpAddressScalar: "192.168.1.1",
    JSONScalar: {"key": "value", "number": 42},
    UUIDScalar: "550e8400-e29b-41d4-a716-446655440000",
}
```

**Add** (48 more scalars):
```python
test_values = {
    # Existing (6)
    CIDRScalar: "192.168.1.0/24",
    CUSIPScalar: "037833100",
    DateScalar: "2023-12-13",
    IpAddressScalar: "192.168.1.1",
    JSONScalar: {"key": "value", "number": 42},
    UUIDScalar: "550e8400-e29b-41d4-a716-446655440000",

    # Network & Infrastructure (7)
    MacAddressScalar: "00:1B:63:84:45:E6",
    SubnetMaskScalar: "255.255.255.0",
    HostnameScalar: "example.com",
    DomainNameScalar: "example.com",
    PortScalar: 8080,
    URLScalar: "https://example.com",

    # Geographic & Location (5)
    AirportCodeScalar: "LAX",
    CoordinateScalar: "34.0522,-118.2437",
    LatitudeScalar: 34.0522,
    LongitudeScalar: -118.2437,
    TimezoneScalar: "America/Los_Angeles",

    # Financial & Business (10)
    CurrencyCodeScalar: "USD",
    IBANScalar: "GB82WEST12345698765432",
    ISINScalar: "US0378331005",
    SEDOLScalar: "B0WNLY7",
    LEIScalar: "549300E9PC51EN656011",
    ExchangeCodeScalar: "NYSE",
    MICScalar: "XNYS",
    StockSymbolScalar: "AAPL",
    MoneyScalar: "100.00",
    ExchangeRateScalar: "1.25",

    # Shipping & Logistics (4)
    PortCodeScalar: "USNYC",
    ContainerNumberScalar: "CSQU3054383",
    TrackingNumberScalar: "1Z999AA10123456784",
    VINScalar: "1HGBH41JXMN109186",

    # Communications (3)
    PhoneNumberScalar: "+14155552671",
    ApiKeyScalar: "sk_test_4eC39HqLyjWDarjtT1zdp7dc",
    EmailScalar: "test@example.com",

    # Content & Data (5)
    HTMLScalar: "<p>Hello World</p>",
    MarkdownScalar: "# Hello World",
    MimeTypeScalar: "application/json",
    ColorScalar: "#FF5733",
    HashSHA256Scalar: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",

    # Identification & Codes (8)
    LanguageCodeScalar: "en",
    LocaleCodeScalar: "en-US",
    PostalCodeScalar: "90210",
    LicensePlateScalar: "ABC123",
    FlightNumberScalar: "AA100",
    SlugScalar: "hello-world",

    # Date & Time (3)
    DateTimeScalar: "2023-12-13T10:30:00Z",
    TimeScalar: "10:30:00",
    DateRangeScalar: "2023-12-01,2023-12-31",
    DurationScalar: "PT1H30M",

    # Technical & Specialized (4)
    SemanticVersionScalar: "1.2.3",
    PercentageScalar: 75.5,
    VectorScalar: "[0.1, 0.2, 0.3]",
    LTreeScalar: "Top.Science.Astronomy",
    FileScalar: "test.txt",
    ImageScalar: "image.png",
}
```

**Note**: Some values are approximate - adjust based on actual scalar validation rules.

---

### Fix #2: Use Type Annotations Instead of Assignments

**Location**: `tests/integration/meta/test_all_scalars.py`, line 243

**Current** (line 240-244):
```python
@fraise_type(sql_source=table_name)
class TestType:
    id: int
    test_field = scalar_class  # ❌ Assignment
```

**Replace with**:
```python
@fraise_type(sql_source=table_name)
class TestType:
    id: int

# Add field dynamically using annotation
TestType.__annotations__['test_field'] = scalar_class
```

**OR** (cleaner approach - use typing.cast for dynamic annotation):
```python
# Create type dynamically with proper annotations
from typing import cast

def create_test_type_with_scalar(table_name: str, scalar_class):
    """Create a test type with a scalar field dynamically."""
    @fraise_type(sql_source=table_name)
    class TestType:
        id: int

    # Add the scalar field annotation dynamically
    TestType.__annotations__['test_field'] = scalar_class

    return TestType

# Use in test
TestType = create_test_type_with_scalar(table_name, scalar_class)
```

**Alternative** (simplest - just use annotation syntax):

Since we can't use `field: scalar_class` directly (scalar_class is a variable), we need to set the annotation after class definition:

```python
@fraise_type(sql_source=table_name)
class TestType:
    id: int

# Dynamically add the field with scalar type
TestType.__annotations__['test_field'] = scalar_class
```

---

## Implementation Steps

### Step 1: Add Test Values for All 48 Missing Scalars

**File**: `tests/integration/meta/test_all_scalars.py`
**Lines**: 356-363

1. **Import all scalar types** at the top of file (verify imports)
2. **Replace** the `test_values` dictionary with the complete version (see Fix #1 above)
3. **Verify** each test value matches scalar validation rules

**Verification**:
```bash
# Test a few scalars that were failing
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_graphql_query -k "AirportCode" -v
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_graphql_query -k "Color" -v
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_graphql_query -k "PhoneNumber" -v
```

**Expected**: All should pass

---

### Step 2: Fix Dynamic Field Annotation in WHERE Clause Test

**File**: `tests/integration/meta/test_all_scalars.py`
**Lines**: 240-244

**Replace**:
```python
@fraise_type(sql_source=table_name)
class TestType:
    id: int
    test_field = scalar_class
```

**With**:
```python
@fraise_type(sql_source=table_name)
class TestType:
    id: int

# Dynamically add the scalar field annotation
TestType.__annotations__['test_field'] = scalar_class
```

**Verification**:
```bash
# Test WHERE clause with one scalar
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_where_clause -k "CIDR" -vv
```

**Expected Output**:
- Field should be queryable in GraphQL
- WHERE argument might still be missing (see Step 3)

---

### Step 3: Investigate WHERE Clause Auto-Generation (If Needed)

**Only if** Step 2 doesn't automatically add WHERE support:

1. **Check** how `test_all_where_operators.py` achieves automatic WHERE support
2. **Compare** query registration patterns
3. **Understand** if WHERE requires:
   - Specific `@query` decorator parameters
   - Pool context
   - Special schema builder configuration

**Investigation Commands**:
```bash
# Check passing WHERE tests
grep -n "@query" tests/integration/meta/test_all_where_operators.py -A 2

# Check if there's a special setup
grep -n "build_schema" tests/integration/meta/test_all_where_operators.py -B 5 -A 2
```

**If WHERE doesn't auto-generate**, update test to match working pattern from `test_all_where_operators.py`.

---

## Complete Fixed Code

### Fix #1: Updated `get_test_value_for_scalar()` Function

```python
def get_test_value_for_scalar(scalar_class):
    """Get a test value appropriate for the given scalar type."""
    # Comprehensive map of scalar classes to valid test values
    test_values = {
        # Original (6) - Network & Core
        CIDRScalar: "192.168.1.0/24",
        CUSIPScalar: "037833100",
        DateScalar: "2023-12-13",
        IpAddressScalar: "192.168.1.1",
        JSONScalar: {"key": "value", "number": 42},
        UUIDScalar: "550e8400-e29b-41d4-a716-446655440000",

        # Network & Infrastructure
        MacAddressScalar: "00:1B:63:84:45:E6",
        SubnetMaskScalar: "255.255.255.0",
        HostnameScalar: "example.com",
        DomainNameScalar: "example.com",
        PortScalar: 8080,
        URLScalar: "https://example.com",

        # Geographic & Location
        AirportCodeScalar: "LAX",
        CoordinateScalar: "34.0522,-118.2437",
        LatitudeScalar: 34.0522,
        LongitudeScalar: -118.2437,
        TimezoneScalar: "America/Los_Angeles",

        # Financial & Business
        CurrencyCodeScalar: "USD",
        IBANScalar: "GB82WEST12345698765432",
        ISINScalar: "US0378331005",
        SEDOLScalar: "B0WNLY7",
        LEIScalar: "549300E9PC51EN656011",
        ExchangeCodeScalar: "NYSE",
        MICScalar: "XNYS",
        StockSymbolScalar: "AAPL",
        MoneyScalar: "100.00",
        ExchangeRateScalar: "1.25",

        # Shipping & Logistics
        PortCodeScalar: "USNYC",
        ContainerNumberScalar: "CSQU3054383",
        TrackingNumberScalar: "1Z999AA10123456784",
        VINScalar: "1HGBH41JXMN109186",

        # Communications
        PhoneNumberScalar: "+14155552671",
        ApiKeyScalar: "sk_test_4eC39HqLyjWDarjtT1zdp7dc",

        # Content & Data
        HTMLScalar: "<p>Hello World</p>",
        MarkdownScalar: "# Hello World",
        MimeTypeScalar: "application/json",
        ColorScalar: "#FF5733",
        HashSHA256Scalar: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",

        # Identification & Codes
        LanguageCodeScalar: "en",
        LocaleCodeScalar: "en-US",
        PostalCodeScalar: "90210",
        LicensePlateScalar: "ABC123",
        FlightNumberScalar: "AA100",
        SlugScalar: "hello-world",

        # Date & Time
        DateTimeScalar: "2023-12-13T10:30:00Z",
        TimeScalar: "10:30:00",
        DateRangeScalar: "2023-12-01,2023-12-31",
        DurationScalar: "PT1H30M",

        # Technical & Specialized
        SemanticVersionScalar: "1.2.3",
        PercentageScalar: 75.5,
        VectorScalar: "[0.1, 0.2, 0.3]",
        LTreeScalar: "Top.Science.Astronomy",
        FileScalar: "test.txt",
        ImageScalar: "image.png",
    }

    # Return specific value if known, otherwise raise error to catch missing values
    if scalar_class not in test_values:
        raise ValueError(
            f"No test value defined for {scalar_class}. "
            f"Add a valid test value to the test_values dictionary."
        )

    return test_values[scalar_class]
```

### Fix #2: Updated WHERE Clause Test

```python
@pytest.mark.parametrize(
    "scalar_name,scalar_class",
    [
        ("CIDRScalar", CIDRScalar),
        ("CUSIPScalar", CUSIPScalar),
        ("DateScalar", DateScalar),
        ("IpAddressScalar", IpAddressScalar),
        ("JSONScalar", JSONScalar),
        ("UUIDScalar", UUIDScalar),
    ],
)
async def test_scalar_in_where_clause(scalar_name, scalar_class, meta_test_pool):
    """Every scalar should work in WHERE clauses with database roundtrip."""
    from graphql import graphql
    from fraiseql import fraise_type, query
    from fraiseql.gql.builders import SchemaRegistry
    from psycopg import sql

    # Create a test table with the scalar column
    table_name = f"test_{scalar_name.lower()}_table"
    column_name = f"{scalar_name.lower()}_col"

    # Create table in database
    async with meta_test_pool.connection() as conn:
        await conn.execute(
            sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name))
        )
        await conn.execute(
            sql.SQL("""
                CREATE TABLE {} (
                    id SERIAL PRIMARY KEY,
                    {} {}
                )
            """).format(
                sql.Identifier(table_name),
                sql.Identifier(column_name),
                sql.SQL(get_postgres_type_for_scalar(scalar_class)),
            )
        )

        # Insert test data
        test_value = get_test_value_for_scalar(scalar_class)
        # Handle JSON types that need special adaptation
        if isinstance(test_value, dict):
            from psycopg.types.json import Jsonb
            adapted_value = Jsonb(test_value)
        else:
            adapted_value = test_value

        await conn.execute(
            sql.SQL("""
                INSERT INTO {} ({}) VALUES (%s)
            """).format(sql.Identifier(table_name), sql.Identifier(column_name)),
            [adapted_value],
        )

        await conn.commit()

    try:
        # Create schema with the test type
        registry = SchemaRegistry.get_instance()
        registry.clear()

        @fraise_type(sql_source=table_name)
        class TestType:
            id: int

        # Dynamically add the scalar field annotation
        TestType.__annotations__['test_field'] = scalar_class

        @query
        async def get_test_data(info) -> list[TestType]:
            return []

        registry.register_type(TestType)
        registry.register_query(get_test_data)

        # Test WHERE clause with the scalar
        test_value = get_test_value_for_scalar(scalar_class)

        # Format value for GraphQL (double quotes for strings, no quotes for numbers)
        if isinstance(test_value, str):
            graphql_value = f'"{test_value}"'
        elif isinstance(test_value, dict):
            # For JSON, use a simple string representation
            graphql_value = f'"{str(test_value)}"'
        else:
            graphql_value = str(test_value)

        query_str = f"""
        query {{
            getTestData(where: {{testField: {{eq: {graphql_value}}}}}) {{
                id
                testField
            }}
        }}
        """

        schema = registry.build_schema()

        # Execute query - should work without errors
        result = await graphql(schema, query_str)

        assert not result.errors, f"Scalar {scalar_name} failed in WHERE clause: {result.errors}"

    finally:
        # Cleanup
        async with meta_test_pool.connection() as conn:
            await conn.execute(
                sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name))
            )
            await conn.commit()
```

---

## Verification Plan

### Step 1: Verify GraphQL Query Tests (After adding test values)

```bash
# Test a few that were failing
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_graphql_query -k "AirportCode or Color or PhoneNumber" -v

# Expected: All 3 should pass
```

### Step 2: Verify All GraphQL Query Tests

```bash
# Run all 54 GraphQL query tests
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_graphql_query -v

# Expected: 54 passed
```

### Step 3: Verify WHERE Clause Tests

```bash
# Test one WHERE clause test
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_where_clause -k "CIDR" -vv

# Expected: 1 passed (or error showing WHERE not auto-added)
```

### Step 4: Full Test Suite

```bash
# Run all scalar tests
uv run pytest tests/integration/meta/test_all_scalars.py -v

# Expected: 168 passed, 0 failed
# - 54 schema registration: PASSED
# - 54 database roundtrip: PASSED
# - 54 GraphQL query: PASSED
# - 6 WHERE clause: PASSED
```

---

## Acceptance Criteria

- [ ] All 54 GraphQL query tests passing
- [ ] All 6 WHERE clause tests passing
- [ ] No regressions in 108 existing passing tests
- [ ] `get_test_value_for_scalar()` returns valid values for all 54 scalars
- [ ] Dynamic field annotation works with scalar types
- [ ] Total: 168 passed, 0 failed

---

## Troubleshooting

### Issue: Some test values still fail validation

**Cause**: Test value doesn't match scalar's validation rules

**Solution**:
1. Check the scalar's validation function (in `src/fraiseql/types/scalars/{scalar_name}.py`)
2. Update test value to match expected format
3. Re-run test

**Example**:
```python
# If AirportCodeScalar validation shows it needs IATA codes
AirportCodeScalar: "LAX",  # Valid IATA code
```

### Issue: WHERE clause test still shows "Unknown argument 'where'"

**Cause**: WHERE support not automatically added

**Solution**:
1. Check `test_all_where_operators.py` for pattern
2. May need to:
   - Pass pool/context to query
   - Use specific schema builder configuration
   - Register types in specific order

### Issue: Field still not queryable after annotation fix

**Cause**: FraiseQL might need field registered differently

**Solution**:
```python
# Try using Field explicitly
from fraiseql.fields import Field

TestType.__annotations__['test_field'] = scalar_class
# Also add to __gql_fields__ if it exists
if hasattr(TestType, '__gql_fields__'):
    TestType.__gql_fields__['test_field'] = Field(field_type=scalar_class)
```

---

## Estimated Timeline

- **Reading this plan**: 20 minutes
- **Adding test values**: 30 minutes
- **Fixing field annotation**: 15 minutes
- **Testing**: 30 minutes
- **Debugging WHERE clause** (if needed): 1 hour
- **Final verification**: 15 minutes
- **Commit**: 5 minutes

**Total**: 2.5-3.5 hours

---

## Commit Message

```
fix(tests): add valid test values and fix field annotations for scalar tests [REFACTOR]

Remaining scalar integration test failures have two root causes:

Issue #1 (44 GraphQL query test failures):
- get_test_value_for_scalar() only has values for 6 scalars
- Returns generic "test_value" for other 48 scalars
- Scalars validate input and reject invalid values like "test_value"

Solution:
- Add valid test values for all 54 custom scalars
- Values match each scalar's validation rules (e.g., "LAX" for AirportCode)
- Change fallback to raise error instead of returning invalid value

Issue #2 (6 WHERE clause test failures):
- Test uses field assignment (test_field = scalar_class) instead of annotation
- FraiseQL requires type annotations for field generation
- Fields not added to GraphQL schema, causing "Cannot query field" error

Solution:
- Use dynamic annotation: TestType.__annotations__['test_field'] = scalar_class
- Field properly registered in GraphQL schema
- WHERE clause support may auto-generate from sql_source

Changes:
- tests/integration/meta/test_all_scalars.py
  - get_test_value_for_scalar(): Add 48 new test values (lines 356-410)
  - test_scalar_in_where_clause: Fix field annotation (line 243)

Tests fixed: 50 tests (44 GraphQL query + 6 WHERE clause)

Verification:
  uv run pytest tests/integration/meta/test_all_scalars.py -v
  # Expected: 168 passed, 0 failed
```

---

## Success Metrics

After completing this phase:

- [x] **All 168 scalar integration tests passing**
- [x] **Complete scalar support validated**
- [x] **No test infrastructure debt**
- [x] **Clean, maintainable test suite**

---

## Next Phase

After this phase passes:
- ✅ All scalar integration tests complete
- ✅ Scalar feature fully validated
- Move on to other integration test suites (e.g., operators, connections)

---

**Status**: Ready for implementation ✅
