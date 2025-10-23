# FraiseQL v1.0 Release Preparation Plan

**Date**: 2025-10-22
**Current Version**: v0.11.5
**Target**: v1.0.0 Production Release
**Estimated Total Time**: 4-5 days

---

## 📊 Current Status Assessment

### Test Suite Health
```
✅ 3,551 tests passing (99.86% pass rate)
⏭️ 0 tests skipped (100% test suite coverage)
❌ 5 tests failing (0.14% failure rate)
```

### Failing Tests Breakdown
1. **PostgreSQL Placeholder Format Issues** (2 tests)
   - `test_complex_nested_dict_filters` - Invalid placeholder `%m`
   - `test_production_mixed_filtering_comprehensive` - Invalid placeholder `%.`

2. **Hybrid Table Nested Object Filtering** (3 tests)
   - `test_nested_object_filter_on_hybrid_table`
   - `test_nested_object_filter_with_results`
   - `test_multiple_nested_object_filters`

### Code Quality
- Production stable codebase
- Rust pipeline fully operational
- Recent critical bugs fixed (Phases 1-6 complete)
- Comprehensive documentation exists (needs organization)

---

## 🎯 Release Goals

### Technical Goals
- ✅ 100% test pass rate (0 failures)
- ✅ 0 skipped tests
- ✅ All examples working
- ✅ Clean linting (ruff, pyright)
- ✅ No unresolved TODOs in critical paths

### Documentation Goals
- ✅ Clean root directory (move planning docs to archive)
- ✅ Clear v1.0 release documentation
- ✅ Updated CHANGELOG
- ✅ VERSION_STATUS.md created
- ✅ All examples documented and tested

### Release Goals
- ✅ Version bumped to 1.0.0
- ✅ Git tagged with v1.0.0
- ✅ PyPI package published
- ✅ Documentation site updated

---

## 📋 Phase 1: Critical Bug Fixes (Priority: CRITICAL)

**Objective**: Achieve 100% test pass rate
**Estimated Time**: 2-3 days
**Blocker**: Cannot release with failing tests

### Phase 1A: PostgreSQL Placeholder Format Bug Fix

**Status**: 🔴 BLOCKING
**Tests Affected**: 2
**Estimated Time**: 4-6 hours

#### Problem Analysis
Two tests fail with PostgreSQL placeholder errors:
```
psycopg.ProgrammingError: only '%s', '%b', '%t' are allowed as placeholders, got '%m'
psycopg.ProgrammingError: only '%s', '%b', '%t' are allowed as placeholders, got '%.'
```

#### Root Cause
The SQL WHERE clause generator is creating invalid PostgreSQL placeholders when handling complex nested filters.

#### Files to Investigate
1. `src/fraiseql/sql/where/core/sql_builder.py` - WHERE clause SQL generation
2. `src/fraiseql/sql/graphql_where_generator.py` - GraphQL → SQL conversion
3. `src/fraiseql/db.py` - `_convert_dict_where_to_sql()` method

#### Implementation Steps

**Step 1: Reproduce the Bug (30 minutes)**
```bash
# Run failing tests with full output
uv run pytest tests/integration/database/repository/test_dynamic_filter_construction.py::TestDynamicFilterConstruction::test_complex_nested_dict_filters -vv --tb=long

uv run pytest tests/regression/where_clause/test_industrial_where_clause_generation.py::TestREDPhaseProductionScenarios::test_production_mixed_filtering_comprehensive -vv --tb=long
```

**Step 2: Identify Placeholder Generation Code (1 hour)**
```bash
# Search for placeholder generation
grep -r "%s" src/fraiseql/sql/
grep -r "placeholder" src/fraiseql/sql/
grep -r "parameterized" src/fraiseql/
```

Look for code that constructs SQL placeholders. Common patterns:
```python
# WRONG - creates invalid placeholders
sql = f"field = %{some_var}"  # Could create %m, %.

# RIGHT - uses parameterized queries correctly
sql = "field = %s"
params.append(value)
```

**Step 3: Fix Placeholder Generation (2-3 hours)**

Likely fix locations:
```python
# In sql_builder.py or similar
def build_where_clause(...):
    # Before (buggy):
    sql = f"field = %{format_char}"  # BAD

    # After (fixed):
    sql = "field = %s"  # GOOD
    params.append(value)
```

**Step 4: Verify Fix (30 minutes)**
```bash
# Run the two failing tests
uv run pytest tests/integration/database/repository/test_dynamic_filter_construction.py::TestDynamicFilterConstruction::test_complex_nested_dict_filters -v

uv run pytest tests/regression/where_clause/test_industrial_where_clause_generation.py::TestREDPhaseProductionScenarios::test_production_mixed_filtering_comprehensive -v

# Ensure no regressions
uv run pytest tests/integration/database/repository/ -v
uv run pytest tests/regression/where_clause/ -v
```

**Step 5: Document and Commit (30 minutes)**
```bash
git add src/fraiseql/sql/
git commit -m "fix: correct PostgreSQL placeholder format in WHERE clause generation

- Fixed invalid placeholder generation (%m, %.) in complex nested filters
- Ensured all SQL uses only %s, %b, %t placeholders as per psycopg3 spec
- Updated placeholder generation to use parameterized queries correctly

Fixes: test_complex_nested_dict_filters, test_production_mixed_filtering_comprehensive
Tests: 2 failing → passing
"
```

---

### Phase 1B: Hybrid Table Nested Object Filtering Fix

**Status**: 🔴 BLOCKING
**Tests Affected**: 3
**Estimated Time**: 6-8 hours

#### Problem Analysis
Three tests fail with assertion errors indicating nested object filters on hybrid tables are not working:
```
AssertionError: Expected 0 allocations for machine1, but got 3.
FraiseQL is failing to handle nested object filtering on hybrid tables.
```

#### Root Cause
When filtering hybrid tables (tables with both regular columns AND JSONB data columns) using nested object syntax like:
```python
where = {"machine": {"id": {"eq": machine_1_id}}}
```
The filter is not being applied correctly, resulting in unfiltered results.

#### Files to Investigate
1. `src/fraiseql/db.py` - `_convert_dict_where_to_sql()` method (lines 758-822)
2. `src/fraiseql/sql/where/core/sql_builder.py` - Nested object detection
3. `tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py` - Test expectations

#### Implementation Steps

**Step 1: Understand Test Expectations (1 hour)**
```bash
# Read the failing test
cat tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py

# Run one failing test with full output
uv run pytest tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py::TestHybridTableNestedObjectFiltering::test_nested_object_filter_on_hybrid_table -vv --tb=long
```

Understand:
- What is the database schema for the hybrid table?
- What filter is being applied?
- What SQL is being generated?
- What results are expected vs actual?

**Step 2: Trace SQL Generation (2 hours)**

Add debug logging to trace the filter conversion:
```python
# In src/fraiseql/db.py _convert_dict_where_to_sql()
import logging
logger = logging.getLogger(__name__)

def _convert_dict_where_to_sql(...):
    logger.debug(f"Converting where dict: {where_dict}")
    logger.debug(f"Table columns: {table_columns}")
    # ... existing code ...
    logger.debug(f"Generated SQL: {sql_conditions}")
```

Run the test again and examine the logs:
```bash
uv run pytest tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py::TestHybridTableNestedObjectFiltering::test_nested_object_filter_on_hybrid_table -vv --log-cli-level=DEBUG -s
```

**Step 3: Identify the Bug (1-2 hours)**

Common issues with hybrid table filtering:
1. **Not detecting nested object correctly**: The `is_nested_object` flag may not be set
2. **Wrong SQL path**: May be querying wrong column (regular vs JSONB)
3. **Missing table metadata**: `table_columns` may not include nested relationship info
4. **JSONB path incorrect**: May need `data->'machine'->>'id'` syntax

Check the logic in `_convert_dict_where_to_sql()` around lines 758-822:
```python
# Look for nested object detection
is_nested_object = (
    isinstance(field_filter, dict)
    and len(field_filter) == 1
    and list(field_filter.keys())[0] != "eq"  # Not an operator
)
```

**Step 4: Implement Fix (2-3 hours)**

Likely fix scenarios:

**Scenario A: Nested object not detected**
```python
# Before (buggy)
is_nested_object = field in table_columns  # Too strict

# After (fixed)
is_nested_object = (
    field not in table_columns  # NOT a regular column
    and isinstance(field_filter, dict)
    and "id" in field_filter  # Has id filter
)
```

**Scenario B: Wrong SQL path for hybrid tables**
```python
# Before (buggy) - queries regular column
sql = f"{field} = %s"

# After (fixed) - queries JSONB path
sql = f"(data->>'{field}_id')::uuid = %s"
# OR
sql = f"data @> %s::jsonb"  # For JSONB containment
```

**Scenario C: Missing foreign key resolution**
```python
# For nested object filters like {"machine": {"id": {"eq": value}}}
# Need to resolve to the foreign key column or JSONB path

if is_nested_object:
    nested_field = list(field_filter.keys())[0]  # "id"
    if nested_field == "id":
        # Check if there's a foreign key column
        fk_column = f"{field}_id"  # e.g., "machine_id"
        if fk_column in table_columns:
            # Use regular column
            sql = f"{fk_column} = %s"
        else:
            # Use JSONB path
            sql = f"(data->>'{fk_column}')::uuid = %s"
```

**Step 5: Test Fix (1 hour)**
```bash
# Run the three failing tests
uv run pytest tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py -v

# Check for regressions in related tests
uv run pytest tests/integration/database/repository/ -k "filter" -v
uv run pytest tests/integration/database/repository/ -k "hybrid" -v
uv run pytest tests/integration/database/repository/ -k "nested" -v
```

**Step 6: Document and Commit (30 minutes)**
```bash
git add src/fraiseql/db.py
git commit -m "fix: handle nested object filtering on hybrid tables correctly

- Fixed nested object detection for hybrid tables with JSONB columns
- Properly resolve foreign key references in nested filters
- Support both regular column and JSONB path queries for nested objects

Example:
  where = {\"machine\": {\"id\": {\"eq\": machine_id}}}
  → Correctly filters by machine_id (regular or JSONB column)

Fixes: test_nested_object_filter_on_hybrid_table,
       test_nested_object_filter_with_results,
       test_multiple_nested_object_filters
Tests: 3 failing → passing
"
```

---

### Phase 1C: Final Validation

**Estimated Time**: 30 minutes

```bash
# Run full test suite
uv run pytest --tb=short

# Should see:
# ✅ 3,556 tests passing (all 5 previously failing now pass)
# ⏭️ 0 tests skipped
# ❌ 0 tests failing

# Verify specific test categories
uv run pytest tests/integration/ -v
uv run pytest tests/regression/ -v
uv run pytest tests/unit/ -v
```

**Success Criteria**:
- [ ] All 5 previously failing tests now pass
- [ ] No new test failures introduced
- [ ] No tests skipped
- [ ] All test categories passing

**Commit Phase 1 Completion**:
```bash
git tag phase-1-tests-fixed
git push origin phase-1-tests-fixed
```

---

## 📚 Phase 2: Documentation Cleanup (Priority: HIGH)

**Objective**: Organize documentation for v1.0 release
**Estimated Time**: 1 day
**Dependencies**: None (can run parallel with Phase 1)

### Phase 2A: Archive Planning and Status Documents

**Estimated Time**: 2 hours

#### Step 1: Create Archive Structure
```bash
mkdir -p archive/session-docs/2025-10-22-v1-prep
mkdir -p archive/session-docs/2025-10-16-rust-pipeline
mkdir -p archive/session-docs/2025-10-13-test-fixes
```

#### Step 2: Move Planning Documents
```bash
# Session status documents (not needed in root)
mv AGENT_FINAL_STATUS.md archive/session-docs/2025-10-22-v1-prep/
mv AGENT_PROGRESS_STATUS.md archive/session-docs/2025-10-22-v1-prep/
mv COMPREHENSIVE_FIX_PLAN.md archive/session-docs/2025-10-22-v1-prep/
mv CURRENT_TEST_STATUS_AND_NEXT_PHASES.md archive/session-docs/2025-10-22-v1-prep/
mv EXACT_CODE_CHANGES.md archive/session-docs/2025-10-22-v1-prep/
mv FINAL_5_TESTS_FIX_PLAN.md archive/session-docs/2025-10-22-v1-prep/
mv FIXING_ALL_TESTS_EXECUTIVE_SUMMARY.md archive/session-docs/2025-10-22-v1-prep/
mv FIX_REMAINING_16_TESTS.md archive/session-docs/2025-10-22-v1-prep/
mv PHASE_2_OBSOLETE_TESTS_ARCHIVED.md archive/session-docs/2025-10-22-v1-prep/
mv PHASE_6A_HYBRID_TABLE_FILTERING_FIX_PLAN.md archive/session-docs/2025-10-22-v1-prep/
mv PHASE_6A_SIMPLIFIED_JSONB_ONLY_RESPONSE.md archive/session-docs/2025-10-22-v1-prep/
mv PHASE_6B_BLOG_EXAMPLES_FIXED.md archive/session-docs/2025-10-22-v1-prep/
mv POST_RELEASE_EXECUTIVE_SUMMARY.md archive/session-docs/2025-10-22-v1-prep/
mv POST_RELEASE_IMPLEMENTATION_PLAN.md archive/session-docs/2025-10-22-v1-prep/
mv QUICK_FIX_SUMMARY.md archive/session-docs/2025-10-22-v1-prep/
mv QUICK_WIN_30_MINUTES.md archive/session-docs/2025-10-22-v1-prep/
mv TEST_FIX_ROADMAP.md archive/session-docs/2025-10-22-v1-prep/

# Planning documents
mv PLAN_COORDINATES_DATATYPE.md archive/session-docs/2025-10-16-rust-pipeline/
mv PLAN_DOCUMENTATION_ALIGNMENT_RUST_PIPELINE.md archive/session-docs/2025-10-16-rust-pipeline/
mv PLAN_FIX_FAILING_TESTS_RUST_PIPELINE.md archive/session-docs/2025-10-16-rust-pipeline/
mv PLAN_LTREE_OPERATOR_ENRICHMENT.md archive/session-docs/2025-10-16-rust-pipeline/

# Rust bug fix documentation
mv RUST_JSON_BUG_FIXED.md archive/session-docs/2025-10-13-test-fixes/
mv RUST_JSON_BUG_REPORT.md archive/session-docs/2025-10-13-test-fixes/
mv RUST_PIPELINE_PUBLISHING_ASSESSMENT.md archive/session-docs/2025-10-16-rust-pipeline/

# Coordinate system documentation
mv COORDINATE_DISTANCE_METHODS.md archive/session-docs/2025-10-16-rust-pipeline/
```

#### Step 3: Create Archive Index
```bash
cat > archive/session-docs/README.md << 'EOF'
# Session Documentation Archive

This directory contains historical planning, status, and implementation documents from development sessions. These documents are kept for reference but are not part of the active documentation.

## Directory Structure

### 2025-10-22-v1-prep/
Documentation from v1.0 release preparation session
- Test fixing plans and progress tracking
- Phase completion summaries
- Implementation plans

### 2025-10-16-rust-pipeline/
Documentation from Rust pipeline enhancements
- LTREE operator enrichment
- Coordinate distance methods
- Performance optimization

### 2025-10-13-test-fixes/
Documentation from critical bug fix session
- Rust JSON generation bug
- Test suite health improvements

## Active Documentation

For current project documentation, see:
- `/README.md` - Project overview
- `/GETTING_STARTED.md` - Quick start guide
- `/docs/` - Comprehensive documentation
- `/CHANGELOG.md` - Version history
EOF
```

#### Step 4: Commit Archive Organization
```bash
git add archive/session-docs/
git add archive/session-docs/README.md
git commit -m "docs: archive session planning and status documents

- Moved 25+ planning/status docs to archive/session-docs/
- Organized by session date and topic
- Created archive index for reference
- Cleaned up root directory for v1.0 release
"
```

---

### Phase 2B: Create VERSION_STATUS.md

**Estimated Time**: 1 hour

```bash
cat > VERSION_STATUS.md << 'EOF'
# FraiseQL Version Status

**Last Updated**: 2025-10-22

## Current Production Version: v1.0.0

FraiseQL v1.0.0 is the stable, production-ready release suitable for all users.

## Version Overview

| Version | Status | Recommended For | Stability |
|---------|--------|----------------|-----------|
| **v1.0.0** | Production Stable | All users | ✅ Stable |
| v0.11.5 | Superseded | Legacy projects | ⚠️ Use v1.0.0 |
| Rust Pipeline | Integrated | Included in v1.0+ | ✅ Stable |

## What's in v1.0.0

### Core Features
- ✅ CQRS architecture with PostgreSQL
- ✅ Rust-accelerated JSON transformation (7-10x faster)
- ✅ Hybrid table support (regular + JSONB columns)
- ✅ Advanced type system (UUID, DateTime, IP, CIDR, LTree, MAC, etc.)
- ✅ Nested object filtering
- ✅ Trinity identifier pattern support
- ✅ Comprehensive GraphQL introspection

### Performance
- Sub-millisecond query latency (0.5-5ms typical)
- Rust pipeline: 7-10x faster than pure Python
- APQ (Automatic Persisted Queries) support
- PostgreSQL-native caching

### Test Coverage
- 3,556 tests passing (100% pass rate)
- 0 skipped tests
- 0 failing tests
- Comprehensive integration and regression testing

## Installation

### For New Projects (Recommended)
```bash
pip install fraiseql>=1.0.0
```

### For Existing Projects
```bash
pip install --upgrade fraiseql
```

See [MIGRATION_GUIDE.md](docs/migration/v0-to-v1.md) for upgrade instructions.

## Version Support Policy

| Version | Status | Security Fixes | Bug Fixes | New Features |
|---------|--------|----------------|-----------|--------------|
| v1.0.x | Supported | ✅ Yes | ✅ Yes | ✅ Yes |
| v0.11.x | Limited | ✅ Critical only | ❌ No | ❌ No |
| < v0.11 | Unsupported | ❌ No | ❌ No | ❌ No |

## Experimental Features

None currently. All features in v1.0.0 are production-stable.

## Future Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md) for planned features in v1.1+.

### Planned for v1.1
- CLI code generation from database schema
- Enhanced multi-tenancy patterns
- Performance monitoring dashboard

### Planned for v1.2
- GraphQL federation support
- Real-time subscriptions
- Advanced caching strategies

## Getting Help

- **Documentation**: https://fraiseql.readthedocs.io
- **Issues**: https://github.com/fraiseql/fraiseql/issues
- **Discussions**: https://github.com/fraiseql/fraiseql/discussions
- **Email**: lionel.hamayon@evolution-digitale.fr

## Reporting Issues

If you encounter issues with v1.0.0, please:
1. Check [CHANGELOG.md](CHANGELOG.md) for known issues
2. Search existing [GitHub issues](https://github.com/fraiseql/fraiseql/issues)
3. Create a new issue with:
   - FraiseQL version (`pip show fraiseql`)
   - Python version
   - PostgreSQL version
   - Minimal reproduction example

---

**Note**: This project follows [Semantic Versioning](https://semver.org/).
EOF

git add VERSION_STATUS.md
git commit -m "docs: add VERSION_STATUS.md for v1.0.0 release

- Clear version status and recommendations
- Installation instructions
- Version support policy
- Roadmap preview
"
```

---

### Phase 2C: Update CHANGELOG.md for v1.0.0

**Estimated Time**: 1 hour

```bash
# Edit CHANGELOG.md to add v1.0.0 release notes at the top
```

Add this section after the `[Unreleased]` section:

```markdown
## [1.0.0] - 2025-10-22

### 🎉 Major Release: FraiseQL v1.0.0

FraiseQL v1.0.0 is the first production-stable release, marking the culmination of extensive development, testing, and refinement. This release represents a mature, battle-tested framework ready for production use.

### 🏆 Release Highlights

**100% Test Suite Health**
- ✅ 3,556 tests passing (100% pass rate)
- ✅ 0 skipped tests
- ✅ 0 failing tests
- ✅ Comprehensive integration and regression coverage

**Production Stability**
- ✅ Rust pipeline fully operational and stable
- ✅ All critical bugs resolved
- ✅ Performance optimizations complete
- ✅ Documentation comprehensive and accurate

**Enterprise-Ready Features**
- ✅ CQRS architecture with PostgreSQL
- ✅ Rust-accelerated JSON transformation
- ✅ Hybrid table support
- ✅ Advanced type system
- ✅ Nested object filtering
- ✅ Trinity identifier patterns

### 🔧 What's New in v1.0.0

#### Fixed
- **PostgreSQL placeholder format bug** - Corrected invalid placeholder generation in complex nested filters
- **Hybrid table nested object filtering** - Fixed filtering logic for tables with both regular and JSONB columns
- **Field name conversion** - Proper camelCase ↔ snake_case conversion in all SQL generation paths
- **JSONB column metadata** - Enhanced database registry for type-safe JSONB operations

#### Added
- **VERSION_STATUS.md** - Clear versioning and support policy documentation
- **Comprehensive examples** - All examples tested and documented
- **Archive organization** - Historical documentation properly organized

#### Changed
- **Documentation structure** - Reorganized for clarity and maintainability
- **Test organization** - Archived obsolete tests, 100% active test health
- **Root directory** - Cleaned up for production release

### 📊 Performance Metrics

- **Query latency**: 0.5-5ms typical (sub-millisecond for cached queries)
- **Rust acceleration**: 7-10x faster than pure Python JSON processing
- **Test execution**: ~64 seconds for full suite (3,556 tests)
- **Code quality**: All linting passes (ruff, pyright)

### 🔄 Migration from v0.11.x

FraiseQL v1.0.0 is fully backward compatible with v0.11.5. Simply upgrade:

```bash
pip install --upgrade fraiseql
```

For detailed migration instructions, see [docs/migration/v0-to-v1.md](docs/migration/v0-to-v1.md).

### 🙏 Acknowledgments

This release represents months of development, testing, and refinement. Special thanks to:
- The PostgreSQL team for an amazing database
- The Rust community for excellent tooling
- Early adopters and testers for valuable feedback

### 📚 Documentation

- **Getting Started**: [GETTING_STARTED.md](GETTING_STARTED.md)
- **Installation**: [INSTALLATION.md](INSTALLATION.md)
- **Full Docs**: [docs/](docs/)
- **Examples**: [examples/](examples/)

### 🚀 Next Steps

See [VERSION_STATUS.md](VERSION_STATUS.md) for the v1.1+ roadmap.

---
```

Commit the changes:
```bash
git add CHANGELOG.md
git commit -m "docs: add v1.0.0 release notes to CHANGELOG

- Comprehensive v1.0.0 release summary
- Migration instructions
- Performance metrics
- Acknowledgments
"
```

---

### Phase 2D: Update README.md for v1.0.0

**Estimated Time**: 1 hour

Update the following sections in README.md:

1. **Version badge**: Update to v1.0.0
2. **Status line**: Change to "v1.0.0 - Production Stable"
3. **Version overview table**: Update v1.0 status to "Production Stable"
4. **Quick Start**: Ensure commands are for v1.0.0
5. **Installation**: Add v1.0.0 installation instructions

Example changes:
```markdown
# Before
**📍 You are here: Main FraiseQL Framework (v0.11.5) - Production Stable**

# After
**📍 You are here: Main FraiseQL Framework (v1.0.0) - Production Stable**
```

```markdown
# Before
| Version | Location | Status | Purpose | For Users? |
|---------|----------|--------|---------|------------|
| **v0.11.5** | Root level | Production Stable | Current framework | ✅ Recommended |

# After
| Version | Location | Status | Purpose | For Users? |
|---------|----------|--------|---------|------------|
| **v1.0.0** | Root level | Production Stable | Stable release | ✅ Recommended |
```

Commit:
```bash
git add README.md
git commit -m "docs: update README.md for v1.0.0 release

- Updated version badges and status
- Updated version table
- Clarified v1.0.0 as production stable
"
```

---

### Phase 2E: Verify All Examples Work

**Estimated Time**: 2 hours

Test each example to ensure they work with v1.0.0:

```bash
# Test each example
cd examples/blog_simple
uv run python -m pytest tests/ -v
cd ../..

cd examples/blog_enterprise
uv run python -m pytest tests/ -v
cd ../..

cd examples/ecommerce_api
uv run python -m pytest tests/ -v
cd ../..

# Continue for all examples...
```

Create a checklist:
- [ ] blog_simple
- [ ] blog_enterprise
- [ ] ecommerce_api
- [ ] complete_cqrs_blog
- [ ] hybrid_tables
- [ ] filtering
- [ ] specialized_types
- [ ] mutations_demo
- [ ] documented_api
- [ ] fastapi
- [ ] context_parameters
- [ ] admin-panel
- [ ] saas-starter
- [ ] native-auth-app
- [ ] analytics_dashboard
- [ ] security
- [ ] turborouter

Document any failing examples and fix them.

---

## 🧹 Phase 3: Code Cleanup (Priority: MEDIUM)

**Objective**: Clean up code for production release
**Estimated Time**: 4 hours
**Dependencies**: None

### Phase 3A: Resolve TODO/FIXME Comments

**Estimated Time**: 2 hours

#### Step 1: Audit All TODOs
```bash
# Find all TODO/FIXME comments
grep -r "TODO\|FIXME\|XXX\|HACK" src/fraiseql/ --include="*.py" > todo_audit.txt

# Found in these files:
# - src/fraiseql/sql/graphql_where_generator.py
# - src/fraiseql/cli/commands/init.py
# - src/fraiseql/cli/commands/generate.py
# - src/fraiseql/types/scalars/mac_address.py
```

#### Step 2: Review Each TODO

For each TODO:
1. **Read the TODO comment and surrounding code**
2. **Decide action**:
   - **Fix now** (if critical or quick)
   - **Create GitHub issue** (if future enhancement)
   - **Document as known limitation** (if design decision)
   - **Remove** (if no longer relevant)

#### Step 3: Fix or Document

Example actions:
```python
# BEFORE
# TODO: Add support for custom operators
def build_where(...):
    pass

# OPTION A: Fix it now (if critical)
def build_where(...):
    # Now supports custom operators via operator_registry
    pass

# OPTION B: Create issue and reference
# GitHub Issue #123: Add support for custom operators
def build_where(...):
    pass

# OPTION C: Remove if not needed
def build_where(...):
    pass
```

#### Step 4: Commit Changes
```bash
git add src/fraiseql/
git commit -m "refactor: resolve TODO comments for v1.0.0

- Fixed critical TODOs in SQL generation
- Created GitHub issues for enhancement TODOs
- Removed obsolete TODO comments
- Documented known limitations where appropriate
"
```

---

### Phase 3B: Run Linters and Fix Issues

**Estimated Time**: 1 hour

```bash
# Run ruff linter
uv run ruff check src/fraiseql/

# Fix auto-fixable issues
uv run ruff check --fix src/fraiseql/

# Check for remaining issues
uv run ruff check src/fraiseql/

# If issues remain, fix manually
# Then commit:
git add src/fraiseql/
git commit -m "style: fix linting issues for v1.0.0

- Resolved all ruff linting warnings
- Fixed code style inconsistencies
- Ensured PEP 8 compliance
"
```

---

### Phase 3C: Audit and Clean Experimental Directories

**Estimated Time**: 1 hour

#### Directories to Review:
1. `fraiseql-v1/` - Appears to be a prototype, archive or remove
2. `benchmark_submission/` - Archive or keep in root?
3. `marketing/` - Archive or keep in root?
4. `scripts/` - Audit for unused scripts

#### Steps:
```bash
# Review fraiseql-v1/
ls -la fraiseql-v1/
# Decision: Archive to archive/prototypes/fraiseql-v1/
mv fraiseql-v1/ archive/prototypes/

# Review benchmark_submission/
ls -la benchmark_submission/
# Decision: Keep if actively maintained, else archive

# Review marketing/
ls -la marketing/
# Decision: Archive to archive/marketing/
mv marketing/ archive/

# Review scripts/
ls -la scripts/
# Remove any scripts that are:
# - Unused
# - Obsolete (related to old architecture)
# - Development-only and not documented
```

Create a `scripts/README.md` documenting what each script does.

#### Commit:
```bash
git add archive/
git add scripts/
git commit -m "refactor: organize experimental and utility code

- Archived fraiseql-v1 prototype to archive/prototypes/
- Archived marketing materials to archive/marketing/
- Documented scripts/ directory
- Removed obsolete utility scripts
"
```

---

## ✅ Phase 4: Final Validation (Priority: HIGH)

**Objective**: Verify release readiness
**Estimated Time**: 3 hours
**Dependencies**: Phases 1, 2, 3 complete

### Phase 4A: Full Test Suite Validation

**Estimated Time**: 30 minutes

```bash
# Run complete test suite
uv run pytest --tb=short -v

# Expected results:
# ✅ 3,556 tests passing
# ⏭️ 0 tests skipped
# ❌ 0 tests failing

# Run with coverage
uv run pytest --cov=src/fraiseql --cov-report=html --cov-report=term

# Review coverage report
open htmlcov/index.html
```

**Success Criteria**:
- [ ] 100% test pass rate
- [ ] 0 skipped tests
- [ ] 0 failing tests
- [ ] Coverage > 85% (target: 90%+)

---

### Phase 4B: Fresh Installation Test

**Estimated Time**: 1 hour

Test installation from scratch in a clean environment:

```bash
# Create fresh virtual environment
cd /tmp
python -m venv test_fraiseql_install
source test_fraiseql_install/bin/activate

# Install from local build
pip install /home/lionel/code/fraiseql/

# Verify installation
python -c "import fraiseql; print(fraiseql.__version__)"
# Should print: 1.0.0

# Test basic functionality
python << EOF
from fraiseql import FraiseQL
print("FraiseQL imported successfully")
EOF

# Run quickstart example
fraiseql --help

# Test example
cd /home/lionel/code/fraiseql/examples/blog_simple
uv run python -m pytest tests/ -v

# Clean up
deactivate
rm -rf /tmp/test_fraiseql_install
```

**Success Criteria**:
- [ ] Installs without errors
- [ ] Version is 1.0.0
- [ ] Can import fraiseql
- [ ] CLI works
- [ ] Examples work

---

### Phase 4C: Documentation Review

**Estimated Time**: 1 hour

Review all documentation for accuracy:

#### Checklist:
- [ ] README.md
  - [ ] Installation instructions work
  - [ ] Quick start example works
  - [ ] Links are valid
  - [ ] Version references are correct

- [ ] GETTING_STARTED.md
  - [ ] All commands work
  - [ ] Examples are accurate
  - [ ] References are up-to-date

- [ ] docs/ directory
  - [ ] All guides are accurate
  - [ ] Code examples work
  - [ ] API references are complete
  - [ ] No broken links

- [ ] examples/ directory
  - [ ] All READMEs are accurate
  - [ ] Examples run successfully
  - [ ] Database schemas are documented

```bash
# Check for broken links
find docs/ -name "*.md" -exec grep -H "](.*)" {} \; | grep -v "^#" > links.txt
# Manually verify links or use a link checker tool

# Verify code examples
# Extract code blocks from markdown and test them
# (Manual process or use markdown code extraction tool)
```

---

### Phase 4D: Performance Benchmarks

**Estimated Time**: 30 minutes

Run performance benchmarks to verify no regressions:

```bash
# Run benchmark suite (if exists)
cd benchmarks/
./run_benchmarks.sh

# Or run specific benchmarks
uv run pytest benchmarks/ -v --benchmark-only

# Document results
cat > BENCHMARK_RESULTS_V1.0.0.md << 'EOF'
# FraiseQL v1.0.0 Benchmark Results

**Date**: 2025-10-22
**Hardware**: [Your hardware specs]
**Database**: PostgreSQL 15

## Query Performance
- Simple query: 1-2ms avg
- Complex query: 5-10ms avg
- Nested query: 8-15ms avg

## Rust Pipeline Performance
- JSON transformation: 0.5ms avg
- Field projection: 0.3ms avg
- camelCase conversion: 0.2ms avg

## Comparison vs v0.11.5
- No performance regressions
- Maintained 7-10x speedup over pure Python
EOF

git add BENCHMARK_RESULTS_V1.0.0.md
git commit -m "docs: add v1.0.0 benchmark results"
```

---

## 🚀 Phase 5: Release (Priority: CRITICAL)

**Objective**: Tag, build, and publish v1.0.0
**Estimated Time**: 2 hours
**Dependencies**: All previous phases complete

### Phase 5A: Version Bump

**Estimated Time**: 15 minutes

```bash
# Update version in pyproject.toml
sed -i 's/version = "0.11.5"/version = "1.0.0"/' pyproject.toml

# Update version in __init__.py if present
find src/fraiseql -name "__init__.py" -exec grep -l "__version__" {} \;
# If found, update to 1.0.0

# Commit version bump
git add pyproject.toml src/fraiseql/
git commit -m "chore: bump version to 1.0.0"
```

---

### Phase 5B: Create Git Tag

**Estimated Time**: 5 minutes

```bash
# Create annotated tag
git tag -a v1.0.0 -m "FraiseQL v1.0.0 - Production Stable Release

Release Highlights:
- 100% test suite health (3,556 passing, 0 failing)
- All critical bugs resolved
- Comprehensive documentation
- Production-ready stability

See CHANGELOG.md for full release notes.
"

# Verify tag
git tag -l -n9 v1.0.0

# Push tag
git push origin v1.0.0
```

---

### Phase 5C: Build Distribution

**Estimated Time**: 15 minutes

```bash
# Clean previous builds
rm -rf dist/ build/ src/fraiseql.egg-info/

# Build distribution packages
uv run python -m build

# Verify build
ls -lh dist/
# Should see:
# - fraiseql-1.0.0.tar.gz (source distribution)
# - fraiseql-1.0.0-py3-none-any.whl (wheel)

# Test installation from wheel
pip install dist/fraiseql-1.0.0-py3-none-any.whl
python -c "import fraiseql; print(fraiseql.__version__)"
# Should print: 1.0.0
```

---

### Phase 5D: Publish to PyPI

**Estimated Time**: 15 minutes

```bash
# Check PyPI credentials are configured
# Ensure ~/.pypirc exists or use token authentication

# Upload to Test PyPI first (optional but recommended)
uv run twine upload --repository testpypi dist/*

# Test installation from Test PyPI
pip install --index-url https://test.pypi.org/simple/ fraiseql==1.0.0

# If successful, upload to production PyPI
uv run twine upload dist/*

# Verify on PyPI
# Visit: https://pypi.org/project/fraiseql/
# Should show version 1.0.0
```

---

### Phase 5E: Update Documentation Site

**Estimated Time**: 30 minutes

If you have a documentation site (e.g., ReadTheDocs):

```bash
# Trigger documentation build
# This depends on your documentation setup

# For ReadTheDocs:
# - Push to main/master branch (if auto-build is configured)
# - Or manually trigger build via ReadTheDocs dashboard

# Verify documentation
# Visit: https://fraiseql.readthedocs.io/
# Should show v1.0.0 documentation
```

---

### Phase 5F: Create GitHub Release

**Estimated Time**: 30 minutes

1. Go to: https://github.com/fraiseql/fraiseql/releases/new
2. Select tag: v1.0.0
3. Release title: **FraiseQL v1.0.0 - Production Stable Release**
4. Description: Copy from CHANGELOG.md v1.0.0 section
5. Attach assets:
   - dist/fraiseql-1.0.0.tar.gz
   - dist/fraiseql-1.0.0-py3-none-any.whl
6. Mark as "latest release"
7. Publish release

---

### Phase 5G: Announcement

**Estimated Time**: 15 minutes

#### Update README.md badges
```markdown
[![Release](https://img.shields.io/github/v/release/fraiseql/fraiseql)](https://github.com/fraiseql/fraiseql/releases/latest)
```

#### Create announcement (optional)
```markdown
# 🎉 FraiseQL v1.0.0 Released!

We're excited to announce the release of FraiseQL v1.0.0, the first production-stable version!

## Highlights
- ✅ 100% test suite health (3,556 passing tests)
- ✅ Rust-accelerated JSON transformation (7-10x faster)
- ✅ Comprehensive documentation
- ✅ Production-ready stability

## Get Started
```bash
pip install fraiseql
```

See the [Getting Started Guide](GETTING_STARTED.md) for more.

## Links
- [Release Notes](CHANGELOG.md#100---2025-10-22)
- [Documentation](https://fraiseql.readthedocs.io)
- [GitHub](https://github.com/fraiseql/fraiseql)

Thank you to everyone who contributed to making this release possible!
```

Post to:
- GitHub Discussions
- Twitter/X
- Reddit (r/Python)
- Hacker News (Show HN)
- Python mailing lists

---

## 📋 Final Checklist

Before considering v1.0.0 complete, verify:

### Code Quality
- [ ] All tests pass (3,556/3,556)
- [ ] No skipped tests (0)
- [ ] No failing tests (0)
- [ ] No linting errors
- [ ] No unresolved TODOs in critical code
- [ ] Code coverage > 85%

### Documentation
- [ ] README.md updated for v1.0.0
- [ ] CHANGELOG.md has v1.0.0 release notes
- [ ] VERSION_STATUS.md created
- [ ] All examples work
- [ ] All docs reviewed and accurate
- [ ] No broken links

### Release
- [ ] Version bumped to 1.0.0 in pyproject.toml
- [ ] Git tagged with v1.0.0
- [ ] Built distributions (tar.gz, whl)
- [ ] Published to PyPI
- [ ] GitHub release created
- [ ] Documentation site updated

### Cleanup
- [ ] Planning docs archived
- [ ] Root directory cleaned
- [ ] Experimental code organized
- [ ] Scripts documented
- [ ] No temporary files

---

## 🎯 Success Criteria

**v1.0.0 is ready for release when:**
- ✅ All tests pass (100% pass rate)
- ✅ All documentation is accurate
- ✅ Code is clean and linted
- ✅ Examples all work
- ✅ Published to PyPI
- ✅ GitHub release created
- ✅ Announcement published

---

## ⏱️ Time Estimates Summary

| Phase | Estimated Time | Priority |
|-------|----------------|----------|
| Phase 1: Bug Fixes | 2-3 days | CRITICAL |
| Phase 2: Documentation | 1 day | HIGH |
| Phase 3: Code Cleanup | 4 hours | MEDIUM |
| Phase 4: Validation | 3 hours | HIGH |
| Phase 5: Release | 2 hours | CRITICAL |
| **Total** | **4-5 days** | - |

---

## 📝 Notes for Agent Execution

### Execution Order
1. **Start with Phase 1** (bug fixes) - blocking
2. **Phase 2 can run in parallel** (documentation) - independent
3. **Phase 3 after Phase 1** (code cleanup) - requires working code
4. **Phase 4 after all** (validation) - requires everything done
5. **Phase 5 last** (release) - requires Phase 4 validation

### Parallel Execution
- Phases 1 and 2 can run in parallel
- Phase 3 can start after Phase 1 completes
- Phases 4 and 5 must be sequential

### Decision Points
- **Phase 1B Step 3**: May require deeper investigation if bug is complex
- **Phase 2E**: Some examples may need fixes beyond documentation
- **Phase 3C**: Decisions on what to archive vs keep
- **Phase 5D**: May need manual PyPI authentication

### Rollback Plan
If issues are discovered late:
1. Do NOT publish to PyPI if validation fails
2. Fix issues and restart from Phase 4
3. If published, consider yanking from PyPI and releasing v1.0.1

---

**Good luck with the v1.0.0 release! 🚀**
