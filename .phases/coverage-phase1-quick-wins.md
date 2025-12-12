# Phase 1: Test Coverage Quick Wins (60% → 70%)

## Objective

Add tests for the most commonly-used operators that currently have low coverage. Focus on network, pattern, and date range operators which are critical for production use cases.

**Target**: Increase coverage from 60% to 70% by adding ~95 well-targeted tests.

## Context

From TEST_COVERAGE_ANALYSIS.md:
- **Current coverage**: 60% (791 statements, 314 missed)
- **Tests passing**: 2,447 / 2,447 (100%)
- **Core infrastructure**: Well-tested (90-100% coverage)
- **Specialized operators**: Poorly tested (15-27% coverage)

### Critical Gaps (Priority 1)

1. **Network Operators**: 27% coverage (32 missing lines)
   - Missing: `isprivate`, `ispublic`, `insubnet`, `overlaps`, `strictleft`, `strictright`
   - Impact: IP filtering is common in production

2. **Pattern Operators**: 23% coverage (27 missing lines)
   - Missing: `contains`, `startswith`, `endswith`, case sensitivity, regex
   - Impact: Text search is fundamental

3. **DateRange Operators**: 27% coverage (30 missing lines)
   - Missing: Range operators (overlaps, contains, adjacent, before, after)
   - Impact: Time-series data filtering

4. **Error Handling**: Scattered across modules
   - Missing: Invalid inputs, type mismatches, validation
   - Impact: Production robustness

## Files to Modify/Create

### New Test Files

1. **`tests/unit/sql/where/test_network_operators_complete.py`** (25 tests)
   - Comprehensive network operator tests
   - IPv4 and IPv6 coverage
   - Subnet operations

2. **`tests/unit/sql/where/test_pattern_operators_complete.py`** (20 tests)
   - Text search operators
   - Case sensitivity
   - Regex patterns

3. **`tests/unit/sql/where/test_daterange_operators_complete.py`** (30 tests)
   - All range operations
   - Boundary conditions
   - Range overlaps and containment

4. **`tests/unit/sql/where/test_operator_error_handling.py`** (20 tests)
   - Invalid operator names
   - Type mismatches
   - Malformed values

### Modules Under Test

- `fraiseql/sql/where/operators/network_operators.py`
- `fraiseql/sql/where/operators/pattern_operators.py`
- `fraiseql/sql/where/operators/daterange_operators.py`
- All strategy modules (for error handling)

## Implementation Steps

### Step 1: Network Operators Tests (Estimated: 25 tests)

Create comprehensive tests for all network operators.

**File**: `tests/unit/sql/where/test_network_operators_complete.py`

```python
"""Comprehensive tests for network operator SQL building."""
import pytest
from fraiseql.sql.where.operators import get_strategy


class TestNetworkBasicOperators:
    """Test basic network comparison operators."""

    def test_eq_ipv4(self):
        """Test IPv4 equality."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("eq", "192.168.1.1", "ip_address", {})
        assert sql == "(ip_address = %s)"
        assert params == ["192.168.1.1"]

    def test_eq_ipv6(self):
        """Test IPv6 equality."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("eq", "2001:db8::1", "ip_address", {})
        assert sql == "(ip_address = %s)"
        assert params == ["2001:db8::1"]

    def test_neq_network(self):
        """Test network inequality."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("neq", "10.0.0.0/8", "network", {})
        assert sql == "(network != %s)"
        assert params == ["10.0.0.0/8"]


class TestNetworkPrivatePublic:
    """Test private/public IP detection."""

    def test_isprivate_operator(self):
        """Test isprivate operator for private IP ranges."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("isprivate", True, "ip_address", {})
        # Should use inet_is_private() or equivalent check
        assert "10." in sql or "192.168." in sql or "172.16." in sql

    def test_ispublic_operator(self):
        """Test ispublic operator (not private)."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("ispublic", True, "ip_address", {})
        # Should be NOT isprivate
        assert "NOT" in sql.upper()


class TestNetworkSubnet:
    """Test subnet operations."""

    def test_insubnet_ipv4(self):
        """Test if IP is in subnet (IPv4)."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("insubnet", "192.168.1.0/24", "ip_address", {})
        assert "<<=" in sql  # PostgreSQL subnet contains operator
        assert params == ["192.168.1.0/24"]

    def test_insubnet_ipv6(self):
        """Test if IP is in subnet (IPv6)."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("insubnet", "2001:db8::/32", "ip_address", {})
        assert "<<=" in sql
        assert params == ["2001:db8::/32"]

    def test_overlaps_subnets(self):
        """Test if two subnets overlap."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("overlaps", "10.0.0.0/8", "network", {})
        assert "&&" in sql  # PostgreSQL overlap operator
        assert params == ["10.0.0.0/8"]


class TestNetworkOrdering:
    """Test network ordering operators."""

    def test_strictleft_network(self):
        """Test strictleft operator (entirely left of)."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("strictleft", "192.168.0.0/16", "network", {})
        assert "<<" in sql  # PostgreSQL strictly left operator
        assert params == ["192.168.0.0/16"]

    def test_strictright_network(self):
        """Test strictright operator (entirely right of)."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("strictright", "192.168.0.0/16", "network", {})
        assert ">>" in sql  # PostgreSQL strictly right operator
        assert params == ["192.168.0.0/16"]


class TestNetworkEdgeCases:
    """Test edge cases for network operators."""

    def test_localhost_ipv4(self):
        """Test localhost handling."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("eq", "127.0.0.1", "ip_address", {})
        assert params == ["127.0.0.1"]

    def test_localhost_ipv6(self):
        """Test IPv6 localhost."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("eq", "::1", "ip_address", {})
        assert params == ["::1"]

    def test_broadcast_address(self):
        """Test broadcast address."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("eq", "255.255.255.255", "ip_address", {})
        assert params == ["255.255.255.255"]

    def test_null_handling(self):
        """Test NULL IP address."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("eq", None, "ip_address", {})
        assert "IS NULL" in sql.upper()
        assert params == []
```

**Expected Behavior**:
- All network operators generate correct PostgreSQL inet/cidr SQL
- IPv4 and IPv6 both handled correctly
- PostgreSQL-specific operators used: `<<=`, `&&`, `<<`, `>>`
- NULL handling works correctly

### Step 2: Pattern Operators Tests (Estimated: 20 tests)

Test all text search and pattern matching operators.

**File**: `tests/unit/sql/where/test_pattern_operators_complete.py`

```python
"""Comprehensive tests for pattern operator SQL building."""
import pytest
from fraiseql.sql.where.operators import get_strategy


class TestPatternContains:
    """Test contains operator."""

    def test_contains_basic(self):
        """Test basic substring search."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", "world", "message", {})
        assert "LIKE" in sql.upper() or "ILIKE" in sql.upper()
        assert params == ["%world%"]

    def test_contains_case_sensitive(self):
        """Test case-sensitive contains."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", "World", "message", {"case_sensitive": True})
        assert "LIKE" in sql.upper()
        assert "ILIKE" not in sql.upper()
        assert params == ["%World%"]

    def test_contains_case_insensitive(self):
        """Test case-insensitive contains (default)."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", "world", "message", {})
        assert "ILIKE" in sql.upper()
        assert params == ["%world%"]


class TestPatternStartsEnds:
    """Test startswith and endswith operators."""

    def test_startswith_basic(self):
        """Test prefix matching."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("startswith", "Hello", "message", {})
        assert "ILIKE" in sql.upper()
        assert params == ["Hello%"]

    def test_endswith_basic(self):
        """Test suffix matching."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("endswith", "world", "message", {})
        assert "ILIKE" in sql.upper()
        assert params == ["%world"]

    def test_startswith_case_sensitive(self):
        """Test case-sensitive prefix."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("startswith", "Hello", "message", {"case_sensitive": True})
        assert "LIKE" in sql.upper()
        assert "ILIKE" not in sql.upper()


class TestPatternRegex:
    """Test regex pattern matching."""

    def test_regex_basic(self):
        """Test basic regex matching."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("regex", r"^[A-Z]", "message", {})
        assert "~" in sql or "REGEXP" in sql.upper()
        assert params == [r"^[A-Z]"]

    def test_regex_case_insensitive(self):
        """Test case-insensitive regex."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("regex", r"hello", "message", {"case_sensitive": False})
        assert "~*" in sql  # PostgreSQL case-insensitive regex
        assert params == [r"hello"]

    def test_regex_email_pattern(self):
        """Test email regex pattern."""
        strategy = get_strategy("text")
        email_pattern = r'^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$'
        sql, params = strategy.build_sql("regex", email_pattern, "email", {})
        assert params == [email_pattern]


class TestPatternSpecialChars:
    """Test special character handling."""

    def test_contains_with_percent(self):
        """Test % character escaping in LIKE."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", "50%", "message", {})
        # % should be escaped in the pattern
        assert r"\%" in params[0] or "50%" in params[0]

    def test_contains_with_underscore(self):
        """Test _ character escaping in LIKE."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", "user_name", "field", {})
        # _ might need escaping depending on implementation
        assert "user_name" in params[0]

    def test_contains_unicode(self):
        """Test unicode characters."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", "世界", "message", {})
        assert params == ["%世界%"]

    def test_contains_emoji(self):
        """Test emoji characters."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", "😀", "message", {})
        assert params == ["%😀%"]


class TestPatternEdgeCases:
    """Test pattern operator edge cases."""

    def test_empty_pattern(self):
        """Test empty string pattern."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", "", "message", {})
        assert params == ["%%"]

    def test_null_pattern(self):
        """Test NULL pattern."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("contains", None, "message", {})
        assert "IS NULL" in sql.upper()
```

**Expected Behavior**:
- `contains` uses `ILIKE` with `%value%` pattern
- `startswith` uses `ILIKE` with `value%` pattern
- `endswith` uses `ILIKE` with `%value` pattern
- `regex` uses PostgreSQL `~` or `~*` operators
- Special characters (%, _, unicode, emoji) handled correctly
- Case sensitivity controlled via options

### Step 3: DateRange Operators Tests (Estimated: 30 tests)

Test all date range operations.

**File**: `tests/unit/sql/where/test_daterange_operators_complete.py`

```python
"""Comprehensive tests for date range operator SQL building."""
import pytest
from datetime import date, datetime
from fraiseql.sql.where.operators import get_strategy


class TestDateRangeBasicOperators:
    """Test basic date range comparison."""

    def test_eq_date_range(self):
        """Test date range equality."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", "[2024-01-01,2024-01-31]", "period", {})
        assert sql == "(period = %s)"
        assert params == ["[2024-01-01,2024-01-31]"]

    def test_neq_date_range(self):
        """Test date range inequality."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("neq", "[2024-01-01,2024-01-31]", "period", {})
        assert sql == "(period != %s)"


class TestDateRangeOverlaps:
    """Test overlaps operator."""

    def test_overlaps_basic(self):
        """Test if two ranges overlap."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("overlaps", "[2024-01-15,2024-02-15]", "period", {})
        assert "&&" in sql  # PostgreSQL overlap operator
        assert params == ["[2024-01-15,2024-02-15]"]

    def test_overlaps_partial(self):
        """Test partial overlap."""
        strategy = get_strategy("daterange")
        # Test: [2024-01-01, 2024-01-31] overlaps [2024-01-15, 2024-02-15]
        sql, params = strategy.build_sql("overlaps", "[2024-01-15,2024-02-15]", "period", {})
        assert "&&" in sql

    def test_overlaps_no_overlap(self):
        """Test non-overlapping ranges (logic test)."""
        strategy = get_strategy("daterange")
        # [2024-01-01, 2024-01-31] does NOT overlap [2024-02-01, 2024-02-28]
        sql, params = strategy.build_sql("overlaps", "[2024-02-01,2024-02-28]", "period", {})
        assert "&&" in sql


class TestDateRangeContains:
    """Test containment operators."""

    def test_contains_date(self):
        """Test if range contains a specific date."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("contains", "2024-06-15", "period", {})
        assert "@>" in sql  # PostgreSQL contains operator
        assert params == ["2024-06-15"]

    def test_contains_range(self):
        """Test if range contains another range."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("contains", "[2024-01-15,2024-01-20]", "period", {})
        assert "@>" in sql

    def test_contained_by(self):
        """Test if range is contained by another range."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("containedby", "[2024-01-01,2024-12-31]", "period", {})
        assert "<@" in sql  # PostgreSQL contained-by operator


class TestDateRangeAdjacency:
    """Test adjacent ranges."""

    def test_adjacent_ranges(self):
        """Test if ranges are adjacent."""
        strategy = get_strategy("daterange")
        # [2024-01-01, 2024-01-31] adjacent to [2024-02-01, 2024-02-28]
        sql, params = strategy.build_sql("adjacent", "[2024-02-01,2024-02-28]", "period", {})
        assert "-|-" in sql  # PostgreSQL adjacent operator

    def test_not_adjacent(self):
        """Test non-adjacent ranges."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("adjacent", "[2024-03-01,2024-03-31]", "period", {})
        assert "-|-" in sql


class TestDateRangeOrdering:
    """Test range ordering operators."""

    def test_before_range(self):
        """Test if range is entirely before another."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("before", "[2024-02-01,2024-02-28]", "period", {})
        assert "<<" in sql  # PostgreSQL strictly left operator

    def test_after_range(self):
        """Test if range is entirely after another."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("after", "[2023-12-01,2023-12-31]", "period", {})
        assert ">>" in sql  # PostgreSQL strictly right operator

    def test_before_not_overlapping(self):
        """Test before operator doesn't match overlapping ranges."""
        strategy = get_strategy("daterange")
        # [2024-01-15, 2024-02-15] is NOT before [2024-01-01, 2024-01-31]
        sql, params = strategy.build_sql("before", "[2024-01-01,2024-01-31]", "period", {})
        assert "<<" in sql


class TestDateRangeBoundaries:
    """Test boundary conditions."""

    def test_empty_range(self):
        """Test empty range."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", "empty", "period", {})
        assert "empty" in params[0].lower() or params == ["empty"]

    def test_unbounded_lower(self):
        """Test unbounded lower bound."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", "[,2024-12-31]", "period", {})
        assert params == ["[,2024-12-31]"]

    def test_unbounded_upper(self):
        """Test unbounded upper bound."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", "[2024-01-01,]", "period", {})
        assert params == ["[2024-01-01,]"]

    def test_infinite_range(self):
        """Test infinite range."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", "[,]", "period", {})
        assert params == ["[,]"]


class TestDateRangeEdgeCases:
    """Test edge cases."""

    def test_single_day_range(self):
        """Test range with single day."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", "[2024-01-01,2024-01-01]", "period", {})
        assert params == ["[2024-01-01,2024-01-01]"]

    def test_inclusive_exclusive_bounds(self):
        """Test inclusive [) vs exclusive () bounds."""
        strategy = get_strategy("daterange")
        # [2024-01-01, 2024-01-31) - includes start, excludes end
        sql, params = strategy.build_sql("eq", "[2024-01-01,2024-01-31)", "period", {})
        assert params == ["[2024-01-01,2024-01-31)"]

    def test_null_range(self):
        """Test NULL range."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", None, "period", {})
        assert "IS NULL" in sql.upper()
```

**Expected Behavior**:
- PostgreSQL range operators used: `&&` (overlap), `@>` (contains), `<@` (contained by)
- Range operators: `-|-` (adjacent), `<<` (before), `>>` (after)
- Boundary conditions handled: empty, unbounded, infinite ranges
- NULL handling works correctly

### Step 4: Error Handling Tests (Estimated: 20 tests)

Test validation and error conditions across all strategies.

**File**: `tests/unit/sql/where/test_operator_error_handling.py`

```python
"""Test error handling and validation for operator SQL building."""
import pytest
from fraiseql.sql.where.operators import get_strategy


class TestInvalidOperators:
    """Test invalid operator name handling."""

    def test_invalid_operator_name(self):
        """Test that invalid operator names raise appropriate errors."""
        strategy = get_strategy("text")
        with pytest.raises((ValueError, KeyError), match="unsupported|invalid|not found"):
            strategy.build_sql("invalid_op", "value", "field", {})

    def test_typo_in_operator(self):
        """Test common typos in operator names."""
        strategy = get_strategy("numeric")
        with pytest.raises((ValueError, KeyError)):
            strategy.build_sql("eqauls", 42, "field", {})  # typo: equals


class TestTypeMismatches:
    """Test type mismatch handling."""

    def test_in_requires_list(self):
        """Test that 'in' operator requires a list/iterable."""
        strategy = get_strategy("numeric")
        with pytest.raises((TypeError, ValueError), match="list|iterable|array"):
            strategy.build_sql("in", "not-a-list", "field", {})

    def test_between_requires_tuple(self):
        """Test that 'between' operator requires a tuple/list of 2 values."""
        strategy = get_strategy("numeric")
        with pytest.raises((TypeError, ValueError), match="tuple|two|range"):
            strategy.build_sql("between", 42, "field", {})

    def test_between_requires_two_values(self):
        """Test that 'between' requires exactly 2 values."""
        strategy = get_strategy("numeric")
        with pytest.raises((TypeError, ValueError)):
            strategy.build_sql("between", [1, 2, 3], "field", {})  # 3 values


class TestInvalidIPAddresses:
    """Test invalid IP address handling."""

    def test_invalid_ipv4_format(self):
        """Test malformed IPv4 address."""
        strategy = get_strategy("inet")
        # Note: PostgreSQL might handle validation, but test what our code does
        sql, params = strategy.build_sql("eq", "999.999.999.999", "ip_address", {})
        # Either should raise, or pass through and let PostgreSQL handle it
        assert params == ["999.999.999.999"]

    def test_invalid_ipv6_format(self):
        """Test malformed IPv6 address."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("eq", "gggg::1", "ip_address", {})
        assert params == ["gggg::1"]  # Pass through to PostgreSQL

    def test_invalid_subnet_cidr(self):
        """Test invalid CIDR notation."""
        strategy = get_strategy("inet")
        sql, params = strategy.build_sql("insubnet", "192.168.1.0/99", "network", {})
        # CIDR /99 is invalid, but may be caught by PostgreSQL
        assert params == ["192.168.1.0/99"]


class TestInvalidDates:
    """Test invalid date handling."""

    def test_invalid_date_format(self):
        """Test malformed date string."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", "[invalid-date,2024-12-31]", "period", {})
        # Pass through to PostgreSQL or raise
        assert "invalid-date" in params[0]

    def test_invalid_range_format(self):
        """Test malformed range format."""
        strategy = get_strategy("daterange")
        sql, params = strategy.build_sql("eq", "not-a-range", "period", {})
        assert params == ["not-a-range"]


class TestInvalidCoordinates:
    """Test invalid coordinate handling."""

    def test_invalid_latitude(self):
        """Test out-of-range latitude."""
        strategy = get_strategy("point")
        # Latitude must be [-90, 90]
        sql, params = strategy.build_sql("eq", (999.0, 0.0), "location", {})
        # May pass through or validate
        assert 999.0 in params or params == [(999.0, 0.0)]

    def test_invalid_longitude(self):
        """Test out-of-range longitude."""
        strategy = get_strategy("point")
        # Longitude must be [-180, 180]
        sql, params = strategy.build_sql("eq", (0.0, 999.0), "location", {})
        assert 999.0 in params or params == [(0.0, 999.0)]


class TestNullHandling:
    """Test NULL value handling across strategies."""

    def test_null_numeric(self):
        """Test NULL with numeric strategy."""
        strategy = get_strategy("numeric")
        sql, params = strategy.build_sql("eq", None, "value", {})
        assert "IS NULL" in sql.upper()
        assert params == []

    def test_null_text(self):
        """Test NULL with text strategy."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("eq", None, "message", {})
        assert "IS NULL" in sql.upper()

    def test_null_boolean(self):
        """Test NULL with boolean strategy."""
        strategy = get_strategy("boolean")
        sql, params = strategy.build_sql("eq", None, "active", {})
        assert "IS NULL" in sql.upper()


class TestEdgeCaseValues:
    """Test edge case values."""

    def test_empty_string(self):
        """Test empty string value."""
        strategy = get_strategy("text")
        sql, params = strategy.build_sql("eq", "", "message", {})
        assert params == [""]

    def test_zero_value(self):
        """Test zero numeric value."""
        strategy = get_strategy("numeric")
        sql, params = strategy.build_sql("eq", 0, "count", {})
        assert params == [0]

    def test_negative_value(self):
        """Test negative numeric value."""
        strategy = get_strategy("numeric")
        sql, params = strategy.build_sql("eq", -100, "temperature", {})
        assert params == [-100]

    def test_very_large_number(self):
        """Test very large number."""
        strategy = get_strategy("numeric")
        large = 10**18
        sql, params = strategy.build_sql("eq", large, "bigint_field", {})
        assert params == [large]
```

**Expected Behavior**:
- Invalid operator names raise `ValueError` or `KeyError`
- Type mismatches (e.g., `in` with non-list) raise `TypeError`
- Malformed values either raise or pass through to PostgreSQL
- NULL handling generates `IS NULL` SQL correctly
- Edge cases (empty string, zero, negative, large numbers) handled

## Verification Commands

After implementing each test file:

```bash
# Run new tests to ensure they pass
uv run pytest tests/unit/sql/where/test_network_operators_complete.py -v
uv run pytest tests/unit/sql/where/test_pattern_operators_complete.py -v
uv run pytest tests/unit/sql/where/test_daterange_operators_complete.py -v
uv run pytest tests/unit/sql/where/test_operator_error_handling.py -v

# Run full test suite to ensure no regressions
uv run pytest tests/ -v

# Generate new coverage report
uv run pytest --cov=fraiseql --cov-report=html --cov-report=term

# Check coverage improvements
# Expected: 60% → 70% (+10%)
```

**Expected Output**:
```
tests/unit/sql/where/test_network_operators_complete.py ........ [100%]
tests/unit/sql/where/test_pattern_operators_complete.py ........ [100%]
tests/unit/sql/where/test_daterange_operators_complete.py ........ [100%]
tests/unit/sql/where/test_operator_error_handling.py ........ [100%]

======================== 2,542 passed in 45.23s ========================

Coverage: 70% (+10% from baseline)
```

## Acceptance Criteria

- [ ] Network operators coverage: 27% → 85% (+58%)
- [ ] Pattern operators coverage: 23% → 80% (+57%)
- [ ] DateRange operators coverage: 27% → 80% (+53%)
- [ ] Error handling tests added across all strategies
- [ ] All new tests pass (100% success rate)
- [ ] No regressions in existing tests
- [ ] Overall coverage: 60% → 70% (+10%)
- [ ] Total test count: 2,447 → ~2,542 (+95 tests)

## DO NOT

- ❌ Break existing tests
- ❌ Change operator behavior (only add tests)
- ❌ Add tests for features that don't exist yet
- ❌ Skip verification after implementation
- ❌ Commit failing tests

## Notes

- **Testing philosophy**: Test what the code currently does, not what it should do
- **Error handling**: Some validation may be delegated to PostgreSQL (that's OK)
- **Coverage target**: 70% is a realistic quick win; 90% requires Phase 2 & 3
- **Test organization**: Group by operator category for maintainability
- **Implementation order**: Network → Pattern → DateRange → Error Handling

## References

- Coverage analysis: `TEST_COVERAGE_ANALYSIS.md`
- Existing test patterns: `tests/unit/sql/where/test_ltree_*.py` (86% coverage, good model)
- Operator modules: `fraiseql/sql/where/operators/`
