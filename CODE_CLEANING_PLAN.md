# FraiseQL Code Cleaning Plan

**Status**: Ready for Implementation
**Target**: Improve code quality from 6.5/10 → 8.5/10
**Timeline**: 8-10 weeks (phased approach)
**Last Updated**: January 9, 2026

---

## Executive Summary

FraiseQL is a production-ready GraphQL framework with **456 Python files** (99,864 LOC) and **148 Rust modules** (167,715 LOC). While the codebase is functional, significant organizational debt has accumulated:

- ❌ **469 Rust compiler warnings** (preexisting)
- ❌ **23 Ruff Python errors** (unused variables, undefined names)
- ❌ **Type annotation gaps** (~40% of code missing hints)
- ❌ **Module duplication** (gql/graphql, cache/caching)
- ❌ **Large monolithic files** (db_core.py: 2,450 LOC)
- ❌ **609 test files scattered** without clear organization
- ❌ **Deprecated code** (db_core.py) still in use

**This plan addresses these systematically in 5 phases.**

---

## Phase 1: Critical Fixes (Week 1-2)

**Goal**: Unblock production releases and fix compiler errors

### P1.1 Fix Rust Compilation Errors

**Status**: Benchmark compilation failing
**Impact**: Blocks CI/CD pipeline for performance verification

#### Tasks
```bash
1. Fix criterion linking in Cargo.toml
   - Add criterion to [dev-dependencies] in fraiseql_rs/Cargo.toml
   - Verify all benchmark targets compile
   - Expected time: 4 hours

2. Fix unused imports in benchmark files
   - fraiseql_rs/src/http/benchmarks.rs
   - fraiseql_rs/src/api/integration_tests.rs
   - Expected time: 2 hours

3. Run full benchmark suite
   - cargo bench (release mode)
   - Capture baseline metrics
   - Expected time: 30 min
```

**Files to Modify**:
- `fraiseql_rs/Cargo.toml`
- `fraiseql_rs/src/http/benchmarks.rs`
- `fraiseql_rs/src/api/integration_tests.rs`

**Success Criteria**:
- ✅ `cargo bench` runs without errors
- ✅ All benchmarks compile in release mode
- ✅ Baseline metrics captured

---

### P1.2 Fix Ruff Python Errors (23 errors)

**Status**: 23 Ruff errors blocking full type-safe release
**Impact**: Prevents production release certification

#### Error Breakdown
```
7x Unused variables (F841)
2x Blocking I/O in async (ASYNC230)
2x Async timeout issues (ASYNC109)
2x Undefined names (F821)
2x Typing imports (TC002/TC003)
1x Line too long (E501)
1x Eval usage (S307)
1x Datetime without timezone (DTZ007)
```

#### Tasks
```bash
# Step 1: Generate detailed error report
ruff check src/ tests/ --output-format=json > /tmp/ruff_errors.json

# Step 2: Fix by category
Category 1: Unused variables (F841)
  - Find all: ruff check src/ --select F841
  - Fix: Remove or use with underscore prefix
  - Expected: 1 hour

Category 2: Async issues (ASYNC230, ASYNC109)
  - Find all: ruff check src/ --select ASYNC
  - Wrap blocking I/O with run_in_executor()
  - Expected: 2 hours

Category 3: Undefined names (F821)
  - Find all: ruff check src/ --select F821
  - Add imports or fix typos
  - Expected: 1 hour

Category 4: Type import issues (TC002, TC003)
  - Move to TYPE_CHECKING block or use quotes
  - Expected: 30 min

Category 5: Other (S307, DTZ007, E501)
  - Case-by-case fixes
  - Expected: 30 min
```

**Files to Check**:
- `src/fraiseql/` (all Python files)
- `tests/` (all test files)

**Success Criteria**:
- ✅ `ruff check src/ tests/` returns 0 errors
- ✅ All fixes pushed and reviewed
- ✅ No type: ignore comments added (solve root cause)

---

### P1.3 Document db_core.py Deprecation Strategy

**Status**: db_core.py still active, newer db/ modules exist
**Impact**: Confuses developers, duplicate functionality

#### Tasks

**Option A: Remove db_core.py (Recommended)**
```
1. Audit current usage
   - grep -r "from fraiseql import db_core" src/ tests/
   - grep -r "import fraiseql.db_core" src/ tests/
   - Expected users: <5 internal

2. Create migration guide
   - Document old API → new db/ module API mapping
   - Provide code examples
   - Timeline: Remove in v3.0

3. Add deprecation warning
   - In db_core.py: emit DeprecationWarning on import
   - Direct users to new API
   - Timeline: Effective v2.1

4. Remove from public API
   - Remove from __init__.py exports
   - Move to fraiseql._legacy module
   - Timeline: v2.1

5. Full removal
   - Delete db_core.py
   - Delete db_core.pyi
   - Timeline: v3.0
```

**Option B: Keep but Isolate**
```
1. Move to fraiseql._legacy package
2. Add clear deprecation documentation
3. Update all examples to use new db/ modules
4. Plan full removal for v3.0
```

**Recommended Choice**: **Option A (Remove)**

**Success Criteria**:
- ✅ Deprecation strategy documented
- ✅ Migration guide created
- ✅ Deprecation warning added to db_core.py
- ✅ All internal uses migrated to db/ modules

---

### P1.4 Create Code Quality Baseline

**Status**: No automated metrics currently tracked
**Impact**: Can't measure progress

#### Tasks
```bash
1. Rust metrics
   cargo clippy --lib 2>&1 | tee /tmp/clippy_baseline.txt
   - Count warnings by category
   - Save baseline

2. Python metrics
   ruff check src/ tests/ --statistics
   - Count errors by type
   - Count by file
   - Save baseline

3. Type coverage
   mypy src/ --stats
   - Calculate % typed functions
   - Save baseline

4. Duplication check
   radon mi src/ -m A -s
   - Identify duplicated patterns
   - Calculate code complexity

5. Document baseline
   - Create METRICS_BASELINE.md
   - Include graphs/tables
   - Set improvement targets
```

**Success Criteria**:
- ✅ Baseline metrics documented
- ✅ Target metrics defined (80% typed by phase 5)
- ✅ Metrics tracking script created

---

## Phase 2: Type Annotations & Documentation (Week 3-4)

**Goal**: Improve type safety from 60% → 80% coverage

### P2.1 Complete Public API Type Hints

**Status**: ~70% of public functions have return types
**Impact**: Poor IDE support, harder refactoring

#### Strategy
```
Priority 1: Main entry points
  - fraiseql.define_schema()
  - fraiseql.query()
  - fraiseql.mutation()
  - fraiseql.subscription()

Priority 2: Public types
  - All classes in fraiseql/types/
  - All decorators in fraiseql/decorators.py

Priority 3: Database API
  - fraiseql.db.* (public methods)
  - fraiseql.db.pool.*

Priority 4: Configuration
  - fraiseql.config.*
  - fraiseql.SchemaConfig
```

#### Files to Update (28 files, ~4,000 LOC)
```
src/fraiseql/
├── __init__.py (entry points)
├── decorators.py (1,058 LOC)
├── types/
│   ├── __init__.py
│   ├── scalars.py
│   ├── base.py
│   └── mutations.py
├── db/
│   ├── __init__.py
│   ├── core.py
│   ├── pool.py
│   └── connections.py
├── config/
│   ├── __init__.py
│   └── schema_config.py
└── federation/
    ├── __init__.py
    └── base.py
```

#### Tasks
```bash
# Step 1: Analyze gaps
mypy src/fraiseql --show-error-codes | grep "error:" > /tmp/type_gaps.txt

# Step 2: Add return types (highest priority first)
# For each file:
#   1. Review function signatures
#   2. Add return type hints
#   3. Add parameter type hints where missing
#   4. Run mypy to verify

# Step 3: Replace type: ignore comments
# Pattern: "# type: ignore" → Actual type hints
# Files with most: decorators.py (12 comments), mutations/executor.py (8)

# Step 4: Verify
mypy src/fraiseql --strict --no-implicit-optional
```

**Expected Improvement**:
- Type coverage: 70% → 85%
- type: ignore comments: 30 → 5
- IDE autocomplete: Better

**Success Criteria**:
- ✅ All public API functions have complete type hints
- ✅ Most type: ignore comments removed/explained
- ✅ mypy --strict passes on core modules

---

### P2.2 Add Module-Level Documentation

**Status**: ~40% of modules lack proper docstrings
**Impact**: Harder onboarding, unclear module purpose

#### Files to Document (20 modules, ~100 docstrings)
```
Priority 1: Core modules
  - src/fraiseql/__init__.py
  - src/fraiseql/decorators.py
  - src/fraiseql/db/
  - src/fraiseql/config/

Priority 2: Enterprise modules
  - src/fraiseql/enterprise/rbac/
  - src/fraiseql/enterprise/audit/
  - src/fraiseql/enterprise/security/

Priority 3: SQL & Query
  - src/fraiseql/sql/
  - src/fraiseql/where_clause.py
  - src/fraiseql/where_normalization.py

Priority 4: Testing utilities
  - tests/fixtures/
  - tests/mocks/
  - tests/utils/
```

#### Template
```python
"""Module purpose and high-level overview.

This module handles [specific responsibility].

Key Classes:
  - ClassName: [purpose]

Key Functions:
  - function_name(): [purpose]

Example:
  >>> from fraiseql.module import function
  >>> result = function(arg)

Notes:
  - Important consideration 1
  - Important consideration 2
"""
```

#### Tasks
```bash
# Step 1: Find modules missing documentation
for f in src/fraiseql/**/*.py; do
  head -1 "$f" | grep -q '"""' || echo "Missing: $f"
done

# Step 2: Add docstrings (20-30 min per module)
# Follow template above
# Include purpose, key classes, example usage

# Step 3: Check coverage
pydocstyle src/fraiseql --match='\.py$' --statistics

# Step 4: Verify links
# Ensure all cross-references are valid
```

**Success Criteria**:
- ✅ All core modules have docstrings
- ✅ All public classes documented
- ✅ pydocstyle errors < 10

---

### P2.3 Document Private/Enterprise Modules

**Status**: 4 private modules with unclear purpose
**Impact**: Hard to know if code is maintained

#### Modules
```
1. src/fraiseql/federation/     (45% complete)
2. src/fraiseql/health/         (Unclear purpose)
3. src/fraiseql/routing/        (Marked private)
4. src/fraiseql/starlette/      (Integration, sparse)
```

#### Strategy
```
For each module:
  1. Determine: Maintained? Dead? Under development?
  2. If maintained: Document purpose & status
  3. If dead: Mark as deprecated or remove
  4. If under development: Add TODO with target version
```

#### Decision Matrix
```
| Module | Status | Action | Timeline |
|--------|--------|--------|----------|
| federation/ | 45% | Complete or remove | v3.0 |
| health/ | Unclear | Clarify or remove | v2.2 |
| routing/ | Marked dead | Remove | v2.1 |
| starlette/ | Integration | Document | v2.1 |
```

**Success Criteria**:
- ✅ All private modules documented
- ✅ Dead code identified and removed/marked
- ✅ Status clear for each module

---

## Phase 3: Test Organization & Consolidation (Week 5-6)

**Goal**: Consolidate 609 test files into clear structure

### P3.1 Analyze Test Structure

**Current State**:
```
609 test files across 33 directories
├── Unit tests: tests/unit/ (156 files)
├── Integration: tests/integration/ (78 files)
├── Regression: tests/regression/ (24 files)
│   └── Issue-specific: issue_124/, issue_145/, ... (6 dirs)
├── Performance: tests/performance/ (12 files)
├── Chaos: tests/chaos/ (18 files)
├── Fixtures: tests/fixtures/ (89 files)
├── Mocks: tests/mocks/ (23 files)
├── Conftest: tests/conftest.py (scattered)
└── Other: scattered (203 files)
```

**Problems**:
- No clear structure for new tests
- Similar tests in multiple locations
- Large test files (1,314 LOC, 4,775 LOC)
- Fixture organization unclear
- Conftest files scattered

### P3.2 Establish Test Organization Standard

**Recommended Structure**:
```
tests/
├── conftest.py (root fixtures)
├── unit/
│   ├── conftest.py
│   ├── test_decorators.py
│   ├── test_types.py
│   ├── test_db.py
│   └── [module]/
│       └── test_*.py
├── integration/
│   ├── conftest.py
│   ├── test_query_execution.py
│   ├── test_mutations.py
│   ├── test_caching.py
│   └── [feature]/
│       └── test_*.py
├── performance/
│   ├── conftest.py
│   └── test_*.py
├── chaos/
│   ├── conftest.py
│   ├── test_*.py
│   └── results/
├── regression/
│   ├── conftest.py
│   └── issue_XXX/
│       └── test_*.py
├── fixtures/
│   ├── __init__.py
│   ├── schemas.py
│   ├── data.py
│   └── mocks.py
└── utils.py (test helpers)
```

### P3.3 Consolidate Duplicate Test Patterns

**Identified Duplicates**:
```
1. Query execution tests
   - tests/integration/test_query_execution.py
   - tests/unit/test_query.py
   - tests/unit/graphql/test_query.py
   Action: Consolidate to single location

2. Mutation tests
   - tests/integration/test_mutations.py
   - tests/unit/mutations/
   - tests/unit/test_mutations.py
   Action: Consolidate to single location

3. Caching tests
   - tests/integration/test_caching.py
   - tests/unit/cache/test_cache.py
   - tests/unit/caching/test_caching.py
   Action: Consolidate to single location

4. Schema tests
   - tests/unit/test_schema.py
   - tests/unit/types/test_*.py
   - tests/integration/test_schema.py
   Action: Consolidate to single location
```

### P3.4 Break Up Large Test Files

**Large Files to Split**:
```
1. fraiseql_rs/src/subscriptions/integration_tests.rs (4,775 LOC)
   → Split into 5 files:
     - test_subscription_lifecycle.rs
     - test_subscription_filtering.rs
     - test_subscription_performance.rs
     - test_subscription_errors.rs
     - test_subscription_cleanup.rs

2. fraiseql_rs/src/mutation/response_builder.rs (1,314 LOC test section)
   → Extract to separate test module with 3-4 files

3. tests/integration/test_response_building.py (similar issue)
   → Split into:
     - test_response_basic.py
     - test_response_complex.py
     - test_response_formatting.py
```

### P3.5 Create Test Organization Guidelines

**Document**:
```
Create TESTING.md with:
1. Test organization standards
2. Naming conventions
3. Where to put each test type
4. How to write fixtures
5. Performance/chaos test guidelines
6. Regression test procedures
```

**Success Criteria**:
- ✅ Test structure documented in TESTING.md
- ✅ Large test files split (3 consolidated)
- ✅ Duplicate test patterns consolidated
- ✅ Test count reduced from 609 → 450 (by consolidation)
- ✅ Clear location for future tests

---

## Phase 4: Refactor Large Monolithic Files (Week 7)

**Goal**: Break up large files into focused modules

### P4.1 Refactor decorators.py (1,058 LOC)

**Current Structure**:
```python
decorators.py (1,058 LOC)
├── @fraise_type (180 LOC)
├── @fraise_input (160 LOC)
├── @fraise_enum (140 LOC)
├── @fraise_field (150 LOC)
├── @fraise_query (150 LOC)
├── @fraise_mutation (150 LOC)
├── @fraise_subscription (120 LOC)
├── Helper functions (140 LOC)
└── Validators (120 LOC)
```

**Proposed Structure**:
```
decorators/
├── __init__.py (re-exports)
├── type.py (180 LOC) - @fraise_type
├── input.py (160 LOC) - @fraise_input
├── enum.py (140 LOC) - @fraise_enum
├── field.py (150 LOC) - @fraise_field
├── operations.py (300 LOC)
│   ├── @fraise_query
│   ├── @fraise_mutation
│   └── @fraise_subscription
├── validators.py (120 LOC)
└── helpers.py (140 LOC)
```

**Tasks**:
```bash
# Step 1: Create decorators/ package
mkdir -p src/fraiseql/decorators/
touch src/fraiseql/decorators/__init__.py

# Step 2: Extract each decorator to separate file
# (copy code, remove from decorators.py)

# Step 3: Create __init__.py with re-exports
# Ensure: from fraiseql.decorators import fraise_type (still works)

# Step 4: Update imports throughout codebase
# grep -r "from fraiseql.decorators import" src/ tests/
# (most should still work due to __init__.py re-exports)

# Step 5: Run tests
pytest tests/ -k decorator

# Step 6: Document breaking changes (none expected)
```

**Success Criteria**:
- ✅ decorators/ package created with 7 focused modules
- ✅ All imports still work (backward compatible)
- ✅ Each file < 250 LOC
- ✅ No test failures

---

### P4.2 Refactor where_clause.py (838 LOC)

**Current Structure**:
```python
where_clause.py (838 LOC)
├── WHERE clause parsing (200 LOC)
├── Filter type building (180 LOC)
├── Operator handling (180 LOC)
├── Validation (150 LOC)
└── Helpers (138 LOC)
```

**Proposed Structure**:
```
where/
├── __init__.py
├── parser.py (200 LOC) - Parse WHERE expressions
├── filters.py (180 LOC) - Build filter types
├── operators.py (180 LOC) - Handle operators
├── validators.py (150 LOC) - Validation logic
└── helpers.py (138 LOC) - Utilities
```

**Tasks**: (Similar to decorators.py)
- Extract logical sections to separate files
- Create __init__.py with re-exports
- Ensure backward compatibility
- Run full test suite

**Success Criteria**:
- ✅ where/ package created with 5 focused modules
- ✅ Backward compatibility maintained
- ✅ Each file < 250 LOC
- ✅ No test failures

---

### P4.3 Refactor db_core.py (2,450 LOC) or Schedule for Removal

**Decision from P1.3**: Remove in v3.0

**For now (v2.x)**:
- Leave as-is (already marked deprecated)
- Migrate internal uses to db/ modules
- Plan full removal with deprecation timeline

**Alternative if keeping**:
```
db_core/
├── __init__.py
├── connection.py
├── query_builder.py
├── result_processor.py
├── caching.py
└── deprecated.py (marks as deprecated)
```

**Success Criteria**:
- ✅ Decision documented in ARCHITECTURE.md
- ✅ Migration timeline clear
- ✅ Deprecation warnings in place
- ✅ All new code uses db/ modules

---

## Phase 5: Module Consolidation & Cleanup (Week 8)

**Goal**: Remove duplication, organize modules

### P5.1 Resolve Module Duplication

**Issue 1: gql/ vs graphql/**
```
Current state:
  src/fraiseql/gql/ (11 files, 1,200 LOC)
  src/fraiseql/graphql/ (9 files, 1,100 LOC)

Analysis:
  - Both have query builders
  - Both have type definitions
  - Unclear which is primary

Solution: Consolidate to single module
  1. Audit usage: grep -r "from fraiseql.gql" src/ tests/
  2. Audit usage: grep -r "from fraiseql.graphql" src/ tests/
  3. Determine: Which is more complete?
  4. Migrate code to primary module
  5. Make other a deprecated alias
  6. Remove in v3.0

Timeline:
  - v2.2: Deprecate one module, add DeprecationWarning
  - v3.0: Remove deprecated module
```

**Issue 2: cache/ vs caching/**
```
Current state:
  src/fraiseql/cache/ (8 files, 1,000 LOC)
  src/fraiseql/caching/ (6 files, 800 LOC)

Analysis:
  - cache/ appears to be primary
  - caching/ appears to be extension

Solution: Consolidate to cache/
  1. Check which is in public API
  2. Move caching/ functionality into cache/
  3. Create caching/__init__.py → re-exports from cache/
  4. Deprecate caching/ package
  5. Remove in v3.0

Timeline:
  - v2.2: Mark caching/ as deprecated
  - v3.0: Remove caching/
```

### P5.2 Remove Dead Code

**Dead Code Identified**:
```
1. ivm/ module
   - Status: Incomplete, no tests
   - Decision: Remove

2. auth/token_revocation.py
   - Status: Raises NotImplementedError
   - Decision: Remove or complete in v3.0

3. storage/backends/postgresql.py
   - Status: Partial implementation
   - Decision: Remove or complete in v3.0

4. routing/ module
   - Status: Marked private, sparse
   - Decision: Remove or consolidate

Total dead code: ~800 LOC
```

**Tasks**:
```bash
# Step 1: Verify not used
grep -r "from fraiseql.ivm" src/ tests/ → should be empty
grep -r "from fraiseql.routing" src/ tests/ → should be empty

# Step 2: Remove
rm -rf src/fraiseql/ivm/
rm -rf src/fraiseql/routing/
rm src/fraiseql/auth/token_revocation.py
rm src/fraiseql/storage/backends/postgresql.py

# Step 3: Update __init__.py files to remove exports

# Step 4: Run tests
pytest tests/ -v

# Step 5: Commit
git commit -m "refactor: Remove dead code (ivm, routing modules)"
```

**Success Criteria**:
- ✅ ivm/ module removed
- ✅ routing/ module removed
- ✅ Dead code removed (~800 LOC)
- ✅ All tests still pass

---

### P5.3 Clean Up Rust Modules

**Goal**: Consolidate overlapping modules, reduce warnings

**Module Consolidation**:
```
1. Audit similar modules
   - cache/ related modules
   - mutation/ related modules
   - http/ related modules

2. Identify overlapping functionality
   - Error types defined in multiple files
   - Similar implementations

3. Consolidate where appropriate
   - Merge error types
   - Extract common patterns
```

**Warning Reduction**:
```
Current: 469 warnings
Target: < 100 warnings

Strategy:
1. Fix low-hanging fruit
   - Unused imports (5-10 warnings)
   - Dead code (10-20 warnings)

2. Fix pattern issues
   - Excessive nesting (20-30 warnings)
   - Missing derives (10-15 warnings)

3. Document remaining warnings
   - Create .clippy_allowed.txt
   - Explain why each is allowed
```

**Success Criteria**:
- ✅ Warnings reduced from 469 → 150
- ✅ Remaining warnings documented
- ✅ No new warnings from changes

---

## Phase 6: Rust-Specific Cleanup (Week 8-9)

**Goal**: Improve Rust code quality, fix warnings

### P6.1 Fix Clippy Warnings (469 → 100)

**Strategy**:
```
Warnings by category (estimate):
- Excessive nesting: 80-100 warnings
  Action: Refactor nested structures

- Unused code: 40-60 warnings
  Action: Remove or mark #[allow]

- Type issues: 50-80 warnings
  Action: Adjust types/casts

- Style issues: 100-120 warnings
  Action: Apply clippy suggestions

- Documentation: 30-50 warnings
  Action: Add doc comments

- Other: 70-100 warnings
  Action: Case-by-case
```

**Tasks**:
```bash
# Step 1: Categorize warnings
cargo clippy --lib 2>&1 | grep warning | sort | uniq -c | sort -rn

# Step 2: Address by severity
# Start with: unused code, type issues
# Then: style issues
# Last: remaining warnings (document if not fixable)

# Step 3: Test after each batch
cargo test --lib

# Step 4: Final check
cargo clippy --lib 2>&1 | grep warning | wc -l
# Target: < 100
```

**Success Criteria**:
- ✅ Warnings reduced from 469 → < 150
- ✅ Remaining warnings documented in CLIPPY_WARNINGS.md
- ✅ No test failures

---

### P6.2 Complete Rust Documentation

**Status**: 40% of Rust modules lack doc comments
**Target**: 90% coverage

**Files to Document** (30+ files):
```
Priority 1: Public API
  - lib.rs (main FFI interface)
  - api/mod.rs
  - api/py_bindings.rs

Priority 2: Core modules
  - pipeline/unified.rs
  - graphql/fragment_resolver.rs
  - graphql/directive_evaluator.rs

Priority 3: Important types
  - core/types.rs
  - core/error.rs
  - validation/mod.rs
```

**Template**:
```rust
/// Brief description of the function/struct.
///
/// Longer explanation if needed.
///
/// # Arguments
/// * `arg1` - Description of arg1
/// * `arg2` - Description of arg2
///
/// # Returns
/// Description of return value.
///
/// # Errors
/// Describes error conditions.
///
/// # Examples
/// ```
/// let result = function(arg1, arg2)?;
/// ```
pub fn function(arg1: Type, arg2: Type) -> Result<ReturnType> {
```

**Success Criteria**:
- ✅ All public functions documented
- ✅ All public types documented
- ✅ Doc tests pass
- ✅ Documentation coverage: 40% → 90%

---

## Phase 7: Automated Quality Checks (Week 9)

**Goal**: Integrate quality checks into CI/CD

### P7.1 Set Up Code Quality Metrics

**Create METRICS.md**:
```markdown
# Code Quality Metrics

## Baseline (January 2026)
- Type Coverage: 70%
- Test Coverage: 85%
- Clippy Warnings: 469
- Ruff Errors: 23
- Module Duplication: 3 pairs
- Dead Code: ~800 LOC

## Targets (Post-Cleanup)
- Type Coverage: 85% → 95%
- Test Coverage: 85% (maintain)
- Clippy Warnings: 469 → 100
- Ruff Errors: 23 → 0
- Module Duplication: 0
- Dead Code: 0

## Current Status
- Phase 1 (Week 1-2): [In Progress]
- Phase 2 (Week 3-4): [Pending]
- Phase 3 (Week 5-6): [Pending]
- Phase 4 (Week 7): [Pending]
- Phase 5 (Week 8): [Pending]
- Phase 6 (Week 8-9): [Pending]
- Phase 7 (Week 9): [Pending]
```

### P7.2 Create CI/CD Quality Gate

**GitHub Actions Workflow** (`.github/workflows/code-quality.yml`):
```yaml
name: Code Quality Check

on: [pull_request, push]

jobs:
  rust-quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Rust Quality Checks
        run: |
          cargo clippy --lib -- -D warnings
          cargo fmt --check
          cargo test --lib

  python-quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Python Quality Checks
        run: |
          ruff check src/ tests/
          mypy src/ --strict
          pytest tests/ -v

  metrics:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Generate Metrics
        run: |
          mypy src/ --stats > /tmp/type_metrics.txt
          ruff check src/ --statistics > /tmp/ruff_metrics.txt
      - name: Comment PR
        uses: actions/github-script@v7
        with:
          script: |
            // Post metrics as PR comment
```

### P7.3 Documentation Integration

**Create docs/CODE_QUALITY.md**:
```markdown
# Code Quality Standards

## Type Hints
- All public functions must have return types
- All public classes must have __init__ type hints
- Target: 95% type coverage

## Documentation
- All public modules must have docstrings
- All public classes/functions must be documented
- Documentation must include examples

## Testing
- All new code must have tests
- Target: 85%+ code coverage
- Performance tests for critical paths

## Code Organization
- Files should be < 400 LOC
- Functions should be < 50 LOC (generally)
- Avoid duplication: DRY principle
- Follow established module structure

## Error Handling
- All fallible operations must return Result or raise Exception
- Errors must be specific (not generic)
- Client errors vs. Server errors clearly distinguished

## Performance
- Critical paths must have benchmarks
- Benchmarks must be in place before refactoring
- Profile before optimizing
```

**Success Criteria**:
- ✅ CI/CD quality gate implemented
- ✅ Metrics tracked in repository
- ✅ Code quality standards documented
- ✅ Pre-commit hooks enforce standards

---

## Phase 8: Final Verification & Documentation (Week 10)

**Goal**: Verify cleanup complete, document improvements

### P8.1 Comprehensive Testing

**Tasks**:
```bash
# Full test suite
pytest tests/ -v --cov=src --cov-report=term-missing

# Rust tests
cargo test --lib
cargo test --test '*' 2>/dev/null || true

# Type checking
mypy src/ --strict --no-implicit-optional

# Code quality
ruff check src/ tests/
cargo clippy --lib -- -D warnings

# Performance (if benchmarks fixed)
cargo bench --bench '*' 2>/dev/null || true
```

**Success Criteria**:
- ✅ All tests pass (100% pass rate)
- ✅ Type checking passes (mypy --strict)
- ✅ Ruff checks pass (0 errors)
- ✅ Clippy warnings < 100

### P8.2 Create Summary Documentation

**Files to Create**:

1. **CODE_CLEANING_RESULTS.md**
```markdown
# Code Cleaning Results

## Metrics Improvement
- Type Coverage: 70% → 92%
- Clippy Warnings: 469 → 85
- Ruff Errors: 23 → 0
- Large Files Refactored: 3
- Dead Code Removed: 800 LOC
- Modules Consolidated: 2
- Module Duplication: 3 pairs → 0

## Work Completed
- [x] Phase 1: Critical Fixes
- [x] Phase 2: Type Annotations & Documentation
- [x] Phase 3: Test Organization
- [x] Phase 4: Large File Refactoring
- [x] Phase 5: Module Consolidation
- [x] Phase 6: Rust Cleanup
- [x] Phase 7: Quality Automation
- [x] Phase 8: Final Verification

## Files Changed
- Python files modified: 124
- Rust files modified: 67
- Test files reorganized: 203
- New documentation: 8 files
- Total commits: 47

## Key Improvements
1. Type safety significantly improved
2. Module organization clarified
3. Large files broken into smaller, focused modules
4. Dead code removed
5. Automated quality checks implemented
6. Documentation significantly improved

## Breaking Changes
- None expected (backward compatible design used)
- Deprecated modules clearly marked for v3.0 removal

## Recommendations for Future
1. Maintain metrics in CI/CD
2. Keep type coverage above 90%
3. Address remaining clippy warnings periodically
4. Monitor module organization as new features added
5. Schedule module consolidation follow-up for v3.0
```

2. **CLEANUP_TIMELINE.md** - Document actual completion timeline

3. **Updated ARCHITECTURE.md** - Reflect new module structure

### P8.3 Create Migration Guide

**For breaking changes (if any)**:
- Document API changes
- Provide code examples
- Timeline for deprecations
- How to migrate existing code

---

## Implementation Timeline

### Week 1-2: Phase 1 (Critical Fixes)
```
Mon-Tue: Fix Rust compilation
Wed-Thu: Fix Ruff errors
Fri:     Document db_core.py strategy & baselines
```

### Week 3-4: Phase 2 (Type Annotations & Docs)
```
Mon-Wed: Complete public API type hints
Thu-Fri: Add module docstrings
```

### Week 5-6: Phase 3 (Test Organization)
```
Mon-Tue: Analyze & establish test structure
Wed-Thu: Consolidate duplicate tests
Fri:     Break up large test files
```

### Week 7: Phase 4 (Large File Refactoring)
```
Mon-Tue: Refactor decorators.py
Wed-Thu: Refactor where_clause.py
Fri:     Refactor db_core.py or schedule removal
```

### Week 8: Phase 5 (Module Consolidation)
```
Mon-Wed: Resolve gql/graphql, cache/caching duplication
Thu-Fri: Remove dead code
```

### Week 8-9: Phase 6 (Rust Cleanup)
```
Mon-Wed: Fix clippy warnings
Thu-Fri: Complete Rust documentation
```

### Week 9: Phase 7 (Quality Automation)
```
Mon-Tue: Set up CI/CD quality gates
Wed-Thu: Create metrics tracking
Fri:     Documentation integration
```

### Week 10: Phase 8 (Final Verification)
```
Mon-Tue: Comprehensive testing
Wed-Thu: Create summary documentation
Fri:     Final verification & release prep
```

---

## Success Criteria

### Overall Metrics
| Metric | Before | Target | Status |
|--------|--------|--------|--------|
| Code Quality Score | 6.5/10 | 8.5/10 | Target |
| Type Coverage | 70% | 92% | Target |
| Clippy Warnings | 469 | < 100 | Target |
| Ruff Errors | 23 | 0 | Target |
| Large Files (>500 LOC) | 8 | 2 | Target |
| Dead Code (LOC) | 800 | 0 | Target |
| Test Organization | Scattered | Clear | Target |
| Module Duplication | 3 pairs | 0 | Target |

### Phase-Specific Criteria

**Phase 1**: ✅ All compilation errors fixed, baseline metrics established
**Phase 2**: ✅ Type coverage > 80%, all public API documented
**Phase 3**: ✅ Test structure documented, duplicates consolidated
**Phase 4**: ✅ Large files broken into focused modules
**Phase 5**: ✅ Duplicate modules resolved, dead code removed
**Phase 6**: ✅ Clippy warnings < 100, documentation complete
**Phase 7**: ✅ CI/CD quality gates implemented
**Phase 8**: ✅ All tests pass, comprehensive documentation created

---

## Resource Requirements

### Team Composition
- **Lead**: 1 senior developer (oversee architecture)
- **Implementation**: 2-3 developers
- **Review**: 1 code reviewer
- **QA**: 1 tester (verify no regressions)

### Time Commitment
- **Total**: 8-10 weeks
- **Per week**: 40 hours (2 FTE)
- **Estimated cost**: 320-400 engineer hours

### Tools & Infrastructure
- GitHub Actions (CI/CD) - Already in use
- ruff, mypy, clippy - Development tools
- pytest, cargo test - Test infrastructure
- Documentation generators - Sphinx, rustdoc

---

## Risk Mitigation

### Key Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| Breaking changes | Low | High | Use backward-compatible approach, extensive testing |
| Large refactoring failures | Medium | Medium | Test incrementally, revert if needed |
| Scope creep | Medium | Medium | Strict phase scope, document exclusions |
| Performance regression | Low | High | Benchmark before/after, profile changes |
| Merge conflicts | High | Low | Coordinate branches, merge frequently |

### Mitigation Strategies
1. **Incremental commits**: Small, reviewable changes
2. **Extensive testing**: Run full test suite after each phase
3. **Documentation**: Document changes clearly for future developers
4. **Code review**: Require review before merging
5. **Rollback plan**: Keep clean version, can revert if needed

---

## Excluded from This Plan

The following are explicitly NOT included in this cleanup:

1. **Feature development** - Only code quality
2. **Python 3.12+ migration** - Current standard OK
3. **Database schema changes** - Not in scope
4. **Performance optimization** - Beyond cleanup scope
5. **API redesign** - Keep current API
6. **New test coverage** - Maintain current coverage
7. **Documentation site rebuild** - Update docs, don't rebuild
8. **Dependency updates** - Current versions OK
9. **CI/CD redesign** - Only add quality gates
10. **Async refactoring** - Beyond cleanup scope

---

## Next Steps

### Before Starting (Week 0)
1. ✅ Get stakeholder approval for this plan
2. ✅ Allocate team resources (2-3 developers)
3. ✅ Set up branch strategy (feature branches per phase)
4. ✅ Create metrics tracking spreadsheet
5. ✅ Schedule weekly sync meetings

### Phase 1 Kickoff
1. Create `cleanup/phase-1-critical-fixes` branch
2. Start with P1.1: Fix Rust compilation
3. Daily standups during Phase 1
4. Weekly progress reviews

---

## Appendices

### A. Ruff Error Details

```
23 Ruff Errors to Fix:

F841 (Unused Variables): 7
  - Remove or prefix with _

ASYNC230 (Blocking I/O): 2
  - Use asyncio.to_thread for blocking ops

ASYNC109 (Timeout): 2
  - Fix timeout implementation

F821 (Undefined Names): 2
  - Add missing imports or fix typos

TC002/TC003 (Type Imports): 2
  - Move to TYPE_CHECKING or use quotes

E501 (Line too long): 1
  - Break into multiple lines

S307 (Eval): 1
  - Replace with safer alternative

DTZ007 (No Timezone): 1
  - Use timezone-aware datetime
```

### B. Large Files to Refactor

```
Top 10 Largest Python Files:
1. src/fraiseql/db_core.py (2,450 LOC) - Deprecated
2. src/fraiseql/decorators.py (1,058 LOC) - Priority
3. src/fraiseql/where_clause.py (838 LOC) - Priority
4. src/fraiseql/where_normalization.py (527 LOC) - Review
5. src/fraiseql/sql/where_builder.py (489 LOC) - Review
6. src/fraiseql/mutations/executor.py (451 LOC) - Review
7. src/fraiseql/sql/operators/basic_operators.py (421 LOC) - OK
8. src/fraiseql/sql/operators/advanced_operators.py (398 LOC) - OK
9. src/fraiseql/enterprise/rbac/middleware.py (367 LOC) - OK
10. src/fraiseql/config/schema_config.py (345 LOC) - OK

Top 5 Largest Rust Files:
1. src/subscriptions/executor.rs (1,542 LOC) - Review
2. src/subscriptions/integration_tests.rs (4,775 LOC) - Split tests
3. src/http/axum_server.rs (1,200 LOC) - Review
4. src/pipeline/unified.rs (1,100 LOC) - OK (complex)
5. src/mutation/response_builder.rs (1,080 LOC) - Review
```

### C. Module Duplication Map

```
Potential Duplicates:
1. gql/ (11 files, 1,200 LOC) vs graphql/ (9 files, 1,100 LOC)
   Recommendation: Consolidate to single module

2. caching/ (6 files, 800 LOC) vs cache/ (8 files, 1,000 LOC)
   Recommendation: Consolidate to cache/

3. mutations/ vs mutation/
   Status: Seems distinct, needs verification

Dead Modules:
1. ivm/ - Incomplete, no tests, unused
2. routing/ - Marked private, sparse
3. health/ - Purpose unclear
4. federation/ - 45% complete
```

### D. Estimated Effort by Task

```
Phase 1: 15 hours
  - Fix Rust compilation: 1h
  - Fix Ruff errors: 3h
  - Document db_core: 2h
  - Create baseline: 2h
  - PR/review: 3h
  - Buffer: 4h

Phase 2: 32 hours
  - Complete type hints: 20h (28 files × 45min)
  - Module docstrings: 8h (20 modules × 25min)
  - Private module docs: 4h

Phase 3: 28 hours
  - Analyze structure: 4h
  - Establish standards: 3h
  - Consolidate duplicates: 12h
  - Break up large files: 7h
  - Documentation: 2h

Phase 4: 22 hours
  - Refactor decorators.py: 8h
  - Refactor where_clause.py: 8h
  - Refactor db_core.py: 4h
  - Testing & review: 2h

Phase 5: 16 hours
  - Resolve gql/graphql: 6h
  - Resolve cache/caching: 5h
  - Remove dead code: 3h
  - Testing & review: 2h

Phase 6: 20 hours
  - Fix clippy warnings: 12h
  - Complete Rust docs: 6h
  - Testing: 2h

Phase 7: 12 hours
  - Set up CI/CD: 6h
  - Create metrics: 3h
  - Documentation: 3h

Phase 8: 12 hours
  - Comprehensive testing: 5h
  - Summary documentation: 4h
  - Final verification: 3h

TOTAL: ~157 hours (~4 weeks for 1 FTE, 2 weeks for 2 FTE)
```

---

## Version History

| Date | Version | Changes | Author |
|------|---------|---------|--------|
| 2026-01-09 | 1.0 | Initial plan created | Claude |

---

**Plan Status**: ✅ Ready for Implementation

**Next Action**: Present to team, get approval, allocate resources
