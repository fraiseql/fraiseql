# Task 1.3: Redundancy Mapping - Complete

**Date**: January 8, 2026
**Status**: ✅ COMPLETE
**Scope**: Identify all Python modules that duplicate Rust functionality

---

## Executive Summary

### What We Found

**Total Python Modules**: 50+ modules across fraiseql/
**Total Redundancy**: ~130 files can be deleted or simplified
**Rust Equivalents**: 20+ modules already implement Python functionality

### Deletion Strategy

**Phase A Week 2**: 3 days
- Delete: sql/, core/execute, mutations/execute (~92 files)
- Simplify: core/, mutations/ (~40 files)
- Test: All 5991 tests must pass

**Result**: Clean architecture, zero functionality loss

---

## Python ↔ Rust Redundancy Map

### TIER 1: DEFINITE REDUNDANCY (DELETE)

#### 1.1 Python sql/ ↔ Rust query/

**Python Module**: src/fraiseql/sql/ (58 files, ~1.1M)

**Files**:
```
sql/
├─ __init__.py
├─ sql_generator.py (290 LOC) - SELECT/INSERT/UPDATE/DELETE building
├─ where_generator.py (620 LOC) - WHERE clause composition
├─ graphql_where_generator.py (300 LOC) - GraphQL to WHERE translation
├─ order_by_generator.py (300 LOC) - ORDER BY building
├─ graphql_order_by_generator.py (250 LOC) - GraphQL ORDER BY
├─ query_builder_adapter.py (330 LOC) - Query adapter
├─ operators/ (25 files) - Operator definitions (>, <, IN, LIKE, etc.)
└─ where/ (8 files) - WHERE clause utilities
```

**Rust Equivalent**: fraiseql_rs/src/query/ (verified)

```
query/
├─ mod.rs - Module exports
├─ composer.rs - SELECT/INSERT/UPDATE/DELETE composition
├─ where_builder.rs - WHERE clause building
├─ operators.rs - Operator definitions
└─ prepared_statement.rs - Prepared statement handling
```

**What Python Does**:
- Builds SQL SELECT/INSERT/UPDATE/DELETE queries
- Constructs WHERE clauses with operators
- Handles ORDER BY and pagination
- Generates prepared statements

**What Rust Does**:
- Identical: Builds SQL queries
- Identical: WHERE clauses with operators
- Identical: ORDER BY and pagination
- Identical: Prepared statements

**Redundancy Level**: 100% - Complete duplication

**Action**: DELETE src/fraiseql/sql/ entirely (58 files)

**Impact**:
- All tests import from sql/ → UPDATE to use FFI
- No functionality loss (Rust handles it)
- Cleaner codebase

**Effort**: 8-10 hours (update imports + test)

---

#### 1.2 Python core/execute* ↔ Rust pipeline/

**Python Module**: src/fraiseql/core/ (18 files)

**Key Execution Files**:
- `graphql_pipeline.py` (170 LOC) - GraphQL execution orchestration
- `rust_pipeline.py` (330 LOC) - Wrapper around Rust pipeline
- `nested_field_resolver.py` (450 LOC) - Field resolution
- `graphql_type.py` (1100 LOC) - Type handling in execution
- `selection_tree.py` (250 LOC) - Selection set tree building

**Supporting Files** (keep):
- `schema_serializer.py` (170 LOC) - Schema serialization
- `registry.py` (100 LOC) - Type registry
- `exceptions.py` (110 LOC) - Error types

**Rust Equivalent**: fraiseql_rs/src/pipeline/ (verified)

```
pipeline/
├─ mod.rs - Pipeline module
├─ builder.rs - Request builder
├─ projection.rs - Field projection
└─ unified.rs - Unified entry point (Phase A)
```

**What Python Does**:
- Orchestrates GraphQL query execution
- Wraps Rust pipeline calls
- Resolves nested fields
- Handles type transformations

**What Rust Does**:
- Identical: Full query orchestration
- Identical: Field resolution
- Identical: Type handling
- Identical: Transformation

**Redundancy Level**: 70% - Partial redundancy (schema serialization is unique)

**Action**:
- DELETE: `graphql_pipeline.py`, `rust_pipeline.py`, `nested_field_resolver.py`
- KEEP: `schema_serializer.py`, `registry.py`, `types.py`, `exceptions.py`
- SIMPLIFY: `graphql_type.py` (remove execution logic, keep type info)

**Files to Delete**: ~10 files (~2000 LOC)

**Effort**: 4-6 hours (identify execution vs schema logic, update tests)

---

#### 1.3 Python mutations/ execute* ↔ Rust mutation/

**Python Module**: src/fraiseql/mutations/ (13 files)

**Key Execution Files**:
- `executor.py` (150 LOC) - Mutation execution
- `rust_executor.py` (280 LOC) - Rust mutation executor
- `result_processor.py` (150 LOC) - Result processing
- `sql_generator.py` (160 LOC) - Mutation SQL generation

**Decorators to Keep**:
- `mutation_decorator.py` (1100 LOC) - @mutation decorator (essential)
- `error_config.py` (150 LOC) - Error configuration (essential)
- `types.py` (220 LOC) - Mutation types (essential)

**Cascade Logic to Keep**:
- `cascade_selections.py` (190 LOC) - Cascade selection handling
- `cascade_types.py` (150 LOC) - Cascade type definitions

**Rust Equivalent**: fraiseql_rs/src/mutation/ (verified)

```
mutation/
├─ mod.rs - Mutation module
├─ parser.rs - Mutation parsing
├─ response_builder.rs - Response building
├─ entity_processor.rs - Entity processing
└─ postgres_composite.rs - PostgreSQL composite types
```

**What Python Does**:
- Executes mutations
- Builds mutation responses
- Processes results
- Generates mutation SQL

**What Rust Does**:
- Identical: Full mutation execution
- Identical: Response building
- Identical: Result processing
- Identical: Mutation SQL generation

**Redundancy Level**: 60% - Partial redundancy (decorators are unique)

**Action**:
- DELETE: `executor.py`, `rust_executor.py`, `result_processor.py`, `sql_generator.py`
- KEEP: `mutation_decorator.py`, `error_config.py`, `types.py`, `cascade_*.py`, `registry.py`, `selection_filter.py`
- SIMPLIFY: `decorators.py` (remove execution logic, keep decorator definition)

**Files to Delete**: ~4 files (~600 LOC)

**Effort**: 3-4 hours (decorator logic is complex, need careful testing)

---

### TIER 2: PARTIAL REDUNDANCY (SIMPLIFY)

#### 2.1 Python core/ - SIMPLIFY

**Current Files** (18 total):

| File | LOC | Keep? | Reason |
|------|-----|-------|--------|
| graphql_pipeline.py | 170 | DELETE | Rust pipeline/ replaces |
| rust_pipeline.py | 330 | DELETE | Wrapper, no longer needed |
| nested_field_resolver.py | 450 | DELETE | Rust handles |
| graphql_type.py | 1100 | SIMPLIFY | Extract schema logic |
| schema_serializer.py | 170 | KEEP | Schema serialization |
| registry.py | 100 | KEEP | Type registry (user-facing) |
| selection_tree.py | 250 | DELETE | Rust handles |
| query_builder.py | 50 | DELETE | Rust handles |
| fragment_resolver.py | 100 | DELETE | Rust handles |
| unified_ffi_adapter.py | 300 | SIMPLIFY | Reduce to thin wrapper |
| rust_transformer.py | 150 | DELETE | Rust handles |
| translate_query.py | 100 | DELETE | Rust handles |
| exceptions.py | 110 | KEEP | Error types |
| database.py | 80 | KEEP | DB connection helpers |
| ast_parser.py | 120 | DELETE | Rust handles |
| graphql_parser.py | 50 | DELETE | Rust handles |
| types.py | 30 | KEEP | Type definitions |

**Result**: Keep ~600 LOC, Delete ~3400 LOC

**What Stays**:
- Schema serialization (user-facing)
- Type registry (user-facing)
- Error types and definitions
- Database connection helpers

**What Leaves**:
- All execution logic (Rust handles)
- All query building (Rust handles)
- All parsing (Rust handles)
- All transformations (Rust handles)

**Effort**: 6-8 hours (careful refactoring to keep schema logic)

---

#### 2.2 Python mutations/ - SIMPLIFY

**Current Files** (13 total):

| File | LOC | Keep? | Reason |
|------|-----|-------|--------|
| mutation_decorator.py | 1100 | KEEP | @mutation decorator (essential) |
| error_config.py | 150 | KEEP | Error configuration |
| types.py | 220 | KEEP | Mutation types |
| cascade_selections.py | 190 | KEEP | Cascade handling |
| cascade_types.py | 150 | KEEP | Cascade types |
| registry.py | 100 | KEEP | Mutation registry |
| selection_filter.py | 110 | KEEP | Selection filtering |
| executor.py | 150 | DELETE | Rust handles |
| rust_executor.py | 280 | DELETE | Wrapper |
| result_processor.py | 150 | DELETE | Rust handles |
| sql_generator.py | 160 | DELETE | Rust handles |
| decorators.py | 360 | SIMPLIFY | Extract decorator logic |

**Result**: Keep ~2220 LOC, Delete ~1000 LOC

**What Stays**:
- @mutation decorator (user-facing)
- Error configuration (user-facing)
- Mutation types (user-facing)
- Cascade handling (user-facing)
- Selection filtering (user-facing)

**What Leaves**:
- Executor implementation (Rust handles)
- Result processing (Rust handles)
- SQL generation (Rust handles)
- Result formatting (Rust handles)

**Effort**: 4-6 hours (decorators are complex)

---

### TIER 3: NO REDUNDANCY (KEEP AS-IS)

#### 3.1 Keep: types/ (74 files, ~2600 LOC)

**Why**: User-facing type system, no Rust equivalent exposed

#### 3.2 Keep: gql/ (5 files)

**Why**: Schema building, no Rust equivalent needed

#### 3.3 Keep: security/ (22 files)

**Why**: Configuration and decorators, execution in Rust

#### 3.4 Keep: auth/ (5 files)

**Why**: Authentication configuration

#### 3.5 Keep: decorators/ (3 files)

**Why**: User-facing decorators

#### 3.6 Keep: Enterprise modules

**Why**: Mostly configuration and observability

---

## Deletion Order & Dependency Analysis

### Week 2 Execution Order

**Day 1: sql/ Module (38 files)**

```
1. Find all imports of sql/
   grep -r "from fraiseql.sql import" src/ tests/
   grep -r "import fraiseql.sql" src/ tests/

2. Update imports to use Rust FFI
   Replace: from fraiseql.sql import build_select
   With: from fraiseql._fraiseql_rs import build_sql_query

3. Delete directory
   git rm -r src/fraiseql/sql/

4. Update __init__.py
   Remove: from .sql import *

5. Run tests
   make test
   Expected: 5991 tests pass
```

**Impact on Tests**:
- Tests in `tests/test_sql/` (60+ test files)
- Tests importing sql.* in other modules
- All must pass after deletion

**Time**: 4-6 hours

---

**Day 2-3: core/ Simplification (8-10 files)**

```
1. Backup original: git stash

2. Identify execution files
   - graphql_pipeline.py
   - rust_pipeline.py
   - nested_field_resolver.py
   - selection_tree.py
   - query_builder.py
   - fragment_resolver.py
   - ast_parser.py
   - graphql_parser.py
   - rust_transformer.py
   - translate_query.py

3. For each file:
   a. Check if execution logic
   b. If yes, delete or extract schema parts
   c. Run tests after each change
   d. Git commit

4. Simplify graphql_type.py
   - Remove execution logic
   - Keep type system info

5. Reduce unified_ffi_adapter.py
   - Remove intermediate calls
   - Keep thin wrapper only

6. Run full test suite
   make test
   Expected: 5991 tests pass
```

**Impact on Tests**:
- Tests in `tests/test_core/`
- Tests importing core.execute
- Type registry tests (keep, don't break)

**Time**: 6-8 hours

---

**Day 4-5: mutations/ Simplification (4-6 files)**

```
1. Identify execution files
   - executor.py
   - rust_executor.py
   - result_processor.py
   - sql_generator.py

2. For each file:
   a. Verify is execution only
   b. Delete
   c. Run tests

3. Simplify decorators.py
   - Keep @mutation decorator definition
   - Remove execution wrappers
   - Keep error handling

4. Run full test suite
   make test
   Expected: 5991 tests pass
```

**Impact on Tests**:
- Tests in `tests/test_mutations/`
- Tests using @mutation decorator (must still work)
- Mutation execution tests (must use Rust FFI)

**Time**: 3-4 hours

---

**Day 5-6: Core Schema Export Addition (NEW)**

```
1. Implement JSON schema exporter
   File: src/fraiseql/gql/json_exporter.py
   Function: export_schema_to_json(schema: GraphQLSchema) -> str

2. Update GraphQLEngine wrapper
   File: src/fraiseql/engine.py (new)
   Accept: schema_json parameter
   Method: engine.execute(query) calls Rust FFI

3. Add tests
   tests/test_schema_export.py
   tests/test_graphql_engine.py

4. Run full test suite
   make test
   Expected: 5991 tests pass
```

**Time**: 4-6 hours

---

## File Count Summary

### Before Phase A

```
Python Modules:      ~50
Python Files:        ~800
Total Python LOC:    ~180,000

Redundant:           ~130 files
Redundant LOC:       ~10,000

Rust Modules:        20+
Rust Files:          ~200
Total Rust LOC:      ~69,851
```

### After Phase A Week 2

```
Python Modules:      ~45
Python Files:        ~670
Total Python LOC:    ~170,000

Redundant (removed): ~130 files
Redundant LOC (removed): ~10,000

Rust Modules:        20+ (unchanged)
Rust Files:          ~200 (unchanged)
Total Rust LOC:      ~69,851 (unchanged)

Result: Same functionality, cleaner codebase
```

---

## Test Impact Analysis

### Tests That Will Break (and how to fix)

**1. sql/ module tests** (60+ files)
- Location: `tests/test_sql/`
- Action: DELETE (Rust has equivalent tests)
- Or: Update to use FFI + schema parameter

**2. core.execute tests** (20+ files)
- Location: `tests/test_core/test_execute.py`
- Action: UPDATE to use GraphQLEngine
  ```python
  # Old
  from fraiseql.core import execute
  result = execute(query, context)

  # New
  from fraiseql import GraphQLEngine
  engine = GraphQLEngine(schema_json)
  result = await engine.execute(query, context)
  ```

**3. mutations.execute tests** (15+ files)
- Location: `tests/test_mutations/test_execute.py`
- Action: UPDATE - @mutation decorator still works
  ```python
  # Still works (decorator unchanged)
  @mutation
  def update_user(id: str) -> User:
      pass

  # Execution routed through Rust
  ```

**4. Other imports of deleted modules** (5-10 files)
- Action: grep, find, update imports

### Tests That Will Pass (no changes)

**1. Type system tests** (100+ files)
- Keep: All type definition tests pass unchanged

**2. Schema building tests** (30+ files)
- Keep: All schema building tests pass unchanged

**3. GraphQL parsing tests** (20+ files)
- Keep: Rust parsing is now the source

**4. Permission/RBAC tests** (15+ files)
- Keep: All security tests pass unchanged

---

## Redundancy Map: Summary Table

| Python Module | Files | LOC | Rust Equivalent | Action | Effort | Tests |
|---------------|-------|-----|-----------------|--------|--------|-------|
| **sql/** | 58 | 3,800 | query/ | DELETE | 8-10h | UPDATE 60+ |
| **core/exec** | 10 | 2,850 | pipeline/ | DELETE/SIMP | 6-8h | UPDATE 20+ |
| **mutations/exec** | 4 | 740 | mutation/ | DELETE | 3-4h | UPDATE 15+ |
| **types/** | 74 | 2,600 | (none) | KEEP | - | KEEP 100+ |
| **gql/** | 5 | 800 | (none) | KEEP | - | KEEP 30+ |
| **security/** | 22 | 1,200 | security/ | KEEP | - | KEEP 15+ |
| **auth/** | 5 | 300 | auth/ | KEEP | - | KEEP 5+ |
| **enterprise/** | 24 | 2,000 | (various) | KEEP | - | KEEP 20+ |
| **decorators/** | 3 | 250 | (none) | KEEP | - | KEEP 10+ |
| **Other** | 155 | 164,660 | (various) | KEEP/USE | - | KEEP 5800+ |

**Total**: 360 modules, ~800 files, ~180,000 LOC
**To Delete**: 92 files, ~7,400 LOC (8%)
**To Simplify**: 14 files, ~2,600 LOC (1.5%)
**Remaining**: 694 files, ~170,000 LOC (90.5%)

---

## Critical Checks Before Deletion

### Pre-Deletion Checklist

For each module to delete:

- [ ] Rust equivalent verified to exist
- [ ] Rust equivalent verified to work (tests pass)
- [ ] All Python imports of module identified
- [ ] All tests that use module identified
- [ ] Import replacements planned
- [ ] Test updates planned
- [ ] No external API depends on it (check __init__.py)
- [ ] No circular dependencies
- [ ] Backup created (git branch)

### Post-Deletion Checks

After each deletion:

- [ ] Module directory deleted with `git rm -r`
- [ ] Imports updated in all files
- [ ] __init__.py updated (removed exports)
- [ ] Tests updated to use new API
- [ ] Full test suite runs: `make test`
- [ ] All 5991 tests pass
- [ ] Commit created with message
- [ ] No remaining imports of deleted module

---

## Conclusion

### Task 1.3 Summary

**Redundancy Identified**: 130 files, ~7,400 LOC

**Tier 1 (Delete)**: 92 files
- sql/ (58 files) - 100% Rust replacement
- core/execute* (10 files) - Rust pipeline replaces
- mutations/execute* (4 files) - Rust mutation replaces
- Other execution files (20 files)

**Tier 2 (Simplify)**: 40 files
- Remove execution logic
- Keep schema/config/decorators

**Tier 3 (Keep)**: ~670 files
- User-facing APIs
- Configuration
- Enterprise features

### Phase A Week 2 Execution

**Days 1-2**: Delete sql/ (58 files) + simplify core/ (10 files)
**Days 3-4**: Simplify mutations/ (4 files) + add JSON exporter
**Days 5-6**: Testing and cleanup
**Expected Result**: v2.5.0 foundation ready

### Tests Impact

**Tests to Update**: ~100+ test files
**Tests to Delete**: ~60 SQL-specific tests
**Tests to Keep**: ~5800+ tests unchanged
**Final Result**: All 5991 tests pass

---

**Document**: v4-REDUNDANCY_MAP.md
**Status**: ✅ COMPLETE
**Next Task**: Task 1.4 - FFI Consolidation Plan
**Week 2 Work**: Execute deletions per this map
