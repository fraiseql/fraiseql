# Phase 1: Assessment & Planning

**Phase:** ANALYSIS (Read-only inventory and planning)
**Duration:** 15-20 minutes
**Risk:** None (no code changes)
**Status:** Ready for Execution

---

## Objective

Perform comprehensive analysis of current integration test structure, document all files, plan new organization, and identify any dependencies or special cases.

**Success:** Complete inventory with categorization, dependency map, and execution plan.

---

## Implementation Steps

### Step 1: Inventory Current Tests (5 min)

#### 1.1 List All Integration Test Files

```bash
# Get complete list of integration tests
cd /home/lionel/code/fraiseql
find tests/integration/database/sql -name "test_*.py" -type f | sort > /tmp/integration-tests-inventory.txt

# Display with line numbers
cat -n /tmp/integration-tests-inventory.txt

# Count total files
echo "Total integration test files: $(wc -l < /tmp/integration-tests-inventory.txt)"
```

**Expected Output:** ~30-40 integration test files

#### 1.2 Filter WHERE-Related Tests

```bash
# Find tests related to WHERE clause, filtering, operators
grep -E "filter|where|operator|network|ltree|daterange|coordinate|mac" \
    /tmp/integration-tests-inventory.txt \
    > /tmp/where-related-tests.txt

# Display
cat /tmp/where-related-tests.txt

# Count
echo "WHERE-related tests: $(wc -l < /tmp/where-related-tests.txt)"
```

**Expected Output:** ~15-20 files

#### 1.3 Analyze Test File Sizes

```bash
# Get file sizes for planning
find tests/integration/database/sql -name "test_*.py" -type f -exec wc -l {} \; | \
    sort -rn > /tmp/test-file-sizes.txt

# Show largest tests (might need special attention)
head -20 /tmp/test-file-sizes.txt

# Show statistics
awk '{sum+=$1; count++} END {print "Average lines:", int(sum/count), "\nTotal lines:", sum}' \
    /tmp/test-file-sizes.txt
```

**Acceptance:**
- [ ] Complete file inventory created
- [ ] WHERE-related tests filtered
- [ ] File sizes analyzed

---

### Step 2: Categorize Tests (5 min)

#### 2.1 Create Categorization Plan

```bash
# Create categorization worksheet
cat > /tmp/test-categorization.md << 'EOF'
# Integration Test Categorization Plan

## Network Tests (IP, MAC, Hostname, Email, Port)
Files:
- test_end_to_end_ip_filtering_clean.py → where/network/test_ip_filtering.py
- test_network_address_filtering.py → where/network/test_ip_operations.py
- test_network_filtering_fix.py → where/network/test_network_fixes.py
- test_production_cqrs_ip_filtering_bug.py → where/network/test_production_bugs.py
- test_network_operator_consistency_bug.py → where/network/test_consistency.py
- test_jsonb_network_filtering_bug.py → where/network/test_jsonb_integration.py
- test_mac_address_filter_operations.py → where/network/test_mac_operations.py
- test_end_to_end_mac_address_filtering.py → where/network/test_mac_filtering.py

Total: 8 files

## Specialized PostgreSQL Tests (LTree, FullText, etc.)
Files:
- test_end_to_end_ltree_filtering.py → where/specialized/test_ltree_filtering.py
- test_ltree_filter_operations.py → where/specialized/test_ltree_operations.py

Total: 2 files

## Temporal Tests (Date, DateTime, DateRange)
Files:
- test_daterange_filter_operations.py → where/temporal/test_daterange_operations.py
- test_end_to_end_daterange_filtering.py → where/temporal/test_daterange_filtering.py

Total: 2 files

## Spatial Tests (Coordinates, Geometry)
Files:
- test_coordinate_filter_operations.py → where/spatial/test_coordinate_operations.py

Total: 1 file

## Mixed/Cross-Cutting Tests (Stay in root)
Files:
- test_end_to_end_phase4_filtering.py → where/test_mixed_phase4.py
- test_end_to_end_phase5_filtering.py → where/test_mixed_phase5.py
- test_issue_resolution_demonstration.py → where/test_issue_resolution.py (if exists)
- test_restricted_filter_types.py → where/test_restricted_types.py (if exists)

Total: 2-4 files

## Other Tests (Keep in sql/)
Files:
- test_repository_*.py (stay in integration/repository/)
- test_graphql_*.py (stay in integration/graphql/)
- Non-WHERE tests

Total: Varies

---

## Summary
- Network: 8 files
- Specialized: 2 files
- Temporal: 2 files
- Spatial: 1 file
- Mixed: 2-4 files
- **Total to Move: 15-17 files**

EOF

cat /tmp/test-categorization.md
```

#### 2.2 Verify Categorization

```bash
# Check if all listed files actually exist
echo "Verifying files exist..."

# Network tests
for file in \
    test_end_to_end_ip_filtering_clean.py \
    test_network_address_filtering.py \
    test_network_filtering_fix.py \
    test_production_cqrs_ip_filtering_bug.py \
    test_network_operator_consistency_bug.py \
    test_jsonb_network_filtering_bug.py \
    test_mac_address_filter_operations.py \
    test_end_to_end_mac_address_filtering.py; do

    if [ -f "tests/integration/database/sql/$file" ]; then
        echo "✓ $file"
    else
        echo "✗ MISSING: $file"
    fi
done

# Specialized tests
for file in \
    test_end_to_end_ltree_filtering.py \
    test_ltree_filter_operations.py; do

    if [ -f "tests/integration/database/sql/$file" ]; then
        echo "✓ $file"
    else
        echo "✗ MISSING: $file"
    fi
done

# Temporal tests
for file in \
    test_daterange_filter_operations.py \
    test_end_to_end_daterange_filtering.py; do

    if [ -f "tests/integration/database/sql/$file" ]; then
        echo "✓ $file"
    else
        echo "✗ MISSING: $file"
    fi
done

# Spatial tests
if [ -f "tests/integration/database/sql/test_coordinate_filter_operations.py" ]; then
    echo "✓ test_coordinate_filter_operations.py"
else
    echo "✗ MISSING: test_coordinate_filter_operations.py"
fi
```

**Acceptance:**
- [ ] All tests categorized into groups
- [ ] New file names planned
- [ ] Files verified to exist

---

### Step 3: Analyze Dependencies (5 min)

#### 3.1 Find Import Dependencies

```bash
# Find imports between test files
grep -r "from tests.integration" tests/integration/database/sql/*.py 2>/dev/null | \
    grep -v ".pyc" > /tmp/test-imports.txt

# Display
cat /tmp/test-imports.txt

# Count
echo "Cross-test imports: $(wc -l < /tmp/test-imports.txt)"
```

**Expected:** Likely 0-2 imports (integration tests usually independent)

#### 3.2 Find Fixture Dependencies

```bash
# Find conftest.py files
find tests/integration/database -name "conftest.py" -type f

# Check if tests use fixtures from conftest
grep -l "conftest" tests/integration/database/sql/test_*.py 2>/dev/null || echo "No direct conftest references"

# List fixtures in conftest
if [ -f "tests/integration/database/conftest.py" ]; then
    echo "=== Fixtures in conftest.py ==="
    grep -E "^def |^async def " tests/integration/database/conftest.py | head -20
fi
```

**Expected:** Tests use fixtures from `conftest.py` but don't import directly

#### 3.3 Find CI/CD References

```bash
# Find test paths in CI configuration
find . -name "*.yml" -o -name "*.yaml" -o -name "Makefile" | \
    xargs grep -l "tests/integration" 2>/dev/null | \
    grep -v ".git" > /tmp/ci-files.txt

echo "=== CI/CD files referencing integration tests ==="
cat /tmp/ci-files.txt

# Check specific paths
for file in $(cat /tmp/ci-files.txt); do
    echo "=== $file ==="
    grep "tests/integration" "$file" | head -5
done
```

**Acceptance:**
- [ ] Import dependencies documented
- [ ] Fixture dependencies understood
- [ ] CI/CD references identified

---

### Step 4: Create Execution Plan (5 min)

#### 4.1 Generate Migration Script

```bash
# Create detailed migration plan
cat > /tmp/migration-plan.sh << 'EOF'
#!/bin/bash
# Integration Test Reorganization - Migration Plan
# Generated: 2025-12-11
# DO NOT EXECUTE - This is a planning document

set -e

BASE_DIR="tests/integration/database/sql"
TARGET_DIR="$BASE_DIR/where"

echo "=== Phase 3: Migration Plan ==="
echo "This script shows what will be done in Phase 3"
echo ""

# Network tests
echo "# Network Tests (8 files)"
echo "git mv $BASE_DIR/test_end_to_end_ip_filtering_clean.py $TARGET_DIR/network/test_ip_filtering.py"
echo "git mv $BASE_DIR/test_network_address_filtering.py $TARGET_DIR/network/test_ip_operations.py"
echo "git mv $BASE_DIR/test_network_filtering_fix.py $TARGET_DIR/network/test_network_fixes.py"
echo "git mv $BASE_DIR/test_production_cqrs_ip_filtering_bug.py $TARGET_DIR/network/test_production_bugs.py"
echo "git mv $BASE_DIR/test_network_operator_consistency_bug.py $TARGET_DIR/network/test_consistency.py"
echo "git mv $BASE_DIR/test_jsonb_network_filtering_bug.py $TARGET_DIR/network/test_jsonb_integration.py"
echo "git mv $BASE_DIR/test_mac_address_filter_operations.py $TARGET_DIR/network/test_mac_operations.py"
echo "git mv $BASE_DIR/test_end_to_end_mac_address_filtering.py $TARGET_DIR/network/test_mac_filtering.py"
echo ""

# Specialized tests
echo "# Specialized Tests (2 files)"
echo "git mv $BASE_DIR/test_end_to_end_ltree_filtering.py $TARGET_DIR/specialized/test_ltree_filtering.py"
echo "git mv $BASE_DIR/test_ltree_filter_operations.py $TARGET_DIR/specialized/test_ltree_operations.py"
echo ""

# Temporal tests
echo "# Temporal Tests (2 files)"
echo "git mv $BASE_DIR/test_daterange_filter_operations.py $TARGET_DIR/temporal/test_daterange_operations.py"
echo "git mv $BASE_DIR/test_end_to_end_daterange_filtering.py $TARGET_DIR/temporal/test_daterange_filtering.py"
echo ""

# Spatial tests
echo "# Spatial Tests (1 file)"
echo "git mv $BASE_DIR/test_coordinate_filter_operations.py $TARGET_DIR/spatial/test_coordinate_operations.py"
echo ""

# Mixed tests
echo "# Mixed Tests (2 files)"
echo "git mv $BASE_DIR/test_end_to_end_phase4_filtering.py $TARGET_DIR/test_mixed_phase4.py"
echo "git mv $BASE_DIR/test_end_to_end_phase5_filtering.py $TARGET_DIR/test_mixed_phase5.py"
echo ""

echo "=== Total: 15 files to move ==="
EOF

chmod +x /tmp/migration-plan.sh
cat /tmp/migration-plan.sh
```

#### 4.2 Create Rollback Script

```bash
# Create rollback plan
cat > /tmp/rollback-plan.sh << 'EOF'
#!/bin/bash
# Integration Test Reorganization - Rollback Plan
# Use this if Phase 3-6 need to be reverted

set -e

echo "=== Rollback Plan ==="
echo "Option 1: Git reset (safest)"
echo "  git reset --hard HEAD~1  # Revert last commit"
echo "  git clean -fd            # Remove untracked files/dirs"
echo ""
echo "Option 2: Manual rollback"
echo "  1. Delete tests/integration/database/sql/where/ directory"
echo "  2. Git restore moved files to original locations"
echo ""
echo "Option 3: Revert specific commit"
echo "  git revert <commit-hash>"
echo ""
echo "Current branch: $(git branch --show-current)"
echo "Last 3 commits:"
git log --oneline -3
EOF

chmod +x /tmp/rollback-plan.sh
cat /tmp/rollback-plan.sh
```

**Acceptance:**
- [ ] Migration script created
- [ ] Rollback plan documented
- [ ] Execution order clear

---

### Step 5: Risk Assessment (2 min)

#### 5.1 Identify Potential Issues

```bash
cat > /tmp/risk-assessment.md << 'EOF'
# Risk Assessment - Integration Test Reorganization

## Identified Risks

### 1. Test Discovery Breaks
**Probability:** Low
**Impact:** Medium
**Mitigation:** pytest should auto-discover, but verify with `pytest --co`

### 2. Fixture Imports Break
**Probability:** Low
**Impact:** Medium
**Mitigation:** conftest.py at parent level should still work

### 3. CI/CD Path Changes
**Probability:** Medium
**Impact:** High
**Mitigation:** Update paths in Phase 4, use parent directory paths

### 4. Git History Lost
**Probability:** Low
**Impact:** Low
**Mitigation:** Use `git mv` to preserve history

### 5. Import Errors
**Probability:** Very Low
**Impact:** Medium
**Mitigation:** Integration tests rarely import each other

## Overall Risk: LOW

Rationale:
- Small number of files (15)
- Independent tests (no cross-dependencies)
- Easy rollback (git reset)
- Clear execution plan

EOF

cat /tmp/risk-assessment.md
```

**Acceptance:**
- [ ] Risks identified and documented
- [ ] Mitigations planned
- [ ] Overall risk level acceptable

---

## Verification

### Phase 1 Completion Checklist

- [ ] All integration tests inventoried
- [ ] Tests categorized into network/specialized/temporal/spatial/mixed
- [ ] All files verified to exist
- [ ] Dependencies analyzed (imports, fixtures, CI/CD)
- [ ] Migration plan created
- [ ] Rollback plan created
- [ ] Risk assessment completed
- [ ] All analysis files saved in `/tmp/`

### Generated Artifacts

All analysis saved to `/tmp/` for reference in later phases:
- `/tmp/integration-tests-inventory.txt` - Full file list
- `/tmp/where-related-tests.txt` - Filtered list
- `/tmp/test-file-sizes.txt` - Size analysis
- `/tmp/test-categorization.md` - Categorization plan
- `/tmp/test-imports.txt` - Import dependencies
- `/tmp/ci-files.txt` - CI/CD references
- `/tmp/migration-plan.sh` - Migration script
- `/tmp/rollback-plan.sh` - Rollback script
- `/tmp/risk-assessment.md` - Risk analysis

### Expected Output Summary

```
Total integration test files: 30-40
WHERE-related tests: 15-20
Files to reorganize: 15
- Network: 8 files
- Specialized: 2 files
- Temporal: 2 files
- Spatial: 1 file
- Mixed: 2 files
```

---

## Next Steps

After completing Phase 1:
1. Review categorization plan - adjust if needed
2. Confirm file names are appropriate
3. Get team approval if required
4. Proceed to Phase 2: Create Directory Structure

---

## Notes

### Key Decisions Made
- Keep mixed/cross-cutting tests in root `where/` directory
- Rename files to be more descriptive (e.g., `test_ip_filtering.py` vs `test_end_to_end_ip_filtering_clean.py`)
- Match unit test structure exactly
- Use `git mv` to preserve history

### Assumptions
- Integration tests are independent (no cross-imports)
- Fixtures come from conftest.py (will still work)
- CI/CD uses parent directory paths
- Test count may have changed since initial inventory

---

**Phase Status:** Ready for execution ✅
**Next Phase:** Phase 2 - Create Directory Structure
