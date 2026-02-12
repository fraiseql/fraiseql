# Phase 6: TOML Schema Merger Bug Fix

## Objective
Fix the TOML-to-intermediate schema conversion so that `types` and `fields` are produced as arrays from the source.

## Success Criteria
- [ ] `TomlSchema::to_intermediate_schema()` produces `types` as a JSON array
- [ ] Each type's `fields` are also JSON arrays with `"name"` keys injected
- [ ] `IntermediateSchema` deserialization succeeds without conversion hacks
- [ ] `tests/integration/test_toml_workflow.py` passes
- [ ] `cargo clippy -p fraiseql-cli` clean
- [ ] `cargo test -p fraiseql-cli` passes

## Background

**File**: `crates/fraiseql-cli/src/config/toml_schema.rs:763-829`

The TOML schema conversion currently produces objects that need downstream conversion. The fix involves restructuring to produce arrays directly, matching the `IntermediateSchema` expectations.

## TDD Cycles

### Cycle 1: Fix Array Conversion in TOML Schema

**File**: `crates/fraiseql-cli/src/config/toml_schema.rs`

- **RED**: Write test expecting `types` as a JSON array
- **GREEN**: Modify `to_intermediate_schema()` to produce arrays:
  ```rust
  let types_array = self.types.iter().map(|t| {
      json!({
          "name": t.name,
          "fields": t.fields.iter().map(|f| {
              let mut field = serde_json::to_value(f)?;
              field["name"] = json!(f.name);
              Ok(field)
          }).collect::<Result<Vec<_>>>()?,
          // ... other properties
      })
  }).collect::<Vec<_>>();
  ```
- **REFACTOR**: Extract array mapping logic into helper functions
- **CLEANUP**: Test all type conversions, commit

### Cycle 2: Test Full TOML Workflow

**File**: `crates/fraiseql-cli/tests/toml_schema_integration.rs`

- **RED**: Write test for complete TOML→Intermediate schema flow
- **GREEN**: Verify schema round-trips without conversion hacks
- **REFACTOR**: Add edge case tests (empty types, nested fields)
- **CLEANUP**: All tests pass, commit

## Dependencies
- None (independent of all other phases)

## Status
[ ] Not Started
