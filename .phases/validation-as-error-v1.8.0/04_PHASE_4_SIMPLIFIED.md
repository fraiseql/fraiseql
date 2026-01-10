# Phase 4: Testing (Simplified for Internal Use)

**Timeline:** Immediate (no external users to migrate)
**Risk Level:** LOW (test updates only)
**Dependencies:** Phases 1-3
**Blocking:** Phase 5 (release)

---

## Why Simplified?

**You are the sole user of FraiseQL (PrintOptim backend).**

**What we DON'T need:**
- ❌ Deprecation warnings (no external users to warn)
- ❌ Backward compatibility layers (just update PrintOptim)
- ❌ Public migration guides (internal knowledge only)
- ❌ Extensive documentation updates (you know the changes)
- ❌ Example repositories (you have PrintOptim)

**What we DO need:**
- ✅ Update FraiseQL's own tests
- ✅ Verify no regressions
- ✅ Basic changelog entry
- ✅ Quick internal notes for PrintOptim migration

---

## What to Do

### Step 1: Update FraiseQL Tests (If Needed)

**Check which tests need updates:**

```bash
# Find tests that might be affected
cd /home/lionel/code/fraiseql
uv run pytest tests/ -k "noop or validation" --co -q
```

**Most tests should already be updated** from Phase 1-3:
- ✅ Rust tests already updated (Phase 1)
- ✅ Schema generation tests already added (Phase 3)
- ✅ CASCADE tests already passing

**Only update if tests are failing.**

---

### Step 2: Quick Changelog Entry

**File:** `CHANGELOG.md` (or just git commit messages)

```markdown
## [1.8.0-beta.1] - 2025-12-07

### Breaking Changes
- Validation failures (noop:*) now return Error type with code 422
- Success type entity is always non-null
- Error types include REST-like code field (422, 404, 409, 500)

### Added
- Phase 1: Rust core with map_status_to_code()
- Phase 2: Python error config updates
- Phase 3: Schema generation with union types

### Migration (PrintOptim)
- Update Success types: Remove `| None` from entity fields
- Update Error types: Add `code: int` field
- Update GraphQL queries: Handle union types with fragments
- Update test assertions: Expect Error type for noop statuses
```

---

### Step 3: Internal Migration Notes

**File:** `.phases/validation-as-error-v1.8.0/PRINTOPTIM_MIGRATION.md`

```markdown
# PrintOptim Migration Notes

## Quick Summary
- FraiseQL v1.8.0-beta.1 changes how validation errors work
- noop:* now returns Error type (not Success with null entity)
- Error type has code field (422, 404, 409, 500)

## What to Update in PrintOptim

### 1. Success Types (~5-10 types)
```python
# Before
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # ❌

# After
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅
```

### 2. Error Types (~5-10 types)
```python
# Before
@fraiseql.failure
class CreateMachineError:
    message: str

# After
@fraiseql.failure
class CreateMachineError:
    code: int       # ✅ ADD
    status: str     # ✅ ADD
    message: str
```

### 3. Test Assertions (~30-50 tests)
```python
# Before
assert result["__typename"] == "CreateMachineSuccess"
assert result["machine"] is None  # ❌

# After
assert result["__typename"] == "CreateMachineError"
assert result["code"] == 422  # ✅
assert result["status"] == "noop:invalid_contract_id"
```

### 4. GraphQL Queries (Frontend - if any)
```graphql
# Before
mutation {
  createMachine(input: $input) {
    machine { id }
    message
  }
}

# After
mutation {
  createMachine(input: $input) {
    __typename
    ... on CreateMachineSuccess {
      machine { id }
    }
    ... on CreateMachineError {
      code
      status
      message
    }
  }
}
```

## Testing Strategy

1. Update FraiseQL to v1.8.0-beta.1
2. Run PrintOptim tests: `uv run pytest tests/`
3. Fix failing tests (expect ~30-50 failures)
4. Update Success/Error type definitions
5. Re-run tests until all pass
6. Deploy to staging
7. Test manually
8. Deploy to production

## Timeline
- FraiseQL: Already done (Phases 1-3)
- PrintOptim: 1-2 days to update tests + types
```

---

### Step 4: Remove Unnecessary Code

**Skip these sections from original Phase 4:**
- Section 4.2: Backward Compatibility Tests ❌ Not needed
- Section 4.3: Deprecation Warnings ❌ Not needed
- Section 4.4: Migration Guide (public) ❌ Not needed
- Section 4.5: Documentation Updates ❌ Not needed (you know the changes)

**Only keep:**
- Quick changelog entry
- Internal migration notes for PrintOptim

---

## Verification Checklist

### FraiseQL Tests
- [x] Rust tests passing (Phase 1)
- [x] Schema generation tests passing (Phase 3 - 23 tests)
- [x] CASCADE tests passing (no regression)
- [ ] Run full test suite to check for any missed tests

### Documentation
- [ ] Add changelog entry (1 minute)
- [ ] Write PrintOptim migration notes (5 minutes)
- [ ] Done!

---

## Commands

### Run All FraiseQL Tests
```bash
cd /home/lionel/code/fraiseql
uv run pytest tests/ -v
```

**Expected:** All tests should pass (Phases 1-3 already updated the critical ones)

### Add Changelog Entry
```bash
# Just add to CHANGELOG.md or rely on git commits
# You already have detailed commit messages
```

### Create PrintOptim Migration Notes
```bash
# Use the template above
cat > .phases/validation-as-error-v1.8.0/PRINTOPTIM_MIGRATION.md << 'EOF'
[Template from above]
EOF
```

---

## Estimated Time

**Total: 15-30 minutes**

- Run tests: 5 minutes
- Add changelog: 1 minute
- Write PrintOptim notes: 5-10 minutes
- Fix any failing tests: 5-15 minutes (if any)

---

## Next Steps

After Phase 4 (simplified):
1. Commit any test fixes
2. Proceed to Phase 5: Release v1.8.0-beta.1
3. Update PrintOptim to use v1.8.0-beta.1
4. Test PrintOptim on staging
5. Deploy to production

---

**Phase 4 is MUCH simpler for internal use!**

No need for:
- Backward compatibility
- Deprecation warnings
- Public documentation
- Migration examples
- External user communication

Just:
- ✅ Tests passing
- ✅ Quick notes
- ✅ Ready to release
