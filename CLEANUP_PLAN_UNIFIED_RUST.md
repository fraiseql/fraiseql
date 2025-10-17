# Repository Cleanup Plan for Unified Rust Architecture

**Date:** 2025-10-17
**Status:** Planning
**Goal:** Clean up repository after transitioning to unified Rust-first architecture

---

## 📊 Current State Assessment

### ✅ Implemented:
- Unified Rust pipeline (`src/fraiseql/core/rust_pipeline.py`) ✓
- `RustResponseBytes` response type ✓
- Rust transformation functions in `fraiseql_rs/` ✓
- Updated README reflecting unified architecture ✓

### ❌ Legacy Components (Need Cleanup):
- 20+ Python files still referencing old patterns
- 50+ deprecated documentation files
- Test files for old architecture
- Multiple redundant planning documents

---

## 🗑️ Cleanup Checklist

### 1. Python Code - Deprecated Modules (High Priority)

#### Files to DELETE entirely:
```
src/fraiseql/repositories/passthrough_mixin.py
src/fraiseql/core/json_passthrough_repository.py
src/fraiseql/core/raw_json_executor.py
src/fraiseql/gql/raw_json_wrapper.py
src/fraiseql/gql/raw_json_resolver.py
src/fraiseql/gql/raw_json_execution.py
src/fraiseql/gql/json_executor.py
src/fraiseql/fastapi/raw_json_handler.py
src/fraiseql/graphql/passthrough_type.py
src/fraiseql/graphql/passthrough_context.py
src/fraiseql/repositories/intelligent_passthrough.py (already deleted ✓)
```

**Rationale:** These implement the old "passthrough mode" and `RawJSONResult` patterns that are replaced by the unified Rust pipeline.

#### Files to REFACTOR:

**`src/fraiseql/db.py`** - Major cleanup needed:
- ❌ Remove `PassthroughMixin` inheritance
- ❌ Delete `_determine_mode()` method
- ❌ Remove `self.mode` detection logic
- ❌ Delete deprecated methods: `find_raw_json()`, `find_one_raw_json()`, `find_rust()`, `find_one_rust()`
- ❌ Simplify `find()` and `find_one()` to ONLY use Rust pipeline
- ✅ Keep: Core query building, parameter handling, metadata caching

**`src/fraiseql/fastapi/response_handlers.py`**:
- ❌ Remove `RawJSONResult` handling
- ✅ Keep: `RustResponseBytes` handling

**`src/fraiseql/fastapi/dependencies.py`**:
- ❌ Remove mode detection logic
- ❌ Remove passthrough context setup

**`src/fraiseql/routing/query_router.py`**:
- ❌ Remove `RawJSONResult` imports/handling
- ✅ Update to only use `RustResponseBytes`

**`src/fraiseql/graphql/execute.py`**:
- ❌ Remove branching logic for different execution modes
- ✅ Simplify to single Rust-first path

**`src/fraiseql/execution/unified_executor.py`**:
- ❌ Review and remove any legacy mode detection
- ✅ Ensure it uses Rust pipeline consistently

**`src/fraiseql/fastapi/custom_response.py`**:
- Review for any legacy response types

**`src/fraiseql/repositories/__init__.py`**:
- Remove passthrough exports

**`src/fraiseql/fastapi/routers.py`**:
- Update to only use `RustResponseBytes`

---

### 2. Documentation - Archive & Consolidate

#### Root Directory Planning Docs (Archive to `archive/planning/`):
```
CQRS_RUST_ARCHITECTURE.md → Redundant (implemented)
DATABASE_LEVEL_CACHING.md → Move to docs/performance/
DATAFLOW_SUMMARY.md → Redundant
JSONB_TO_HTTP_SIMPLIFICATION_PLAN.md → Implemented
PASSTHROUGH_FIX_ANALYSIS.md → Obsolete
PATTERNS_TO_IMPLEMENT.md → Review & integrate into docs
PERFORMANCE_OPTIMIZATION_PLAN.md → Consolidate into docs/performance/
POST_V1_ENHANCEMENTS.md → Archive
QUERY_EXECUTION_PATH_ANALYSIS.md → Obsolete
RUST_FIELD_PROJECTION.md → Implemented
RUST_FIRST_CACHING_STRATEGY.md → Move to docs/performance/
RUST_FIRST_IMPLEMENTATION_PROGRESS.md → Archive
RUST_FIRST_PIPELINE.md → Implemented
RUST_FIRST_SIMPLIFICATION.md → Implemented
RUST_PIPELINE_IMPLEMENTATION_GUIDE.md → Archive
RUST_PIPELINE_SUMMARY.md → Archive
UNIFIED_RUST_ARCHITECTURE_PLAN.md → Archive (reference doc)
V1_ADVANCED_PATTERNS.md → Review & integrate
V1_COMPONENT_PRDS.md → Archive
V1_DOCS_MAP.md → Regenerate for current docs
V1_DOCUMENTATION_PLAN.md → Archive
V1_NEXT_STEPS.md → Review & update
V1_PATTERN_UPDATE_SUMMARY.md → Archive
V1_SYNTHESIS_SUMMARY.md → Archive
V1_VISION.md → Update or archive
DATA_FLOW_VISUAL.txt → Archive
RUST_PIPELINE_VISUAL.txt → Archive
```

#### Archive Deleted Docs (Already Done):
```
docs-v1-archive/ → Already archived (150+ files) ✓
```

#### Keep & Update:
```
README.md → ✅ Already updated for unified architecture
CHANGELOG.md → ✅ Keep
CONTRIBUTING.md → ✅ Keep
ENTERPRISE.md → ✅ Keep
FRAMEWORK_SUBMISSION_GUIDE.md → ✅ Keep
MIGRATION_COMPETITIVE_ANALYSIS.md → ✅ Keep
TABLE_NAMING_CONVENTIONS.md → ✅ Move to docs/core/
THEORETICAL_OPTIMAL_ARCHITECTURE.md → Review if still relevant
```

---

### 3. Tests - Update or Delete

#### DELETE (Old architecture tests):
```
tests/test_pure_passthrough_rust.py
tests/test_pure_passthrough_sql.py
tests/regression/json_passthrough/ (entire directory)
tests/unit/core/json_handling/ (entire directory, already deleted ✓)
```

#### UPDATE (Adapt to Rust-first):
```
tests/integration/database/repository/*.py → Update assertions for RustResponseBytes
tests/unit/repository/*.py → Update for unified methods
tests/core/test_jsonb_network_casting_fix.py → Verify still relevant
```

#### NEW TESTS NEEDED:
```
tests/integration/test_unified_rust_pipeline.py → Test Rust-first execution path
tests/unit/core/test_rust_response_bytes.py → Test response type
tests/performance/test_rust_field_projection.py → Benchmark field filtering
```

---

### 4. Documentation Structure

#### Create/Update Key Docs:
```
docs/migration/v1-to-v2.md → NEW: Migration guide for unified architecture
docs/core/rust-pipeline.md → NEW: Document Rust-first execution
docs/core/queries-and-mutations.md → UPDATE: Remove old method signatures
docs/performance/index.md → UPDATE: Unified architecture performance
docs/architecture/rust-integration.md → NEW: How Rust components work
```

#### Archive Structure to Create:
```
archive/
├── planning/          # All *_PLAN.md, *_ARCHITECTURE.md files
├── analysis/          # All *_ANALYSIS.md files
├── progress/          # All *_PROGRESS.md, *_SUMMARY.md files
└── visual/           # All *.txt visual diagrams
```

---

### 5. Examples - Verify & Update

#### Check all example files:
```
examples/*/queries.py → Ensure using unified API (find(), not find_raw_json())
examples/*/models.py → Verify decorator usage
examples/blog_simple/README_TRINITY.md → Update or remove
examples/blog_simple/db/setup_trinity*.sql → Verify naming
```

---

## 📋 Recommended Execution Order

### Phase 1: Code Cleanup (Critical) 🚨

**Priority: HIGH | Estimated Time: 4-6 hours**

1. **Delete deprecated Python modules** (passthrough_mixin, raw_json_*, json_passthrough_repository)
2. **Refactor `db.py`** - Remove mode detection, simplify to Rust-only path
3. **Update response handlers** - Remove RawJSONResult, keep RustResponseBytes
4. **Update imports** across codebase
5. **Run tests** - Fix failures from API changes

**Deliverable:** Clean Python codebase with single execution path

---

### Phase 2: Documentation Cleanup 📚

**Priority: MEDIUM | Estimated Time: 2-3 hours**

1. **Create archive directories** (`archive/planning/`, `archive/analysis/`, etc.)
2. **Move obsolete planning docs** to archive
3. **Update core documentation** (queries-and-mutations.md, architecture/)
4. **Create migration guide** (v1-to-v2.md)
5. **Update V1_DOCS_MAP.md** to reflect current structure

**Deliverable:** Organized documentation with clear migration path

---

### Phase 3: Test Suite Update 🧪

**Priority: HIGH | Estimated Time: 3-4 hours**

1. **Delete old test files** (passthrough tests, json_passthrough directory)
2. **Update integration tests** for RustResponseBytes
3. **Add new tests** for unified pipeline
4. **Run full test suite** and validate

**Deliverable:** Passing test suite for unified architecture

---

### Phase 4: Examples & Polish ✨

**Priority: LOW | Estimated Time: 1-2 hours**

1. **Update all example projects** to use unified API
2. **Verify example READMEs** are accurate
3. **Check for remaining references** to old patterns
4. **Final documentation review**

**Deliverable:** Working examples with unified API

---

## 🎯 Success Metrics

After cleanup, you should have:
- ✅ **Zero references** to `PassthroughMixin`, `RawJSONResult`, `find_raw_json`
- ✅ **Single execution path**: `PostgreSQL → Rust → HTTP`
- ✅ **Clean root directory**: Only essential docs (README, CONTRIBUTING, CHANGELOG, ENTERPRISE)
- ✅ **Organized docs/**: Clear structure with migration guides
- ✅ **All tests passing** with unified architecture
- ✅ **Examples working** with new API

---

## ⚠️ Caution Areas

1. **`db.py` refactoring**: This is the most complex change. Consider doing it in small commits:
   - First: Remove mode detection
   - Second: Simplify find() methods
   - Third: Delete deprecated methods

2. **Breaking API changes**: Document all changes in `docs/migration/v1-to-v2.md`

3. **Test failures**: Some tests may need complete rewrites for the unified architecture

4. **Backward compatibility**: Consider deprecation warnings before complete removal

---

## 💡 Quick Start Guide

If you want to start immediately:

```bash
# 1. Archive planning docs
mkdir -p archive/{planning,analysis,progress,visual}
mv *_PLAN.md *_ARCHITECTURE.md archive/planning/
mv *_ANALYSIS.md archive/analysis/
mv *_PROGRESS.md *_SUMMARY.md archive/progress/
mv *.txt archive/visual/

# 2. Delete obvious deprecated code
rm src/fraiseql/repositories/passthrough_mixin.py
rm src/fraiseql/core/json_passthrough_repository.py
rm src/fraiseql/core/raw_json_executor.py
rm src/fraiseql/gql/raw_json_*.py
rm src/fraiseql/gql/json_executor.py
rm src/fraiseql/fastapi/raw_json_handler.py
rm src/fraiseql/graphql/passthrough_*.py

# 3. Delete old test files
rm tests/test_pure_passthrough_*.py
rm -rf tests/regression/json_passthrough/

# 4. Run tests to see what breaks
uv run pytest
```

---

## 📝 Progress Tracking

### Phase 1: Code Cleanup
- [ ] Delete deprecated Python modules
- [ ] Refactor db.py
- [ ] Update response_handlers.py
- [ ] Update dependencies.py
- [ ] Update query_router.py
- [ ] Update execute.py
- [ ] Update unified_executor.py
- [ ] Fix all imports
- [ ] Run tests and fix failures

### Phase 2: Documentation Cleanup
- [ ] Create archive directories
- [ ] Move planning docs to archive
- [ ] Move analysis docs to archive
- [ ] Move progress docs to archive
- [ ] Move visual diagrams to archive
- [ ] Create v1-to-v2 migration guide
- [ ] Update queries-and-mutations.md
- [ ] Create rust-pipeline.md
- [ ] Update performance docs
- [ ] Update V1_DOCS_MAP.md

### Phase 3: Test Suite Update
- [ ] Delete passthrough test files
- [ ] Delete json_passthrough test directory
- [ ] Update integration tests
- [ ] Create test_unified_rust_pipeline.py
- [ ] Create test_rust_response_bytes.py
- [ ] Create test_rust_field_projection.py
- [ ] Run full test suite
- [ ] Validate all tests pass

### Phase 4: Examples & Polish
- [ ] Update all example queries.py
- [ ] Update example READMEs
- [ ] Verify naming conventions
- [ ] Search for remaining old patterns
- [ ] Final documentation review
- [ ] Update CHANGELOG.md

---

## 🔍 Finding Remaining References

Use these commands to find lingering references to old patterns:

```bash
# Find PassthroughMixin references
rg "PassthroughMixin" --type py

# Find RawJSONResult references
rg "RawJSONResult" --type py

# Find old method names
rg "find_raw_json|find_one_raw_json|find_rust|find_one_rust" --type py

# Find mode detection
rg "_determine_mode|self.mode" --type py src/fraiseql/

# Find json_passthrough references
rg "json_passthrough" --type py
```

---

## 📞 Questions & Decisions

### Open Questions:
1. Should we keep `UNIFIED_RUST_ARCHITECTURE_PLAN.md` as reference or archive it?
2. Do we need deprecation warnings or can we do breaking changes?
3. Should we version bump to v2.0.0 for these changes?
4. Keep or archive `THEORETICAL_OPTIMAL_ARCHITECTURE.md`?

### Decisions Made:
- ✅ Use unified Rust-first architecture (no branching)
- ✅ Remove all passthrough-related code
- ✅ Archive planning docs instead of deleting
- ✅ Create migration guide for users

---

**Last Updated:** 2025-10-17
**Next Review:** After Phase 1 completion
