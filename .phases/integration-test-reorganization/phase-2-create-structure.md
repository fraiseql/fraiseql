# Phase 2: Create Directory Structure

**Phase:** SETUP (Additive changes only)
**Duration:** 10-15 minutes
**Risk:** Low (no existing files modified)
**Status:** Ready for Execution

---

## Objective

Create the new directory structure for reorganized integration tests, including subdirectories, `__init__.py` files, and README documentation for each test category.

**Success:** Complete directory tree with documentation, ready for file migration.

---

## Prerequisites

- [ ] Phase 1 completed (assessment done)
- [ ] Categorization plan reviewed and approved
- [ ] Clean git working directory

---

## Implementation Steps

### Step 1: Create Base Directory Structure (2 min)

#### 1.1 Create Main WHERE Directory

```bash
cd /home/lionel/code/fraiseql

# Create base where directory
mkdir -p tests/integration/database/sql/where

# Verify creation
ls -la tests/integration/database/sql/ | grep where
```

**Expected:** New `where/` directory created

#### 1.2 Create Category Subdirectories

```bash
# Create all subdirectories at once
mkdir -p tests/integration/database/sql/where/{network,specialized,temporal,spatial}

# Verify structure
tree tests/integration/database/sql/where/ -L 1
```

**Expected Output:**
```
tests/integration/database/sql/where/
├── network/
├── specialized/
├── temporal/
└── spatial/
```

**Acceptance:**
- [ ] Base `where/` directory created
- [ ] All 4 subdirectories created
- [ ] Directory structure verified

---

### Step 2: Create __init__.py Files (2 min)

#### 2.1 Create __init__.py in Each Directory

```bash
cd tests/integration/database/sql/where

# Root where/ __init__.py
cat > __init__.py << 'EOF'
"""Integration tests for WHERE clause functionality.

This package contains integration tests organized by operator type:
- network/ - Network operator tests (IP, MAC, hostname, email, port)
- specialized/ - PostgreSQL-specific tests (ltree, fulltext)
- temporal/ - Time-related tests (date, datetime, daterange)
- spatial/ - Spatial/coordinate tests

Root level contains mixed-type and cross-cutting integration tests.
"""
EOF

# Network __init__.py
cat > network/__init__.py << 'EOF'
"""Network operator integration tests.

Tests for IP address, MAC address, hostname, email, and port filtering.
Includes end-to-end filtering, operator validation, and production bug regressions.
"""
EOF

# Specialized __init__.py
cat > specialized/__init__.py << 'EOF'
"""PostgreSQL specialized type integration tests.

Tests for PostgreSQL-specific operators like ltree and fulltext search.
Includes hierarchical path filtering and text search integration.
"""
EOF

# Temporal __init__.py
cat > temporal/__init__.py << 'EOF'
"""Temporal (time-related) integration tests.

Tests for date, datetime, and daterange filtering and operations.
"""
EOF

# Spatial __init__.py
cat > spatial/__init__.py << 'EOF'
"""Spatial/coordinate integration tests.

Tests for coordinate-based filtering and distance operations.
"""
EOF

# Verify all created
find . -name "__init__.py" -type f
```

**Expected:** 5 `__init__.py` files created (root + 4 subdirs)

**Acceptance:**
- [ ] Root `__init__.py` created with package docstring
- [ ] All subdirectory `__init__.py` files created
- [ ] Docstrings explain test category purpose

---

### Step 3: Create README Files (5 min)

#### 3.1 Create Root README

```bash
cat > README.md << 'EOF'
# WHERE Clause Integration Tests

Integration tests for WHERE clause functionality, organized by operator type.

## Directory Structure

```
tests/integration/database/sql/where/
├── network/          # Network operator tests (8 files)
├── specialized/      # PostgreSQL-specific tests (2 files)
├── temporal/         # Time-related tests (2 files)
├── spatial/          # Spatial tests (1 file)
└── <root>            # Mixed-type tests (2-4 files)
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
- `test_mixed_phase4.py` - Phase 4 mixed-type validation
- `test_mixed_phase5.py` - Phase 5 mixed-type validation

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

### Mixed: ~2-4 tests
- Cross-cutting: 2-4 tests

**Total: ~15-17 integration tests**

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
**Files Moved:** 15 files
**Reason:** Match unit test organization, improve maintainability

See `.phases/integration-test-reorganization/` for migration details.
EOF

# Verify README created
ls -lh README.md
wc -l README.md
```

**Expected:** Large (~250 line) README created

#### 3.2 Create Category READMEs

```bash
# Network README
cat > network/README.md << 'EOF'
# Network Operator Integration Tests

Integration tests for network-related operators including IP addresses, MAC addresses, hostnames, email addresses, and ports.

## Tests

### IP Address Tests
- `test_ip_filtering.py` - End-to-end IP filtering with IPv4/IPv6
- `test_ip_operations.py` - IP operator validation (inSubnet, isPrivate, etc.)

### MAC Address Tests
- `test_mac_filtering.py` - MAC address filtering workflows
- `test_mac_operations.py` - MAC operator validation

### Cross-Network Tests
- `test_consistency.py` - Consistency across network operators
- `test_network_fixes.py` - Network operator bug fixes
- `test_production_bugs.py` - Production regression tests
- `test_jsonb_integration.py` - JSONB + network types integration

## Running Tests

```bash
# All network tests
uv run pytest tests/integration/database/sql/where/network/ -v

# Specific test
uv run pytest tests/integration/database/sql/where/network/test_ip_filtering.py -v
```

## Coverage

- IP operators: inSubnet, isPrivate, isPublic, isIPv4, isIPv6, etc.
- MAC operators: Equality, list operations
- JSONB integration: Network types stored in JSONB
- Production scenarios: Real-world bug regressions
EOF

# Specialized README
cat > specialized/README.md << 'EOF'
# PostgreSQL Specialized Type Integration Tests

Integration tests for PostgreSQL-specific types like ltree and fulltext search.

## Tests

### LTree Tests
- `test_ltree_filtering.py` - LTree hierarchical path filtering
- `test_ltree_operations.py` - LTree operator validation (ancestor_of, matches_lquery, etc.)

## Running Tests

```bash
# All specialized tests
uv run pytest tests/integration/database/sql/where/specialized/ -v

# LTree only
uv run pytest tests/integration/database/sql/where/specialized/test_ltree_filtering.py -v
```

## Coverage

- LTree hierarchical operators
- Pattern matching (lquery, ltxtquery)
- Path manipulation operations
- Array operations
EOF

# Temporal README
cat > temporal/README.md << 'EOF'
# Temporal (Time-Related) Integration Tests

Integration tests for date, datetime, and daterange operators.

## Tests

### DateRange Tests
- `test_daterange_filtering.py` - DateRange end-to-end filtering
- `test_daterange_operations.py` - DateRange operator validation (contains_date, overlaps, etc.)

## Running Tests

```bash
# All temporal tests
uv run pytest tests/integration/database/sql/where/temporal/ -v
```

## Coverage

- DateRange operators: contains_date, overlaps, adjacent, etc.
- Date comparisons
- Timestamp handling
EOF

# Spatial README
cat > spatial/README.md << 'EOF'
# Spatial/Coordinate Integration Tests

Integration tests for coordinate-based filtering and distance operations.

## Tests

- `test_coordinate_operations.py` - Coordinate operator validation (distance_within, etc.)

## Running Tests

```bash
# Spatial tests
uv run pytest tests/integration/database/sql/where/spatial/ -v
```

## Coverage

- Distance calculations
- Coordinate comparisons
- Spatial operators
EOF

# Verify all READMEs created
find . -name "README.md" -type f
```

**Expected:** 5 README files created

**Acceptance:**
- [ ] Root README with full documentation
- [ ] Category READMEs with specific info
- [ ] Running instructions included
- [ ] Coverage summaries provided

---

### Step 4: Verify Structure (1 min)

#### 4.1 Display Final Structure

```bash
# Show complete tree
tree tests/integration/database/sql/where/ -L 2

# Alternative (if tree not available)
find tests/integration/database/sql/where -type f -o -type d | sort
```

**Expected Output:**
```
tests/integration/database/sql/where/
├── README.md
├── __init__.py
├── network/
│   ├── README.md
│   └── __init__.py
├── specialized/
│   ├── README.md
│   └── __init__.py
├── temporal/
│   ├── README.md
│   └── __init__.py
└── spatial/
    ├── README.md
    └── __init__.py

4 directories, 9 files
```

#### 4.2 Verify File Counts

```bash
# Count __init__.py files
find tests/integration/database/sql/where -name "__init__.py" | wc -l
# Expected: 5

# Count README files
find tests/integration/database/sql/where -name "README.md" | wc -l
# Expected: 5

# Count total files
find tests/integration/database/sql/where -type f | wc -l
# Expected: 10 (5 __init__.py + 5 README.md)

# Count directories
find tests/integration/database/sql/where -type d | wc -l
# Expected: 5 (root + 4 subdirs)
```

**Acceptance:**
- [ ] 5 __init__.py files
- [ ] 5 README.md files
- [ ] 4 subdirectories + root
- [ ] Total 10 files in structure

---

## Verification

### Phase 2 Completion Checklist

Structure:
- [ ] Base `where/` directory created
- [ ] 4 subdirectories created (network, specialized, temporal, spatial)
- [ ] Directory tree verified with `tree` command

Files:
- [ ] 5 __init__.py files created and documented
- [ ] 5 README.md files created with comprehensive docs
- [ ] Root README includes migration history

Content:
- [ ] Root README explains full structure (~250 lines)
- [ ] Category READMEs provide specific info
- [ ] Running instructions provided
- [ ] Test naming conventions documented

### Quick Verification Commands

```bash
# Run all checks at once
cd /home/lionel/code/fraiseql/tests/integration/database/sql/where

# Verify structure
echo "=== Directory Structure ==="
tree -L 2 || find . -type d | sort

# Verify files
echo "=== File Counts ==="
echo "__init__.py files: $(find . -name '__init__.py' | wc -l)"
echo "README.md files: $(find . -name 'README.md' | wc -l)"
echo "Total files: $(find . -type f | wc -l)"

# Verify content
echo "=== README Sizes ==="
wc -l */README.md README.md

# Check git status
echo "=== Git Status ==="
cd /home/lionel/code/fraiseql
git status --short tests/integration/database/sql/where/
```

**Expected Output:**
- All directories exist
- 10 files total (5 init + 5 readme)
- READMEs have content (50-250 lines each)
- Git shows all new files as untracked

---

## Commit Changes

```bash
cd /home/lionel/code/fraiseql

# Stage all new files
git add tests/integration/database/sql/where/

# Verify what's being committed
git status

# Commit with descriptive message
git commit -m "$(cat <<'EOF'
test(integration): Create WHERE test directory structure [PHASE-2]

Create organized directory structure for integration tests to match
unit test organization.

Structure:
- tests/integration/database/sql/where/
  - network/      (IP, MAC, hostname, email, port tests)
  - specialized/  (ltree, fulltext tests)
  - temporal/     (date, datetime, daterange tests)
  - spatial/      (coordinate tests)

Added:
- 5 __init__.py files with category docstrings
- 5 README.md files with comprehensive documentation
- Running instructions and test naming conventions

Next Phase: Move test files into new structure

Phase: 2/6 (Create Structure)
See: .phases/integration-test-reorganization/phase-2-create-structure.md
EOF
)"

# Verify commit
git log -1 --stat
```

---

## Next Steps

After completing Phase 2:
1. Review created structure
2. Verify all READMEs are comprehensive
3. Proceed to Phase 3: Move & Rename Files

---

## Notes

### Design Decisions

- **5 READMEs instead of 1:** Each category gets specific documentation
- **Comprehensive root README:** Serves as master index and guide
- **__init__.py docstrings:** Helps IDE/LSP provide context
- **Migration history in README:** Documents the reorganization
- **Running instructions:** Makes tests easy to execute

### File Sizes

- Root README: ~250 lines (comprehensive guide)
- Category READMEs: ~20-50 lines each (focused info)
- __init__.py files: 3-5 lines each (docstring only)

---

**Phase Status:** Ready for execution ✅
**Next Phase:** Phase 3 - Move & Rename Files
