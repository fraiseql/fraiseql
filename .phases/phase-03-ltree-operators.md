# Phase 3: LTree Operators

## Objective
Implement PostgreSQL-specific LTree operators for hierarchical data queries.

## Success Criteria
- [ ] All LTree operators implemented: AncestorOf, DescendantOf, MatchesLquery, MatchesLtxtquery, DepthEq, Lca, etc.
- [ ] PostgreSQL-only gating: proper error messages for other databases
- [ ] 12 operators × 1 database (PostgreSQL only) = 12 test cases passing
- [ ] Integration tests execute on actual PostgreSQL LTree data
- [ ] `cargo clippy -p fraiseql-core` clean
- [ ] `cargo test -p fraiseql-core` passes

## Operators to Implement
```
AncestorOf, DescendantOf, MatchesLquery, MatchesLtxtquery,
MatchesAnyLquery, DepthEq, DepthNeq, DepthGt, DepthGte,
DepthLt, DepthLte, Lca
```

## TDD Cycles

### Cycle 1: Add LTree Operator Routing with PostgreSQL Gating

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test expecting LTree operators to work on PostgreSQL, return error on other databases
- **GREEN**: Add routing in `operator_to_sql()`:
  ```rust
  WhereOperator::AncestorOf => {
      if db_type != DatabaseType::PostgreSQL {
          return Err(error::ltree_not_supported(db_type));
      }
      Self::apply_template(db_type, "ancestorOf", field_sql, value)
  }
  // ... all LTree operators with similar gating
  ```
- **REFACTOR**: Extract gating logic into helper function
- **CLEANUP**: Verify error messages are helpful, commit

### Cycle 2: Handle LTree Array Operations

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test for array-based LTree operations (e.g., array of paths)
- **GREEN**: Implement array handling in template application
- **REFACTOR**: Ensure ltree[] syntax is correct
- **CLEANUP**: Test and commit

### Cycle 3: Test LTree Operators on PostgreSQL

**File**: `crates/fraiseql-core/tests/operators_ltree.rs` (new file)

- **RED**: Write 12 test cases (one per operator)
- **GREEN**: Verify each produces valid PostgreSQL LTree SQL
- **REFACTOR**: Add integration tests with actual LTree hierarchies
- **CLEANUP**: Verify all 12 tests pass, commit

## Dependencies
- Requires Phase 0 (Template Integration) ✓
- Requires PostgreSQL database for integration tests
- Independent of Phases 2, 4-5, 6-9

## Status
[ ] Not Started
