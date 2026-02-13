# Mutation Response Rename - Direct Strategy (v1.8.0)

## Decision: Direct Rename (No Backward Compatibility)

**Rationale**: No external users → no compatibility burden

## What Changed

**Strategy switched from:**

- ❌ v1.8.0 alias strategy (both names supported)
- ❌ Deprecation warnings and migration timeline

**To:**

- ✅ Direct rename everywhere
- ✅ Clean, simple find/replace
- ✅ No aliases, no deprecation machinery

## Implementation

### Simple Approach

```bash
# 1. Rename migration file
git mv migrations/trinity/005_add_mutation_result_v2.sql \
      migrations/trinity/005_add_mutation_response.sql

# 2. Find/replace in all files
find . -type f \( -name "*.sql" -o -name "*.py" -o -name "*.rs" -o -name "*.md" \) \
  -not -path "./.git/*" \
  -not -path "./target/*" \
  -not -path "./.venv/*" \
  -exec sed -i 's/mutation_result_v2/mutation_response/g' {} +

# 3. Verify
grep -r "mutation_result_v2" \
  --include="*.py" --include="*.rs" --include="*.sql" --include="*.md" \
  --exclude-dir=".git" --exclude-dir="target" --exclude-dir=".venv" \
  .
# Should find ZERO results
```

## Execution Plan

**Total time**: ~6 hours (was 12 hours with alias strategy)

| Phase | Duration | Action |
|-------|----------|--------|
| 0 | 30 min | Pre-flight checks, branch setup |
| 1 | 1 hour | PostgreSQL: Rename migration, find/replace |
| 2 | 1 hour | Rust: Update comments and docs |
| 3 | 1 hour | Python: Update comments and docs |
| 4 | 1 hour | Documentation: Clean examples |
| 5 | 1 hour | Tests: Update to new name |
| 6 | 30 min | Verification and commit |

## Success Criteria

- [ ] Zero `mutation_result_v2` references anywhere
- [ ] Migration file renamed to `005_add_mutation_response.sql`
- [ ] All tests pass
- [ ] Documentation clean (no deprecation notices needed)
- [ ] Clean git history (1 commit per phase)

## Ready to Execute

All phase files ready:

- `phase0_pre_implementation.md` ✅
- `phase1_postgresql.md` ✅ (simple rename)
- `phase2_rust.md` ✅
- `phase3_python.md` ✅
- `phase4_documentation.md` ✅
- `phase5_tests.md` ✅
- `phase6_verification.md` ✅

Start with: `cat .phases/mutation_response_rename/phase0_pre_implementation.md`
