# Phase: CASCADE Fix for FraiseQL v1.8.0-alpha.5

**Status:** Ready for Implementation
**Estimated Time:** 4-6 hours
**Complexity:** Low
**Impact:** High (Fixes critical CASCADE nesting bug)

---

## What This Phase Does

Fixes the CASCADE nesting bug in FraiseQL where CASCADE data appears inside entity objects instead of at the GraphQL success wrapper level.

**Before (Bug):**
```json
{
  "createAllocation": {
    "allocation": {
      "cascade": { /* WRONG LOCATION! */ }
    },
    "cascade": {}
  }
}
```

**After (Fixed):**
```json
{
  "createAllocation": {
    "allocation": { /* No cascade */ },
    "cascade": { /* CORRECT LOCATION! */ }
  }
}
```

---

## Why This Is Simple

PrintOptim has already migrated to the perfect 8-field `mutation_response` composite type with:
- ✅ Explicit CASCADE field at Position 7
- ✅ entity_type field at Position 4
- ✅ ~70 mutation functions already updated

**All we need:** Teach FraiseQL Rust to parse this 8-field format.

---

## Documents in This Phase

| File | Purpose | Read Time |
|------|---------|-----------|
| **00_OVERVIEW.md** | Problem statement, goals, success criteria | 10 min |
| **01_IMPLEMENTATION_PLAN.md** | Step-by-step implementation with complete code | 20 min |
| **02_TESTING_STRATEGY.md** | Comprehensive testing plan | 15 min |
| **03_QUICK_START.md** | Quick reference for implementation | 5 min |
| **README.md** | This file | 3 min |

---

## Quick Start

```bash
# If you want the quick version
cat 03_QUICK_START.md

# If you want the detailed version
cat 01_IMPLEMENTATION_PLAN.md

# If you want to understand the problem first
cat 00_OVERVIEW.md
```

---

## Implementation Summary

### Files to Create
1. `fraiseql_rs/src/mutation/postgres_composite.rs` (~80 lines)

### Files to Modify
2. `fraiseql_rs/src/mutation/mod.rs` (~5 lines)
3. `fraiseql_rs/src/mutation/tests.rs` (~100 lines tests)

### Files to Update
4. `fraiseql_rs/Cargo.toml` (version bump)
5. `pyproject.toml` (version bump)
6. `CHANGELOG.md` (release notes)

**Total Code:** ~185 lines (mostly tests)

---

## Key Design Decisions

### ✅ Use 8-Field Composite Type Parser

**Why:** PrintOptim already has the perfect structure with CASCADE at Position 7

**Alternative Considered:** Extract CASCADE from metadata
**Why Rejected:** More complex, requires changes to PrintOptim backend

### ✅ Fallback to Simple Format

**Why:** Maintains backward compatibility with non-PrintOptim users

**Impact:** Zero breaking changes

### ✅ Eager Deserialization

**Why:** Simple, predictable, Rust is fast enough

**Alternative Considered:** Lazy deserialization
**Why Rejected:** Over-engineering for small payloads

---

## Prerequisites

### Required
- Rust toolchain (stable)
- Python 3.10+
- uv (Python package manager)
- FraiseQL repository cloned
- PrintOptim repository for testing

### Optional
- PostgreSQL for integration tests
- Docker for full E2E tests

---

## Timeline

```
Hour 1-2:  Implement parser module
Hour 2-3:  Add tests and verify
Hour 3-4:  Integration testing with PrintOptim
Hour 4:    Version bump and release
```

**Buffer:** Add 1-2 hours for unexpected issues

---

## Success Criteria

### Must Have
- [ ] CASCADE at success wrapper level
- [ ] CASCADE NOT in entity
- [ ] All tests pass
- [ ] Zero breaking changes
- [ ] Published to PyPI

### Should Have
- [ ] Test coverage > 90%
- [ ] Clear error messages
- [ ] Documentation updated

### Nice to Have
- [ ] CI/CD pipeline green
- [ ] Performance benchmarks

---

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Backward compat break | Low | Medium | Fallback to simple format |
| Wrong field mapping | Low | High | Comprehensive tests |
| PrintOptim incompatibility | Very Low | High | Test with real mutations |

---

## Dependencies

### Upstream
- None (self-contained change)

### Downstream
- PrintOptim must upgrade to fraiseql>=1.8.0a5 to get fix

---

## Rollback Plan

If issues found after release:

1. **Revert commit:**
   ```bash
   git revert <commit-hash>
   git push
   ```

2. **Publish rollback version:**
   ```bash
   # Bump to v1.8.0-alpha.6 with revert
   uv publish
   ```

3. **Notify PrintOptim team** to stay on v1.8.0-alpha.4

**Recovery Time:** < 1 hour

---

## Testing Strategy

### Unit Tests (Rust)
- 20-25 tests for parser
- 100% coverage of new code
- Run time: < 1 second

### Integration Tests (Python)
- 5-10 tests with real mutations
- PostgreSQL required
- Run time: 5-10 seconds

### E2E Tests (PrintOptim)
- 2-3 full mutation flows
- Full PrintOptim environment
- Run time: 10-30 seconds

**Total Test Time:** < 1 minute

---

## Post-Release

### Immediate (Day 1)
1. Update PrintOptim dependency to fraiseql>=1.8.0a5
2. Deploy to dev environment
3. Monitor logs for errors

### Short Term (Week 1)
1. Gather feedback from team
2. Monitor Sentry for exceptions
3. Plan stable v1.8.0 release

### Long Term (Month 1)
1. Remove backward compatibility code (if safe)
2. Add deprecation warnings for old format
3. Plan v2.0 if breaking changes needed

---

## Related Documents

- **Design Document:** `/tmp/fraiseql_mutation_pipeline_design.md`
- **Bug Report:** `/tmp/fraiseql_v1.8.0a4_test_report.md`
- **PrintOptim Migration:** `/home/lionel/code/printoptim_backend_manual_migration/.phases/phase_fraiseql_mutation_response/`
- **GraphQL CASCADE Spec:** `~/code/graphql-cascade/`

---

## Questions?

**Q: Why not change the database schema?**
A: PrintOptim already migrated! We just need to parse it correctly.

**Q: Will this break existing code?**
A: No. Fallback to simple format ensures backward compatibility.

**Q: How long will this take?**
A: 4-6 hours for a focused developer.

**Q: What if tests fail?**
A: See 02_TESTING_STRATEGY.md for debugging guide.

**Q: Do I need to understand the full design?**
A: No. Read 03_QUICK_START.md and start coding.

---

## Get Started

```bash
# Read the quick start
cat 03_QUICK_START.md

# Or dive into implementation
cat 01_IMPLEMENTATION_PLAN.md

# Questions? Check the overview
cat 00_OVERVIEW.md
```

🚀 **Ready to fix the bug!**
