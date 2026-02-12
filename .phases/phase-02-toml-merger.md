# Phase 2: TOML Schema Merger Bug Fix

## Objective
Fix the TOML-to-intermediate schema conversion so that `types` and `fields` are
produced as arrays (sequences) from the source, not as objects that need
downstream conversion.

## Success Criteria
- [ ] `TomlSchema::to_intermediate_schema()` produces `types` as a JSON array
- [ ] Each type's `fields` are also JSON arrays with `"name"` keys injected
- [ ] `IntermediateSchema` deserialization succeeds without conversion hacks
- [ ] `tests/integration/test_toml_workflow.py` passes without the known-issue workaround
- [ ] `cargo clippy -p fraiseql-cli` clean
- [ ] `cargo test -p fraiseql-cli` passes

## Background

### Root Cause

**File:** `crates/fraiseql-cli/src/config/toml_schema.rs:763-829`

The bug is in `TomlSchema::to_intermediate_schema()`, NOT in `merger.rs`. This
method constructs the intermediate JSON using `serde_json::Map` (objects) for
both `types` and `fields`:

- Line 764: `let mut types_json = serde_json::Map::new()` — creates types as **object**
- Line 767: `let mut fields_json = serde_json::Map::new()` — creates fields as **object**
- Line 786: inserts `"fields": fields_json` — fields remain an **object**

But `IntermediateSchema` expects:
- `types: Vec<IntermediateType>` — needs a JSON **array**
- `fields: Vec<IntermediateField>` — needs a JSON **array** with `"name"` keys

The merger (`merger.rs:351-361`) has conversion logic that transforms objects to
arrays, but this is a band-aid. The correct fix is to produce the right format
at the source.

### Test Documentation

`tests/integration/test_toml_workflow.py:90-96` documents the bug. Lines 159-164
contain the workaround that skips the test when the error is detected:
```
"invalid type: map, expected a sequence"
```

## TDD Cycles

### Cycle 1: Fix `to_intermediate_schema()` to Produce Arrays

**File:** `crates/fraiseql-cli/src/config/toml_schema.rs`

- **RED**: Add a Rust unit test that calls `to_intermediate_schema()` and
  verifies the output format:
  ```rust
  #[test]
  fn test_to_intermediate_schema_produces_arrays() {
      let toml_str = r#"
      [types.User]
      sql_source = "users"
      [types.User.fields.id]
      type = "ID"
      nullable = false
      [types.User.fields.name]
      type = "String"
      nullable = false
      "#;
      let schema: TomlSchema = toml::from_str(toml_str).unwrap();
      let result = schema.to_intermediate_schema();

      // types must be an array
      let types = result.get("types").unwrap();
      assert!(types.is_array(), "types should be an array, got: {types}");
      let types_arr = types.as_array().unwrap();
      assert_eq!(types_arr.len(), 1);

      // fields within each type must be an array
      let user_type = &types_arr[0];
      let fields = user_type.get("fields").unwrap();
      assert!(fields.is_array(), "fields should be an array, got: {fields}");
      let fields_arr = fields.as_array().unwrap();
      assert_eq!(fields_arr.len(), 2);

      // each field must have a "name" key
      assert!(fields_arr.iter().all(|f| f.get("name").is_some()));
  }
  ```
  This test should fail with the current code.

- **GREEN**: Rewrite `to_intermediate_schema()` to produce arrays directly:
  ```rust
  pub fn to_intermediate_schema(&self) -> serde_json::Value {
      let types_array: Vec<serde_json::Value> = self.types.iter()
          .map(|(type_name, type_def)| {
              let fields_array: Vec<serde_json::Value> = type_def.fields.iter()
                  .map(|(field_name, field_def)| {
                      serde_json::json!({
                          "name": field_name,
                          "type": field_def.field_type,
                          "nullable": field_def.nullable,
                          "description": field_def.description,
                      })
                  })
                  .collect();

              serde_json::json!({
                  "name": type_name,
                  "sql_source": type_def.sql_source,
                  "description": type_def.description,
                  "fields": fields_array,
              })
          })
          .collect();

      serde_json::json!({
          "types": types_array,
          // ... other top-level keys (queries, mutations) with same array treatment
      })
  }
  ```
  Apply the same array pattern to any other collections in the method
  (queries, mutations, etc.) that have the same object-instead-of-array problem.

- **REFACTOR**: Review the object-to-array conversion code in `merger.rs`
  (lines 344-372). If `to_intermediate_schema()` now always produces arrays,
  the `Value::Object(types_map)` branch may be dead code. Keep it for backward
  compatibility with external callers that might pass object-format JSON, but
  add a comment noting that the TOML path no longer exercises it.

- **CLEANUP**: `cargo clippy -p fraiseql-cli`, `cargo test -p fraiseql-cli`, commit

---

### Cycle 2: Remove Python Test Workaround

**File:** `tests/integration/test_toml_workflow.py`

- **RED**: Remove the known-issue workaround at lines 159-164 and the doc
  comment at lines 90-96. Run the test — it should now pass.

- **GREEN**: If the test still fails, the error message will indicate which
  field is still an object. Debug by adding `eprintln!()` to `merge_values()`
  to dump the intermediate JSON before deserialization.

- **REFACTOR**: Clean up any remaining workaround artifacts

- **CLEANUP**: `uv run ruff check --fix`, `uv run pytest tests/integration/test_toml_workflow.py`, commit

## Dependencies
- None

## Status
[ ] Not Started
