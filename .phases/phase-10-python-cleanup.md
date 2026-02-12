# Phase 10: Python Runtime Cleanup & Verification

## Objective
Remove Python runtime code (operators, query execution) while keeping Python available as optional schema authoring language.

## Success Criteria
- [ ] All Python operator modules removed (`/src/fraiseql/sql/operators/`)
- [ ] Python WHERE clause generation removed (no runtime SQL gen)
- [ ] `db.py` and v1 legacy code removed
- [ ] All runtime query execution routes to Rust only
- [ ] Python schema authoring tools preserved (compile to IntermediateSchema)
- [ ] All existing tests still pass
- [ ] No broken imports in runtime path
- [ ] `uv run ruff check` clean (for schema authoring tools only)
- [ ] `cargo clippy` clean

## Python Code to Remove (Runtime Only)

### Files to Delete
- `src/fraiseql/sql/operators/` (entire directory, 24 files, ~2,400 lines)
- `src/fraiseql/core/db.py` (v1 legacy, ~500 lines)
- Runtime SQL generation code from `where_generator.py`
- Associated operator registry and strategies

### Files to Update
- `src/fraiseql/sql/where_clause.py` - remove operator registry calls
- `src/fraiseql/sql/where_generator.py` - keep schema compilation, remove runtime SQL generation
- Test files referencing operator strategies

### Python Code to Preserve (Authoring)
- Schema authoring/definition tools (if users want Python for this)
- Schema compilation (Python → IntermediateSchema)
- Configuration parsing (TOML, etc.)
- **These are optional; Rust can also handle schema input if users prefer**

### Architecture After Cleanup
```
Schema (Any Language)     Python (optional authoring tool)
         ↓                       ↓
  IntermediateSchema ←───────────┘
         ↓
  Rust Compiler
         ↓
  GraphQL + TOML Config
         ↓
  User Query (GraphQL)
         ↓
  where_sql_generator.rs (Rust Runtime, 100%)
         ↓
  Database SQL
```

**Key Point**: Python is removed from query execution path. Schema authoring is polyglot; Python is one optional choice, not the only one.

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
