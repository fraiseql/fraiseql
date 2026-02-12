# Phase 10: Python Operator Cleanup & Verification

## Objective
Remove Python operator code now that Rust implementation is complete. Verify all functionality is covered by Rust.

## Success Criteria
- [ ] All Python operator modules removed (`/src/fraiseql/sql/operators/`)
- [ ] All Python where_generator cleanup (keep only schema compilation, remove runtime SQL gen)
- [ ] `db.py` and dependencies removed
- [ ] All imports updated to use Rust WhereSqlGenerator
- [ ] All existing tests still pass
- [ ] No broken imports or references
- [ ] `uv run ruff check` clean
- [ ] `cargo clippy` clean

## Python Code to Remove

### Files to Delete
- `src/fraiseql/sql/operators/` (entire directory, 24 files, ~2,400 lines)
- `src/fraiseql/core/db.py` (v1 legacy, ~500 lines)
- Associated imports and registrations

### Files to Update
- `src/fraiseql/sql/where_clause.py` - remove operator registry calls
- `src/fraiseql/sql/where_generator.py` - schema compilation only, no runtime SQL generation
- Test files referencing operator strategies

### Architecture After Cleanup
```
GraphQL → where_sql_generator.rs (Rust, 100% complete) → SQL
         (Python removed entirely from query path)
         (Python kept only for schema authoring/compilation)
```

## TDD Cycles

### Cycle 1: Verify Rust Coverage Complete

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Run comprehensive test suite comparing Rust output to old Python behavior
- **GREEN**: Verify Rust produces equivalent or better SQL
- **REFACTOR**: Document any intentional differences
- **CLEANUP**: All verification tests pass

### Cycle 2: Remove Python Operator Code

**File**: Multiple Python files

- **RED**: Document what's being removed and why
- **GREEN**: Delete `/src/fraiseql/sql/operators/` directory
- **REFACTOR**: Update imports in remaining files
- **CLEANUP**: Verify no broken imports, commit

### Cycle 3: Clean Up Dependencies

**File**: Multiple Python files

- **RED**: Identify all Python operator dependencies
- **GREEN**: Remove db.py and update where_clause.py imports
- **REFACTOR**: Ensure GraphQL query execution routes to Rust
- **CLEANUP**: All tests pass, commit

### Cycle 4: Final Verification

**File**: Test suite

- **RED**: Run full test suite (Python and Rust)
- **GREEN**: All tests pass with Python operators removed
- **REFACTOR**: Update test assertions for Rust behavior
- **CLEANUP**: Full suite passes, commit

## Dependencies
- **Requires**: Phases 2-5 complete (all operators implemented in Rust)
- **Blocks**: Phase 11 (Finalize depends on cleanup complete)

## Status
[ ] Not Started
