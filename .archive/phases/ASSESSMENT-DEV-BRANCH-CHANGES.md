# Assessment: Dev Branch Changes (v1.9.2 → v1.9.4)

**Date**: January 5, 2026
**Current Branch**: feature/phase-16-rust-http-server
**Target Branch**: dev
**Assessment**: How to integrate latest changes into our HTTP server implementation

---

## Executive Summary

The dev branch contains **critical features and security fixes** that should be integrated:

✅ **MUST INTEGRATE**:
1. **IDPolicy Configuration** (v1.9.2) - New configurable ID scalar behavior
2. **APQ Selection Module** (v1.9.2) - Fixes field selection bugs
3. **Security Fixes** (v1.9.2) - APQ response caching vulnerabilities
4. **IDFilter for WHERE clauses** (v1.9.4) - Policy-aware ID filtering

⚠️ **ALREADY ADDRESSED**:
- APQ field selection issue (we have `.phases/FIX-APQ-FIELD-SELECTION-RUST-LAYER.md`)
- Our branch is ahead in HTTP server implementation

---

## Part 1: IDPolicy Configuration (v1.9.2)

### What Changed

FraiseQL now provides **configurable ID policy** via `SchemaConfig`:

```python
from fraiseql.config.schema_config import SchemaConfig, IDPolicy

# Option 1: UUID enforcement (default, FraiseQL's opinionated approach)
SchemaConfig.set_config(id_policy=IDPolicy.UUID)
# → ID type validates UUID format at input layer

# Option 2: GraphQL spec-compliant (accepts any string)
SchemaConfig.set_config(id_policy=IDPolicy.OPAQUE)
# → ID type accepts any string
```

**Key Design Decision**: GraphQL schema always uses `ID!` (consistent), but runtime behavior changes based on policy.

### New Files Added

- `src/fraiseql/config/schema_config.py` - IDPolicy enum + SchemaConfig class
- `tests/config/test_id_policy.py` - 345+ lines of comprehensive tests

### Changes to Existing Files

**`src/fraiseql/types/scalars/id_scalar.py`**:
- Simplified to use GraphQL's built-in `ID` scalar (avoids redefinition errors)
- ID validation now happens at input layer (via SchemaConfig)
- `uuid.UUID` always maps to `UUIDScalar` (separate from ID policy)

**`src/fraiseql/types/scalars/graphql_utils.py`**:
- Updated `convert_scalar_to_graphql()` to respect IDPolicy configuration

### Integration into Our Branch

**Status**: ✅ Low effort, highly valuable

**Steps**:
1. Copy `src/fraiseql/config/schema_config.py` from dev
2. Copy `tests/config/test_id_policy.py` from dev
3. Update `src/fraiseql/types/scalars/id_scalar.py` with new simplified approach
4. Update `src/fraiseql/types/scalars/graphql_utils.py` with policy awareness
5. Test: `make test-one TEST=tests/config/test_id_policy.py`

**Benefits**:
- Gives developers choice (UUID vs opaque IDs)
- Fixes GraphQL spec compliance issues
- Zero breaking changes (default is UUID, current behavior)

---

## Part 2: APQ Selection Module (v1.9.2)

### What Changed

New module `src/fraiseql/middleware/apq_selection.py` that:
- **Parses GraphQL queries** to extract field selections
- **Filters responses** to only include requested fields
- **Prevents data leakage** from cached responses

### Why It Matters

**The Security Vulnerability**:
```python
# Query 1: { user(id: 1) { name } }
# → Response cached: {"user": {"id": 1, "name": "John"}}

# Query 2: { user(id: 2) { name } }
# → WRONG: Returns cached response from Query 1
# → Data leakage! User 2's request returns User 1's data
```

**How APQ Selection Fixes It**:
1. Parse query to extract selected fields: `["name"]`
2. Before caching: Filter response to only include `["name"]`
3. Cache smaller payload: `{"user": {"name": "John"}}`
4. Query 2 with different fields: Filter accordingly

### New Files

- `src/fraiseql/middleware/apq_selection.py` - Selection parsing & filtering
- `tests/middleware/test_apq_selection.py` - 315 unit tests
- `tests/regression/test_apq_field_selection_bug.py` - 804 lines of regression tests

### Changes to Existing Files

**`src/fraiseql/middleware/apq_caching.py`**:
- Added `compute_response_cache_key()` - Cache keys now include normalized JSON variables
- Uses APQ selection module to filter responses before storing

**`src/fraiseql/fastapi/routers.py`**:
- Updated to pass `query_text` and `operation_name` to caching functions
- Added response filtering on cache retrieval

### Integration into Our Branch

**Status**: ⚠️ Moderate effort, critical for security

**Our Current Approach** (in `.phases/FIX-APQ-FIELD-SELECTION-RUST-LAYER.md`):
- We're disabling response caching in FastAPI layer entirely
- Pushing field selection fix down to Rust HTTP layer

**Dev Branch Approach**:
- Keeps response caching enabled
- Uses APQ selection module to filter responses safely

**Decision**: We should **adopt dev branch approach** for these reasons:
1. Better for performance (response caching helps with repeated queries)
2. Security is properly handled (via selection filtering)
3. Rust HTTP layer can inherit this approach

**Steps**:
1. Copy `src/fraiseql/middleware/apq_selection.py` from dev
2. Update `src/fraiseql/middleware/apq_caching.py` with variable-aware cache keys
3. Revert changes from our `.phases/FIX-APQ-FIELD-SELECTION-RUST-LAYER.md` (re-enable response caching)
4. Update Rust HTTP layer to use APQ selection module
5. Test: `make test-one TEST=tests/regression/test_apq_field_selection_bug.py`

---

## Part 3: Security Fixes (v1.9.2)

### Critical APQ Vulnerabilities Fixed

**Vulnerability 1: Response Cache Data Leakage**
- **Issue**: Cache keys ignored GraphQL variables
- **Impact**: Users could see each other's data
- **Fix**: Cache keys now include normalized JSON variables

**Vulnerability 2: Field Selection Not Respected**
- **Issue**: Cached responses returned full payloads
- **Impact**: Client requesting 2 fields got 20 fields (information disclosure)
- **Fix**: Responses filtered by selection set before caching

**Vulnerability 3: Full Response Cached for Partial Requests**
- **Issue**: If query requested `{id, name}`, full object cached
- **Impact**: Memory waste, potential data exposure
- **Fix**: Only requested fields cached

### Docker Security Updates

Applied 3 CVE patches:
- CVE-2025-14104 (util-linux) - Heap buffer overread
- CVE-2025-6141 (ncurses) - Stack buffer overflow
- CVE-2024-56433 (shadow-utils) - Subordinate ID configuration

**Files Modified**:
- `deploy/docker/Dockerfile` - Updated base image + apt-get upgrade
- `deploy/docker/Dockerfile.hardened` - Same security updates

### Integration into Our Branch

**Status**: ✅ Required for production safety

**Steps**:
1. Copy updated Dockerfiles from dev
2. Apply APQ selection fixes (from Part 2)
3. Test with chaos engineering suite

---

## Part 4: IDFilter for WHERE Clauses (v1.9.4)

### What Changed

New `IDFilter` input type for filtering ID fields in WHERE clauses:

```python
@fraise_input
class IDFilter:
    eq: ID | None = None
    neq: ID | None = None
    in_: list[ID] | None = None
    nin: list[ID] | None = None
    isnull: bool | None = None
```

**Key Insight**: ID type now uses proper filter based on IDPolicy:
- With `IDPolicy.UUID`: Validates UUID format at filtering layer
- With `IDPolicy.OPAQUE`: Accepts any string for filtering

### Changed Files

**`src/fraiseql/sql/graphql_where_generator.py`**:
- Added `IDFilter` class
- Updated `_get_filter_type_for_field()` to recognize ID type
- Returns `IDFilter` for ID fields instead of generic `StringFilter`

### Testing

- 5 new tests in `tests/config/test_id_policy.py` for WHERE filtering
- All existing ID policy tests continue passing

### Integration into Our Branch

**Status**: ✅ Low effort, improves ID handling

**Steps**:
1. Add `IDFilter` class to `src/fraiseql/sql/graphql_where_generator.py`
2. Update field type detection to handle ID fields specially
3. Test: `make test-one TEST=tests/config/test_id_policy.py`

---

## Part 5: Example Files Updates

### What Changed

44 example files updated to use `ID` type instead of `UUID`:

```python
# Before
from fraiseql.types import UUID

@fraise_type
class User:
    id: UUID  # Entity identifier

# After
from fraiseql.types import ID

@fraise_type
class User:
    id: ID  # Entity identifier
```

**Rationale**:
- GraphQL standard compliance
- Clearer intent (entity ID vs generic UUID)
- Future-proof for opaque identifiers

### Files Changed

All example files in `examples/` directory:
- admin-panel/
- blog_api/
- ecommerce/
- multi-tenant-saas/
- etc.

### Integration into Our Branch

**Status**: ✅ Recommended for consistency

**Steps**:
1. Update example files to use `ID` type
2. This is straightforward find-and-replace:
   - Find: `from fraiseql.types import UUID`
   - Replace: `from fraiseql.types import ID`
   - Find: `id: UUID`
   - Replace: `id: ID`

---

## Part 6: Documentation Updates

### Changed Files

1. **`docs/core/id-type.md`** (65+ lines)
   - Added IDPolicy documentation
   - Clarified UUID vs ID distinction
   - Examples for both policies

2. **`docs/core/configuration.md`** (44+ lines added)
   - New `SchemaConfig` section
   - ID Policy configuration examples
   - Best practices

3. **`docs/getting-started/quickstart.md`**
   - Updated to use `ID` type in examples

### Integration into Our Branch

**Status**: ✅ Recommended for clarity

**Steps**:
1. Review and adopt documentation changes
2. Update our own docs to reflect ID policy choices

---

## Part 7: Breaking Changes & Migration Path

### Breaking Changes in v1.9.2 → v1.9.4

❌ **None**. All changes are:
- Backwards compatible
- Default to existing behavior
- New features are opt-in

### Migration Path

✅ **For existing FraiseQL users**:
1. Update to v1.9.2 (get security fixes)
2. Optional: Configure IDPolicy if you want opaque IDs
3. Optional: Update examples to use ID type

✅ **For our Rust HTTP server**:
1. Integrate IDPolicy support
2. Integrate APQ selection module
3. Update docker files
4. Update examples

---

## Integration Roadmap

### Phase 1: Critical (Security & Stability)
**Priority**: ⚠️ MUST DO

1. Copy `src/fraiseql/middleware/apq_selection.py`
2. Update `src/fraiseql/middleware/apq_caching.py` (variable-aware cache keys)
3. Update Docker files (CVE patches)
4. Revert APQ response caching disable (re-enable with selection filtering)

**Effort**: ~4-6 hours
**Risk**: Low (well-tested in dev branch)
**Test Command**:
```bash
make test-one TEST=tests/regression/test_apq_field_selection_bug.py
make test-one TEST=tests/middleware/test_apq_selection.py
```

### Phase 2: Feature (ID Configuration)
**Priority**: 🟢 SHOULD DO

1. Copy `src/fraiseql/config/schema_config.py`
2. Copy `tests/config/test_id_policy.py`
3. Update `src/fraiseql/types/scalars/id_scalar.py`
4. Update `src/fraiseql/types/scalars/graphql_utils.py`

**Effort**: ~2-3 hours
**Risk**: Low (new feature, backward compatible)
**Test Command**:
```bash
make test-one TEST=tests/config/test_id_policy.py
```

### Phase 3: Enhancement (ID WHERE Filtering)
**Priority**: 🟢 SHOULD DO

1. Add `IDFilter` to WHERE generator
2. Update field type detection
3. Test ID filtering

**Effort**: ~1-2 hours
**Risk**: Low (isolated feature)
**Test Command**:
```bash
make test-one TEST=tests/config/test_id_policy.py::TestIDPolicyWhereFilters
```

### Phase 4: Polish (Examples & Docs)
**Priority**: 🔵 NICE TO DO

1. Update example files (ID vs UUID)
2. Update documentation
3. Review and adopt best practices

**Effort**: ~2-3 hours
**Risk**: None (documentation only)

---

## Current Branch Status

### What We Have
- ✅ Rust HTTP server implementation (`feature/phase-16-rust-http-server`)
- ✅ APQ field selection fix plan (`.phases/FIX-APQ-FIELD-SELECTION-RUST-LAYER.md`)
- ✅ Type stubs for IDE autocompletion (phase-23)

### What We Need from Dev
1. IDPolicy configuration system
2. APQ selection module (security fix)
3. Variable-aware cache keys
4. Docker security updates
5. ID WHERE filtering support

### Integration Complexity

**Easy to Integrate** (copies from dev):
- `src/fraiseql/config/schema_config.py`
- `src/fraiseql/middleware/apq_selection.py`
- Docker files
- Tests

**Moderate Complexity** (requires understanding):
- Update `apq_caching.py` for variable-aware keys
- Update Rust HTTP layer to use APQ selection
- Update GraphQL utils for policy awareness

**Complex** (architectural):
- Ensuring Rust HTTP layer properly implements APQ selection
- Making sure both FastAPI and Axum paths use same security model

---

## Recommended Approach

### Option A: Full Integration (Recommended)
**Integrate all changes from dev into our branch**

**Pros**:
- Get all security fixes
- Adopt new features (IDPolicy)
- Stay aligned with main development
- Benefit from extensive testing in dev branch

**Cons**:
- More work upfront (~8-10 hours)
- Need to update Rust HTTP layer

**Timeline**: 2-3 working days

### Option B: Security-Only Integration
**Only integrate critical security fixes**

**Pros**:
- Minimal changes
- Fast implementation (~4-6 hours)
- Low risk

**Cons**:
- Miss out on IDPolicy feature
- Examples remain outdated
- Partial integration

**Timeline**: 1 working day

### Option C: Wait for v1.9.5
**Stick with current branch, integrate later**

**Pros**:
- Focus on finishing HTTP server
- Avoid context switching

**Cons**:
- Security vulnerabilities remain in FastAPI path
- Will need to integrate eventually anyway
- Lost time waiting

**Timeline**: Pushes integration 1-2 weeks

---

## Recommendation

**Go with Option A: Full Integration**

**Rationale**:
1. Security fixes are critical (APQ data leakage, CVEs)
2. IDPolicy is clean, backward-compatible feature
3. Dev branch has extensive test coverage (5991+ tests)
4. We need this for production readiness
5. Better to integrate now than later

**Suggested Schedule**:
- **Day 1**: Integrate APQ selection + security fixes (Phase 1)
- **Day 2**: Integrate IDPolicy + WHERE filtering (Phases 2-3)
- **Day 3**: Update examples & documentation (Phase 4)
- **Total**: ~10 hours of focused work

**Next Steps**:
1. Review this assessment with team
2. Approve integration approach
3. Create phase plans for each integration phase
4. Execute with comprehensive testing

---

## Detailed File Changes Required

### Phase 1: APQ & Security (CRITICAL)

Copy from dev:
```
src/fraiseql/middleware/apq_selection.py (NEW)
tests/middleware/test_apq_selection.py (NEW)
tests/regression/test_apq_field_selection_bug.py (NEW)
deploy/docker/Dockerfile (UPDATE)
deploy/docker/Dockerfile.hardened (UPDATE)
```

Update in our branch:
```
src/fraiseql/middleware/apq_caching.py (UPDATE with variable-aware keys)
src/fraiseql/fastapi/routers.py (REVERT disable + integrate selection)
```

### Phase 2: IDPolicy (FEATURE)

Copy from dev:
```
src/fraiseql/config/schema_config.py (NEW)
tests/config/test_id_policy.py (NEW)
```

Update in our branch:
```
src/fraiseql/types/scalars/id_scalar.py (UPDATE)
src/fraiseql/types/scalars/graphql_utils.py (UPDATE)
```

### Phase 3: ID WHERE Filtering (FEATURE)

Update in our branch:
```
src/fraiseql/sql/graphql_where_generator.py (ADD IDFilter)
```

### Phase 4: Examples & Docs (POLISH)

Copy from dev:
```
examples/*.py (UPDATE all to use ID)
docs/core/id-type.md (UPDATE)
docs/core/configuration.md (UPDATE)
docs/getting-started/quickstart.md (UPDATE)
```

---

## Questions to Resolve

1. **APQ Response Caching**: Should Rust HTTP server implement response caching with selection filtering, or disable it entirely?
   - **Answer**: Implement with selection filtering (matches dev branch, better performance)

2. **IDPolicy Default**: Should we change default from UUID to OPAQUE for new projects?
   - **Answer**: Keep UUID as default (opinionated, matches FraiseQL philosophy)

3. **Backwards Compatibility**: Will this break existing applications using our branch?
   - **Answer**: No, all changes are backwards compatible with sensible defaults

4. **Testing Coverage**: What new tests do we need for Rust HTTP server integration?
   - **Answer**: Ensure APQ selection tests pass, add HTTP-specific tests for field selection

---

## Conclusion

The dev branch contains **essential updates** that should be integrated into our HTTP server branch. The integration is straightforward because:

✅ Changes are well-tested (5991+ tests)
✅ Changes are backwards compatible
✅ Security fixes address critical vulnerabilities
✅ Features add value without complexity

**Recommendation**: Proceed with **Option A: Full Integration** over 2-3 days.
