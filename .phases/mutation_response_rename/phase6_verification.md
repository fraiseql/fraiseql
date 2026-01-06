# Phase 6: Final Verification (v1.8.0)

## Objective
Comprehensive verification that the v1.8.0 alias strategy is correctly implemented.

## Duration
1 hour

---

## Task 6.1: Global Search

```bash
cd /home/lionel/code/fraiseql

# Check that mutation_response is widely used
grep -r "mutation_response" \
  --include="*.py" \
  --include="*.rs" \
  --include="*.sql" \
  --include="*.md" \
  --exclude-dir=".git" \
  --exclude-dir="target" \
  --exclude-dir=".venv" \
  . | wc -l
# Expected: 50+ results

# Check that mutation_result_v2 only appears in:
# - Migration 006 (as alias definition)
# - Deprecation notices in docs
# - Backward compatibility tests
grep -r "mutation_result_v2" \
  --include="*.py" \
  --include="*.rs" \
  --include="*.sql" \
  --include="*.md" \
  --exclude-dir=".git" \
  --exclude-dir="target" \
  --exclude-dir=".venv" \
  .
# Expected: Only in migration 006, deprecation notices, compatibility tests
```

---

## Task 6.2: Run Full Test Suite

```bash
# Python tests
uv run pytest tests/ -v

# Rust tests
cd fraiseql_rs && cargo test

# Type checking
uv run mypy src/fraiseql/mutations/

# Linting
uv run ruff check src/
```

**All must pass**

---

## Task 6.3: Verify Migration Files

```bash
# New migration exists
ls -la migrations/trinity/006_add_mutation_response.sql

# Old migration still exists (for existing users)
ls -la migrations/trinity/005_add_mutation_result_v2.sql

# Both types defined in 006
grep -c "CREATE TYPE mutation_response" migrations/trinity/006_add_mutation_response.sql
# Expected: 1

grep -c "CREATE TYPE mutation_result_v2" migrations/trinity/006_add_mutation_response.sql
# Expected: 1 (as alias)

# Deprecation comment exists
grep -i "DEPRECATED" migrations/trinity/006_add_mutation_response.sql
# Expected: Found
```

---

## Task 6.4: Review Git Status

```bash
git status
git log --oneline -5
```

**Expected**: 5 clean commits (one per phase)

---

## Acceptance Criteria (v1.8.0)

- [ ] Migration 006 creates both `mutation_response` and `mutation_result_v2`
- [ ] `mutation_result_v2` has deprecation comment
- [ ] 50+ `mutation_response` references found
- [ ] `mutation_result_v2` only in migration, docs, tests
- [ ] Old migration 005 still exists (unchanged)
- [ ] New migration 006 exists
- [ ] All Python tests pass
- [ ] All Rust tests pass
- [ ] No type errors
- [ ] No linting errors
- [ ] Backward compatibility test passes
- [ ] 6 git commits (phases 0-5)

---

## Final Steps

### Push to Remote

```bash
git push origin refactor/rename-to-mutation-response
```

### Create PR (if using PR workflow)

```bash
gh pr create \
  --title "feat: introduce mutation_response with backward compatibility (v1.8.0)" \
  --body "Introduce mutation_response as canonical name while maintaining backward compatibility.

## Changes
- PostgreSQL: Both `mutation_response` and `mutation_result_v2` supported
- Migration 006: Creates both types (v2 is deprecated alias)
- All helper functions return `mutation_response`
- Documentation updated with deprecation notices
- Examples updated to demonstrate best practices
- Tests updated to use new name

## Backward Compatibility
- ✅ Both type names work in v1.8.0-v1.9.x
- ✅ No breaking changes
- ✅ Existing code continues to work
- ❌ `mutation_result_v2` will be removed in v2.0.0

## Migration Path
Users can migrate at their own pace:
1. v1.8.x-v1.9.x: Both names supported
2. v2.0.0: Only `mutation_response`

See `migrations/trinity/README.md` for migration guide.

## Verification
- ✅ All tests passing
- ✅ Type checking clean
- ✅ Backward compatibility test added
- ✅ Both type names verified working"
```

### Merge Strategy

**Option 1**: Direct merge to dev
```bash
git checkout dev
git merge refactor/rename-to-mutation-response
git push origin dev
```

**Option 2**: Squash merge (cleaner history)
```bash
git checkout dev
git merge --squash refactor/rename-to-mutation-response
git commit -m "feat: introduce mutation_response with backward compatibility (v1.8.0)

- Add mutation_response as canonical PostgreSQL type name
- Maintain mutation_result_v2 as deprecated alias
- Update all documentation and examples
- Both names work in v1.8.0-v1.9.x
- Remove alias in v2.0.0

Migration guide in migrations/trinity/README.md"
git push origin dev
```

---

## SUCCESS!

✅ Rename complete
✅ All tests passing
✅ Documentation updated
✅ Clean git history

---

## Cleanup

```bash
# Delete working branch (after merge)
git branch -D refactor/rename-to-mutation-response

# Optional: Delete backup branch
git branch -D backup/before-mutation-response-rename
git push origin --delete backup/before-mutation-response-rename
```

---

**Phase Status**: ⏸️ Ready to Start
**Version**: v1.8.0 (alias strategy)
**Dependencies**: Phases 0-5 complete
**Estimated Time**: 1 hour
**Breaking**: No (backward compatible)
