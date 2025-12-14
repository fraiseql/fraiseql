# Custom Scalar WHERE Support Implementation Plan

**Status**: Ready for Implementation
**Created**: 2025-12-13
**Priority**: P1 - High Value Feature Completion

---

## Overview

Enable WHERE clause filtering on custom scalar fields by teaching FraiseQL's WHERE filter generator to recognize and support custom scalar types.

**Current Status**:
- ✅ 162/162 tests passing (scalars work as fields, arguments, database roundtrip)
- ❌ 6/6 WHERE clause tests skipped (filter generation doesn't support custom scalars)

**Goal**: 168/168 tests passing with full custom scalar WHERE support

**Estimated Effort**: 6-10 hours across 5 phases

---

## Problem Statement

### Current Behavior
When a type has a custom scalar field:
```python
@fraise_type
class Allocation:
    id: UUID
    ip_address: CIDRScalar  # Custom scalar field
```

The generated WhereInput treats it as a String:
```graphql
input AllocationWhereInput {
    id: UUIDFilter         # ✅ Works (built-in)
    ipAddress: StringFilter # ❌ Wrong - should be CIDRFilter
}
```

**Error**: `Variable '$filterValue' of type 'CIDR!' used in position expecting type 'String'`

### Root Cause
`create_graphql_where_input()` in `src/fraiseql/sql/graphql_where_generator.py` doesn't recognize custom scalar types. It only handles:
- Built-in scalars (String, Int, Boolean, Float, ID)
- Special cases (UUID, DateTime, Date, Time)
- Enums

**Missing**: Detection and filter generation for custom GraphQLScalarType instances

---

## Solution Design

### Core Insight
Custom scalars need the **same operators as StringFilter**, but using the **scalar type** instead of String:

```graphql
input CIDRFilter {
    eq: CIDR          # Not String!
    ne: CIDR
    in: [CIDR!]
    notIn: [CIDR!]
    contains: CIDR    # Still useful for partial matching
    startsWith: CIDR
    endsWith: CIDR
    # ... etc
}
```

### Implementation Strategy
1. **Detect custom scalars** - Check if field type is GraphQLScalarType (not built-in)
2. **Generate scalar filter** - Create {ScalarName}Filter with standard operators
3. **Cache filters** - Don't regenerate for each field
4. **Reuse logic** - Copy StringFilter structure, swap type

---

## Phases

### Phase 1: RED - Write Failing Tests [TDD]
**File**: `phase-1-red-write-failing-tests.md`
**Objective**: Create comprehensive test coverage for custom scalar WHERE filtering
**Effort**: 2 hours
**Outcome**: Clear test specification of expected behavior

**Deliverables**:
- Unit tests for filter generation (5-8 tests)
- Integration tests using existing 6 WHERE tests (un-skip them)
- Edge case tests (nullable scalars, list fields)
- All tests FAIL with clear error messages

**Success Criteria**:
- [ ] Tests clearly show what's missing
- [ ] Test failures point to exact location in code
- [ ] No false positives (tests would pass when they shouldn't)

---

### Phase 2: GREEN - Minimal Implementation [TDD]
**File**: `phase-2-green-minimal-implementation.md`
**Objective**: Make tests pass with minimal code
**Effort**: 3-4 hours
**Outcome**: All tests passing, feature works

**Deliverables**:
- Modified `create_graphql_where_input()` to detect custom scalars
- New `create_custom_scalar_filter()` function
- Filter caching mechanism
- All 168 tests passing

**Success Criteria**:
- [ ] All RED tests now pass
- [ ] No regressions in existing 162 tests
- [ ] WHERE queries work with all 54 custom scalars
- [ ] Performance acceptable (filter generation not duplicated)

---

### Phase 3: REFACTOR - Clean Implementation [REFACTOR]
**File**: `phase-3-refactor-clean-implementation.md`
**Objective**: Improve code quality without changing behavior
**Effort**: 1-2 hours
**Outcome**: Clean, maintainable implementation

**Deliverables**:
- Extract duplicated filter generation logic
- Consistent naming conventions
- Clear separation of concerns
- Improved type hints and documentation

**Success Criteria**:
- [ ] All 168 tests still passing
- [ ] Code follows FraiseQL patterns
- [ ] No TODO comments or temporary hacks
- [ ] Clear docstrings on new functions

---

### Phase 4: QA - Comprehensive Validation [QA]
**File**: `phase-4-qa-comprehensive-validation.md`
**Objective**: Verify feature works in all scenarios
**Effort**: 1 hour
**Outcome**: Confidence in production readiness

**Deliverables**:
- Manual testing with real GraphQL queries
- Performance benchmarks (filter generation time)
- Edge case validation
- Documentation review

**Success Criteria**:
- [ ] All 168 tests passing
- [ ] Manual GraphQL queries work correctly
- [ ] No memory leaks (filters properly cached)
- [ ] Error messages are clear and helpful
- [ ] Feature documented in appropriate places

---

### Phase 5: GREENFIELD - Archaeological Cleanup [CLEANUP]
**File**: `phase-5-greenfield-archaeological-cleanup.md`
**Objective**: Remove all temporary/investigation artifacts, achieve evergreen state
**Effort**: 30 minutes - 1 hour
**Outcome**: Repository is clean, timeless, ready for any future reader

**Context**: During this implementation, we've accumulated:
- Planning documents in `.phases/`
- Investigation artifacts in `/tmp/`
- Temporary test modifications
- Skip decorators and their evolution
- Multiple commit messages showing the journey

**Deliverables**:
1. **Remove Investigation Artifacts**
   - Delete `/tmp/fraiseql-*` documents
   - Archive or delete `.phases/fix-scalar-integration-tests/` planning docs
   - Keep only `.phases/custom-scalar-where-support/` (this work)

2. **Clean Up Test File**
   - Remove archaeological comments like "This was skipped because..."
   - Remove redundant skip reason updates
   - Keep only final, clear test structure
   - Remove any "temporary" hacks that became permanent

3. **Update Documentation**
   - Add WHERE clause support to scalar usage docs
   - Document filter operators available for custom scalars
   - Remove any "coming soon" or "not yet supported" mentions

4. **Verify No Dead Code**
   - Remove unused imports
   - Remove commented-out code
   - Check for functions that were experiments

5. **Create Evergreen README** (if needed)
   - Document the custom scalar system as if it always worked this way
   - No historical references ("we added", "previously didn't work")
   - Focus on: "Here's how it works"

6. **Final Commit Message Style**
   - Write as if the feature was always planned this way
   - Focus on what it does, not the journey to get there
   - Example: "feat(where): add WHERE filter support for custom scalar types"
   - Not: "fix(where): finally got WHERE filters working after investigation"

**Success Criteria**:
- [ ] No temporary files in repository
- [ ] No "TODO: cleanup" comments
- [ ] Documentation reads as timeless truth, not historical account
- [ ] Git history is clean (squash commits if needed)
- [ ] A developer reading this in 2030 won't see our struggles, just the solution
- [ ] Repository achieves "eternal sunshine of the spotless mind" state

**Rationale**: Future developers (and our future selves) should see a clean, intentional codebase, not the archaeological layers of how we got here. Every commit should look purposeful, every file should have a clear reason to exist.

---

## Technical Investigation Needed

Before Phase 1, quick investigation (30 minutes):

### 1. Understand Current Filter Generation
**File**: `src/fraiseql/sql/graphql_where_generator.py`
**Questions**:
- How does `create_graphql_where_input()` currently work?
- Where does it decide which filter type to use?
- How are built-in scalars (UUID, DateTime) handled?
- Is there already a filter cache?

**Action**: Read code and document findings

### 2. Identify Insertion Point
**Questions**:
- Where should custom scalar detection happen?
- Can we reuse StringFilter logic?
- Do we need to modify SQL generation too?

**Action**: Find the exact location for the fix

### 3. Check Operator Compatibility
**Questions**:
- Do all operators (eq, contains, startsWith) make sense for scalars?
- Should some operators be excluded for certain scalars?
- How does SQL serialization handle custom scalars?

**Action**: Test a sample scalar manually

---

## Files to Modify

### Core Implementation
1. **`src/fraiseql/sql/graphql_where_generator.py`**
   - Add custom scalar detection
   - Add filter generation for custom scalars
   - Add filter caching

### Tests
2. **`tests/integration/meta/test_all_scalars.py`**
   - Un-skip WHERE clause tests
   - Possibly simplify test implementation
   - Ensure proper cleanup

3. **`tests/unit/sql/test_graphql_where_generator.py`** (may need to create)
   - Unit tests for filter generation
   - Test custom scalar detection
   - Test filter caching

### Documentation (Phase 5)
4. **Documentation files** (identify during implementation)
   - Usage guide for custom scalars
   - WHERE clause documentation
   - API reference

---

## Dependencies

**None** - This is a pure feature addition to existing scalar support

---

## Risks & Mitigation

### Risk 1: SQL Generation Doesn't Handle Custom Scalars
**Likelihood**: Low
**Impact**: High
**Mitigation**: Test SQL generation early in Phase 2. Custom scalars already work in database roundtrip tests, so serialization should work.

### Risk 2: Performance Impact from Filter Generation
**Likelihood**: Low
**Impact**: Medium
**Mitigation**: Add caching in Phase 2. Generate filters once per scalar type, reuse across fields.

### Risk 3: Some Operators Don't Make Sense for All Scalars
**Likelihood**: Medium
**Impact**: Low
**Mitigation**: Start with all operators (like StringFilter). Remove specific ones if problems arise. Document which operators are available.

---

## Success Metrics

### Must Have (Phase 1-2)
- [x] All 168 tests passing (162 existing + 6 WHERE)
- [ ] No regressions in existing functionality
- [ ] WHERE queries work with custom scalars

### Should Have (Phase 3)
- [ ] Clean, maintainable code
- [ ] Consistent with FraiseQL patterns
- [ ] Good test coverage (unit + integration)

### Nice to Have (Phase 4-5)
- [ ] Performance benchmarks show no degradation
- [ ] Documentation updated
- [ ] Example queries in docs
- [ ] Repository in evergreen state

---

## Verification Commands

### After Each Phase
```bash
# Run all scalar tests
uv run pytest tests/integration/meta/test_all_scalars.py -v

# Expected: 168 passed (after Phase 2)
```

### After Phase 4 (QA)
```bash
# Run full test suite
uv run pytest tests/ -v

# Check for any failures
```

### After Phase 5 (Cleanup)
```bash
# Verify no temporary files
find . -name "*.tmp" -o -name "*_temp*" -o -name "*TODO*"

# Check for archaeological comments
git grep -i "temporary\|fixme\|hack\|todo" -- '*.py' | grep -v ".phases"

# Verify documentation is evergreen
git grep -i "will be added\|coming soon\|not yet\|pending" -- '*.md' | grep -v ".phases"
```

---

## Open Questions

1. **Should all StringFilter operators be available?**
   - `contains`, `startsWith`, `endsWith` - useful for partial matching?
   - Decision: Start with all, remove if problematic

2. **Should we support scalar-specific operators?**
   - Example: `overlaps` for CIDRScalar network ranges
   - Decision: Phase 1 scope = basic operators only. Special operators = future feature

3. **How should nullable scalars be handled?**
   - Should `null` be a valid filter value?
   - Decision: Follow existing nullable field pattern

4. **Do list/array scalar fields need special handling?**
   - Example: `tags: list[TagScalar]`
   - Decision: Address if tests require it, otherwise defer

---

## Commit Strategy

### During Implementation (Phases 1-4)
- Commit after each phase passes
- Use conventional commit format:
  - Phase 1: `test(where): add tests for custom scalar WHERE filters [RED]`
  - Phase 2: `feat(where): implement custom scalar WHERE filter generation [GREEN]`
  - Phase 3: `refactor(where): clean up filter generation logic [REFACTOR]`
  - Phase 4: `test(where): comprehensive validation of scalar WHERE support [QA]`

### After Phase 5 (Cleanup)
- **Option A**: Keep all commits (shows TDD process)
- **Option B**: Squash into single feature commit (evergreen history)
- **Recommendation**: Option A during development, Option B before merging to main

**Final Commit Message** (if squashing):
```
feat(where): add WHERE filter support for custom scalar types

Custom scalar fields can now be filtered using WHERE clauses with
standard operators (eq, ne, in, contains, etc.). The filter generator
automatically creates scalar-specific filter types (e.g., CIDRFilter,
EmailFilter) that accept the scalar type instead of String.

All 54 custom scalars now support:
- Equality filtering (eq, ne)
- List filtering (in, notIn)
- String pattern matching (contains, startsWith, endsWith)

Closes: #XXX (create issue during Phase 1)
```

---

## Next Steps

1. **Review this plan** - Get approval on approach
2. **Create issue** - Track the feature request
3. **30-minute investigation** - Answer technical questions above
4. **Start Phase 1** - Write failing tests
5. **Execute phases sequentially** - RED → GREEN → REFACTOR → QA → GREENFIELD
6. **Celebrate** - 168/168 tests passing, evergreen repository achieved! 🎉

---

**Note**: This plan follows TDD rigorously:
- Phase 1 (RED) = Write tests that define the behavior
- Phase 2 (GREEN) = Make tests pass with minimal code
- Phase 3 (REFACTOR) = Improve code while keeping tests green
- Phase 4 (QA) = Validate everything works
- Phase 5 (GREENFIELD) = Eternal sunshine - remove all traces of the journey

The repository should feel like custom scalar WHERE support was always there, perfectly designed from the start, no historical baggage.
