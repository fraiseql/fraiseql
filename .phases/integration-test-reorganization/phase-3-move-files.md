# Phase 3: Move & Rename Files

**Phase:** MIGRATION (File moves)
**Duration:** 15-20 minutes
**Risk:** Medium (tests will temporarily fail until imports fixed)
**Status:** Ready for Execution

---

## Objective

Move all integration test files from flat structure into new organized directories, using `git mv` to preserve history.

**Success:** All 15 test files moved to correct locations, git history preserved.

---

## Prerequisites

- [ ] Phase 2 completed (directory structure created)
- [ ] Clean git working directory (Phase 2 changes committed)
- [ ] All tests currently passing

---

## IMPORTANT: Pre-Migration Safety

```bash
# Create backup branch
cd /home/lionel/code/fraiseql
git checkout -b backup/before-test-reorganization
git checkout main  # or your working branch

# Verify current test status
uv run pytest tests/integration/database/sql/ -k "filter or ltree or network" --co -q | wc -l
# Note: This count for comparison after migration
```

---

## Implementation Steps

### Step 1: Move Network Tests (5 min)

```bash
cd /home/lionel/code/fraiseql
BASE="tests/integration/database/sql"
TARGET="$BASE/where/network"

# Move IP-related tests
git mv $BASE/test_end_to_end_ip_filtering_clean.py $TARGET/test_ip_filtering.py
git mv $BASE/test_network_address_filtering.py $TARGET/test_ip_operations.py
git mv $BASE/test_network_filtering_fix.py $TARGET/test_network_fixes.py

# Move production/bug tests
git mv $BASE/test_production_cqrs_ip_filtering_bug.py $TARGET/test_production_bugs.py
git mv $BASE/test_network_operator_consistency_bug.py $TARGET/test_consistency.py
git mv $BASE/test_jsonb_network_filtering_bug.py $TARGET/test_jsonb_integration.py

# Move MAC tests
git mv $BASE/test_mac_address_filter_operations.py $TARGET/test_mac_operations.py
git mv $BASE/test_end_to_end_mac_address_filtering.py $TARGET/test_mac_filtering.py

# Verify moves
echo "Network tests moved: $(ls $TARGET/test_*.py | wc -l)"
ls -1 $TARGET/test_*.py
```

**Expected:** 8 files in `where/network/`

**Acceptance:**
- [ ] All 8 network test files moved
- [ ] Files renamed appropriately
- [ ] Git history preserved (check with `git log --follow`)

---

### Step 2: Move Specialized Tests (2 min)

```bash
cd /home/lionel/code/fraiseql
BASE="tests/integration/database/sql"
TARGET="$BASE/where/specialized"

# Move LTree tests
git mv $BASE/test_end_to_end_ltree_filtering.py $TARGET/test_ltree_filtering.py
git mv $BASE/test_ltree_filter_operations.py $TARGET/test_ltree_operations.py

# Verify moves
echo "Specialized tests moved: $(ls $TARGET/test_*.py | wc -l)"
ls -1 $TARGET/test_*.py
```

**Expected:** 2 files in `where/specialized/`

**Acceptance:**
- [ ] All 2 specialized test files moved
- [ ] Files renamed appropriately

---

### Step 3: Move Temporal Tests (2 min)

```bash
cd /home/lionel/code/fraiseql
BASE="tests/integration/database/sql"
TARGET="$BASE/where/temporal"

# Move DateRange tests
git mv $BASE/test_daterange_filter_operations.py $TARGET/test_daterange_operations.py
git mv $BASE/test_end_to_end_daterange_filtering.py $TARGET/test_daterange_filtering.py

# Verify moves
echo "Temporal tests moved: $(ls $TARGET/test_*.py | wc -l)"
ls -1 $TARGET/test_*.py
```

**Expected:** 2 files in `where/temporal/`

**Acceptance:**
- [ ] All 2 temporal test files moved
- [ ] Files renamed appropriately

---

### Step 4: Move Spatial Tests (1 min)

```bash
cd /home/lionel/code/fraiseql
BASE="tests/integration/database/sql"
TARGET="$BASE/where/spatial"

# Move coordinate tests
git mv $BASE/test_coordinate_filter_operations.py $TARGET/test_coordinate_operations.py

# Verify move
echo "Spatial tests moved: $(ls $TARGET/test_*.py | wc -l)"
ls -1 $TARGET/test_*.py
```

**Expected:** 1 file in `where/spatial/`

**Acceptance:**
- [ ] Spatial test file moved
- [ ] File renamed appropriately

---

### Step 5: Move Mixed/Root Tests (2 min)

```bash
cd /home/lionel/code/fraiseql
BASE="tests/integration/database/sql"
TARGET="$BASE/where"

# Move phase-based mixed tests
git mv $BASE/test_end_to_end_phase4_filtering.py $TARGET/test_mixed_phase4.py
git mv $BASE/test_end_to_end_phase5_filtering.py $TARGET/test_mixed_phase5.py

# Optional: Move other cross-cutting tests if they exist
if [ -f "$BASE/test_issue_resolution_demonstration.py" ]; then
    git mv $BASE/test_issue_resolution_demonstration.py $TARGET/test_issue_resolution.py
fi

if [ -f "$BASE/test_restricted_filter_types.py" ]; then
    git mv $BASE/test_restricted_filter_types.py $TARGET/test_restricted_types.py
fi

# Verify moves
echo "Root where/ tests: $(ls $TARGET/test_*.py 2>/dev/null | wc -l)"
ls -1 $TARGET/test_*.py 2>/dev/null || echo "No root-level test files"
```

**Expected:** 2-4 files in `where/` root

**Acceptance:**
- [ ] Mixed test files moved
- [ ] Files renamed appropriately
- [ ] Optional files moved if present

---

### Step 6: Verification (3 min)

#### 6.1 Verify All Files Moved

```bash
cd /home/lionel/code/fraiseql

# Count files in each directory
echo "=== File Counts ==="
echo "Network: $(ls tests/integration/database/sql/where/network/test_*.py 2>/dev/null | wc -l)"
echo "Specialized: $(ls tests/integration/database/sql/where/specialized/test_*.py 2>/dev/null | wc -l)"
echo "Temporal: $(ls tests/integration/database/sql/where/temporal/test_*.py 2>/dev/null | wc -l)"
echo "Spatial: $(ls tests/integration/database/sql/where/spatial/test_*.py 2>/dev/null | wc -l)"
echo "Root: $(ls tests/integration/database/sql/where/test_*.py 2>/dev/null | wc -l)"

# Total moved files
TOTAL=$(find tests/integration/database/sql/where -name "test_*.py" -type f | wc -l)
echo "Total test files: $TOTAL"

# Expected: 15 files (8+2+2+1+2)
if [ "$TOTAL" -eq 15 ]; then
    echo "✓ Correct number of files moved"
else
    echo "✗ Warning: Expected 15 files, found $TOTAL"
fi
```

**Expected Output:**
```
Network: 8
Specialized: 2
Temporal: 2
Spatial: 1
Root: 2
Total test files: 15
```

#### 6.2 Verify No Files Left in Old Location

```bash
# Check for any leftover WHERE-related tests in old location
echo "=== Checking for leftover files ==="
cd /home/lionel/code/fraiseql/tests/integration/database/sql

# List any test files with filter/network/ltree/etc in name
ls -1 test_*filter*.py test_*network*.py test_*ltree*.py test_*daterange*.py test_*coordinate*.py test_*mac*.py 2>/dev/null | \
    grep -v "^where/" || echo "✓ No leftover files"

# If any files found, they might need to be moved
```

**Expected:** No files found (all moved)

#### 6.3 Display Final Structure

```bash
# Show complete structure with file counts
tree tests/integration/database/sql/where/ -L 2 --filesfirst || \
    find tests/integration/database/sql/where -type f -name "test_*.py" | sort
```

**Expected:** Organized tree with all test files in correct locations

**Acceptance:**
- [ ] 15 total test files in new locations
- [ ] No leftover files in old location
- [ ] Directory structure matches plan

---

### Step 7: Check Git Status (1 min)

```bash
cd /home/lionel/code/fraiseql

# Show what git sees
git status --short tests/integration/database/sql/

# Show detailed renames
git status tests/integration/database/sql/ | grep -A5 "renamed:"

# Count staged changes
echo "Files to commit: $(git diff --cached --name-only | wc -l)"
```

**Expected:** Git shows all moves as "renamed" (preserving history)

**Acceptance:**
- [ ] Git recognizes moves as renames (not delete + add)
- [ ] All changes staged
- [ ] Ready for commit

---

## Expected Test Failures

**Tests WILL fail after this phase** - this is expected!

Reason: pytest may not discover tests in new locations until Phase 4 updates are complete.

```bash
# Try running tests (expected to have issues)
uv run pytest tests/integration/database/sql/where/ --co -q 2>&1 | head -20

# Common errors:
# - "No tests collected" - pytest discovery needs update
# - Import errors - test references need update
# - Fixture errors - unlikely but possible
```

**Don't worry!** Phase 4 will fix all issues. Just verify git renames are correct.

---

## Rollback Plan

If issues occur:

### Option 1: Git Reset (Recommended)
```bash
# Revert all moves
cd /home/lionel/code/fraiseql
git reset --hard HEAD
git clean -fd

# Verify rollback
git status
ls tests/integration/database/sql/ | grep test_
```

### Option 2: Restore from Backup Branch
```bash
# Switch to backup branch
git checkout backup/before-test-reorganization

# Copy back original files
git checkout main tests/integration/database/sql/

# Delete new structure
rm -rf tests/integration/database/sql/where/
```

### Option 3: Manual Restore (Last Resort)
```bash
# Git has rename information, can restore individual files
git log --follow tests/integration/database/sql/where/network/test_ip_filtering.py
# Shows original path, can cherry-pick restore if needed
```

---

## Commit Changes

```bash
cd /home/lionel/code/fraiseql

# Verify staged changes
git status

# Commit moves
git commit -m "$(cat <<'EOF'
test(integration): Move WHERE tests to organized structure [PHASE-3]

Reorganize integration tests from flat structure into hierarchical
organization matching unit test structure.

Moved files:
- Network tests: 8 files → where/network/
- Specialized tests: 2 files → where/specialized/
- Temporal tests: 2 files → where/temporal/
- Spatial tests: 1 file → where/spatial/
- Mixed tests: 2 files → where/ (root)

File renames:
- test_end_to_end_<type>_filtering.py → test_<type>_filtering.py
- test_<type>_filter_operations.py → test_<type>_operations.py
- Removed redundant prefixes for clarity

Total: 15 files moved and renamed using git mv

Note: Tests may not run until Phase 4 (update references) completes.
This is expected behavior.

Phase: 3/6 (Move Files)
See: .phases/integration-test-reorganization/phase-3-move-files.md
EOF
)"

# Verify commit
git log -1 --stat --name-status
```

**Expected:** Commit shows "R" (rename) status, not "D" + "A"

---

## Verification Checklist

- [ ] All 15 test files moved to new locations
- [ ] Git shows moves as renames (preserving history)
- [ ] No leftover files in old location
- [ ] Directory counts match plan (8+2+2+1+2)
- [ ] Changes committed with descriptive message
- [ ] Backup branch created before starting

---

## Next Steps

After completing Phase 3:
1. Don't panic if tests don't run - this is expected
2. Proceed immediately to Phase 4: Update References
3. Phase 4 will fix test discovery and any import issues

---

## Notes

### Why Tests May Fail

- pytest may not auto-discover tests in new locations
- Some tests might have hard-coded paths
- CI/CD paths need updates
- Not a problem - Phase 4 fixes everything

### Git History Preservation

Using `git mv` ensures:
- `git log --follow <file>` shows full history
- `git blame -C <file>` attributes original authors
- GitHub/GitLab show "renamed" not "deleted + added"

---

**Phase Status:** Ready for execution ✅
**Next Phase:** Phase 4 - Update References
**Warning:** Tests expected to fail after this phase - continue to Phase 4!
