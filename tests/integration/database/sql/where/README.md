# WHERE Clause Integration Tests

Integration tests for WHERE clause functionality, organized by operator type.

## Directory Structure

```
tests/integration/database/sql/where/
├── network/          # Network operator tests (8 files)
├── specialized/      # PostgreSQL-specific tests (2 files)
├── temporal/         # Time-related tests (2 files)
├── spatial/          # Spatial tests (1 file)
└── <root>            # Mixed-type tests (3 files)
```

## Test Categories

### Network Tests (`network/`)
Tests for network-related operators:
- IP address filtering (IPv4, IPv6, CIDR)
- MAC address filtering
- Hostname validation
- Email validation
- Port filtering

**Files:**
- `test_ip_filtering.py` - End-to-end IP filtering
- `test_ip_operations.py` - IP operator validation
- `test_mac_filtering.py` - MAC address filtering
- `test_mac_operations.py` - MAC operator validation
- `test_network_fixes.py` - Network operator bug fixes
- `test_consistency.py` - Cross-network operator consistency
- `test_production_bugs.py` - Production regression tests
- `test_jsonb_integration.py` - JSONB + network types

### Specialized Tests (`specialized/`)
PostgreSQL-specific operator tests:
- LTree hierarchical paths
- Full-text search (when implemented)

**Files:**
- `test_ltree_filtering.py` - LTree end-to-end filtering
- `test_ltree_operations.py` - LTree operator validation

### Temporal Tests (`temporal/`)
Time-related operator tests:
- Date filtering
- DateTime filtering
- DateRange operations

**Files:**
- `test_daterange_filtering.py` - DateRange end-to-end
- `test_daterange_operations.py` - DateRange operator validation

### Spatial Tests (`spatial/`)
Coordinate and geometry tests:
- Distance calculations
- Coordinate filtering

**Files:**
- `test_coordinate_operations.py` - Coordinate operator validation

### Mixed-Type Tests (root)
Cross-cutting integration tests:
- Multi-type filtering scenarios
- Phase-based validation tests
- Issue resolution demonstrations

**Files:**
- `test_mixed_hostname.py` - Hostname operator validation
- `test_mixed_datetime.py` - DateTime/Date operator validation
- `test_restricted_types.py` - Restricted filter types

## Running Tests

### Run All WHERE Integration Tests
```bash
uv run pytest tests/integration/database/sql/where/ -v
```

### Run Specific Category
```bash
# Network tests only
uv run pytest tests/integration/database/sql/where/network/ -v

# LTree tests only
uv run pytest tests/integration/database/sql/where/specialized/ -v

# Temporal tests only
uv run pytest tests/integration/database/sql/where/temporal/ -v
```

### Run Single Test File
```bash
uv run pytest tests/integration/database/sql/where/network/test_ip_filtering.py -v
```

## Test Naming Conventions

### Filtering Tests
End-to-end tests that verify complete filtering workflows:
- Pattern: `test_<type>_filtering.py`
- Example: `test_ip_filtering.py`, `test_ltree_filtering.py`

### Operations Tests
Tests that validate specific operator SQL generation and behavior:
- Pattern: `test_<type>_operations.py`
- Example: `test_ip_operations.py`, `test_mac_operations.py`

### Bug/Fix Tests
Regression tests for production bugs or fixes:
- Pattern: `test_<type>_bugs.py` or `test_<category>_fixes.py`
- Example: `test_production_bugs.py`, `test_network_fixes.py`

### Consistency Tests
Tests that validate behavior across multiple operators:
- Pattern: `test_<category>_consistency.py`
- Example: `test_consistency.py` (network consistency)

## Adding New Tests

### For Network Operators
Add to `network/` directory:
```python
# tests/integration/database/sql/where/network/test_new_operator.py
```

### For Specialized PostgreSQL Types
Add to `specialized/` directory:
```python
# tests/integration/database/sql/where/specialized/test_fulltext_filtering.py
```

### For Temporal Operators
Add to `temporal/` directory:
```python
# tests/integration/database/sql/where/temporal/test_datetime_filtering.py
```

### For Cross-Cutting Tests
Add to root `where/` directory:
```python
# tests/integration/database/sql/where/test_mixed_advanced.py
```

## Related Test Directories

### Unit Tests
```
tests/unit/sql/where/
├── core/           # Core WHERE functionality
└── operators/      # Operator-specific unit tests
    ├── network/
    ├── specialized/
    └── temporal/
```

Integration tests in this directory correspond to operator unit tests.

## Test Coverage

### Network: ~8 tests
- IP filtering: 3 tests
- MAC filtering: 2 tests
- Cross-network: 2 tests
- JSONB integration: 1 test

### Specialized: ~2 tests
- LTree: 2 tests

### Temporal: ~2 tests
- DateRange: 2 tests

### Spatial: ~1 test
- Coordinates: 1 test

### Mixed: ~3 tests
- Cross-cutting: 3 tests

**Total: ~16 integration tests**

## CI/CD Integration

Tests are run as part of the integration test suite:
```bash
# In CI/CD pipeline
uv run pytest tests/integration/ -v
```

Parent directory path ensures all tests are discovered.

## Troubleshooting

### Tests Not Discovered
```bash
# Verify pytest can discover tests
uv run pytest tests/integration/database/sql/where/ --collect-only

# Check __init__.py files exist
find tests/integration/database/sql/where -name "__init__.py"
```

### Import Errors
- Ensure `__init__.py` exists in all directories
- Check fixture imports from parent conftest.py
- Verify PYTHONPATH includes project root

### Fixture Not Found
- Fixtures are defined in `tests/integration/database/conftest.py`
- pytest auto-discovers fixtures from parent directories
- Check fixture name spelling

## Migration History

**Reorganized:** 2025-12-11
**From:** `tests/integration/database/sql/*.py` (flat structure)
**To:** `tests/integration/database/sql/where/` (hierarchical structure)
**Files Moved:** 16 files
**Reason:** Match unit test organization, improve maintainability

See `.phases/integration-test-reorganization/` for migration details.
