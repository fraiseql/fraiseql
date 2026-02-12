# Phase 4: Array Length & Full-Text Search Operators

## Objective
Implement array length comparison operators and full-text search operators for all 4 databases.

## Success Criteria
- [ ] Array length operators working: LenEq, LenNeq, LenGt, LenGte, LenLt, LenLte
- [ ] FTS operators working: Matches, PlainQuery, PhraseQuery, WebsearchQuery
- [ ] All 4 databases supported (custom SQL, database-specific syntax)
- [ ] 6 array ops + 4 FTS ops × 4 databases = 40 test cases passing
- [ ] `cargo clippy -p fraiseql-core` clean
- [ ] `cargo test -p fraiseql-core` passes

## Operators to Implement

### Array Length Operators
```
LenEq, LenNeq, LenGt, LenGte, LenLt, LenLte
```
Database-specific implementations:
```
PostgreSQL: array_length(...::text[], 1) = N
MySQL: JSON_LENGTH(...) = N
SQLite: json_array_length(...) = N
SQL Server: (SELECT COUNT(*) FROM OPENJSON(...)) = N
```

### Full-Text Search Operators
```
Matches, PlainQuery, PhraseQuery, WebsearchQuery
```
Database-specific implementations:
```
PostgreSQL: to_tsvector(...) @@ plainto_tsquery(...)
MySQL: MATCH(...) AGAINST(...IN BOOLEAN MODE)
SQLite: ... MATCH ... (FTS5 virtual table)
SQL Server: CONTAINS(...)
```

## TDD Cycles

### Cycle 1: Implement Array Length Operators

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test for array length comparisons
- **GREEN**: Add routing in `operator_to_sql()`:
  ```rust
  WhereOperator::LenEq => Self::generate_array_length_sql(db_type, "=", field_sql, value),
  WhereOperator::LenGt => Self::generate_array_length_sql(db_type, ">", field_sql, value),
  // ... all array length operators
  ```
- **REFACTOR**: Create helper function for database-specific SQL generation
- **CLEANUP**: Test edge cases (empty arrays, null), commit

### Cycle 2: Implement Full-Text Search Operators

**File**: `crates/fraiseql-core/src/db/where_sql_generator.rs`

- **RED**: Write test for FTS queries
- **GREEN**: Add routing in `operator_to_sql()`:
  ```rust
  WhereOperator::Matches => Self::generate_fts_sql(db_type, "plain", field_sql, value),
  WhereOperator::PlainQuery => Self::generate_fts_sql(db_type, "plain", field_sql, value),
  // ... all FTS operators
  ```
- **REFACTOR**: Handle special character escaping per database
- **CLEANUP**: Test with unicode and special characters, commit

### Cycle 3: Comprehensive Testing

**File**: `crates/fraiseql-core/tests/operators_array_fts.rs` (new file)

- **RED**: Write matrix of 40 test cases
- **GREEN**: Verify each produces valid database-specific SQL
- **REFACTOR**: Add integration tests with actual array/FTS data
- **CLEANUP**: All 40 tests pass, commit

## Dependencies
- Requires Phase 0 (Template Integration) ✓
- Independent of Phases 2-3, 5-9

## Status
[ ] Not Started
