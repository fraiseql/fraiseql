# CLI Schema Format Fix - Complete Documentation Index

## Start Here 👇

**New to this topic?** Start with one of these in order:

1. **[EXECUTIVE_SUMMARY_CLI_FIX.md](EXECUTIVE_SUMMARY_CLI_FIX.md)** ⭐ **START HERE**
   - One-page overview of the problem and solution
   - Timeline, risks, success criteria
   - Strategic recommendations
   - **Time to read**: 10 minutes

2. **[CLI_SCHEMA_FIX_SUMMARY.md](CLI_SCHEMA_FIX_SUMMARY.md)** - Deep Overview
   - Root cause analysis
   - What we know about the CLI
   - Implementation strategies (manual vs. local model)
   - **Time to read**: 15 minutes

3. **[QUICK_FIX_CHECKLIST.md](QUICK_FIX_CHECKLIST.md)** - Implementation Guide
   - Step-by-step checklist for fixing each file
   - Verification commands
   - Commit template
   - **Time to implement**: 1-2 hours
   - **Follow this while executing fixes**

---

## Reference Documents

### For Implementation

- **[CLI_SCHEMA_FIX_IMPLEMENTATION_PLAN.md](CLI_SCHEMA_FIX_IMPLEMENTATION_PLAN.md)**
  - Detailed 4-phase implementation plan
  - Troubleshooting guide
  - Timeline breakdown
  - **Use this**: When you need detailed step-by-step guidance

### For Technical Details

- **[CLI_SCHEMA_FORMAT_ANALYSIS.md](CLI_SCHEMA_FORMAT_ANALYSIS.md)**
  - Deep technical analysis of IntermediateSchema
  - Complete struct definitions with all fields
  - Exact field name requirements from CLI
  - Verification strategy
  - **Use this**: When debugging or understanding the CLI requirements

---

## Quick Navigation

### By Role

**Project Lead** 📋

- Read: EXECUTIVE_SUMMARY_CLI_FIX.md
- Understand: Timeline, risk, effort
- Decide: Implementation strategy
- Monitor: E2E test results

**Developer Implementing Fixes** 👨‍💻

- Follow: QUICK_FIX_CHECKLIST.md
- Reference: CLI_SCHEMA_FIX_IMPLEMENTATION_PLAN.md
- Verify: With provided commands
- Commit: When all tests pass

**Code Reviewer** 🔍

- Check: All 11 files have `returns_list` (no `return_list`)
- Verify: No unintended changes
- Test: Run `python3 tests/e2e/velocitybench_compilation_test.py`
- Approve: When E2E tests show ✅ for all 10 languages

**Technical Investigator** 🔬

- Deep-dive: CLI_SCHEMA_FORMAT_ANALYSIS.md
- Reference: IntermediateSchema struct definitions
- Verify: Field name requirements are exact
- Troubleshoot: Using validator.rs and converter.rs

---

## The Problem (TL;DR)

```json
// WRONG (current generators):
{ "name": "users", "return_list": true }

// CORRECT (CLI expects):
{ "name": "users", "returns_list": true }
```

**Impact**: CLI rejects all 10 language schemas

---

## The Solution (TL;DR)

Replace all `"return_list"` with `"returns_list"` in 11 files:

- 1 canonical schema file (tests/e2e/velocitybench_schemas.py)
- 10 language generators (Python, TypeScript, Go, Java, PHP, Kotlin, C#, Rust, JavaScript, Ruby)

**Effort**: ~1-2 hours

---

## Key Documents Map

```
┌─────────────────────────────────────────────┐
│  EXECUTIVE_SUMMARY (Start here!)           │
│  - Problem overview                        │
│  - Solution summary                        │
│  - Timeline & risks                        │
└────────────┬────────────────────────────────┘
             │
             ├─→ CLI_SCHEMA_FIX_SUMMARY
             │   (Strategic context)
             │
             ├─→ QUICK_FIX_CHECKLIST
             │   (Implementation guide)
             │
             ├─→ CLI_SCHEMA_FIX_IMPLEMENTATION_PLAN
             │   (Detailed steps)
             │
             └─→ CLI_SCHEMA_FORMAT_ANALYSIS
                 (Technical deep-dive)
```

---

## Implementation Paths

### Path 1: Quick & Dirty (Manual)

1. Read: QUICK_FIX_CHECKLIST.md
2. Edit: Each file's `return_list` → `returns_list`
3. Test: Commands in checklist
4. Commit: When all tests pass
**Time**: 1-2 hours

### Path 2: Smart & Fast (Local Model Assistance)

1. Read: QUICK_FIX_CHECKLIST.md
2. Use local 8B model for bulk fixes:
   - Prompt: "Replace all 'return_list' with 'returns_list' in [file]"
   - Parallel execution: 5 files at a time
3. Verify: Each change with grep
4. Test: Commands in checklist
5. Commit: When all tests pass
**Time**: ~60 minutes total

### Path 3: Guided Implementation (Claude + Local Model)

1. Claude: Fix canonical schema (velocitybench_schemas.py)
2. Local Model: Fix all 10 generators (parallel)
3. Claude: Run verification + E2E tests
4. Commit: When all pass
**Time**: ~1 hour (recommended)

---

## Files to Modify

### Primary (Canonical Schema)

- [ ] `tests/e2e/velocitybench_schemas.py` - 8 occurrences

### Secondary (Language Generators)

- [ ] Python generator
- [ ] TypeScript generator
- [ ] Go generator
- [ ] Java generator
- [ ] PHP generator
- [ ] Kotlin generator
- [ ] C# generator
- [ ] Rust generator
- [ ] JavaScript generator
- [ ] Ruby generator

**Total**: 11 files, ~50-60 changes

---

## Verification Checklist

After all fixes:

- [ ] No remaining `"return_list"` in codebase (verified with grep)
- [ ] CLI accepts canonical schema (`fraiseql-cli compile` succeeds)
- [ ] All 10 language generators produce valid schemas
- [ ] All 10 compiled schemas are identical
- [ ] E2E test shows all 10 languages with ✅

---

## Success Criteria

**Primary**: E2E test output shows:

```
✅ ALL TIER 1A COMPILATION E2E TESTS PASSED!
✅ Phase 1: All 10 languages generate valid schema code
✅ Phase 2: All 10 languages compile to identical schemas
```

**Secondary**:

- All 10 language boxes show ✅
- No compilation errors
- All compiled schemas are bit-identical

---

## Troubleshooting Quick Links

| Issue | Solution |
|-------|----------|
| CLI still rejects schema | See: CLI_SCHEMA_FORMAT_ANALYSIS.md, "Other Fields" section |
| One language different output | Check: That generator has `returns_list` not `return_list` |
| Can't find generator file | See: QUICK_FIX_CHECKLIST.md, "File Location" commands |
| Verification test fails | See: CLI_SCHEMA_FIX_IMPLEMENTATION_PLAN.md, "Troubleshooting" section |
| Forgot which files changed | Run: `git diff --name-only` |

---

## Related Documentation

**In this project**:

- `.claude/IMPLEMENTATION_ROADMAP.md` - Full 11-phase plan
- `crates/fraiseql-cli/src/schema/intermediate.rs` - CLI schema struct definitions
- `crates/fraiseql-cli/src/schema/validator.rs` - CLI validation rules
- `tests/e2e/velocitybench_compilation_test.py` - E2E test framework

**External**:

- Rust serde documentation - Field renaming with `#[serde(rename = "...")]`
- JSON Schema specification - For understanding field requirements

---

## Progress Tracking

### Phase Status

- ✅ **Phase 1**: Diagnosis complete
- ✅ **Phase 2-4 Planning**: Documentation complete
- ⏳ **Phase 2**: Fix canonical schema (pending)
- ⏳ **Phase 3**: Fix 10 generators (pending)
- ⏳ **Phase 4**: Verification (pending)

### Estimated Timeline

| Phase | Est. Time | Status |
|-------|-----------|--------|
| Diagnosis | 30 min | ✅ Done |
| Planning | 2 hours | ✅ Done |
| Implementation | 1-2 hours | ⏳ Next |
| Verification | 30 min | ⏳ After impl. |
| **Total** | **~4-5 hours** | |

---

## Contact & Questions

**Documentation questions?**

- See: EXECUTIVE_SUMMARY_CLI_FIX.md, "Questions Answered" section

**Implementation stuck?**

- See: CLI_SCHEMA_FIX_IMPLEMENTATION_PLAN.md, "Troubleshooting" section

**Need technical details?**

- See: CLI_SCHEMA_FORMAT_ANALYSIS.md

---

## Summary

This is a **straightforward fix** with:

- ✅ Clear problem (field name mismatch)
- ✅ Clear solution (rename 1 field)
- ✅ Clear scope (11 files)
- ✅ Clear timeline (1-2 hours)
- ✅ High confidence (99%)

**Result**: All 10 languages will compile identically, proving semantic equivalence.

---

## Document Versions

| Document | Version | Last Updated | Purpose |
|----------|---------|--------------|---------|
| EXECUTIVE_SUMMARY_CLI_FIX.md | 1.0 | Now | High-level overview |
| CLI_SCHEMA_FIX_SUMMARY.md | 1.0 | Now | Strategic context |
| QUICK_FIX_CHECKLIST.md | 1.0 | Now | Implementation guide |
| CLI_SCHEMA_FIX_IMPLEMENTATION_PLAN.md | 1.0 | Now | Detailed steps |
| CLI_SCHEMA_FORMAT_ANALYSIS.md | 1.0 | Now | Technical analysis |
| CLI_FIX_INDEX.md | 1.0 | Now | Navigation guide |

---

**Ready to proceed?** 👉 Start with [EXECUTIVE_SUMMARY_CLI_FIX.md](EXECUTIVE_SUMMARY_CLI_FIX.md)
