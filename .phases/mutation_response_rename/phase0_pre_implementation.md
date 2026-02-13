# Phase 0: Pre-Implementation Checklist

## Objective

Prepare the environment and ensure we have a safe starting point for the rename.

## Duration

15-30 minutes

---

## Task 0.1: Verify Current State

### Check for External Users

```bash
# Confirm no external users (already verified, but double-check)
# If fraiseql is published to PyPI, check download stats
# If there are external users, STOP and reconsider this rename
```

**Expected**: No external users (confirmed)

### Check Git Status

```bash
cd /home/lionel/code/fraiseql
git status
```

**Expected**: Clean working tree, or only uncommitted work you're aware of

### Check Current Branch

```bash
git branch --show-current
```

**Expected**: Probably `release/v1.7.2` or `dev`

---

## Task 0.2: Run Initial Tests

### Run Full Test Suite

```bash
cd /home/lionel/code/fraiseql
uv run pytest tests/ -v
```

**Expected**: All tests passing

**If tests fail**:

- Fix failures first before proceeding
- Or note them if they're known/unrelated issues

### Check Rust Build

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo build --release
cargo test
```

**Expected**: Build succeeds, tests pass

---

## Task 0.3: Create Backup Branch

### Create Backup

```bash
cd /home/lionel/code/fraiseql
git checkout -b backup/before-mutation-response-rename
git push origin backup/before-mutation-response-rename
```

**Purpose**: Safety net if we need to revert everything

### Return to Original Branch

```bash
# Return to your working branch
git checkout release/v1.7.2  # or whatever branch you were on
```

---

## Task 0.4: Create Working Branch

### Create Feature Branch

```bash
git checkout -b refactor/rename-to-mutation-response
```

**Branch naming**: Uses `refactor/` prefix since this is internal cleanup

---

## Task 0.5: Initial Scope Verification

### Count Current References

```bash
cd /home/lionel/code/fraiseql

# Count mutation_result_v2 occurrences
grep -r "mutation_result_v2" --include="*.py" --include="*.rs" --include="*.sql" --include="*.md" . | wc -l
```

**Expected**: ~40-60 occurrences

**Record this number**: ____________

### List All Files with References

```bash
grep -r "mutation_result_v2" --include="*.py" --include="*.rs" --include="*.sql" --include="*.md" . -l | sort
```

**Expected files** (from analysis):

```
./CHANGELOG.md
./docs/features/graphql-cascade.md
./docs/features/mutation-result-reference.md
./docs/features/sql-function-return-format.md
./docs/mutations/status-strings.md
./examples/mutations_demo/v2_init.sql
./examples/mutations_demo/v2_mutation_functions.sql
./fraiseql_rs/src/lib.rs
./fraiseql_rs/src/mutation/mod.rs
./migrations/trinity/005_add_mutation_result_v2.sql
./src/fraiseql/mutations/entity_flattener.py
./src/fraiseql/mutations/rust_executor.py
./tests/fixtures/cascade/conftest.py
./tests/integration/graphql/mutations/test_unified_camel_case.py
./tests/test_mutations/test_status_taxonomy.py
```

**Verify**: Does your list match? If not, note the differences.

---

## Task 0.6: Check for Generated Files

### Look for Code Generation

```bash
# Check for generated Rust files
find fraiseql_rs/target -name "*.rs" 2>/dev/null | head -5

# Check for generated Python files
find . -name "*_pb2.py" -o -name "*_generated.py" 2>/dev/null
```

**Action**: If generated files exist with `mutation_result_v2`, note them for regeneration after rename

---

## Task 0.7: Verify Dependencies

### Check Python Dependencies

```bash
uv pip list | grep -i fraiseql
```

**Expected**: Local development version

### Check Rust Dependencies

```bash
cd fraiseql_rs
cargo tree | head -20
```

**Expected**: Standard dependencies, nothing unexpected

---

## Task 0.8: Document Current State

### Create State Snapshot

```bash
cat > /tmp/rename-snapshot.txt <<'EOF'
Mutation Response Rename - Pre-Implementation Snapshot
Date: $(date)
Branch: $(git branch --show-current)
Commit: $(git rev-parse HEAD)
Test Status: [PASS/FAIL]
mutation_result_v2 count: [NUMBER]
EOF

cat /tmp/rename-snapshot.txt
```

**Save this file** for reference

---

## Acceptance Criteria

Check off each item:

- [ ] No external users confirmed
- [ ] Git working tree clean (or intentionally dirty)
- [ ] All tests passing (or known failures documented)
- [ ] Rust builds successfully
- [ ] Backup branch created and pushed
- [ ] Working branch `refactor/rename-to-mutation-response` created
- [ ] Initial reference count recorded: ______
- [ ] File list matches expected (or differences noted)
- [ ] No unexpected generated files
- [ ] Dependencies verified
- [ ] State snapshot created

---

## If Any Item Fails

### Tests Failing

```bash
# Document the failures
uv run pytest tests/ -v --tb=short > /tmp/test-failures.txt

# Decision: Fix first or note as known issues?
```

### Git Not Clean

```bash
# Option 1: Commit or stash current work
git stash

# Option 2: Create a temporary commit
git add -A
git commit -m "WIP: before mutation_response rename"
```

### Backup Branch Exists

```bash
# Delete old backup if safe to do so
git branch -D backup/before-mutation-response-rename
git push origin --delete backup/before-mutation-response-rename

# Then recreate
git checkout -b backup/before-mutation-response-rename
```

---

## Next Steps

Once all checks pass:

✅ **Proceed to Phase 1: PostgreSQL Migration Files**

Location: `.phases/mutation_response_rename/phase1_postgresql.md`

---

## Rollback

If you need to abort at this stage:

```bash
# Switch back to original branch
git checkout release/v1.7.2  # or your original branch

# Delete working branch if created
git branch -D refactor/rename-to-mutation-response
```

**No harm done** - all changes are uncommitted

---

**Phase Status**: ⏸️ Ready to Start
**Estimated Time**: 15-30 minutes
**Next Phase**: Phase 1 - PostgreSQL Migration Files
