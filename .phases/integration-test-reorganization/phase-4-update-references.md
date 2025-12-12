# Phase 4: Update References

**Phase:** FIX (Update imports, paths, documentation)
**Duration:** 10-15 minutes
**Risk:** Low (straightforward find/replace)
**Status:** Ready for Execution

---

## Objective

Update all references to moved test files including imports, CI/CD configuration, documentation, and any hard-coded paths.

**Success:** All references updated, tests discoverable, CI/CD works.

---

## Prerequisites

- [ ] Phase 3 completed (files moved)
- [ ] Changes committed
- [ ] Ready to fix test discovery

---

## Implementation Steps

### Step 1: Verify Test Discovery (2 min)

#### 1.1 Check Current Discovery Status

```bash
cd /home/lionel/code/fraiseql

# Try to collect tests
echo "=== Test Collection Status ==="
uv run pytest tests/integration/database/sql/where/ --collect-only -q

# Count discovered tests
TEST_COUNT=$(uv run pytest tests/integration/database/sql/where/ --co -q 2>&1 | grep "test session starts" -A 100 | grep -c "test_")
echo "Tests discovered: $TEST_COUNT"
```

**Expected:** Tests should be auto-discovered (pytest finds tests in subdirs)

**If tests NOT discovered:** Continue with fixes below

---

### Step 2: Update CI/CD Paths (3 min)

#### 2.1 Find CI/CD Configuration Files

```bash
cd /home/lionel/code/fraiseql

# Find CI configuration
find . -name ".github" -o -name "*.yml" -o -name "*.yaml" -o -name "Makefile" -o -name "tox.ini" | \
    grep -v ".git" | \
    xargs grep -l "tests/integration" 2>/dev/null > /tmp/ci-files.txt

echo "=== CI/CD files to check ==="
cat /tmp/ci-files.txt
```

#### 2.2 Check Current Test Paths

```bash
# For each CI file, show test paths
for file in $(cat /tmp/ci-files.txt); do
    echo "=== $file ==="
    grep -n "tests/integration" "$file" | head -10
done
```

#### 2.3 Update Paths (if needed)

**Most CI configs use parent directory paths and will work without changes:**
```yaml
# These paths still work (no update needed):
pytest tests/integration/              # ✓ Still works
pytest tests/integration/database/     # ✓ Still works
```

**Only update if tests use specific file paths:**
```bash
# Example: If CI references specific test files
# OLD: tests/integration/database/sql/test_network_address_filtering.py
# NEW: tests/integration/database/sql/where/network/test_ip_operations.py

# Use sed to update (if needed)
# sed -i 's|tests/integration/database/sql/test_network|tests/integration/database/sql/where/network/test|g' .github/workflows/test.yml
```

**Likely outcome:** No CI changes needed (parent paths work)

**Acceptance:**
- [ ] CI configuration checked
- [ ] Paths updated if necessary
- [ ] Parent directory paths verified to work

---

### Step 3: Update Documentation References (3 min)

#### 3.1 Find Documentation Files

```bash
cd /home/lionel/code/fraiseql

# Find docs mentioning integration tests
grep -r "tests/integration" docs/ README.md CONTRIBUTING.md 2>/dev/null | \
    grep -v ".pyc" | \
    cut -d: -f1 | sort -u > /tmp/docs-to-update.txt

echo "=== Documentation files ==="
cat /tmp/docs-to-update.txt
```

#### 3.2 Update Documentation

```bash
# For each doc file, show current references
for file in $(cat /tmp/docs-to-update.txt); do
    echo "=== $file ==="
    grep -n "tests/integration/database/sql" "$file" 2>/dev/null || echo "No specific paths found"
done

# Update if needed (example):
# If docs say: "Tests are in tests/integration/database/sql/"
# Update to: "Tests are in tests/integration/database/sql/where/"
```

**Update CONTRIBUTING.md (if exists):**
```bash
# Add section about new test structure
cat >> CONTRIBUTING.md << 'EOF'

## Test Organization

### Integration Tests Structure
Integration tests for WHERE clause functionality are organized by operator type:
- `tests/integration/database/sql/where/network/` - Network operators
- `tests/integration/database/sql/where/specialized/` - PostgreSQL types
- `tests/integration/database/sql/where/temporal/` - Time-related operators
- `tests/integration/database/sql/where/spatial/` - Spatial operators

See `tests/integration/database/sql/where/README.md` for details.
EOF
```

**Acceptance:**
- [ ] Documentation files identified
- [ ] References updated
- [ ] CONTRIBUTING.md updated (if applicable)

---

### Step 4: Check for Hard-Coded Test Paths (2 min)

#### 4.1 Search for Hard-Coded Imports

```bash
cd /home/lionel/code/fraiseql

# Look for explicit imports of moved test files
echo "=== Checking for hard-coded test imports ==="
grep -r "from tests.integration.database.sql import test_" tests/ 2>/dev/null | \
    grep -v ".pyc" | \
    grep -v "__pycache__" || echo "✓ No hard-coded imports found"

# Look for pytest.importorskip with specific paths
grep -r 'pytest.importorskip.*test_' tests/integration/ 2>/dev/null | \
    grep -v ".pyc" || echo "✓ No importorskip issues"
```

**Expected:** No hard-coded imports (integration tests are independent)

#### 4.2 Check for Path References in Test Code

```bash
# Check if any tests reference other test files by path
grep -r "__file__" tests/integration/database/sql/where/ | \
    grep -v ".pyc" || echo "✓ No __file__ references"

# Check for relative imports
grep -r "from \." tests/integration/database/sql/where/*.py 2>/dev/null || echo "✓ No relative imports"
```

**Expected:** No issues (integration tests are self-contained)

**Acceptance:**
- [ ] No hard-coded imports found
- [ ] No path references in test code
- [ ] Tests are independent

---

### Step 5: Update Test Collection Hints (2 min)

#### 5.1 Check pytest.ini / pyproject.toml

```bash
cd /home/lionel/code/fraiseql

# Check if pytest configuration exists
if [ -f "pytest.ini" ]; then
    echo "=== pytest.ini ==="
    cat pytest.ini | grep -A10 "\[pytest\]"
fi

if [ -f "pyproject.toml" ]; then
    echo "=== pyproject.toml [tool.pytest.ini_options] ==="
    grep -A10 "\[tool.pytest.ini_options\]" pyproject.toml
fi

# Check for testpaths configuration
grep -E "testpaths|python_files|python_classes|python_functions" pytest.ini pyproject.toml 2>/dev/null || \
    echo "Using pytest defaults"
```

**Check testpaths setting:**
```toml
# Should include integration tests (probably already there)
[tool.pytest.ini_options]
testpaths = ["tests"]  # ✓ This works - no update needed

# If specific:
testpaths = [
    "tests/unit",
    "tests/integration"  # ✓ Parent path works
]
```

**Likely outcome:** No changes needed (testpaths use parent directories)

**Acceptance:**
- [ ] pytest configuration checked
- [ ] testpaths verified (parent paths work)
- [ ] No updates needed to collection config

---

### Step 6: Verification (3 min)

#### 6.1 Test Discovery Verification

```bash
cd /home/lionel/code/fraiseql

# Collect all tests in new structure
echo "=== Test Collection ==="
uv run pytest tests/integration/database/sql/where/ --collect-only -q

# Count by category
echo ""
echo "=== Test Counts by Category ==="
echo "Network: $(uv run pytest tests/integration/database/sql/where/network/ --co -q 2>&1 | grep -c "test_")"
echo "Specialized: $(uv run pytest tests/integration/database/sql/where/specialized/ --co -q 2>&1 | grep -c "test_")"
echo "Temporal: $(uv run pytest tests/integration/database/sql/where/temporal/ --co -q 2>&1 | grep -c "test_")"
echo "Spatial: $(uv run pytest tests/integration/database/sql/where/spatial/ --co -q 2>&1 | grep -c "test_")"
echo "Root: $(uv run pytest tests/integration/database/sql/where/test_*.py --co -q 2>&1 | grep -c "test_" || echo "0")"

# Total
TOTAL=$(uv run pytest tests/integration/database/sql/where/ --co -q 2>&1 | grep -c "test_")
echo "Total: $TOTAL tests"
```

**Expected:** All tests discovered, counts match file inventory

#### 6.2 Quick Test Run (Smoke Test)

```bash
# Run a single test from each category to verify everything works
echo "=== Smoke Test ==="

# Network test
uv run pytest tests/integration/database/sql/where/network/ -k "test_" --maxfail=1 -x || echo "Network tests need fixing"

# Specialized test
uv run pytest tests/integration/database/sql/where/specialized/ -k "test_" --maxfail=1 -x || echo "Specialized tests need fixing"

# Temporal test
uv run pytest tests/integration/database/sql/where/temporal/ -k "test_" --maxfail=1 -x || echo "Temporal tests need fixing"

echo "✓ Smoke tests complete"
```

**Expected:** Tests run (may pass or fail, but they execute)

#### 6.3 Check CI Would Work

```bash
# Simulate CI test command
echo "=== Simulating CI Test Run ==="
uv run pytest tests/integration/ --co -q | head -50

# Count total integration tests
INTEGRATION_COUNT=$(uv run pytest tests/integration/ --co -q 2>&1 | grep -c "test_")
echo "Total integration tests discoverable: $INTEGRATION_COUNT"
```

**Expected:** CI commands still discover all tests

**Acceptance:**
- [ ] All tests discoverable
- [ ] Tests execute (even if some fail)
- [ ] CI simulation works
- [ ] No import errors

---

## Common Issues & Fixes

### Issue 1: Tests Not Discovered

**Symptom:** `pytest --co` shows 0 tests

**Fix:**
```bash
# Check __init__.py files exist
find tests/integration/database/sql/where -name "__init__.py"

# Should show 5 files - if missing, add them
touch tests/integration/database/sql/where/__init__.py
touch tests/integration/database/sql/where/network/__init__.py
touch tests/integration/database/sql/where/specialized/__init__.py
touch tests/integration/database/sql/where/temporal/__init__.py
touch tests/integration/database/sql/where/spatial/__init__.py
```

### Issue 2: Import Errors

**Symptom:** `ModuleNotFoundError` when running tests

**Fix:**
```bash
# Verify PYTHONPATH includes project root
export PYTHONPATH=/home/lionel/code/fraiseql:$PYTHONPATH

# Or use pytest with explicit path
uv run pytest tests/integration/database/sql/where/ --import-mode=importlib
```

### Issue 3: Fixture Not Found

**Symptom:** `fixture 'db_pool' not found`

**Fix:**
```bash
# Check conftest.py locations
find tests -name "conftest.py"

# pytest auto-discovers fixtures from parent directories
# Should work without changes if conftest.py in tests/ or tests/integration/
```

---

## Commit Changes

```bash
cd /home/lionel/code/fraiseql

# Stage any documentation updates
git add docs/ README.md CONTRIBUTING.md pytest.ini pyproject.toml 2>/dev/null || true

# Check what changed
git status --short

# Commit if there are changes
if git diff --cached --quiet; then
    echo "No documentation updates needed"
else
    git commit -m "$(cat <<'EOF'
docs: Update references for reorganized integration tests [PHASE-4]

Update documentation and configuration for new integration test structure.

Changes:
- Updated test path references in documentation
- Verified CI/CD configuration (no changes needed - parent paths work)
- Confirmed pytest test discovery works
- Added CONTRIBUTING.md section on test organization

Phase: 4/6 (Update References)
See: .phases/integration-test-reorganization/phase-4-update-references.md
EOF
)"
fi
```

---

## Verification Checklist

- [ ] Test discovery works (`pytest --co`)
- [ ] All 15+ tests discoverable
- [ ] CI/CD paths verified (parent paths work)
- [ ] Documentation updated
- [ ] No import errors
- [ ] Smoke tests execute successfully

---

## Next Steps

After completing Phase 4:
1. Verify tests can be collected
2. Note any test failures (not reference-related)
3. Proceed to Phase 5: Verification & QA

---

## Notes

### Why This Phase is Usually Easy

Integration tests are typically:
- Self-contained (no cross-imports)
- Discovered via pytest auto-discovery
- Run using parent directory paths

Most projects need **zero updates** in this phase beyond documentation.

### What pytest Auto-Discovers

pytest automatically finds tests if:
1. Directory has `__init__.py` ✓ (Phase 2 added these)
2. Files named `test_*.py` ✓ (already named correctly)
3. Functions named `test_*` ✓ (in test files)
4. Parent directory in PYTHONPATH ✓ (always true)

---

**Phase Status:** Ready for execution ✅
**Next Phase:** Phase 5 - Verification & QA
