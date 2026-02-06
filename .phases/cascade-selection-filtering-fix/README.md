# CASCADE Selection Filtering Fix - Implementation Plan

**Feature**: Implement GraphQL selection-aware CASCADE filtering
**Version**: v1.8.1
**Status**: Ready for Implementation
**Created**: 2025-12-06

---

## Overview

Implement selection filtering for CASCADE data in GraphQL mutation responses. CASCADE should only be included when explicitly requested in the GraphQL selection set, following GraphQL specification principles.

---

## Problem Statement

Currently, CASCADE data is returned in all mutation responses regardless of whether the client requested it in the GraphQL selection. This:

1. **Violates GraphQL spec**: Only requested fields should be returned
2. **Wastes bandwidth**: Responses are 2-10x larger than necessary
3. **Security concern**: Exposes data clients didn't ask for
4. **Performance impact**: Unnecessary serialization and network transfer

---

## Solution

Implement selection-aware CASCADE filtering:

- Extract CASCADE selections from GraphQL query (Python layer)
- Pass selections to Rust mutation pipeline
- Filter CASCADE response based on client selection
- Support partial CASCADE selections

---

## Implementation Phases

This implementation follows TDD workflow with 7 phases:

### 1. 🔴 RED - Write Failing Tests

**File**: `1-RED-tests.md`

Write comprehensive tests that demonstrate the bug:

- CASCADE not returned when not requested
- Full CASCADE returned when requested
- Partial CASCADE selection support
- Edge cases and performance tests

**Expected**: Tests FAIL (demonstrates bug exists)

---

### 2. 🟢 GREEN - Implement Fix

**File**: `2-GREEN-implementation.md`

Minimal implementation to make tests pass:

- Extract CASCADE selections in Python executor
- Pass selections to Rust pipeline
- Filter CASCADE in Rust response builder
- Create cascade_filter.rs module

**Expected**: Tests PASS (bug fixed)

---

### 3. 🔵 REFACTOR - Code Cleanup

**File**: `3-REFACTOR-cleanup.md`

Improve code quality without changing behavior:

- Improve error handling
- Add type safety
- Optimize performance
- Extract helper functions
- Add unit tests

**Expected**: Tests still PASS, code is cleaner

---

### 4. 🟡 QA - Quality Assurance

**File**: `4-QA-validation.md`

Comprehensive testing and validation:

- Update existing CASCADE tests
- Test edge cases
- Validate GraphQL spec compliance
- Performance benchmarking
- Integration testing

**Expected**: All tests pass, no regressions

---

### 5. 🧹 CLEAN - Remove Artifacts

**File**: `5-CLEAN-artifacts.md`

Remove development artifacts:

- Remove debug print statements
- Remove explanatory comments about bug
- Remove TODO markers
- Clean up imports
- Format code

**Expected**: Production-ready code

---

### 6. 📝 DOCUMENTATION - Update Docs

**File**: `6-DOCUMENTATION-updates.md`

Update all documentation:

- CASCADE architecture docs
- Best practices guide
- Performance guide
- Migration guide
- API reference
- Changelog

**Expected**: Complete, accurate documentation

---

### 7. 🚀 COMMIT - Release

**File**: `7-COMMIT-and-release.md`

Final commit and release:

- Version bump to v1.8.1
- Comprehensive commit message
- Create git tag
- GitHub release
- PyPI release (if applicable)
- Announce to community

**Expected**: Feature shipped to users

---

## Key Files Modified

### Python

- `fraiseql/mutations/executor.py` - Extract CASCADE selections
- `fraiseql/mutations/cascade_selections.py` - Selection parser (already exists)

### Rust

- `fraiseql_rs/src/mutation/mod.rs` - Accept cascade_selections parameter
- `fraiseql_rs/src/mutation/response_builder.rs` - Filter CASCADE
- `fraiseql_rs/src/mutation/cascade_filter.rs` - Selection filtering logic (NEW)

### Tests

- `tests/integration/test_cascade_selection_filtering.py` - Core tests (NEW)
- `tests/integration/test_cascade_edge_cases.py` - Edge cases (NEW)
- `tests/integration/test_cascade_graphql_spec.py` - Spec compliance (NEW)
- `tests/integration/test_cascade_performance.py` - Performance (NEW)
- `tests/integration/test_graphql_cascade.py` - Update existing tests

### Documentation

- `docs/mutations/cascade_architecture.md` - Add selection filtering
- `docs/guides/cascade-best-practices.md` - When to request CASCADE
- `docs/guides/performance-guide.md` - CASCADE optimization
- `docs/guides/migrating-to-cascade.md` - v1.8.1 migration
- `CHANGELOG.md` - v1.8.1 entry
- `README.md` - Update examples

---

## Success Metrics

- ✅ All tests pass (new and existing)
- ✅ 20-50% response size reduction when CASCADE not requested
- ✅ GraphQL spec compliance validated
- ✅ Zero regressions in existing functionality
- ✅ Complete documentation
- ✅ Successful release to production

---

## Timeline

Each phase should be completed before moving to the next:

1. RED: 1-2 hours (write tests)
2. GREEN: 2-3 hours (implement fix)
3. REFACTOR: 1-2 hours (cleanup)
4. QA: 2-3 hours (comprehensive testing)
5. CLEAN: 30 minutes (remove artifacts)
6. DOCUMENTATION: 1-2 hours (update docs)
7. COMMIT: 30 minutes (release)

**Total**: 8-13 hours

---

## Risk Assessment

### Low Risk

- Infrastructure already exists (cascade_selections.py)
- Clear requirements from issue analysis
- Comprehensive test coverage planned

### Mitigation

- TDD approach ensures no regressions
- Existing test suite catches breaking changes
- Migration guide for users
- Can rollback if critical issues found

---

## Breaking Changes

**⚠️ Users must update queries**

Clients relying on CASCADE must add it to their selections:

```diff
  mutation CreatePost($input: CreatePostInput!) {
    createPost(input: $input) {
      ... on CreatePostSuccess {
        post { id title }
+       cascade {
+         updated { __typename id entity }
+       }
      }
    }
  }
```

---

## Architecture Decision

**Why this approach?**

1. **Leverages existing code**: `cascade_selections.py` already exists
2. **GraphQL best practice**: Follows spec correctly
3. **Performance benefit**: Significant payload reduction
4. **Future-proof**: Supports partial selections from day 1
5. **Type-safe**: Rust enforces correctness

**Alternative considered**: Boolean flag (simpler but less flexible)
**Decision**: Full selection parsing (better long-term solution)

---

## Related Issues

- Issue #XXX: CASCADE selection filtering bug
- PR #164: CASCADE nesting fix (v1.8.0-alpha.5)
- PR #163: CASCADE bug fix (v1.8.0-alpha.4)

---

## Resources

- [Issue Analysis](../../../../tmp/cascade_selection_filtering_issue.md)
- [CASCADE Architecture](../../../../docs/mutations/cascade_architecture.md)
- [GraphQL Spec](https://spec.graphql.org/October2021/#sec-Selection-Sets)

---

## How to Use This Plan

1. **Read each phase file in order** (1-RED through 7-COMMIT)
2. **Complete each phase before moving to next**
3. **Run verification commands after each phase**
4. **Check acceptance criteria before proceeding**
5. **Follow DO NOT lists to avoid common mistakes**

Each phase file contains:

- Objective and context
- Step-by-step implementation
- Code examples
- Verification commands
- Acceptance criteria
- Common pitfalls to avoid

---

## Status Tracking

- [ ] Phase 1: RED - Tests written
- [ ] Phase 2: GREEN - Implementation complete
- [ ] Phase 3: REFACTOR - Code cleaned up
- [ ] Phase 4: QA - All tests passing
- [ ] Phase 5: CLEAN - Artifacts removed
- [ ] Phase 6: DOCUMENTATION - Docs updated
- [ ] Phase 7: COMMIT - Released

---

**Ready to begin? Start with Phase 1: RED**

Read `1-RED-tests.md` for detailed instructions.
