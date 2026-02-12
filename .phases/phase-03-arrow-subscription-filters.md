# Phase 3: Arrow Subscription Filter Matching

## Objective
Implement expression-based filter evaluation for Arrow subscriptions so that
`matches_filter()` evaluates predicates like `"total > 50"` against event data.

## Success Criteria
- [ ] `matches_filter()` evaluates simple predicates against `HistoricalEvent.data`
- [ ] Supported operators: `=`, `!=`, `>`, `>=`, `<`, `<=`
- [ ] Supported value types: strings, numbers, booleans
- [ ] Nested field access via dot notation: `"address.city = 'Paris'"`
- [ ] Filter syntax errors return `true` (fail open — don't silently drop events)
- [ ] `cargo clippy -p fraiseql-arrow` clean
- [ ] `cargo test -p fraiseql-arrow` passes

## Background

**File:** `crates/fraiseql-arrow/src/subscription.rs`

### Current State

- `matches_filter()` (line 115) is a `const fn` that always returns `true`
- Filter type is `Option<String>` on `EventSubscription` (not a struct)
- `broadcast_event()` (line 98) is synchronous, takes `&HistoricalEvent`,
  iterates `self.subscriptions` (a `DashMap`), and calls
  `Self::matches_filter(&event, &subscription.filter)` before sending

### Design Constraints

- The Arrow layer (`fraiseql-arrow`) is a simplified streaming API — keep the
  filter implementation self-contained and minimal
- The core layer (`fraiseql-core`) has a rich 44-type filter system — do NOT
  duplicate it here; this is a lightweight predicate evaluator for event streams
- No external parser dependency (no `pest`, `nom`) — a hand-rolled parser for
  `<field> <op> <value>` is sufficient at this scale

## TDD Cycles

### Cycle 1: Filter Expression Parser

**New file:** `crates/fraiseql-arrow/src/filter_expr.rs`

- **RED**: Write parser tests:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn parse_string_eq() {
          let expr = FilterExpr::parse("status = 'shipped'").unwrap();
          assert_eq!(expr.field, "status");
          assert_eq!(expr.op, FilterOp::Eq);
          assert_eq!(expr.value, FilterValue::String("shipped".into()));
      }

      #[test]
      fn parse_numeric_gt() {
          let expr = FilterExpr::parse("total > 50").unwrap();
          assert_eq!(expr.field, "total");
          assert_eq!(expr.op, FilterOp::Gt);
          assert_eq!(expr.value, FilterValue::Number(50.0));
      }

      #[test]
      fn parse_nested_field() {
          let expr = FilterExpr::parse("address.city = 'Paris'").unwrap();
          assert_eq!(expr.field, "address.city");
      }

      #[test]
      fn parse_boolean() {
          let expr = FilterExpr::parse("active = true").unwrap();
          assert_eq!(expr.value, FilterValue::Bool(true));
      }

      #[test]
      fn parse_not_equal() {
          let expr = FilterExpr::parse("status != 'cancelled'").unwrap();
          assert_eq!(expr.op, FilterOp::Neq);
      }

      #[test]
      fn parse_lte() {
          let expr = FilterExpr::parse("price <= 99.99").unwrap();
          assert_eq!(expr.op, FilterOp::Lte);
          assert_eq!(expr.value, FilterValue::Number(99.99));
      }

      #[test]
      fn parse_invalid_no_operator() {
          assert!(FilterExpr::parse("invalid").is_err());
      }

      #[test]
      fn parse_invalid_empty() {
          assert!(FilterExpr::parse("").is_err());
      }
  }
  ```

- **GREEN**: Implement the parser:
  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub(crate) enum FilterOp { Eq, Neq, Gt, Gte, Lt, Lte }

  #[derive(Debug, Clone, PartialEq)]
  pub(crate) enum FilterValue {
      String(String),
      Number(f64),
      Bool(bool),
      Null,
  }

  #[derive(Debug, Clone)]
  pub(crate) struct FilterExpr {
      pub field: String,
      pub op: FilterOp,
      pub value: FilterValue,
  }

  impl FilterExpr {
      pub fn parse(input: &str) -> Result<Self, String> {
          // Find operator token (longest match first: >=, <=, !=, then >, <, =)
          // Split into field (left) and value (right)
          // Trim field, parse value (quoted string, number, bool, null)
          // ...
      }
  }
  ```

- **REFACTOR**: Handle edge cases — leading/trailing whitespace, single vs
  double quotes for strings
- **CLEANUP**: `cargo clippy -p fraiseql-arrow`, commit

---

### Cycle 2: Filter Expression Evaluator

**File:** `crates/fraiseql-arrow/src/filter_expr.rs`

- **RED**: Write evaluator tests:
  ```rust
  #[test]
  fn eval_string_eq_match() {
      let data = serde_json::json!({"status": "shipped"});
      let expr = FilterExpr::parse("status = 'shipped'").unwrap();
      assert!(expr.matches(&data));
  }

  #[test]
  fn eval_string_eq_no_match() {
      let data = serde_json::json!({"status": "pending"});
      let expr = FilterExpr::parse("status = 'shipped'").unwrap();
      assert!(!expr.matches(&data));
  }

  #[test]
  fn eval_numeric_gt() {
      let data = serde_json::json!({"total": 100.0});
      let expr = FilterExpr::parse("total > 50").unwrap();
      assert!(expr.matches(&data));
  }

  #[test]
  fn eval_numeric_gt_no_match() {
      let data = serde_json::json!({"total": 30.0});
      let expr = FilterExpr::parse("total > 50").unwrap();
      assert!(!expr.matches(&data));
  }

  #[test]
  fn eval_nested_field() {
      let data = serde_json::json!({"address": {"city": "Paris"}});
      let expr = FilterExpr::parse("address.city = 'Paris'").unwrap();
      assert!(expr.matches(&data));
  }

  #[test]
  fn eval_missing_field_no_match() {
      let data = serde_json::json!({"name": "Alice"});
      let expr = FilterExpr::parse("status = 'shipped'").unwrap();
      assert!(!expr.matches(&data));
  }

  #[test]
  fn eval_json_number_as_integer() {
      let data = serde_json::json!({"count": 5});
      let expr = FilterExpr::parse("count >= 5").unwrap();
      assert!(expr.matches(&data));
  }
  ```

- **GREEN**: Implement `matches()` method on `FilterExpr`:
  ```rust
  impl FilterExpr {
      pub fn matches(&self, data: &serde_json::Value) -> bool {
          let value = self.resolve_path(data);
          match value {
              Some(v) => self.compare(v),
              None => false,  // Missing field = no match
          }
      }

      fn resolve_path<'a>(&self, data: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
          self.field.split('.').try_fold(data, |current, segment| current.get(segment))
      }

      fn compare(&self, json_value: &serde_json::Value) -> bool {
          // Convert JSON value to FilterValue, then compare with self.op
          // Handle numeric coercion (JSON integers compare with f64 filter values)
          // ...
      }
  }
  ```

- **REFACTOR**: N/A — keep the evaluator simple
- **CLEANUP**: Clippy, test, commit

---

### Cycle 3: Integrate into `matches_filter()`

**File:** `crates/fraiseql-arrow/src/subscription.rs`

- **RED**: Update the existing test at line ~245 to assert real filter behavior:
  ```rust
  #[test]
  fn test_matches_filter_no_filter_passes_all() {
      let event = HistoricalEvent {
          id:          Uuid::new_v4(),
          event_type:  "INSERT".to_string(),
          entity_type: "Order".to_string(),
          entity_id:   Uuid::new_v4(),
          data:        serde_json::json!({"total": 100}),
          user_id:     None,
          tenant_id:   None,
          timestamp:   Utc::now(),
      };
      // No filter → always matches
      assert!(SubscriptionManager::matches_filter(&event, &None));
  }

  #[test]
  fn test_matches_filter_matching_expression() {
      let event = HistoricalEvent {
          id:          Uuid::new_v4(),
          event_type:  "INSERT".to_string(),
          entity_type: "Order".to_string(),
          entity_id:   Uuid::new_v4(),
          data:        serde_json::json!({"total": 100}),
          user_id:     None,
          tenant_id:   None,
          timestamp:   Utc::now(),
      };
      assert!(SubscriptionManager::matches_filter(
          &event,
          &Some("total > 50".to_string()),
      ));
  }

  #[test]
  fn test_matches_filter_non_matching_expression() {
      let event = HistoricalEvent {
          id:          Uuid::new_v4(),
          event_type:  "INSERT".to_string(),
          entity_type: "Order".to_string(),
          entity_id:   Uuid::new_v4(),
          data:        serde_json::json!({"total": 30}),
          user_id:     None,
          tenant_id:   None,
          timestamp:   Utc::now(),
      };
      assert!(!SubscriptionManager::matches_filter(
          &event,
          &Some("total > 50".to_string()),
      ));
  }

  #[test]
  fn test_matches_filter_invalid_expression_passes() {
      let event = HistoricalEvent {
          id:          Uuid::new_v4(),
          event_type:  "INSERT".to_string(),
          entity_type: "Order".to_string(),
          entity_id:   Uuid::new_v4(),
          data:        serde_json::json!({}),
          user_id:     None,
          tenant_id:   None,
          timestamp:   Utc::now(),
      };
      // Invalid filter expression → fail open (pass all events)
      assert!(SubscriptionManager::matches_filter(
          &event,
          &Some("garbage".to_string()),
      ));
  }
  ```

- **GREEN**: Replace the `const fn` with a real implementation. The function
  remains a static method (no `&self`) since it doesn't need instance state:
  ```rust
  fn matches_filter(event: &crate::HistoricalEvent, filter: &Option<String>) -> bool {
      match filter {
          None => true,
          Some(expr_str) => match FilterExpr::parse(expr_str) {
              Ok(expr) => expr.matches(&event.data),
              Err(_) => true,  // Invalid filter = fail open
          },
      }
  }
  ```
  Add `mod filter_expr;` to `lib.rs` and `use filter_expr::FilterExpr;` in
  `subscription.rs`.

- **REFACTOR**: N/A — re-parsing on every event is acceptable for the current
  scale. If profiling shows this is hot, cache parsed expressions in
  `EventSubscription` as a future optimization (would require changing the
  filter field from `Option<String>` to a struct holding both raw string and
  parsed AST). Defer this to a future phase.

- **CLEANUP**: Clippy, full test suite, commit

## Dependencies
- None (independent of Phase 6)

## Status
[ ] Not Started
