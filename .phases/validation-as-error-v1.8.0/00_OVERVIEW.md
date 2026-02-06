# FraiseQL v1.8.0-beta.1: Validation as Error Type - Implementation Plan

**Decision:** Big Bang Rollout (Incorporated into v1.8.0 beta)
**Target:** FraiseQL Core First, PrintOptim Second
**Timeline:** 4 phases over 2-3 weeks
**Breaking Change:** Yes (part of v1.8.0 CASCADE feature)
**Status:** To be included in v1.8.0-beta.1 (not yet released)

---

## Executive Summary

This plan implements Tim Berners-Lee's architectural recommendation to treat validation failures as Error type (not Success type with null entity).

### What Changes

**Before (v1.7.x - DEPRECATED):**

```graphql
type CreateMachineSuccess {
  machine: Machine      # Nullable - can be null on validation failure
  message: String!
  cascade: Cascade
}

# Returns Success with machine=null when validation fails ❌
```

**After (v1.8.0 - NEW):**

```graphql
union CreateMachineResult = CreateMachineSuccess | CreateMachineError

type CreateMachineSuccess {
  machine: Machine!     # Always non-null ✅
  cascade: Cascade!
}

type CreateMachineError {
  code: Int!           # REST-like: 422, 404, 409, 500
  status: String!      # Domain: "noop:invalid_contract_id"
  message: String!     # Human-readable
  cascade: Cascade!
}
```

### Key Architectural Principles

1. ✅ **HTTP 200 OK always** - GraphQL compliant (never HTTP 422/404/500)
2. ✅ **`code` field for DX** - Application-level REST semantics (422=validation, 404=not found)
3. ✅ **`status` for domain** - Preserves FraiseQL's detailed status strings
4. ✅ **Type safety** - Success = has entity (non-null), Error = doesn't

### Status Code Mapping

| Status Pattern | GraphQL Type | Code | Meaning |
|---------------|--------------|------|---------|
| `created`, `updated`, `deleted` | Success | - | Operation succeeded |
| `noop:*` | **Error** | **422** | Validation/business rule failure |
| `not_found:*` | Error | 404 | Entity doesn't exist |
| `unauthorized:*` | Error | 401 | Authentication missing |
| `forbidden:*` | Error | 403 | Insufficient permissions |
| `conflict:*` | Error | 409 | Resource conflict |
| `timeout:*` | Error | 408 | Operation timeout |
| `failed:*` | Error | 500 | System failure |

**Critical change:** `noop:*` is now an **Error** (was Success).

---

## Implementation Phases

### Phase 1: Rust Core Changes (Week 1)

**Files:** 4 files
**Objective:** Update Rust mutation pipeline to return Error type for all non-success statuses

**Details:** See `01_PHASE_1_RUST_CORE.md`

### Phase 2: Python Layer Updates (Week 1)

**Files:** 6 files
**Objective:** Update Python mutation decorators, error config, and type definitions

**Details:** See `02_PHASE_2_PYTHON_LAYER.md`

### Phase 3: Schema & GraphQL Generation (Week 2)

**Files:** 5 files
**Objective:** Generate union types, update schema generation, add code field

**Details:** See `03_PHASE_3_SCHEMA_GENERATION.md`

### Phase 4: Testing & Documentation (Week 2)

**Files:** Multiple test files + docs
**Objective:** Update all tests, write migration guide, update docs

**Details:** See `04_PHASE_4_TESTING_DOCS.md`

### Phase 5: Verification & Release (Week 3)

**Objective:** Final verification, version bump, release coordination

**Details:** See `05_PHASE_5_VERIFICATION_RELEASE.md`

---

## Breaking Changes Summary

### GraphQL API

**Schema Changes:**

- ❌ **BREAKING:** All mutations now return union types (`<Mutation>Result`)
- ❌ **BREAKING:** Success types never have null entities
- ❌ **BREAKING:** Validation failures return Error type (not Success)
- ✅ **COMPATIBLE:** HTTP 200 OK preserved
- ✅ **COMPATIBLE:** `status`, `message`, `cascade` fields preserved

**Client Impact:**

```typescript
// OLD (v1.7.x)
if (result.__typename === "CreateMachineSuccess") {
  if (result.machine !== null) { /* ... */ }
}

// NEW (v1.8.0)
if (result.__typename === "CreateMachineSuccess") {
  // machine is GUARANTEED non-null
  handleSuccess(result.machine);
} else if (result.__typename === "CreateMachineError") {
  handleError(result.code, result.status);
}
```

### Python API

**Decorator Changes:**

- ✅ **COMPATIBLE:** `@fraiseql.success` and `@fraiseql.failure` unchanged
- ✅ **COMPATIBLE:** `error_config` still works (updated mappings)
- ❌ **BREAKING:** `error_as_data_prefixes` removed (all are now errors)

**Type Changes:**

```python
# OLD (v1.7.x)
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # Nullable ❌

# NEW (v1.8.0)
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # Non-nullable ✅

@fraiseql.failure
class CreateMachineError:
    code: int          # NEW: REST-like code
    status: str
    message: str
    cascade: Cascade | None = None
```

---

## Rollout Strategy

### Stage 1: FraiseQL Core (This Plan)

**Timeline:** Immediate (part of v1.8.0 development)
**Scope:** FraiseQL library only
**Deliverable:** v1.8.0-beta.1 (includes CASCADE + validation-as-error)

**Tasks:**

1. Implement Rust changes (Phase 1)
2. Implement Python changes (Phase 2)
3. Update schema generation (Phase 3)
4. Update FraiseQL tests (Phase 4)
5. Write migration guide
6. Release v1.8.0-beta.1 (combined with CASCADE feature)

**Note:** This is being incorporated into v1.8.0-beta.1, which already includes:

- CASCADE selection filtering (v1.8.0-alpha.1 through v1.8.0-alpha.5)
- Validation as Error type (this plan)

### Stage 2: PrintOptim Migration (Separate Plan)

**Timeline:** After v1.8.0-beta.1 testing
**Scope:** PrintOptim backend
**Prerequisite:** FraiseQL v1.8.0-beta.1 released and tested

**Tasks:**

1. Update FraiseQL dependency to v1.8.0-beta.1
2. Update ~30-50 test assertions
3. Update GraphQL fragments (union types)
4. Update frontend error handling
5. Regression testing
6. Deploy to staging
7. Production release

---

## Success Criteria

### Phase 1-3 (Core Implementation)

- [ ] All Rust response builder tests pass
- [ ] All Python mutation tests pass
- [ ] Schema generation produces union types
- [ ] Error types include `code` field
- [ ] Success types never have null entities
- [ ] HTTP 200 OK for all responses
- [ ] CASCADE works in both Success and Error types

### Phase 4 (Testing & Docs)

- [ ] All FraiseQL integration tests updated and passing
- [ ] All unit tests pass
- [ ] Migration guide complete with code examples
- [ ] Status strings documentation updated
- [ ] CASCADE documentation updated
- [ ] API reference updated

### Phase 5 (Release)

- [ ] No regressions in FraiseQL test suite
- [ ] Beta release published to PyPI
- [ ] GitHub release notes published
- [ ] Migration guide on docs site
- [ ] PrintOptim team notified with timeline

---

## Risk Mitigation

### Risk 1: Breaking Existing Users

**Mitigation:**

- Clear migration guide with before/after examples
- Beta release period (1 week minimum)
- Deprecation warnings in v1.7.x (if possible)
- Direct coordination with PrintOptim team

### Risk 2: Incomplete Migration

**Mitigation:**

- Comprehensive test coverage updates
- Automated schema validation
- Integration tests for all status codes
- Manual verification of edge cases

### Risk 3: Performance Regression

**Mitigation:**

- Benchmark before/after
- Profile Rust response builder
- Ensure no extra allocations
- Load testing on staging

### Risk 4: Documentation Gaps

**Mitigation:**

- Migration guide with real-world examples
- Updated API reference
- Code examples in docs
- FAQ section for common issues

---

## Communication Plan

### Week 1 (Implementation Start)

- [ ] Announce v1.8.0 development on GitHub Discussions
- [ ] Create tracking issue for breaking changes
- [ ] Notify PrintOptim team of upcoming changes

### Week 2 (Beta Release)

- [ ] Publish v1.8.0-beta.1 to PyPI
- [ ] Release blog post on docs site
- [ ] Share migration guide
- [ ] Request beta testing feedback

### Week 3 (Stabilization)

- [ ] Address beta feedback
- [ ] Fix any discovered issues
- [ ] Finalize documentation
- [ ] Prepare final release

### Week 4 (GA Release)

- [ ] Publish v1.8.0 to PyPI
- [ ] GitHub release with full changelog
- [ ] Update docs site to default to v1.8.0
- [ ] Coordinate PrintOptim migration

---

## File Organization

```
./.phases/validation-as-error-v1.8.0/
├── 00_OVERVIEW.md                    (this file)
├── 01_PHASE_1_RUST_CORE.md
├── 02_PHASE_2_PYTHON_LAYER.md
├── 03_PHASE_3_SCHEMA_GENERATION.md
├── 04_PHASE_4_TESTING_DOCS.md
├── 05_PHASE_5_VERIFICATION_RELEASE.md
├── code_examples/
│   ├── rust_response_builder_before.rs
│   ├── rust_response_builder_after.rs
│   ├── python_decorator_before.py
│   ├── python_decorator_after.py
│   ├── client_code_before.ts
│   └── client_code_after.ts
└── migration_guide/
    ├── MIGRATION_GUIDE.md
    ├── printoptim_checklist.md
    └── common_issues.md
```

---

## Next Steps

1. **Review this plan** - Ensure alignment with team
2. **Begin Phase 1** - Start with Rust core changes (highest risk)
3. **Set up tracking** - GitHub project/issue for v1.8.0
4. **Schedule sync** - Weekly sync with PrintOptim team
5. **Create beta branch** - `feature/v1.8.0-validation-as-error`

**Ready to proceed with Phase 1?** See `01_PHASE_1_RUST_CORE.md` for detailed implementation steps.
