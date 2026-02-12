# Phase 2: Network Operators

## Objective
Implement network-specific WHERE operators (IPv4/IPv6 detection, subnet checks, etc.) using the template infrastructure from Phase 0.

## Success Criteria
- [ ] All network operators implemented: IsIPv4, IsIPv6, IsPrivate, IsPublic, IsLoopback, InSubnet, ContainsIP, etc.
- [ ] All 4 databases supported (templates exist for each)
- [ ] 15 operators × 4 databases = 60 test cases passing
- [ ] Integration tests execute actual SQL on each database
- [ ] `cargo clippy -p fraiseql-core` clean
- [ ] `cargo test -p fraiseql-core` passes

## Operators to Implement
```
IsIPv4, IsIPv6, IsPrivate, IsPublic, IsLoopback, IsLinkLocal,
IsMulticast, IsDocumentation, IsCarrierGrade, InSubnet,
ContainsSubnet, ContainsIP, Overlaps, StrictlyLeft, StrictlyRight
```

## TDD Cycles

### Cycle 1: Add Network Operator Routing

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test expecting network operators to produce SQL with database-specific functions (inet check for PostgreSQL, INET() for MySQL, etc.)
- **GREEN**: Add routing in `operator_to_sql()` for each network operator:
  ```rust
  WhereOperator::IsIPv4 => Self::apply_template(db_type, "isIPv4", field_sql, value),
  WhereOperator::IsIPv6 => Self::apply_template(db_type, "isIPv6", field_sql, value),
  // ... all network operators
  ```
- **REFACTOR**: Extract common patterns into helper functions
- **CLEANUP**: Run clippy, commit

### Cycle 2: Handle Database-Specific Behavior

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test for unsupported database × operator combinations
- **GREEN**: Add validation to check template existence:
  ```rust
  fn apply_template(...) -> Result<String> {
      let template = Self::get_template_for_operator(db_type, operator_name)
          .ok_or_else(|| error::unsupported_operator(operator_name, db_type))?;
      // ...
  }
  ```
- **REFACTOR**: Improve error messages for unsupported combinations
- **CLEANUP**: Test and commit

### Cycle 3: Test Network Operators Comprehensively

**File**: `crates/fraiseql-core/tests/operators_network.rs` (new file)

- **RED**: Write matrix of tests: 15 operators × 4 databases
- **GREEN**: Verify each operator produces valid SQL for its database
- **REFACTOR**: Add integration tests (execute on actual databases if available)
- **CLEANUP**: Verify all 60 test cases pass, commit

## Dependencies
- Requires Phase 0 (Template Integration) ✓
- Independent of Phases 3-5, 6-9

## Status
[ ] Not Started
