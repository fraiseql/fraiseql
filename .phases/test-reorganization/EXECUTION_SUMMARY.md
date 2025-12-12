# Test Reorganization - Execution Summary

## Quick Start

To execute this reorganization plan, run each phase sequentially:

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs

# Phase 0: Planning (30 min)
# - Create test inventory
# - Analyze edge_case_tests.rs
# - Map all tests to new locations
# See: .phases/test-reorganization/phase-0-planning.md

# Phase 1: Create New Structure (1 hour)
# - Create 5 new test files
# - Copy tests from old files
# - Keep old files for verification
# See: .phases/test-reorganization/phase-1-create-new-structure.md

# Phase 2: Verify Tests (30 min)
# - Update mod.rs imports (old + new)
# - Run all tests (should pass)
# - Verify no tests lost
# See: .phases/test-reorganization/phase-2-verify-tests.md

# Phase 3: Remove Old Files (30 min)
# - Delete old test files
# - Clean up mod.rs
# - Verify tests still pass
# See: .phases/test-reorganization/phase-3-remove-old-files.md

# Phase 4: Documentation (30 min)
# - Update CHANGELOG.md
# - Create README.md in tests/
# - Create migration notes
# - Final cleanup
# See: .phases/test-reorganization/phase-4-documentation.md
```

**Total Time**: 3-4 hours

---

## Phase Summary

### Phase 0: Planning & Preparation
**Duration**: 30 minutes
**Deliverables**:
- Test inventory
- Migration mapping
- edge_case_tests.rs analysis
- Backup branches created

**Key Activities**:
- Create `test-reorganization-backup` branch
- Run baseline test count
- Analyze all test files
- Create complete migration map

### Phase 1: Create New Structure
**Duration**: 1 hour
**Deliverables**:
- 5 new test files created
- Tests copied with organization
- Old files preserved
- mod.rs updated (both old + new)

**Key Activities**:
- Create `parsing.rs` (~470 lines)
- Create `response_building.rs` (~900 lines)
- Rename `status_tests.rs` → `classification.rs`
- Rename `integration_tests.rs` → `integration.rs`
- Rename `property_tests.rs` → `properties.rs`

### Phase 2: Verify Tests
**Duration**: 30 minutes
**Deliverables**:
- All tests pass (old + new together)
- Test count verification
- Individual module verification
- Phase 2 report

**Key Activities**:
- Run tests with both structures active
- Verify test counts match
- Check for missing imports
- Confirm no tests lost

### Phase 3: Remove Old Files
**Duration**: 30 minutes
**Deliverables**:
- Old files deleted
- mod.rs cleaned up
- Tests pass with new structure only
- Safety commits created

**Key Activities**:
- Comment out old imports
- Test with only new files
- Delete old test files
- Create safety tags

### Phase 4: Documentation
**Duration**: 30 minutes
**Deliverables**:
- CHANGELOG.md updated
- Test README.md created
- Migration notes created
- PR ready

**Key Activities**:
- Document reorganization
- Create test navigation guide
- Add migration notes for developers
- Push and create PR

---

## File Mapping Reference

| Old File | New Location | Lines |
|----------|--------------|-------|
| `format_tests.rs` | → `parsing.rs` + `response_building.rs` | 405 → 470 + 900 |
| `auto_populate_fields_tests.rs` | → `response_building.rs` | 196 → (merged) |
| `error_array_generation.rs` | → `response_building.rs` | 130 → (merged) |
| `validation_tests.rs` | → `response_building.rs` | 162 → (merged) |
| `composite_tests.rs` | → `parsing.rs` | 64 → (merged) |
| `edge_case_tests.rs` | → distributed | 359 → (distributed) |
| `status_tests.rs` | → `classification.rs` (renamed) | 133 → 133 |
| `integration_tests.rs` | → `integration.rs` (renamed) | 442 → 442 |
| `property_tests.rs` | → `properties.rs` (renamed) | 92 → 92 |

**Total**: 10 files (~2000 lines) → 5 files (~2000 lines)

---

## Success Criteria

### Must Pass Before Each Phase

**Phase 1 → Phase 2**:
- [ ] New files compile
- [ ] No syntax errors
- [ ] All sections have headers

**Phase 2 → Phase 3**:
- [ ] All tests pass (old + new together)
- [ ] Test count verification complete
- [ ] No tests lost confirmed

**Phase 3 → Phase 4**:
- [ ] Old files deleted
- [ ] All tests pass (new only)
- [ ] Test count matches baseline
- [ ] No imports broken

**Phase 4 → Complete**:
- [ ] Documentation complete
- [ ] PR created
- [ ] Team notified

---

## Rollback Procedures

### Rollback from Phase 1 or 2
```bash
git checkout test-reorganization-backup
git branch -D refactor/test-reorganization
# Start over
```

### Rollback from Phase 3
```bash
git checkout test-reorg-phase2-complete -- src/mutation/tests/
# Restore state before deletion
```

### Rollback from Phase 4
```bash
git reset --hard test-reorg-phase3-complete
# Restore to before documentation
```

---

## Common Issues

### Issue: "Cannot find module"
**Phase**: 2 or 3
**Solution**: Check mod.rs imports

### Issue: "Test count doesn't match"
**Phase**: 2
**Solution**: Review migration map, find missing tests

### Issue: "Tests failing"
**Phase**: Any
**Solution**: Compare with original file, check for copy errors

### Issue: "Duplicate test names"
**Phase**: 2 (expected)
**Solution**: Continue to Phase 3 to resolve

---

## Tools & Commands

### Useful Commands

```bash
# Count tests in a file
grep -c "^fn test_" src/mutation/tests/FILE.rs

# Find test by name
grep -r "test_name" src/mutation/tests/

# Run specific test file
cargo test --lib mutation::tests::parsing

# Compare test counts
cargo test --lib mutation 2>&1 | grep "test result:"

# Check git status
git status --short
```

### Test Verification Script

```bash
#!/bin/bash
# Save as: verify-tests.sh

echo "=== Test Verification ==="

echo "Running all mutation tests..."
cargo test --lib mutation 2>&1 | tee /tmp/test-results.log

echo ""
echo "Test Summary:"
grep "test result:" /tmp/test-results.log

echo ""
echo "Test Counts by Module:"
for module in parsing classification response_building integration properties; do
    count=$(cargo test --lib mutation::tests::$module 2>&1 | grep "test result:" | grep -oP '\d+ passed' | cut -d' ' -f1)
    echo "  $module: $count tests"
done

echo ""
echo "Total Lines:"
wc -l src/mutation/tests/*.rs | tail -1
```

---

## Documentation Files

After completion, the following documentation will exist:

1. **CHANGELOG.md** (project root)
   - Test reorganization entry
   - Migration details

2. **src/mutation/tests/README.md** (NEW)
   - Test organization guide
   - Where to add tests
   - File purpose descriptions

3. **.phases/test-reorganization/** (project root)
   - Phase 0-4 detailed plans
   - This execution summary
   - Migration notes

4. **Test file headers** (updated)
   - Comprehensive documentation in each file
   - Clear section organization

---

## Timeline

| Day | Activity | Duration |
|-----|----------|----------|
| Day 1 | Phases 0-2 | 2 hours |
| Day 1 | Phase 3 | 30 minutes |
| Day 1 | Phase 4 | 30 minutes |
| Day 1 | PR & Review | 1 hour |

**Recommended**: Do all phases in one session for consistency.

---

## Questions?

- **Why 5 files?** - Maps to data pipeline stages (parsing → classification → response building → integration → properties)
- **Is this breaking?** - No functional changes, only reorganization
- **Can I rollback?** - Yes, git history and tags preserved
- **What about PRs?** - Rebase and move tests to new files (see migration notes)

---

## Ready to Start?

1. **Read**: Phase 0 plan (`.phases/test-reorganization/phase-0-planning.md`)
2. **Prepare**: Create backup branch
3. **Execute**: Follow phase plans sequentially
4. **Verify**: Each phase before proceeding
5. **Complete**: PR and team notification

**Good luck!** 🚀
