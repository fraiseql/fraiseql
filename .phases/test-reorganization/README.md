# Test Reorganization Project - Greenfield Approach

## Overview

Reorganize the Rust mutation tests from a fragmented, feature-based structure to a clean, pipeline-based structure that reflects the actual data flow through the mutation system.

**Status**: Planning Phase
**Priority**: Medium (Technical Debt Reduction)
**Estimated Time**: 3-4 hours
**Risk Level**: Medium (large refactor, but with verification at each step)

---

## Current State

**10 test files, ~2000 lines, fragmented organization:**

```
fraiseql_rs/src/mutation/tests/
├── format_tests.rs (405 lines)              # MIXED: parsing + response building
├── auto_populate_fields_tests.rs (196)      # Response building (isolated)
├── error_array_generation.rs (130)          # Response building (isolated)
├── validation_tests.rs (162)                # Response building (v1.8.0 routing)
├── integration_tests.rs (442)               # End-to-end (keep)
├── edge_case_tests.rs (359)                 # MIXED: various concerns
├── status_tests.rs (133)                    # Classification (good)
├── composite_tests.rs (64)                  # Parsing (isolated)
├── property_tests.rs (92)                   # Property-based (keep)
└── mod.rs (18)                              # Module imports
```

**Problems**:
- ❌ Unclear where to add new tests
- ❌ Duplication across files
- ❌ Mixed concerns (format_tests does parsing AND response building)
- ❌ Feature-based organization doesn't reflect architecture
- ❌ "Archaeological layers" accumulating over time

---

## Target State

**5 test files, ~2000 lines, pipeline-based organization:**

```
fraiseql_rs/src/mutation/tests/
├── parsing.rs (~470 lines)                  # Stage 1: JSON → MutationResult
├── classification.rs (~133 lines)           # Stage 2: Status taxonomy
├── response_building.rs (~900 lines)        # Stage 3: MutationResult → JSON
├── integration.rs (~442 lines)              # Stage 4: End-to-end
├── properties.rs (~92 lines)                # Property-based tests
└── mod.rs (~25 lines)                       # Module imports + shared utilities
```

**Benefits**:
- ✅ Clear responsibility boundaries
- ✅ Tests organized by data pipeline stage
- ✅ Easy to find where to add new tests
- ✅ Reduced duplication
- ✅ Reflects actual architecture

---

## Migration Phases

### Phase 0: Planning & Preparation (30 min)
- Create test inventory (map every test to new location)
- Set up rollback strategy
- Create backup branch

### Phase 1: Create New Test Structure (1 hour)
- Create new test files with proper structure
- Copy tests from old files to new locations
- Add comprehensive section headers
- Keep old files intact for verification

### Phase 2: Update Imports & Verify (30 min)
- Update mod.rs to import new test files
- Run all tests (old + new should both pass)
- Verify no tests lost or duplicated

### Phase 3: Remove Old Files (30 min)
- Comment out old file imports
- Verify all tests still pass
- Delete old test files
- Clean up mod.rs

### Phase 4: Documentation & Cleanup (30 min)
- Update test file headers
- Add migration notes to CHANGELOG
- Document new test organization
- Final verification

---

## Test Migration Mapping

### → `parsing.rs` (NEW)

**From `format_tests.rs` (lines 1-148)**:
- `test_parse_simple_format`
- `test_parse_simple_format_array`
- `test_parse_full_success_result`
- `test_parse_full_error_result`
- `test_parse_full_with_updated_fields`
- `test_format_detection_simple_vs_full`
- `test_parse_missing_status_fails`
- `test_parse_invalid_json_fails`
- `test_parse_simple_format_with_cascade`

**From `composite_tests.rs` (all)**:
- All PostgreSQL composite type parsing tests

**Total**: ~470 lines

### → `classification.rs` (RENAME)

**From `status_tests.rs` (rename file)**:
- All status taxonomy tests (keep as-is)

**Total**: ~133 lines

### → `response_building.rs` (NEW, largest consolidation)

**From `format_tests.rs` (lines 149-405)**:
- `test_build_simple_format_response`
- `test_build_simple_format_with_status_data_field`
- `test_build_full_success_response`
- `test_build_full_error_response`
- `test_build_simple_format_array_response`
- `test_build_simple_format_response_with_cascade`

**From `auto_populate_fields_tests.rs` (all)**:
- `test_success_response_has_status_field`
- `test_success_response_has_errors_field`
- `test_success_response_all_standard_fields`
- `test_success_status_preserves_detail`
- `test_success_fields_order`

**From `error_array_generation.rs` (all)**:
- `test_extract_identifier_from_failed_with_colon`
- `test_extract_identifier_from_noop_with_colon`
- `test_extract_identifier_from_simple_status`
- `test_generate_errors_array_auto`
- `test_generate_errors_array_explicit`
- All other error array tests

**From `validation_tests.rs` (response routing tests)**:
- `test_noop_returns_error_type_v1_8`
- `test_not_found_returns_error_type_with_404`
- `test_conflict_returns_error_type_with_409`
- `test_success_with_null_entity_returns_error`
- `test_error_response_includes_cascade`

**From `edge_case_tests.rs` (response building edge cases)**:
- Tests related to response building (TBD: need to analyze file)

**Total**: ~900 lines

### → `integration.rs` (RENAME)

**From `integration_tests.rs` (rename file)**:
- All end-to-end tests (keep as-is)

**Total**: ~442 lines

### → `properties.rs` (RENAME)

**From `property_tests.rs` (rename file)**:
- All property-based tests (keep as-is)

**Total**: ~92 lines

---

## Rollback Strategy

### If Things Go Wrong

**Phase 1-2**: Keep old files, easy rollback
```bash
git checkout fraiseql_rs/src/mutation/tests/
```

**Phase 3**: Old files deleted but recoverable
```bash
git checkout HEAD~1 fraiseql_rs/src/mutation/tests/
```

**Emergency**: Full rollback
```bash
git reset --hard <commit-before-reorganization>
```

### Verification Points

After each phase:
```bash
# Must pass:
cargo test --lib mutation

# Should see ~same number of tests
# Example: 45 tests before → 45 tests after
```

---

## Success Criteria

- [ ] All existing tests pass
- [ ] No tests lost (count matches)
- [ ] No tests duplicated
- [ ] New structure documented
- [ ] Old test files deleted
- [ ] mod.rs updated correctly
- [ ] CI/CD passes
- [ ] Code review approved

---

## Files Affected

### New Files (5)
- `fraiseql_rs/src/mutation/tests/parsing.rs`
- `fraiseql_rs/src/mutation/tests/classification.rs` (renamed from status_tests.rs)
- `fraiseql_rs/src/mutation/tests/response_building.rs`
- `fraiseql_rs/src/mutation/tests/integration.rs` (renamed from integration_tests.rs)
- `fraiseql_rs/src/mutation/tests/properties.rs` (renamed from property_tests.rs)

### Modified Files (1)
- `fraiseql_rs/src/mutation/tests/mod.rs`

### Deleted Files (8)
- `fraiseql_rs/src/mutation/tests/format_tests.rs`
- `fraiseql_rs/src/mutation/tests/auto_populate_fields_tests.rs`
- `fraiseql_rs/src/mutation/tests/error_array_generation.rs`
- `fraiseql_rs/src/mutation/tests/validation_tests.rs`
- `fraiseql_rs/src/mutation/tests/edge_case_tests.rs`
- `fraiseql_rs/src/mutation/tests/composite_tests.rs`
- `fraiseql_rs/src/mutation/tests/status_tests.rs` (renamed, not deleted)
- `fraiseql_rs/src/mutation/tests/integration_tests.rs` (renamed, not deleted)
- `fraiseql_rs/src/mutation/tests/property_tests.rs` (renamed, not deleted)

---

## Timeline

| Phase | Duration | Blocking | Risk |
|-------|----------|----------|------|
| Phase 0 | 30 min | No | Low |
| Phase 1 | 1 hour | No | Low |
| Phase 2 | 30 min | Yes | Medium |
| Phase 3 | 30 min | Yes | High |
| Phase 4 | 30 min | No | Low |

**Total**: 3-4 hours

**Blocking**: Phases 2-3 are blocking (can't do other work while in progress)

---

## Next Steps

1. **Read Phase Plans**: Review phase-0 through phase-4 markdown files
2. **Create Backup**: `git checkout -b test-reorganization-backup`
3. **Execute Phase 0**: Create test inventory and analysis
4. **Get Approval**: Review inventory before proceeding
5. **Execute Phases 1-4**: Follow detailed phase plans sequentially

---

## References

- Original discussion: QA review of auto-populate mutation fields
- Issue: "Archaeological layers of tests" accumulating
- Goal: Clean, maintainable test structure for long-term health

---

**Created**: 2025-12-11
**Status**: ✅ Ready for Phase 0
**Owner**: Claude Code (Senior Architect)
