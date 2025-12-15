# Phase 4: Clean Content - Remove Development Markers (REFACTOR)

## Objective
Remove all development process markers from test file content, replacing them with clear, evergreen descriptions of what each test validates.

## Context
Test files contain comments, docstrings, and code that reference:
- Work packages (WP-XXX)
- Development phases
- TDD markers (RED/GREEN/REFACTOR)
- Version numbers and dates
- "This fixes..." / "Regression test..." language
- Historical architectural decisions

These need to be replaced with professional, timeless documentation.

## Files to Modify
~50 integration test files need content cleanup (most of the suite)

## Implementation Steps

### Step 1: Create Search Patterns List

First, understand what markers exist:

```bash
# Create a reference file of patterns to search for
cat > /tmp/cleanup-patterns.txt << 'EOF'
WP-[0-9]
Phase [0-9]
\[RED\]
\[GREEN\]
\[REFACTOR\]
\[QA\]
regression test
verifies the fix
This test verifies
Before the fix
After the fix
Fixed in version
v[0-9]+\.[0-9]+\.[0-9]+
2025-[0-9]{2}-[0-9]{2}
TODO.*implementation
old behavior
new behavior
historical
architectural decision
_fix
_regression
EOF

cat /tmp/cleanup-patterns.txt
```

### Step 2: Identify Files Needing Content Cleanup

```bash
# Find all files with development markers
for pattern in "WP-" "Phase " "\[RED\]" "\[GREEN\]" "regression" "verifies the fix" "Before the fix"; do
    echo "=== Files containing: $pattern ==="
    grep -l "$pattern" tests/integration/**/*.py 2>/dev/null | head -10
    echo ""
done
```

**Expected output**: List of files grouped by marker type

**Action**: Create a prioritized list starting with files that have the most markers.

### Step 2.5: Create File Analysis Tool

Create a script to analyze individual files and show exactly what needs changing:

```bash
cat > /tmp/analyze-file-markers.sh << 'EOF'
#!/bin/bash
# Analyze a single test file and show line-by-line what markers need removal

file="$1"
if [ -z "$file" ]; then
    echo "Usage: $0 <test-file.py>"
    exit 1
fi

if [ ! -f "$file" ]; then
    echo "Error: File not found: $file"
    exit 1
fi

echo "=========================================="
echo "ANALYSIS: $file"
echo "=========================================="
echo ""

marker_count=0

# WP- references
echo "📍 WP- references (work packages):"
if grep -n "WP-[0-9]" "$file" 2>/dev/null; then
    marker_count=$((marker_count + $(grep -c "WP-[0-9]" "$file" 2>/dev/null)))
else
    echo "  ✓ None found"
fi
echo ""

# Phase references
echo "📍 Phase references:"
if grep -n "Phase [0-9]" "$file" 2>/dev/null; then
    marker_count=$((marker_count + $(grep -c "Phase [0-9]" "$file" 2>/dev/null)))
else
    echo "  ✓ None found"
fi
echo ""

# TDD markers
echo "📍 TDD markers ([RED]/[GREEN]/[REFACTOR]):"
if grep -n "\[RED\]\|\[GREEN\]\|\[REFACTOR\]\|\[QA\]" "$file" 2>/dev/null; then
    marker_count=$((marker_count + $(grep -c "\[RED\]\|\[GREEN\]\|\[REFACTOR\]\|\[QA\]" "$file" 2>/dev/null)))
else
    echo "  ✓ None found"
fi
echo ""

# Class names with Fix/Regression/Phase
echo "📍 Process hints in class names:"
if grep -n "class Test.*Fix\|class Test.*Regression\|class TestPhase" "$file" 2>/dev/null; then
    marker_count=$((marker_count + $(grep -c "class Test.*Fix\|class Test.*Regression\|class TestPhase" "$file" 2>/dev/null)))
else
    echo "  ✓ None found"
fi
echo ""

# Function names with _fix/_regression
echo "📍 Process hints in function names:"
if grep -n "def test_.*_fix\|def test_.*_regression" "$file" 2>/dev/null; then
    marker_count=$((marker_count + $(grep -c "def test_.*_fix\|def test_.*_regression" "$file" 2>/dev/null)))
else
    echo "  ✓ None found"
fi
echo ""

# Regression language
echo "📍 Regression/fix language in comments/docstrings:"
if grep -n "regression test\|verifies the fix\|This fixes\|Before the fix\|After the fix" "$file" -i 2>/dev/null; then
    marker_count=$((marker_count + $(grep -c "regression test\|verifies the fix\|This fixes\|Before the fix\|After the fix" "$file" -i 2>/dev/null)))
else
    echo "  ✓ None found"
fi
echo ""

# Version numbers
echo "📍 Version numbers:"
if grep -n "v[0-9]\+\.[0-9]\+\.[0-9]\+" "$file" 2>/dev/null; then
    marker_count=$((marker_count + $(grep -c "v[0-9]\+\.[0-9]\+\.[0-9]\+" "$file" 2>/dev/null)))
else
    echo "  ✓ None found"
fi
echo ""

# TODO in implementations
echo "📍 TODO comments:"
if grep -n "TODO" "$file" 2>/dev/null; then
    marker_count=$((marker_count + $(grep -c "TODO" "$file" 2>/dev/null)))
else
    echo "  ✓ None found"
fi
echo ""

echo "=========================================="
echo "SUMMARY: $marker_count total markers found"
if [ $marker_count -eq 0 ]; then
    echo "✓ File is clean!"
else
    echo "⚠ File needs cleanup"
fi
echo "=========================================="
EOF

chmod +x /tmp/analyze-file-markers.sh

# Test it on one file
echo "Example usage:"
/tmp/analyze-file-markers.sh tests/integration/graphql/mutations/test_error_arrays.py
```

**Expected output**: Line-by-line report showing what needs to be changed

**Usage pattern**:
```bash
# Analyze a file before editing
/tmp/analyze-file-markers.sh tests/integration/graphql/test_example.py

# Edit the file based on the report
$EDITOR tests/integration/graphql/test_example.py

# Re-analyze to verify cleanup
/tmp/analyze-file-markers.sh tests/integration/graphql/test_example.py
# Should show: "✓ File is clean!"
```

### Step 3: Clean High-Impact Files (Heavy Markers)

These files were mentioned in the cleanup plan as having heavy process markers.

**Workflow for each file**:
1. Run analysis script: `/tmp/analyze-file-markers.sh <file>`
2. Note the line numbers with markers
3. Open file in editor
4. Fix each marker line-by-line
5. Re-run analysis script to verify
6. Run tests for that file
7. Move to next file

#### 3.1: Clean test_error_arrays.py (formerly test_native_error_arrays.py)

**File**: `tests/integration/graphql/mutations/test_error_arrays.py`

**First, analyze the file**:
```bash
/tmp/analyze-file-markers.sh tests/integration/graphql/mutations/test_error_arrays.py
```

**Expected analysis output**: Shows lines with WP-034, Phase markers, version numbers

**Markers to remove**:
- All WP-034 references (work package markers)
- Phase markers (Phase 3, etc.)
- "Native implementation" comments
- Version numbers (v1.8.0-beta.4, etc.)
- Class names like `TestPhaseX` → `TestErrorArrays`
- Function names like `test_xxx_fix` → `test_xxx`

**Strategy**:
1. Run the analysis script (above)
2. Open the file in editor
3. For each line identified:
   - Module docstring: Remove WP-034, Phase 3, version → Replace with "Tests for mutation error array handling"
   - Class names: `TestPhase3ErrorArrays` → `TestMutationErrorArrays`
   - Test docstrings: Remove "fixed in Phase X" → Focus on expected behavior
   - Inline comments: Remove references to fixes/implementation timeline
4. Save and verify: `/tmp/analyze-file-markers.sh tests/integration/graphql/mutations/test_error_arrays.py`
   - Should show: "✓ File is clean!"
5. Run tests: `uv run pytest tests/integration/graphql/mutations/test_error_arrays.py -v`

**Example transformation**:
```python
# BEFORE (bad)
"""WP-034: Native Error Arrays Implementation - Phase 3

This test validates the Phase 3 implementation of native error arrays.
Fixed in v1.8.0-beta.4 (2025-12-09).
"""

class TestPhase3ErrorArrays:
    def test_mutation_error_array_fix(self):
        """Test that mutation error arrays work (fixed in Phase 3)."""

# AFTER (good)
"""Tests for mutation error array handling.

Validates that mutations can return arrays of error objects in GraphQL
responses, properly serialized and accessible to clients.
"""

class TestMutationErrorArrays:
    def test_mutation_error_array_serialization(self):
        """Test that mutation error arrays are properly serialized in responses."""
```

**Commands**:
```bash
# Open file for editing
$EDITOR tests/integration/graphql/mutations/test_error_arrays.py

# After editing, verify no markers remain
grep -E "WP-|Phase|v[0-9]+\.[0-9]+" tests/integration/graphql/mutations/test_error_arrays.py
# Should output nothing

# Run tests
uv run pytest tests/integration/graphql/mutations/test_error_arrays.py -v
```

#### 3.2: Clean test_fastapi_jsonb_integration.py

**File**: `tests/integration/graphql/test_fastapi_jsonb_integration.py`

**First, analyze**:
```bash
/tmp/analyze-file-markers.sh tests/integration/graphql/test_fastapi_jsonb_integration.py
```

**Markers to remove**:
- Phase references
- JSONB implementation notes
- Timeline/version information

**Focus**: Rewrite docstrings to explain JSONB passthrough behavior, not when it was implemented.

**Commands**:
```bash
# Edit based on analysis
$EDITOR tests/integration/graphql/test_fastapi_jsonb_integration.py

# Verify clean
/tmp/analyze-file-markers.sh tests/integration/graphql/test_fastapi_jsonb_integration.py

# Test
uv run pytest tests/integration/graphql/test_fastapi_jsonb_integration.py -v
```

#### 3.3: Clean test_graphql_cascade.py

**File**: `tests/integration/graphql/test_graphql_cascade.py`

**First, analyze**:
```bash
/tmp/analyze-file-markers.sh tests/integration/graphql/test_graphql_cascade.py
```

**Markers to remove**:
- "Phase 3 validation" references
- Cascade implementation notes
- Historical context about when feature was added

**Focus**: Describe cascade delete behavior in domain terms.

**Commands**:
```bash
$EDITOR tests/integration/graphql/test_graphql_cascade.py
/tmp/analyze-file-markers.sh tests/integration/graphql/test_graphql_cascade.py
uv run pytest tests/integration/graphql/test_graphql_cascade.py -v
```

#### 3.4: Clean test_schema_validation.py (formerly test_phase0_validation.py)

**File**: `tests/integration/meta/test_schema_validation.py`

**First, analyze**:
```bash
/tmp/analyze-file-markers.sh tests/integration/meta/test_schema_validation.py
```

**Markers to remove**:
- All "phase0" references (already removed from filename)
- Bootstrap/initialization timeline language
- Historical context about initial setup

**Focus**: Describe schema validation requirements as current behavior.

**Commands**:
```bash
$EDITOR tests/integration/meta/test_schema_validation.py
/tmp/analyze-file-markers.sh tests/integration/meta/test_schema_validation.py
uv run pytest tests/integration/meta/test_schema_validation.py -v
```

### Step 4: Clean Class and Function Names

Many test classes/functions have process hints in their names.

#### 4.1: Find classes/functions with process hints

```bash
# Find test classes with process hints
grep -rn "class Test.*Fix\|class Test.*Regression\|class TestPhase" tests/integration/ --include="*.py"

# Find test functions with process hints
grep -rn "def test_.*_fix\|def test_.*_regression" tests/integration/ --include="*.py"
```

**Expected output**: List of classes/functions to rename

#### 4.2: Rename test classes

**Pattern**: `TestXxxFix` → `TestXxx`

**Example**:
```python
# BEFORE
class TestMutationNameCollisionFix:

# AFTER
class TestMutationNameResolution:
```

**Commands**: For each file identified:
```bash
# Example: test_mutation_name_resolution.py
$EDITOR tests/integration/graphql/mutations/test_mutation_name_resolution.py

# After editing, verify no "Fix" in class names
grep "class.*Fix" tests/integration/graphql/mutations/test_mutation_name_resolution.py
# Should output nothing
```

#### 4.3: Rename test functions

**Patterns**:
- `test_xxx_fix` → `test_xxx`
- `test_xxx_regression` → `test_xxx` (or more descriptive name)

**Example**:
```python
# BEFORE
def test_resolver_names_fix(self):
    """Test that resolver names are fixed."""

# AFTER
def test_resolver_names_match_function_names(self):
    """Test that resolver names correctly correspond to function names."""
```

### Step 5: Clean Docstrings and Comments

For ALL test files, update docstrings to be evergreen.

#### 5.1: Rewrite module docstrings

**Bad patterns to remove**:
- "Regression test for..."
- "This test verifies the fix for..."
- "Fixed in version X"
- "WP-XXX implementation"
- Version numbers and dates

**Good patterns to use**:
- "Tests for [feature name]"
- "Validates that [expected behavior]"
- "Ensures [domain requirement]"

**Example transformation**:
```python
# BEFORE (bad)
"""Regression test for enum conversion fix.

This test verifies that the bug where GraphQL enum values weren't
properly converted has been fixed.

Fixed in v1.5.0 (2025-11-20).
"""

# AFTER (good)
"""Tests for GraphQL enum type handling.

Validates that Python enum values are correctly converted to/from
GraphQL enum types in queries and mutations.
"""
```

#### 5.2: Rewrite test function docstrings

**Bad patterns**:
- "Test that X is fixed"
- "Verify the fix for Y"
- "Regression test"

**Good patterns**:
- "Test that X behaves as expected"
- "Verify that Y produces Z"
- "Ensure A when B"

**Example**:
```python
# BEFORE (bad)
def test_enum_conversion_fix(self):
    """Test that enum conversion is fixed."""

# AFTER (good)
def test_enum_values_serialize_correctly(self):
    """Test that Python enum values serialize to GraphQL enum strings."""
```

#### 5.3: Remove TODO comments in implementations

```bash
# Find TODO comments in test implementations
grep -rn "TODO" tests/integration/ --include="*.py"
```

**Action**: For each TODO:
- If test is complete: remove the TODO
- If test is incomplete: complete it or remove the test

### Step 6: Systematic Cleanup of Remaining Files

Use the analysis script to systematically clean all remaining files.

#### 6.1: Generate list of files needing cleanup

```bash
# Create a script to find all files with markers and sort by marker count
cat > /tmp/find-all-files-needing-cleanup.sh << 'EOF'
#!/bin/bash
echo "Scanning all integration test files for markers..."
echo ""

declare -A file_markers

for file in tests/integration/**/*.py; do
    if [ -f "$file" ]; then
        count=0
        count=$((count + $(grep -c "WP-[0-9]" "$file" 2>/dev/null || echo 0)))
        count=$((count + $(grep -c "Phase [0-9]" "$file" 2>/dev/null || echo 0)))
        count=$((count + $(grep -c "\[RED\]\|\[GREEN\]\|\[REFACTOR\]" "$file" 2>/dev/null || echo 0)))
        count=$((count + $(grep -c "class Test.*Fix\|class Test.*Regression" "$file" 2>/dev/null || echo 0)))
        count=$((count + $(grep -c "def test_.*_fix\|def test_.*_regression" "$file" 2>/dev/null || echo 0)))

        if [ $count -gt 0 ]; then
            echo "$count:$file"
        fi
    fi
done | sort -rn | while IFS=: read count file; do
    echo "[$count markers] $file"
done
EOF

chmod +x /tmp/find-all-files-needing-cleanup.sh
/tmp/find-all-files-needing-cleanup.sh
```

**Expected output**: List of files sorted by marker count (most markers first)

**Example output**:
```
[15 markers] tests/integration/graphql/test_example.py
[8 markers] tests/integration/auth/test_another.py
[3 markers] tests/integration/repository/test_third.py
```

#### 6.2: Clean files in priority order

Work through the list from highest marker count to lowest:

```bash
# Get the list
/tmp/find-all-files-needing-cleanup.sh > /tmp/cleanup-order.txt

# For each file (can be done manually or with a loop)
while IFS='] ' read -r markers file; do
    markers=${markers#[}
    echo ""
    echo "=========================================="
    echo "Processing: $file ($markers markers)"
    echo "=========================================="

    # Analyze
    /tmp/analyze-file-markers.sh "$file"

    # Pause for manual editing
    echo ""
    echo "Press ENTER to edit this file, or Ctrl+C to stop"
    read

    # Edit
    $EDITOR "$file"

    # Verify
    echo "Re-analyzing after edit..."
    /tmp/analyze-file-markers.sh "$file"

    # Test
    echo "Running tests..."
    uv run pytest "$file" -v --tb=short

    echo ""
    echo "File complete. Continue? (y/n)"
    read continue
    if [ "$continue" != "y" ]; then
        break
    fi
done < /tmp/cleanup-order.txt
```

**Action**: This semi-automated workflow will:
1. Show you each file that needs cleanup
2. Analyze it to show what markers exist
3. Let you edit the file
4. Re-analyze to verify cleanup
5. Run tests to verify nothing broke
6. Move to the next file

**Alternative manual approach** (if you prefer more control):
```bash
# Get the list
/tmp/find-all-files-needing-cleanup.sh > /tmp/cleanup-order.txt
cat /tmp/cleanup-order.txt

# Manually work through each file:
# 1. Pick a file from the list
# 2. Analyze: /tmp/analyze-file-markers.sh <file>
# 3. Edit: $EDITOR <file>
# 4. Verify: /tmp/analyze-file-markers.sh <file>
# 5. Test: uv run pytest <file> -v
# 6. Mark as done in your notes
# 7. Repeat
```

### Step 7: Verify Content Cleanup Complete

#### 7.1: Check for any remaining files with markers

```bash
# Use the finder script
/tmp/find-all-files-needing-cleanup.sh
```

**Expected output**: Empty (no files listed)

**If files are listed**: Return to Step 6 and clean those files.

#### 7.2: Detailed marker verification

```bash
# Check for remaining markers (should all be 0)
echo "=== Checking for remaining development markers ==="

echo "WP- references:"
grep -r "WP-[0-9]" tests/integration --include="*.py" | wc -l

echo "Phase references:"
grep -r "Phase [0-9]" tests/integration --include="*.py" | wc -l

echo "TDD markers:"
grep -r "\[RED\]\|\[GREEN\]\|\[REFACTOR\]" tests/integration --include="*.py" | wc -l

echo "Regression language:"
grep -ri "regression test\|verifies the fix\|before the fix\|after the fix" tests/integration --include="*.py" | wc -l

echo "Class names with Fix:"
grep -r "class Test.*Fix" tests/integration --include="*.py" | wc -l

echo "Function names with fix/regression:"
grep -r "def test_.*_fix\|def test_.*_regression" tests/integration --include="*.py" | wc -l

echo "TODO comments:"
grep -r "TODO" tests/integration --include="*.py" | wc -l
```

**Expected output**: All counts should be 0

**If any count > 0**:
1. Find the specific files: `grep -r "<pattern>" tests/integration --include="*.py"`
2. Analyze each file: `/tmp/analyze-file-markers.sh <file>`
3. Clean and re-verify

### Step 9: Full Test Suite Verification

```bash
# Run complete integration test suite
uv run pytest tests/integration/ -v --tb=short

# Optional: Check coverage maintained
uv run pytest tests/integration/ --cov=fraiseql --cov-report=term-missing
```

**Expected output**: All tests pass, coverage maintained

## Verification Commands

```bash
# Verify no development markers remain
/tmp/find-markers.sh | wc -l  # Should be 0

# Verify all tests pass
uv run pytest tests/integration/ -v

# Count of clean files (should be ~70+ files)
find tests/integration -name "test_*.py" | wc -l
```

## Acceptance Criteria

- [ ] No files contain WP-XXX references
- [ ] No files contain Phase markers
- [ ] No files contain TDD cycle markers
- [ ] No class names contain "Fix" or "Regression"
- [ ] No function names contain "_fix" or "_regression"
- [ ] All module docstrings describe WHAT not WHEN
- [ ] All test docstrings focus on expected behavior
- [ ] Full integration test suite passes
- [ ] Test coverage maintained

## Commit

After verification passes:

```bash
git add tests/integration/
git commit -m "refactor(tests): remove development markers from integration test content [REFACTOR]

Clean all test docstrings, comments, and names to be evergreen:
- Remove WP-XXX, Phase, and TDD markers
- Remove regression/fix language
- Remove version numbers and dates
- Rename classes: TestXxxFix → TestXxx
- Rename functions: test_xxx_fix → test_xxx
- Rewrite docstrings to focus on expected behavior

Tests remain functionally identical. All tests passing."
```

## DO NOT

- ❌ Change test logic or assertions (only change names/docs)
- ❌ Remove useful comments that explain complex test setups
- ❌ Batch commit all changes (commit after each major file or group)
- ❌ Skip running tests after editing a file

## Troubleshooting

**Problem**: Hard to decide how to rewrite a docstring
**Solution**: Ask yourself: "What does this test prove about the system?" Not "What bug did this fix?"

**Problem**: Test name is unclear after removing "_fix"
**Solution**: Use a more descriptive name. Example: `test_mutation_parameters` instead of `test_mutation_fix`

**Problem**: Cleaning takes too long
**Solution**:
1. Focus on high-impact files first (those with most markers)
2. Batch process files with only 1-2 markers
3. Take breaks - this phase is tedious but important

**Problem**: Removed a comment and now test is confusing
**Solution**: Add back a comment, but make it evergreen:
- Bad: "# This fixes the bug where X"
- Good: "# X requires Y because Z"

## Notes for Junior Engineers

**Why is this phase important?**
Professional codebases don't reveal their development history in test files. Tests should document expected behavior, not past bugs.

**How to use the analysis script effectively?**
The `/tmp/analyze-file-markers.sh` script is your friend:
1. Run it BEFORE editing to see what needs changing
2. Keep the output visible while editing (split terminal or print it)
3. Run it AFTER editing to verify you got everything
4. The line numbers help you navigate directly to problem areas

**Workflow for each file**:
```bash
# 1. Analyze (see what needs fixing)
/tmp/analyze-file-markers.sh tests/integration/graphql/test_example.py

# 2. Edit (fix the identified issues)
$EDITOR tests/integration/graphql/test_example.py

# 3. Verify (confirm it's clean)
/tmp/analyze-file-markers.sh tests/integration/graphql/test_example.py
# Should show: "✓ File is clean!"

# 4. Test (make sure nothing broke)
uv run pytest tests/integration/graphql/test_example.py -v
```

**How much detail in docstrings?**
- Module docstring: 2-3 sentences about what feature area is tested
- Class docstring: 1-2 sentences about the specific aspect
- Function docstring: 1 sentence about what this test proves

**What if a test name doesn't make sense without "_fix"?**
The test name probably wasn't descriptive enough. Choose a name that describes the feature being tested:
- `test_enum_conversion_fix` → `test_enum_values_serialize_to_graphql`
- `test_auth_regression` → `test_unauthorized_users_rejected`

**Should I remove ALL comments?**
No! Keep comments that explain:
- Complex test setups
- Why certain data is used
- What a non-obvious assertion validates

Remove comments that explain:
- What bug this fixed
- What version it was added in
- References to work packages or phases

**Can I batch multiple files?**
Yes! Use the systematic cleanup workflow in Step 6.2, which:
- Shows you files in priority order (most markers first)
- Analyzes each file automatically
- Prompts you to edit
- Re-verifies after your edit
- Runs tests
- Moves to the next file

**Time estimate**: ~3 hours
- Setting up scripts: ~10 minutes
- High-impact files: ~1 hour
- Systematic cleanup (remaining files): ~1.5 hours
- Verification: ~20 minutes

This is the longest phase but the new tooling makes it much more efficient than manual grep commands.
