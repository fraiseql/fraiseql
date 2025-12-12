# FraiseQL Mutation Schema Fix - Streamlined Implementation

## 🎯 Issue Summary

**Problem**: Auto-populated mutation fields (`status`, `message`, `errors`, `updatedFields`) are added by decorator to `__annotations__` but NOT to `__gql_fields__`, making them invisible to GraphQL schema.

**Impact**: 138 failing tests in PrintOptim, cannot query these fields

**CTO Approval**: ✅ APPROVED with simplified approach

---

## 🔧 Solution (Corrected - Two Parts)

### Part 1: Python Decorator Fix
Add auto-populated fields to `__gql_fields__` after `define_fraiseql_type()` completes.

**Fields to add:**
- `status: str` (always)
- `message: str | None` (always)
- `errors: list[Error] | None` (always)
- `updatedFields: list[str] | None` (always) ⚠️ **NEW per CTO feedback**
- `id: str | None` (conditional - only if entity field detected)

### Part 2: Rust Field Selection Fix ⚠️ **CORRECTED**
**Rust changes ARE needed** - FraiseQL mutations use `RustResponseBytes` which bypasses GraphQL executor's field filtering.

**What needs fixing:**
- Modify `build_success_response()` to filter based on `success_type_fields`
- Modify `build_error_response_with_code()` to filter based on `error_type_fields`
- Only include fields that were explicitly requested in GraphQL query
- Maintain backward compatibility (None selection = all fields)

---

## 📁 Streamlined Phase Plans

### Phase 1: Python Decorator Fix (RED/GREEN)
**TDD implementation of decorator changes**
- Write failing tests
- Modify `@success` and `@failure` decorators
- Add `updatedFields` to auto-injected list
- Make tests pass

**Time**: 1.5 hours

### Phase 2: Rust Field Selection (GREEN/REFACTOR) ⚠️ **NEW - REQUIRED**
**Implement field filtering in Rust response builder**
- Write Rust tests for field selection
- Modify `build_success_response()` to filter fields
- Modify `build_error_response_with_code()` to filter fields
- Rebuild with maturin develop
- Test integration

**Time**: 2 hours

### Phase 3: Integration & Verification (QA)
**End-to-end testing**
- Verify fields in GraphQL schema (Python)
- Test field selection behavior (Rust)
- GraphQL spec compliance tests
- External validation (PrintOptim)

**Time**: 1 hour

### Phase 4: Documentation & Commit
**Finalize and ship**
- Update CHANGELOG
- Commit with descriptive message (Python + Rust changes)
- No migration guide needed (sole user, no breaking changes)

**Time**: 30 minutes

**Total**: 5 hours ⚠️ **(revised from 3 hours)**

---

## 🎯 Key Simplifications (per CTO feedback)

### ✅ What We're Doing
1. **Python decorator fix** - Add fields to `__gql_fields__`
2. **Add `updatedFields`** - CTO confirmed it's useful (not in cascade spec but OK)
3. **Simple implementation** - No backward compat complexity
4. **Trust GraphQL executor** - No Rust changes needed

### ❌ What We're NOT Doing
1. **No backward compat complexity** - Simple None check is sufficient
2. **No feature flags** - Clean, straightforward fix
3. **No migration guide** - Just release notes
4. **No performance optimization** - Field filtering is already O(n) with small n

---

## ✅ Success Criteria

- [ ] All auto-fields appear in GraphQL schema introspection
- [ ] Fields queryable without "Cannot query field X" errors
- [ ] Fields only in response when requested (GraphQL spec compliance)
- [ ] PrintOptim 138 tests pass
- [ ] FraiseQL test suite passes
- [ ] Implementation completed in ~3 hours

---

## 🚀 Implementation Order

1. **Read Phase 1** - Understand decorator fix approach
2. **Implement Phase 1** - TDD: Write tests, fix decorator, pass tests
3. **Execute Phase 2** - Verify schema generation, test queries, validate externally
4. **Complete Phase 3** - Document, commit, close issue

---

## 📝 CTO Feedback Summary

From `/tmp/fraiseql-schema-fix-simplified-plan.md`:

> ✅ APPROVE their proposal WITH these modifications:
> 1. Add Rust response fix (they didn't address this)
>    - Let GraphQL executor filter fields
>    - No manual selection checking needed
> 2. Add updatedFields to auto-injected fields
>    - Not in cascade spec, but useful extension
>    - Already working, just needs schema registration
> 3. Remove backward compat complexity
>    - You're sole user
>    - Fast iteration over stability
> 4. Python decorator fix (keep as-is)
>    - Their approach is correct
>    - Add updatedFields to the list

**Confidence: 95%** - This will solve all issues once implemented.

⚠️ **CORRECTION**: After code investigation, CTO feedback about "GraphQL executor filtering" was incorrect for FraiseQL mutations. Rust changes ARE required. See `CRITICAL-ADDENDUM.md` for details.

---

**Next**: [Phase 1: Python Decorator Fix](./phase-1-decorator-fix.md)
