# Phase 3: REFACTOR - Clean Implementation

**Status**: Ready for Implementation (after Phase 2)
**Effort**: 30 minutes
**Type**: TDD - Improve Without Breaking

---

## Objective

Improve code quality **without changing behavior**. All tests must remain green.

---

## Prerequisites

- [ ] Phase 2 completed
- [ ] All tests PASS
- [ ] No regressions in existing tests

---

## Refactoring Checklist

### 1. Rust Code Review

**Files likely modified in Phase 2**:
- `fraiseql_rs/src/json_transform.rs`
- `fraiseql_rs/src/pipeline/builder.rs`
- `fraiseql_rs/src/core/transform.rs`

**For each modified file, check**:

#### Remove Debug Code
```rust
// REMOVE any debug prints added during Phase 2
println!("DEBUG: ...");   // DELETE
eprintln!("DEBUG: ...");  // DELETE
dbg!(value);              // DELETE
#[cfg(debug_assertions)]
eprintln!("...");         // KEEP only if intentional
```

#### Verify Documentation
```rust
// GOOD: Clear, concise doc comment
/// Recursively transform all JSON object keys from snake_case to camelCase.
///
/// Handles nested objects and arrays. Preserves null values and primitives.
pub fn transform_value(value: Value) -> Value {

// BAD: Missing or outdated doc
fn transform_value(value: Value) -> Value {  // No docs
```

#### Consistent Error Handling
```rust
// Check for consistent patterns across the codebase
// FraiseQL typically uses:
match result {
    Ok(v) => v,
    Err(e) => {
        // Return sensible default, don't panic
        Value::Null
    }
}
```

#### Efficient Iteration
```rust
// GOOD: Consume the map, avoid cloning
for (key, val) in map {
    let transformed = transform_value(val);
}

// BAD: Unnecessary clone
for (key, val) in map.iter() {
    let transformed = transform_value(val.clone());
}
```

### 2. Test Code Review

**Files created in Phase 1**:
- `tests/regression/test_jsonb_nested_camelcase.py`
- `tests/unit/core/test_jsonb_camelcase_conversion.py`

#### Remove Verbose Assertions
```python
# BEFORE (Phase 1 - verbose for debugging)
assert "smtpServer" in config, f"Expected 'smtpServer', got keys: {list(config.keys())}"

# AFTER (Phase 3 - clean)
assert "smtpServer" in config
assert config["smtpServer"]["ipAddress"] == "13.16.1.10"
```

#### Consolidate Test Constants
```python
# GOOD: Class-level constant
class TestJSONBNestedCamelCase:
    TEST_CONFIG_ID = "01436121-0000-0000-0000-000000000000"

# BAD: Duplicated in tests
def test_one(self):
    test_id = "01436121-0000-0000-0000-000000000000"
def test_two(self):
    test_id = "01436121-0000-0000-0000-000000000000"
```

#### Clear Test Names
```python
# GOOD: Describes behavior
def test_underscore_nested_object_converts_to_camelcase(self):
def test_numbered_fields_convert_correctly(self):

# AVOID: References bug/phase
def test_fix_for_smtp_server_bug(self):
def test_phase2_dns1_present(self):
```

### 3. Run Linters

```bash
# Rust - format and check
cd fraiseql_rs
cargo fmt
cargo clippy -- -D warnings
cd ..

# Python - ruff
uv run ruff check tests/regression/test_jsonb_nested_camelcase.py --fix
uv run ruff check tests/unit/core/test_jsonb_camelcase_conversion.py --fix

# Python - format
uv run ruff format tests/regression/test_jsonb_nested_camelcase.py
uv run ruff format tests/unit/core/test_jsonb_camelcase_conversion.py
```

### 4. Specific Refactoring Tasks

Based on likely Phase 2 changes:

#### If `transform_value()` was modified:

```rust
// Ensure the function is:
// 1. Well-documented
// 2. Uses efficient iteration (no unnecessary clones)
// 3. Has clear match arm structure

/// Transform all object keys in a JSON value from snake_case to camelCase.
///
/// Recursively processes nested objects and arrays. Primitive values
/// (strings, numbers, booleans, null) are returned unchanged.
pub fn transform_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let new_map = map
                .into_iter()
                .map(|(key, val)| {
                    (to_camel_case(&key), transform_value(val))
                })
                .collect();
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            Value::Array(arr.into_iter().map(transform_value).collect())
        }
        other => other,
    }
}
```

#### If `transform_with_schema()` was modified:

Ensure fallback path is clear and documented:

```rust
// In the fallback branch, add a comment explaining why
else {
    // Field not in schema registry - apply basic camelCase transformation
    // This handles dynamic JSONB fields not defined in GraphQL schema
    let transformed = transform_value(val.clone());
    new_map.insert(camel_key, transformed);
}
```

---

## Verification

### After Each Change
```bash
# Quick test - ensure nothing broke
uv run pytest tests/regression/test_jsonb_nested_camelcase.py -v
uv run pytest tests/unit/core/test_jsonb_camelcase_conversion.py -v
```

### Before Committing
```bash
# Full verification
cd fraiseql_rs && cargo fmt && cargo clippy && cd ..
uv run ruff check .
uv run pytest tests/ -v --tb=short
```

---

## Acceptance Criteria

- [ ] All tests still PASS
- [ ] No debug code remaining (`println!`, `dbg!`, temporary comments)
- [ ] Docstrings are clear and accurate
- [ ] Code follows project style (cargo fmt, ruff)
- [ ] Linters pass with no warnings
- [ ] No unnecessary clones or allocations

---

## Commit Message

```
refactor(jsonb): clean up nested JSONB camelCase implementation [REFACTOR]

- Remove debug statements from Phase 2
- Add documentation for transform functions
- Optimize iteration (avoid unnecessary clones)
- Fix linter warnings
- Simplify verbose test assertions

No behavior changes. All tests remain green.
```

---

## DO NOT

- Do NOT change any behavior
- Do NOT add new tests
- Do NOT fix unrelated code
- Do NOT make tests fail

## DO

- DO remove debug code
- DO improve documentation
- DO fix linter warnings
- DO simplify verbose code
- DO run tests after each change

---

**Next Phase**: Phase 4 - QA (Comprehensive validation)
