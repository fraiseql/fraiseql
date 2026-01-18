# FraiseQL Next Steps: Fix CLI Schema Format

## Priority: Unblock E2E Compilation Testing

### Current Blocker

`fraiseql-cli compile` rejects our `schema.json` format with:

```
Error: Failed to parse schema.json
```

### Why This Matters

- ✅ All 10 languages can generate code
- ✅ E2E compilation test framework is ready
- ❌ CLI won't accept schema format
- → We can't prove all 10 languages compile identically

### What Needs to Happen

#### 1. Investigate CLI Schema Parser

**Location**: FraiseQL Rust core code
**Task**: Find the schema parsing logic that's rejecting our format

Questions to answer:

- What fields does CLI expect in `schema.json`?
- What's the exact validation error?
- Is our schema format correct or needs adjustment?
- Are there field naming conventions we're missing?

#### 2. Compare Against Working Examples

**Find**: Any existing `schema.json` files in the codebase that CLI accepts

Look for:

- Examples in `/examples` or `/tests`
- Documentation on schema format
- CLI test fixtures
- Integration test schemas

#### 3. Adjust or Fix

**Either**:

- Fix our canonical schema format to match CLI expectations
- **Or** Fix CLI parser to accept valid schemas

**Recommendation**: If CLI is too strict, fix the parser. Our schema format is valid and reasonable.

#### 4. Test the Fix

Once resolved:

```bash
# Should succeed
python tests/e2e/velocitybench_compilation_test.py

# Expected output:
✅ Phase 1: All 10 languages generate valid code
✅ Phase 2: All 10 languages compile to identical schemas
```

### Success Criteria

```
PASS: fraiseql-cli compile velocitybench_schema.json -o velocitybench_schema.compiled.json
PASS: All 10 language versions compile to identical JSON
PASS: velocitybench_compilation_test.py Phase 2 tests pass
```

### Files Involved

**To Investigate**:

- Rust code for CLI schema parser
- FraiseQL core type definitions
- Existing test schemas

**Already Created**:

- `tests/e2e/velocitybench_schemas.py` - Canonical schema
- `tests/e2e/velocitybench_compilation_test.py` - Test framework
- Schema format examples from all 10 languages

### Time Estimate

2-4 hours depending on CLI architecture complexity

### Next Meeting Topics

1. What does CLI parser expect?
2. Is schema format issue or CLI strictness?
3. What's the fix strategy?
4. Once fixed, run full E2E test
