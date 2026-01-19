# FraiseQL v2 Fixes - Quick Reference & Execution Guide

## 📋 What Needs to Be Fixed (At a Glance)

| Priority | Issue | LOC | Est. Fix Time | Risk |
|----------|-------|-----|---------------|------|
| 🔴 CRITICAL | Doctest fails build | 17 | 1h | None |
| 🟡 HIGH | Warnings in code | 2 | 0.5h | None |
| 🟡 HIGH | GraphQL parser incomplete | 300 | 6h | Medium |
| 🟠 MEDIUM | Server tests missing | 500 | 6h | Medium |
| 🟢 LOW | Optimizer test ignored | 50 | 1h | Low |
| 🟢 LOW | Documentation gaps | 100 | 1.5h | None |
| | **TOTAL** | | **~16h** | |

---

## 🎯 Quick Fix Priority (Do These First)

### If You Have 1 Hour
**Phase 1 Only**: Fix the failing doctest
```bash
cd /home/lionel/code/fraiseql
# This blocks CI/CD - must be first
```

### If You Have 1.5 Hours
**Phases 1-2**: Doctest + Warnings
```bash
# Quick wins that improve baseline
```

### If You Have Full Day (16 Hours)
**All Phases 1-6**: Complete implementation

---

## 📍 Where to Find Each Issue

### Issue #1: Failing Doctest ⛔
**File**: `crates/fraiseql-core/src/runtime/query_tracing.rs`
**Lines**: 61-77
**Error**:
```
error[E0599]: no method named `record_phase` found
error[E0061]: this method takes 3 arguments but 2 supplied
```
**Fix**: Update doctest to use actual API methods

### Issue #2: Type Warnings ⚠️
**File 1**: `crates/fraiseql-core/src/runtime/query_tracing.rs` (line 339)
**File 2**: `crates/fraiseql-core/src/runtime/sql_logger.rs` (line 282)
**Warning**: `comparison is useless due to type limits`
**Fix**: Remove assertions or change to meaningful assertions

### Issue #3: GraphQL Parser TODOs 📋
**File**: `crates/fraiseql-core/src/compiler/parser.rs`
**Lines**: 138-140
**TODOs**:
- Line 138: `interfaces: Vec::new()` - TODO: Parse interfaces
- Line 139: `unions: Vec::new()` - TODO: Parse unions
- Line 140: `input_types: Vec::new()` - TODO: Parse input types
**Fix**: Implement 3 new parser functions + tests

### Issue #4: Server Tests Missing 🧪
**File**: `crates/fraiseql-server/src/server.rs`
**Comment**: `// TODO: Add server tests`
**Coverage**: Endpoints, middleware, health checks, error handling
**Fix**: Create `tests/integration_test.rs` with 20+ tests

### Issue #5: Optimizer Test Ignored 🔍
**File**: `crates/fraiseql-cli/src/schema/optimizer.rs`
**Status**: `#[ignore = "TODO: Schema optimizer behavior changed..."]`
**Fix**: Investigate, fix, or properly remove

### Issue #6: Documentation Gaps 📚
**Scattered**: Various modules
**Fix**: Add security warnings, examples, update docs

---

## 🚀 How to Run This Plan

### Step 1: Verify Current Status
```bash
cd /home/lionel/code/fraiseql

# Check test failures
cargo test --doc -p fraiseql-core 2>&1 | grep -A 10 "FAILED\|failures:"

# Check warnings
cargo clippy --all-targets --all-features -- -D warnings 2>&1 | head -20

# Check all tests
cargo test 2>&1 | tail -5
```

### Step 2: Start Phase 1 (1 hour)
```bash
# Create branch
git checkout -b feature/fixes-code-quality

# Edit doctest in query_tracing.rs
# Lines 61-77 - update example to use actual API
# Update lines 339 and sql_logger.rs:282

# Verify
cargo test --doc -p fraiseql-core
cargo clippy --all-targets --all-features -- -D warnings

# Commit
git add -A
git commit -m "fix(tracing): Fix QueryTraceBuilder doctest and type warnings"
```

### Step 3: Continue Through Phases
Follow the detailed plan in `IMPLEMENTATION_PLAN_FIXES.md` for phases 2-6.

---

## 📊 Success Metrics

### Before Fixes
```
Tests: 3235 passed, 1 failed ❌
Doctests: 1 failed ❌
Warnings: 2 clippy warnings ⚠️
Parser: Incomplete (3 TODOs) ⚠️
Server Tests: 0 tests ❌
```

### After Fixes
```
Tests: 3235+ passed, 0 failed ✅
Doctests: All passing ✅
Warnings: 0 warnings ✅
Parser: Complete feature parity ✅
Server Tests: 25+ tests ✅
Documentation: Complete & current ✅
Quality Score: 9.0+/10 ✅
```

---

## 💡 Key Implementation Tips

### Doctest Fix (Phase 1)
- The actual API is: `record_phase_success()` and `record_phase_error()`
- Manual timing tracking required
- Third parameter to `finish()` is result_count
- See current tests for examples of proper usage

### Warning Fixes (Phase 2)
- Both are comparing unsigned integers to 0
- Options: Change to meaningful assertion or remove entirely
- Check if assertion is even needed (by design, u64 >= 0)

### Parser Implementation (Phase 3)
- Study existing `parse_types()` function as template
- Look at IR structures in `compiler/ir.rs`
- Follow same error handling patterns
- Add round-trip test (JSON → IR → verify)

### Server Tests (Phase 4)
- Use testcontainers for database
- Mock OIDC provider if testing auth
- Test both happy path and error cases
- Keep tests isolated (don't depend on each other)

### Optimizer Investigation (Phase 5)
- Is it still used in current architecture?
- Does it produce correct output if used?
- Quick decision needed - either fix or remove

### Documentation (Phase 6)
- Add examples for new parser features
- Security note for `execute_raw_query()`
- Update README if needed
- Add inline code examples

---

## 🔗 Related Files to Study

**For understanding codebase**:
- `crates/fraiseql-core/src/error.rs` - Error handling patterns
- `crates/fraiseql-core/src/compiler/ir.rs` - IR data structures
- `crates/fraiseql-core/tests/phase*_integration.rs` - Test patterns
- `crates/fraiseql-server/src/lib.rs` - Server structure

**For reference implementations**:
- `parse_types()` in parser.rs - Reference for new parser functions
- Existing tests in phase*_integration.rs - Test style guide
- Error handling in db/ modules - Error patterns

---

## ⚡ Shortcuts & Gotchas

### The Doctest MUST Be Fixed First
- Blocks `cargo test --doc`
- Blocks CI/CD
- Takes only 1 hour
- Do this before anything else

### Round-Trip Testing for Parser
- After implementing interfaces/unions/input_types
- Test: JSON → parse → IR → to_json → parse again
- Ensures no data loss

### Server Tests Need Setup
- Database connection needed
- Might need testcontainers
- Consider mocking database for some tests
- Keep test execution time < 5s total

### The Optimizer Status
- This decision affects architecture clarity
- Document the choice in PR
- Either fix it properly or remove it completely
- No "maybe we'll fix it later" - makes codebase confusing

---

## 📝 Commit Message Template

For each phase, use this pattern:

```
<type>(<scope>): <description> [Phase N]

## Changes
- Specific change 1
- Specific change 2

## Why
Explanation of what was broken and why this fixes it

## Verification
✅ Tests pass
✅ Clippy clean
✅ [Specific check]

## Issues Addressed
- Doctest failures
- Warnings
- [etc]
```

**Example**:
```
fix(tracing): Fix QueryTraceBuilder doctest example [Phase 1]

## Changes
- Updated doctest to use actual API (record_phase_success/error)
- Added result_count parameter to finish() call
- Fixed useless type comparisons in assertions

## Why
The doctest example was calling non-existent methods (record_phase)
and missing required parameters, causing doctest failure during build.

## Verification
✅ cargo test --doc passes
✅ cargo clippy clean
✅ Example demonstrates correct API usage

## Issues Addressed
- Doctest failure blocking build
- Type comparison warnings
```

---

## 🤔 FAQ

**Q: Can I skip Phase 5 (Optimizer)?**
A: Not recommended. It should take 1 hour to clarify status. Either it's broken (fix it) or it's unnecessary (remove it). Leaving it ignored creates technical debt.

**Q: Do I need to test all three parser features equally?**
A: Yes. Interfaces, unions, and input types each need 10+ test cases. Follow same pattern for consistency.

**Q: What if a phase takes longer than estimated?**
A: Phases 3-4 are most likely to run over. Phase 1-2 are guaranteed < 2h. If you need to cut something, defer Phase 5 (optimizer) to next sprint.

**Q: Can tests run in parallel?**
A: Yes. Use `cargo nextest run` for 2-3x speedup on Phase 4 (server tests).

**Q: Do I need to update the main README?**
A: No, unless new features materially change capabilities. Update .claude/IMPLEMENTATION_PLAN_FIXES.md instead for documentation.

---

## ✅ Pre-Implementation Checklist

- [ ] Read the full `IMPLEMENTATION_PLAN_FIXES.md`
- [ ] Understand Phase 1 (doctest fix) completely
- [ ] Create feature branch: `git checkout -b feature/fixes-code-quality`
- [ ] Verify current state: `cargo test 2>&1 | tail -5`
- [ ] Review error messages carefully
- [ ] Have CI/CD pipeline ready to test after changes

---

## 🎯 Success = This

```bash
$ cargo test
...
test result: ok. 3240+ passed; 0 failed; 0 ignored

$ cargo test --doc -p fraiseql-core
...
test result: ok. 138 passed; 0 failed; 0 ignored

$ cargo clippy --all-targets --all-features -- -D warnings
... (no output = no warnings)

$ cd crates/fraiseql-server/tests && cargo test
test result: ok. 25+ passed; 0 failed
```

---

**Ready to begin? Start with Phase 1! ⚡**
