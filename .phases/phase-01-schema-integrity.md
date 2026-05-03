# Phase 1: Schema Integrity Verification

## Objective

Verify that the SHA-256 content hash flows correctly from CLI compile through to runtime
`from_json`, and that tampering is detected in strict mode while non-strict gracefully
degrades.

## Success Criteria

- [ ] CLI `compile` produces a `_content_hash` field in output JSON
- [ ] `from_json(output, true)` accepts the CLI-produced JSON without error
- [ ] Byte-tampering any field in the JSON body causes `from_json(_, true)` to reject
- [ ] `from_json(output, false)` with a tampered hash logs a warning but succeeds
- [ ] `--skip-hash` flag produces JSON without `_content_hash`
- [ ] Existing test fixtures (golden fixtures, property tests) still work without hash
- [ ] `from_json` with `strict=true` and no hash field returns an error

## TDD Cycles

### Cycle 1: CLI round-trip integration test

- **RED**: Write test that runs CLI compile on a fixture, then calls `from_json(_, true)`
  and asserts success. (Similar to `test_field_values_survive_full_cli_pipeline` but with
  strict=true.)
- **GREEN**: Ensure the canonical-Value hashing in compile.rs produces a hash that
  `from_json` can verify.
- **REFACTOR**: Deduplicate any hash-computation helper into a shared function if CLI and
  core duplicate logic.
- **CLEANUP**: Clippy, remove any dead code.

### Cycle 2: Tamper detection

- **RED**: Write test that modifies one character in the compiled JSON body (not the hash
  field) and asserts `from_json(_, true)` returns `Err`.
- **GREEN**: Already implemented — verify test passes.
- **REFACTOR**: Ensure error message includes both expected and actual hash for debugging.
- **CLEANUP**: Lint.

### Cycle 3: Graceful degradation

- **RED**: Write test for `from_json(_, false)` with present-but-wrong hash. Assert it
  returns `Ok` (current behaviour after our fix).
- **GREEN**: Already implemented.
- **REFACTOR**: Verify the `warn!` is emitted (use `tracing-test` or check log capture).
- **CLEANUP**: Lint.

### Cycle 4: `--skip-hash` flag

- **RED**: Write test that invokes CLI with `--skip-hash` and verifies the output JSON
  has no `_content_hash` field.
- **GREEN**: Verify the existing `skip_hash` parameter path works.
- **REFACTOR**: N/A.
- **CLEANUP**: Lint.

## Dependencies

- None (this is self-contained)

## Status
[ ] Not Started
