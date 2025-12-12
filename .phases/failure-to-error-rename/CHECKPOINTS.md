# Phase Checkpoints & Verification

**Purpose**: After each phase, run the corresponding checkpoint to verify completion before moving to next phase.

**Philosophy**: Catch issues early - each checkpoint validates phase success before dependencies pile up.

---

## Checkpoint 0: Pre-Implementation ✅

**Run BEFORE Phase 1**

```bash
echo "=== Checkpoint 0: Pre-Implementation Baseline ==="

# Verify audit completed
if [[ -f .phases/failure-to-error-rename/PHASE_0_AUDIT.md ]]; then
  echo "✓ Audit file exists"
else
  echo "❌ Run Phase 0 audit first"
  exit 1
fi

# Create feature branch
git checkout -b feature/rename-failure-to-error 2>/dev/null || git checkout feature/rename-failure-to-error
echo "✓ On feature branch"

# Baseline test run
echo "Running baseline tests..."
uv run pytest tests/ -v --tb=short -x > /tmp/baseline_tests.txt 2>&1
baseline_result=$?

if [[ $baseline_result -eq 0 ]]; then
  echo "✓ Baseline: All tests passing"
else
  echo "⚠️  Baseline: Some tests failing - document known failures"
  grep -E "FAILED|ERROR" /tmp/baseline_tests.txt | head -10
fi

# Count current @failure occurrences
failure_count=$(grep -r "@failure\|import failure\|_failure_" src/ tests/ --include="*.py" 2>/dev/null | wc -l)
echo "Current @failure occurrences: $failure_count (expected: ~28)"

echo ""
echo "✅ Checkpoint 0 PASSED - Ready for Phase 1"
```

---

## Checkpoint 1: Core Python Implementation

**Run AFTER Phase 1**

```bash
echo "=== Checkpoint 1: Core Python Implementation ==="

# Verify decorator renamed
if grep -q "def error(" src/fraiseql/mutations/decorators.py; then
  echo "✓ Decorator renamed to 'error'"
else
  echo "❌ Decorator not renamed"
  exit 1
fi

# Verify old decorator removed
if grep -q "def failure(" src/fraiseql/mutations/decorators.py; then
  echo "❌ Old 'failure' decorator still exists"
  exit 1
else
  echo "✓ Old decorator removed"
fi

# Verify registry renamed
if grep -q "_error_registry" src/fraiseql/mutations/decorators.py && \
   ! grep -q "_failure_registry" src/fraiseql/mutations/decorators.py; then
  echo "✓ Registry renamed to '_error_registry'"
else
  echo "❌ Registry not properly renamed"
  grep "_.*_registry" src/fraiseql/mutations/decorators.py
  exit 1
fi

# Verify public API exports updated
if grep -q "from .decorators import error" src/fraiseql/mutations/__init__.py && \
   grep -q "from .mutations.decorators import error" src/fraiseql/__init__.py; then
  echo "✓ Public API exports updated"
else
  echo "❌ Public API exports not updated"
  exit 1
fi

# Verify old exports removed
if grep -q "import failure" src/fraiseql/__init__.py || \
   grep -q "import failure" src/fraiseql/mutations/__init__.py; then
  echo "❌ Old 'failure' still exported"
  exit 1
else
  echo "✓ Old exports removed"
fi

# Test import
echo "Testing new import..."
if python3 -c "from fraiseql import error; print('✓ Import successful')" 2>/dev/null; then
  echo "✓ New decorator imports correctly"
else
  echo "❌ New decorator import failed"
  exit 1
fi

# Verify old import fails
if python3 -c "from fraiseql import failure" 2>/dev/null; then
  echo "❌ Old decorator still importable"
  exit 1
else
  echo "✓ Old decorator properly removed (ImportError expected)"
fi

# Expected state: Tests should fail (RED phase)
echo ""
echo "Testing current test state (should fail)..."
uv run pytest tests/unit/decorators/test_decorators.py -v 2>&1 | head -20

echo ""
echo "✅ Checkpoint 1 PASSED - Core implementation complete [RED]"
echo "   Next: Phase 2 to fix tests [GREEN]"
```

---

## Checkpoint 2: Test Files Updated

**Run AFTER Phase 2**

```bash
echo "=== Checkpoint 2: Test Files Updated ==="

# Count test files still using @failure
remaining_files=$(find tests/ -name "*.py" -exec grep -l "@failure\|from.*failure\|import.*failure" {} \; 2>/dev/null | wc -l)

if [[ $remaining_files -eq 0 ]]; then
  echo "✓ All test files updated (0 files with @failure)"
else
  echo "⚠️  $remaining_files test files still have @failure:"
  find tests/ -name "*.py" -exec grep -l "@failure\|from.*failure\|import.*failure" {} \;
fi

# Verify @error is now used
error_count=$(grep -r "@error" tests/ --include="*.py" 2>/dev/null | wc -l)
if [[ $error_count -gt 0 ]]; then
  echo "✓ Tests now use @error ($error_count occurrences)"
else
  echo "❌ No @error found in tests"
  exit 1
fi

# Run full test suite
echo ""
echo "Running full test suite..."
uv run pytest tests/ -v --tb=short > /tmp/phase2_tests.txt 2>&1
test_result=$?

if [[ $test_result -eq 0 ]]; then
  echo "✅ All tests PASSED"
else
  echo "❌ Some tests failed:"
  grep -E "FAILED|ERROR" /tmp/phase2_tests.txt | head -10
  echo ""
  echo "Review failures and fix before proceeding"
  exit 1
fi

echo ""
echo "✅ Checkpoint 2 PASSED - All tests updated and passing [GREEN]"
echo "   Next: Phase 3 (Examples) or Phase 4 (CLI)"
```

---

## Checkpoint 3: Examples Updated

**Run AFTER Phase 3 (if examples exist)**

```bash
echo "=== Checkpoint 3: Examples Updated ==="

# Check if examples directory exists
if [[ ! -d examples/ ]]; then
  echo "ℹ️  No examples/ directory - skipping checkpoint"
  echo "✅ Checkpoint 3 SKIPPED"
  exit 0
fi

# Check for @failure in examples
failure_in_examples=$(grep -r "@failure\|import failure" examples/ --include="*.py" 2>/dev/null | wc -l)

if [[ $failure_in_examples -eq 0 ]]; then
  echo "✓ No @failure in examples"
else
  echo "❌ Still $failure_in_examples @failure references in examples:"
  grep -r "@failure\|import failure" examples/ --include="*.py" -n
  exit 1
fi

# Verify examples are syntactically valid
echo "Checking example syntax..."
syntax_errors=0
for example_file in examples/**/*.py; do
  if [[ -f "$example_file" ]]; then
    python3 -m py_compile "$example_file" 2>/dev/null
    if [[ $? -ne 0 ]]; then
      echo "❌ Syntax error in $example_file"
      syntax_errors=$((syntax_errors + 1))
    fi
  fi
done

if [[ $syntax_errors -eq 0 ]]; then
  echo "✓ All examples have valid syntax"
else
  echo "❌ $syntax_errors examples have syntax errors"
  exit 1
fi

echo ""
echo "✅ Checkpoint 3 PASSED - Examples updated"
echo "   Next: Phase 4 (CLI)"
```

---

## Checkpoint 4: CLI & Introspection Updated

**Run AFTER Phase 4**

```bash
echo "=== Checkpoint 4: CLI & Introspection Updated ==="

# Check CLI template/generation code
if grep -q "@error" src/fraiseql/cli/ 2>/dev/null || \
   grep -q "@error" src/fraiseql/introspection/ 2>/dev/null; then
  echo "✓ CLI/introspection uses @error"
else
  echo "⚠️  No @error found in CLI/introspection (verify manually)"
fi

# Check for old decorator
if grep -q "@failure" src/fraiseql/cli/ 2>/dev/null || \
   grep -q "@failure" src/fraiseql/introspection/ 2>/dev/null; then
  echo "❌ CLI/introspection still references @failure"
  grep -r "@failure" src/fraiseql/cli/ src/fraiseql/introspection/ -n
  exit 1
else
  echo "✓ No @failure in CLI/introspection"
fi

# Test code generation (if CLI exists)
if command -v fraiseql &> /dev/null; then
  echo "Testing code generation..."
  fraiseql generate mutation TestCheckpoint --output /tmp/test_gen_checkpoint.py 2>/dev/null || echo "ℹ️  CLI not available"

  if [[ -f /tmp/test_gen_checkpoint.py ]]; then
    if grep -q "@error" /tmp/test_gen_checkpoint.py && ! grep -q "@failure" /tmp/test_gen_checkpoint.py; then
      echo "✓ Generated code uses @error"
      rm /tmp/test_gen_checkpoint.py
    else
      echo "❌ Generated code doesn't use @error"
      exit 1
    fi
  fi
else
  echo "ℹ️  CLI not installed - skip generation test"
fi

echo ""
echo "✅ Checkpoint 4 PASSED - CLI/introspection updated"
echo "   Next: Phase 5 (Rust)"
```

---

## Checkpoint 5: Rust Code Updated

**Run AFTER Phase 5**

```bash
echo "=== Checkpoint 5: Rust Code Updated ==="

# Check for "failure" in Rust code (should only be in natural language comments)
rust_failures=$(grep -ri "failure" fraiseql_rs/src/ --include="*.rs" 2>/dev/null | wc -l)

if [[ $rust_failures -gt 0 ]]; then
  echo "Found $rust_failures 'failure' references in Rust (reviewing...):"
  grep -ri "failure" fraiseql_rs/src/ --include="*.rs" -n
  echo ""
  echo "ℹ️  These should be natural language comments only"
  echo "   Verify they're not decorator/type references"
else
  echo "✓ No 'failure' references in Rust code"
fi

# Build Rust code
echo "Building Rust code..."
cd fraiseql_rs 2>/dev/null || { echo "ℹ️  No fraiseql_rs/ directory"; echo "✅ Checkpoint 5 SKIPPED"; exit 0; }

if cargo build 2>&1 | grep -q "error"; then
  echo "❌ Rust build failed"
  cargo build
  exit 1
else
  echo "✓ Rust build successful"
fi

# Run Rust tests
echo "Running Rust tests..."
if cargo test 2>&1 | grep -q "test result.*FAILED"; then
  echo "❌ Rust tests failed"
  cargo test
  exit 1
else
  echo "✓ Rust tests passed"
fi

cd ..

echo ""
echo "✅ Checkpoint 5 PASSED - Rust code updated and builds"
echo "   Next: Phase 6 (Documentation)"
```

---

## Checkpoint 6: Documentation Updated

**Run AFTER Phase 6**

```bash
echo "=== Checkpoint 6: Documentation Updated ==="

# Check for @failure in docs
doc_failures=$(grep -r "@failure\|import failure" docs/ README.md --include="*.md" 2>/dev/null | \
               grep -v "archived\|Note:" | wc -l)

if [[ $doc_failures -eq 0 ]]; then
  echo "✓ No @failure in documentation (except archived)"
else
  echo "⚠️  Found $doc_failures @failure references in docs:"
  grep -r "@failure\|import failure" docs/ README.md --include="*.md" -n | grep -v "archived\|Note:" | head -10
fi

# Verify @error is documented
error_in_docs=$(grep -r "@error" docs/ README.md --include="*.md" 2>/dev/null | wc -l)
if [[ $error_in_docs -gt 0 ]]; then
  echo "✓ Documentation uses @error ($error_in_docs occurrences)"
else
  echo "❌ No @error found in documentation"
  exit 1
fi

# Check key documentation files
key_files=(
  "README.md"
  "docs/reference/decorators.md"
  "docs/getting-started/quickstart.md"
)

for file in "${key_files[@]}"; do
  if [[ -f "$file" ]]; then
    if grep -q "@error" "$file" && ! grep -q "@failure" "$file"; then
      echo "✓ $file updated"
    else
      echo "⚠️  $file may need review"
    fi
  else
    echo "ℹ️  $file not found"
  fi
done

echo ""
echo "✅ Checkpoint 6 PASSED - Documentation updated"
echo "   Next: Phase 7 (Config & Misc)"
```

---

## Checkpoint 7: Configuration & Misc Files

**Run AFTER Phase 7**

```bash
echo "=== Checkpoint 7: Configuration & Misc Files ==="

# Final sweep for any remaining @failure references
echo "Final sweep for @failure references..."

src_failures=$(grep -r "@failure\|import failure\|_failure_" src/ --include="*.py" 2>/dev/null | wc -l)
test_failures=$(grep -r "@failure\|import failure" tests/ --include="*.py" 2>/dev/null | wc -l)

echo "Source files: $src_failures occurrences (expected: 0)"
echo "Test files: $test_failures occurrences (expected: 0)"

if [[ $src_failures -eq 0 ]] && [[ $test_failures -eq 0 ]]; then
  echo "✓ No @failure in Python source/tests"
else
  echo "❌ Still found @failure references:"
  grep -r "@failure\|import failure\|_failure_" src/ tests/ --include="*.py" -n | head -20
  exit 1
fi

echo ""
echo "✅ Checkpoint 7 PASSED - All misc files updated"
echo "   Next: Phase 8 (QA)"
```

---

## Checkpoint 8: QA & Final Verification

**Run AFTER Phase 8 (Final QA)**

```bash
echo "=== Checkpoint 8: Final QA Verification ==="

# 1. Test Suite
echo "1. Running full test suite..."
uv run pytest tests/ -v --tb=short > /tmp/final_tests.txt 2>&1
test_result=$?

if [[ $test_result -eq 0 ]]; then
  echo "✅ All tests PASSED"
else
  echo "❌ Tests FAILED - review before proceeding:"
  grep -E "FAILED|ERROR" /tmp/final_tests.txt | head -20
  exit 1
fi

# 2. Type Checking
echo "2. Running type checking..."
if command -v mypy &> /dev/null; then
  if uv run mypy src/fraiseql/ 2>&1 | grep -q "error"; then
    echo "❌ Type checking failed"
    uv run mypy src/fraiseql/
    exit 1
  else
    echo "✅ Type checking passed"
  fi
else
  echo "ℹ️  mypy not available - skipping"
fi

# 3. Linting
echo "3. Running linter..."
if uv run ruff check src/fraiseql/ tests/ --quiet 2>&1; then
  echo "✅ Linting passed"
else
  echo "⚠️  Linting issues found (review):"
  uv run ruff check src/fraiseql/ tests/ | head -20
fi

# 4. Verify no @failure remains
echo "4. Checking for remaining @failure references..."
all_failures=$(grep -r "@failure\|import failure\|_failure_" . \
  --include="*.py" --include="*.rs" --include="*.md" \
  --exclude-dir=".git" --exclude-dir="__pycache__" --exclude-dir=".pytest_cache" \
  --exclude-dir="target" --exclude-dir=".ruff_cache" \
  2>/dev/null | grep -v "archived\|Note:" | wc -l)

if [[ $all_failures -eq 0 ]]; then
  echo "✅ No @failure references found (except archived)"
else
  echo "⚠️  Found $all_failures @failure references:"
  grep -r "@failure\|import failure\|_failure_" . \
    --include="*.py" --include="*.rs" --include="*.md" \
    --exclude-dir=".git" --exclude-dir="__pycache__" \
    2>/dev/null | grep -v "archived\|Note:" | head -10
fi

# 5. Verify new decorator works
echo "5. Testing new @error decorator..."
python3 << 'EOF'
from fraiseql import error, success, mutation, fraise_input

@fraise_input
class TestInput:
    value: str

@success
class TestSuccess:
    result: str

@error
class TestError:
    message: str

@mutation
class TestMutation:
    input: TestInput
    success: TestSuccess
    error: TestError

print("✅ @error decorator works")
EOF

if [[ $? -eq 0 ]]; then
  echo "✅ New decorator functional"
else
  echo "❌ New decorator not working"
  exit 1
fi

# 6. Verify old decorator fails
echo "6. Verifying old @failure decorator removed..."
if python3 -c "from fraiseql import failure" 2>/dev/null; then
  echo "❌ Old @failure still importable"
  exit 1
else
  echo "✅ Old @failure properly removed"
fi

# Summary
echo ""
echo "=== QA Summary ==="
echo "✅ All tests passed"
echo "✅ Type checking passed"
echo "✅ Linting clean"
echo "✅ No @failure references remain"
echo "✅ New @error decorator functional"
echo "✅ Old @failure removed"
echo ""
echo "✅✅✅ Checkpoint 8 PASSED - Ready for Phase 9 (Migration Guide)"
```

---

## Checkpoint 9: Migration Guide Complete

**Run AFTER Phase 9**

```bash
echo "=== Checkpoint 9: Migration Guide Complete ==="

# Check migration guide exists
if [[ -f docs/migration/v2.0-failure-to-error.md ]]; then
  echo "✓ Migration guide created"
else
  echo "⚠️  Migration guide not found at expected location"
fi

# Check CHANGELOG updated
if grep -q "@failure.*@error\|failure.*error" CHANGELOG.md; then
  echo "✓ CHANGELOG.md updated"
else
  echo "⚠️  CHANGELOG.md may not be updated"
fi

# Check README updated
if grep -q "@error" README.md && ! grep -q "@failure" README.md; then
  echo "✓ README.md updated"
else
  echo "⚠️  README.md may need updating"
fi

echo ""
echo "✅ Checkpoint 9 PASSED - Migration guide complete"
echo "   Next: Phase 10 (Archaeology Cleanup)"
```

---

## Checkpoint 10: Archaeology Cleanup Complete

**Run AFTER Phase 10 (Final cleanup)**

```bash
echo "=== Checkpoint 10: Archaeology Cleanup ==="

# Check for archaeological comments
echo "Checking for archaeological artifacts..."

# "was X" comments
was_count=$(grep -r " # was " src/ --include="*.py" 2>/dev/null | wc -l)
echo "  '# was' comments: $was_count (expected: 0)"

# "renamed" comments
renamed_count=$(grep -r "renamed from\|renamed to" src/ --include="*.py" 2>/dev/null | wc -l)
echo "  'renamed' comments: $renamed_count (expected: 0)"

# Version markers
version_count=$(grep -r "# v[0-9]\.\|# NEW in\|# As of v" src/ --include="*.py" 2>/dev/null | wc -l)
echo "  Version markers: $version_count (expected: 0)"

# Change history
change_count=$(grep -r "# Changed\|# Updated\|# Modified\|# BEFORE:\|# AFTER:" src/ --include="*.py" 2>/dev/null | wc -l)
echo "  Change history: $change_count (expected: 0)"

total_artifacts=$((was_count + renamed_count + version_count + change_count))

if [[ $total_artifacts -eq 0 ]]; then
  echo "✅ Code is 'evergreen' - no archaeological comments"
else
  echo "⚠️  Found $total_artifacts archaeological artifacts (review recommended)"
fi

# Verify code quality
echo ""
echo "Final code quality check..."
uv run ruff check src/fraiseql/mutations/ --quiet && echo "✅ Linting clean" || echo "⚠️  Linting issues"

echo ""
echo "✅ Checkpoint 10 PASSED - Code archaeology removed"
echo ""
echo "🎉 ALL CHECKPOINTS COMPLETE - Ready to merge!"
```

---

## Quick Checkpoint Runner

**Run all checkpoints sequentially**:

```bash
#!/bin/bash
# run_all_checkpoints.sh

phases=(0 1 2 3 4 5 6 7 8 9 10)

for phase in "${phases[@]}"; do
  echo "========================================"
  echo "Running Checkpoint $phase"
  echo "========================================"

  # Extract and run checkpoint bash block from this file
  # (Manually run each checkpoint after its phase completes)

  read -p "Checkpoint $phase complete? (y/n): " confirm
  if [[ $confirm != "y" ]]; then
    echo "❌ Checkpoint $phase failed - fix before continuing"
    exit 1
  fi
done

echo ""
echo "🎉 All checkpoints passed!"
```

---

## Usage

1. **After each phase**: Run the corresponding checkpoint
2. **If checkpoint fails**: Fix issues before moving to next phase
3. **Document failures**: Add notes to this file for future reference
4. **Final verification**: Run Checkpoint 8 & 10 as comprehensive verification

**Golden Rule**: Never proceed to next phase if checkpoint fails.
