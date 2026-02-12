# Phase 7: Arrow Subscription Filter Matching

## Objective
Implement expression-based filter evaluation for Arrow subscriptions to support predicates like `"total > 50"`.

## Success Criteria
- [ ] `matches_filter()` evaluates simple predicates against `HistoricalEvent.data`
- [ ] Supported operators: `=`, `!=`, `>`, `>=`, `<`, `<=`
- [ ] Supported value types: strings, numbers, booleans
- [ ] Nested field access via dot notation: `"address.city = 'Paris'"`
- [ ] Filter syntax errors return `true` (fail open — don't silently drop events)
- [ ] `cargo clippy -p fraiseql-arrow` clean
- [ ] `cargo test -p fraiseql-arrow` passes

## TDD Cycles

### Cycle 1: Parse Filter Expressions

**File**: `crates/fraiseql-arrow/src/subscription.rs`

- **RED**: Write test for parsing simple predicates
- **GREEN**: Implement filter parser:
  ```rust
  fn parse_filter(filter: &str) -> Option<(String, String, String)> {
      // Parse "field = 'value'" into ("field", "=", "value")
      // Parse "nested.field > 42" into ("nested.field", ">", "42")
  }
  ```
- **REFACTOR**: Handle all 6 operators (=, !=, >, >=, <, <=)
- **CLEANUP**: Test malformed filters, commit

### Cycle 2: Evaluate Predicates Against Data

**File**: `crates/fraiseql-arrow/src/subscription.rs`

- **RED**: Write test for `matches_filter()` with actual event data
- **GREEN**: Implement evaluation:
  ```rust
  fn matches_filter(event: &HistoricalEvent, filter: &Option<String>) -> bool {
      if let Some(filter_str) = filter {
          let (field, op, value) = parse_filter(filter_str)?;
          let field_value = event.data.get_path(&field)?;
          evaluate_comparison(field_value, op, value)
      } else {
          true  // No filter = accept all
      }
  }
  ```
- **REFACTOR**: Handle type conversions (string, number, boolean)
- **CLEANUP**: Test edge cases, commit

### Cycle 3: Test Filter Matching

**File**: `crates/fraiseql-arrow/tests/subscription_filters.rs`

- **RED**: Write comprehensive test matrix
- **GREEN**: Verify filters work correctly
- **REFACTOR**: Add integration tests with real subscriptions
- **CLEANUP**: All tests pass, commit

## Dependencies
- None (independent of all other phases)

## Status
[ ] Not Started
