# Schema Refresh API - Implementation Plan

**Feature**: Dynamic GraphQL schema refresh for testing
**Status**: Ready for implementation
**Total Estimated Effort**: 6-8 hours
**Created**: December 12, 2025

---

## Overview

This feature adds a `refresh_schema()` method to FraiseQL's FastAPI app, enabling the GraphQL schema to be rebuilt after database changes. This unblocks testing of features that require dynamically created database functions (e.g., WP-034 native error arrays).

**Problem**: FraiseQL builds its schema once at startup. Tests that create database functions after app initialization cannot discover those functions.

**Solution**: Implement `app.refresh_schema()` that:
1. Clears all internal caches (Python, Rust, TurboRegistry)
2. Re-runs auto-discovery to find new functions/views
3. Rebuilds the GraphQL schema
4. Updates the FastAPI router with the new schema

---

## Architecture Decision

After analyzing 5 alternative approaches, **Schema Refresh API** was selected as the best long-term solution:

| Approach | Effort | Maintainability | Reusability | Decision |
|----------|--------|-----------------|-------------|----------|
| Schema Refresh API | 6-8h | ✅ Clean | ✅ High | **SELECTED** |
| Custom Template DB | 8-12h | ⚠️ Complex | ⚠️ Medium | Rejected |
| Modify blog_simple | 1h | ❌ Pollutes | ❌ Low | Rejected |
| Skip Tests | 5min | ❌ Tech debt | ❌ None | Rejected |
| Mock Testing | 2h | ❌ No value | ❌ Low | Rejected |

See `/tmp/fraiseql-phase1.5-blocker-analysis.md` for detailed analysis.

---

## TDD 4-Phase Implementation

This feature follows FraiseQL's TDD workflow with 4 phases:

### Phase 1: RED (Test-First)
- **Goal**: Write failing tests for `app.refresh_schema()`
- **Effort**: 1-1.5 hours
- **Deliverable**: 3 failing tests in `test_schema_refresh.py`
- **File**: `.phases/schema-refresh-api/phase-1-red.md`

### Phase 2: GREEN (Implementation)
- **Goal**: Implement the feature to make tests pass
- **Effort**: 2.5-3 hours
- **Deliverable**: Working `refresh_schema()` method
- **File**: `.phases/schema-refresh-api/phase-2-green.md`

### Phase 3: REFACTOR (Code Quality)
- **Goal**: Extract utilities, improve organization
- **Effort**: 1.5-2 hours
- **Deliverable**: Reusable testing utilities in `fraiseql.testing`
- **File**: `.phases/schema-refresh-api/phase-3-refactor.md`

### Phase 4: QA (Integration & Docs)
- **Goal**: Unblock WP-034 tests, document feature
- **Effort**: 1.5-2 hours
- **Deliverable**: WP-034 tests passing, testing guide
- **File**: `.phases/schema-refresh-api/phase-4-qa.md`

---

## Key Design Decisions

### 1. Method Attached to App Instance
```python
app = create_fraiseql_app(...)
await app.refresh_schema()  # Method on app object
```

**Why**: Natural API, follows FastAPI conventions, easy to discover.

### 2. Store Original Config in App State
```python
app.state._fraiseql_refresh_config = {
    "database_url": "...",
    "original_types": [...],
    "auto_discover": True,
    # ...
}
```

**Why**: Refresh needs to replay original creation parameters. Private `_` prefix indicates internal use.

### 3. Comprehensive Cache Clearing
```python
clear_fraiseql_caches()  # Clears:
# - Python GraphQL type cache
# - Type-to-view registry
# - Python SchemaRegistry
# - Rust schema registry
# - TurboRegistry execution cache
```

**Why**: Stale caches cause schema inconsistencies. All layers must be cleared.

### 4. Router Replacement
```python
# Remove old route
app.routes[:] = [r for r in app.routes if r.path != "/graphql"]
# Mount new route with fresh schema
app.include_router(create_graphql_router(schema=new_schema, ...))
```

**Why**: GraphQL route holds reference to old schema. Must be replaced to serve new schema.

### 5. Extracted Testing Utilities

Created `fraiseql.testing.schema_utils` with:
- `clear_fraiseql_caches()` - Reusable cache clearing
- `clear_fraiseql_state()` - Complete teardown
- `validate_schema_refresh()` - Debug helper

**Why**: Code reuse, better organization, easier testing.

---

## Files Modified/Created

### Created
- `tests/unit/fastapi/test_schema_refresh.py` - Feature tests (Phase 1)
- `src/fraiseql/testing/schema_utils.py` - Utilities (Phase 3)
- `docs/testing.md` - Testing guide (Phase 4)

### Modified
- `src/fraiseql/fastapi/app.py` - Add `refresh_schema()` method (Phase 2)
- `src/fraiseql/testing/__init__.py` - Export utilities (Phase 3)
- `tests/conftest.py` - Use extracted utilities (Phase 3)
- `tests/integration/graphql/mutations/conftest.py` - Refresh fixture (Phase 4)
- `tests/integration/graphql/mutations/test_native_error_arrays.py` - Remove skips (Phase 4)

---

## Success Criteria

### Technical
- [ ] `app.refresh_schema()` method implemented and tested
- [ ] All Phase 1-3 unit tests pass (7 tests total)
- [ ] WP-034 integration tests pass (4 tests)
- [ ] No regressions in existing test suite
- [ ] Code passes linting and type checking

### Quality
- [ ] Comprehensive docstrings on all new code
- [ ] Testing utilities extracted and reusable
- [ ] Logging at appropriate levels (DEBUG, INFO, WARNING)
- [ ] Error handling for edge cases

### Documentation
- [ ] Testing guide created with examples
- [ ] Best practices documented
- [ ] Performance considerations noted
- [ ] WP-034 blocker marked resolved

---

## Implementation Workflow

### Standard TDD Cycle (Per Phase)

```bash
# 1. Read phase plan
cat .phases/schema-refresh-api/phase-N-*.md

# 2. Implement according to plan
# (Write code following phase instructions)

# 3. Run verification commands
uv run pytest tests/unit/fastapi/test_schema_refresh.py -v
uv run ruff check src/fraiseql/

# 4. Verify acceptance criteria met
# (Check all boxes in phase plan)

# 5. Commit with phase tag
git add .
git commit -m "feat(fastapi): implement refresh [PHASE]"

# 6. Move to next phase
```

### Recommended Execution

**Option 1: Manual (learning focus)**
- Read each phase plan thoroughly
- Implement step-by-step
- Good for understanding the architecture

**Option 2: Automated (speed focus)**
- Use `opencode run .phases/schema-refresh-api/phase-*.md`
- Review and verify each phase
- Good for rapid implementation

**Option 3: Hybrid (recommended)**
- Phase 1-2: Manual (learn the patterns)
- Phase 3-4: Automated (refactor and integrate)

---

## Testing Strategy

### Unit Tests (Phase 1-3)
```
tests/unit/fastapi/test_schema_refresh.py
├── test_refresh_schema_discovers_new_mutations
├── test_refresh_schema_preserves_existing_types
└── test_refresh_schema_clears_caches
```

**Coverage**:
- ✅ Discovery of new functions
- ✅ Preservation of existing schema
- ✅ Cache clearing
- ✅ Error handling

### Integration Tests (Phase 4)
```
tests/integration/graphql/mutations/test_native_error_arrays.py
├── TestAutoGeneratedErrors (4 tests)
├── TestMultipleNativeErrors (4 tests)
├── TestFieldSpecificErrors (4 tests)
└── TestNativeErrorIdentifiers (4 tests)
```

**Coverage**:
- ✅ Real database functions
- ✅ GraphQL execution
- ✅ WP-034 feature validation
- ✅ End-to-end workflow

---

## Performance Characteristics

**Schema refresh cost**: ~50-200ms depending on schema size

**Breakdown**:
- Database introspection: ~20-50ms
- Schema building: ~20-80ms
- Rust registry init: ~10-40ms
- Router replacement: ~5-10ms

**Acceptable because**:
- Used only in testing, not production
- Called once per test class (class-scoped fixture)
- Still faster than app restart

**Optimization tip**: Use `scope="class"` on refresh fixtures:
```python
@pytest.fixture(scope="class")  # ← Refresh once per class
async def app_with_mutations(app, db_url):
    await app.refresh_schema()
    yield app
```

---

## Future Enhancements

Once this API exists, it enables:

1. **Hot-reloading in development**
   - Watch for SQL file changes
   - Auto-refresh schema without restart

2. **Dynamic plugin systems**
   - Load GraphQL types from plugins
   - Refresh schema to activate plugins

3. **Multi-tenant schemas**
   - Different database per tenant
   - Refresh schema per tenant connection

4. **Migration testing**
   - Apply migration
   - Refresh schema
   - Verify GraphQL changes

**These are NOT part of the current plan** - just possibilities.

---

## Risk Mitigation

### Risk 1: Memory Leaks
**Risk**: Old schema objects not garbage collected
**Mitigation**: Explicit cache clearing, no global references retained

### Risk 2: Router Replacement Conflicts
**Risk**: Removing wrong routes, breaking app
**Mitigation**: Filter specifically by path=="/graphql", comprehensive testing

### Risk 3: Rust Registry State Corruption
**Risk**: Multiple initializations cause conflicts
**Mitigation**: Call `reset_schema_registry_for_testing()` before re-init

### Risk 4: TurboRegistry Cache Staleness
**Risk**: Old execution plans used with new schema
**Mitigation**: Explicitly clear TurboRegistry cache during refresh

**All mitigations implemented in Phase 2.**

---

## Dependencies

### Required Packages
- `psycopg` (already in use) - Database connections
- `graphql-core` (already in use) - Schema building
- `fraiseql._fraiseql_rs` (already in use) - Rust extension

### Internal Dependencies
- `fraiseql.gql.schema_builder` - Schema building logic
- `fraiseql.introspection` - Auto-discovery system
- `fraiseql.core.graphql_type` - Type caching
- `fraiseql.fastapi.routers` - GraphQL router creation

**No new dependencies required.**

---

## Rollback Plan

If issues discovered after implementation:

1. **Immediate**: Revert commits in reverse phase order (4→3→2→1)
2. **Re-skip WP-034 tests**: Add `@pytest.mark.skip` back
3. **Document issues**: Update blocker analysis with findings
4. **Re-assess approach**: Consider alternative solutions

**Low risk**: Feature is opt-in (only used when explicitly called).

---

## Questions & Answers

### Q: Why not just add functions to blog_simple's init.sql?
**A**: Pollutes example app with test-only code. Not a general solution for other apps.

### Q: Can this be used in production?
**A**: Yes, but not recommended. Schema refresh is expensive and designed for testing. For production, restart the app.

### Q: Does this work with manual type registration?
**A**: Yes! Refresh supports both auto-discovery and manual types. Manual types are preserved from original config.

### Q: What if auto_discover=False?
**A**: Refresh still works, just skips the discovery step. Only rebuilds with original types.

### Q: Can I refresh multiple times in one test?
**A**: Yes, but it's expensive. Each refresh costs ~50-200ms. Use sparingly.

---

## Phase Execution Checklist

Track your progress through the phases:

- [ ] **Phase 1 (RED)**: Tests written and failing
  - [ ] File created: `tests/unit/fastapi/test_schema_refresh.py`
  - [ ] 3 tests fail with `AttributeError`
  - [ ] Committed with `[RED]` tag

- [ ] **Phase 2 (GREEN)**: Feature implemented
  - [ ] Method added: `app.refresh_schema()`
  - [ ] Config stored in `app.state._fraiseql_refresh_config`
  - [ ] All 3 tests pass
  - [ ] Committed with `[GREEN]` tag

- [ ] **Phase 3 (REFACTOR)**: Code organized
  - [ ] File created: `src/fraiseql/testing/schema_utils.py`
  - [ ] Utilities extracted: 3 functions
  - [ ] `conftest.py` refactored to use utilities
  - [ ] All tests still pass
  - [ ] Committed with `[REFACTOR]` tag

- [ ] **Phase 4 (QA)**: Integration complete
  - [ ] WP-034 tests enabled (skips removed)
  - [ ] 4 WP-034 tests pass
  - [ ] Documentation created: `docs/testing.md`
  - [ ] Blocker marked resolved
  - [ ] Committed with `[QA]` tag

---

## Final Verification

Before marking this feature complete:

```bash
# 1. All unit tests pass
uv run pytest tests/unit/fastapi/test_schema_refresh.py -v
# Expected: 3/3 PASSED

# 2. All WP-034 tests pass
uv run pytest tests/integration/graphql/mutations/test_native_error_arrays.py -v
# Expected: 4/4 PASSED (no skips)

# 3. No regressions
uv run pytest tests/
# Expected: All tests pass (or pre-existing failures only)

# 4. Linting clean
uv run ruff check src/fraiseql/ tests/
# Expected: No errors

# 5. Documentation exists
cat docs/testing.md
# Expected: Schema refresh section with examples
```

---

## Related Documents

- **Blocker Analysis**: `/tmp/fraiseql-phase1.5-blocker-analysis.md`
- **WP-034 Tests**: `tests/integration/graphql/mutations/test_native_error_arrays.py`
- **Test Architecture**: `/home/lionel/.claude/skills/fraiseql-testing.md`
- **CI/CD Workflow**: `/home/lionel/.claude/skills/printoptim-cicd.md`

---

**Ready to start?** Begin with Phase 1: `.phases/schema-refresh-api/phase-1-red.md`
