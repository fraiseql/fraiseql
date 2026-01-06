# FraiseQL v1.8.0: Validation as Error Type

**Implementation Plan - Big Bang Rollout**

---

## 📋 Quick Reference

### What's Changing

**v1.7.x (OLD):**
- Validation failures → Success type with `machine: null`
- Confusing semantics ("Success" for failures)
- Type safety issues

**v1.8.0 (NEW):**
- Validation failures → Error type with `code: 422`
- Clear semantics (Success = succeeded, Error = failed)
- Type safe (Success always has non-null entity)

### Status Code Mapping

| Status | v1.7.x Type | v1.8.0 Type | Code |
|--------|-------------|-------------|------|
| `created` | Success | Success | - |
| `noop:*` | Success ❌ | Error ✅ | 422 |
| `not_found:*` | Error | Error | 404 |
| `conflict:*` | Error | Error | 409 |
| `failed:*` | Error | Error | 500 |

---

## 📁 Implementation Phases

### [Phase 1: Rust Core Changes](./01_PHASE_1_RUST_CORE.md)
**Timeline:** Week 1, Days 1-3
**Risk:** HIGH (core mutation pipeline)

**Key Changes:**
- Update `response_builder.rs:24` - Remove `|| result.status.is_noop()`
- Add `build_error_response_with_code()` function
- Add `map_status_to_code()` function (noop→422, not_found→404, etc.)
- Validate Success type never has null entity
- Update status enum methods

**Files Modified:**
- `fraiseql_rs/src/mutation/response_builder.rs`
- `fraiseql_rs/src/mutation/mod.rs`
- `fraiseql_rs/src/mutation/types.rs`
- `fraiseql_rs/src/mutation/tests.rs`

---

### [Phase 2: Python Layer Updates](./02_PHASE_2_PYTHON_LAYER.md)
**Timeline:** Week 1, Days 4-5
**Risk:** MEDIUM (backward compatibility)

**Key Changes:**
- Remove `error_as_data_prefixes` from error config
- Move `noop:`, `blocked:` to `error_prefixes`
- Add `get_error_code()` method
- Add `code: int` field to MutationError
- Validate Success types have non-null entity

**Files Modified:**
- `src/fraiseql/mutations/error_config.py`
- `src/fraiseql/mutations/types.py`
- `src/fraiseql/mutations/rust_executor.py`
- `src/fraiseql/mutations/mutation_decorator.py`
- `src/fraiseql/mutations/__init__.py`

---

### [Phase 3: Schema Generation](./03_PHASE_3_SCHEMA_GENERATION.md)
**Timeline:** Week 2, Days 1-3
**Risk:** MEDIUM (schema changes)

**Key Changes:**
- Generate union types for all mutations
- Ensure Success types have non-nullable entity fields
- Ensure Error types include `code: Int!` field
- Add schema validation

**Files Modified:**
- `src/fraiseql/schema/mutation_schema_generator.py`
- `src/fraiseql/graphql/schema_builder.py`
- `src/fraiseql/schema/validator.py` (new)

---

### [Phase 4: Testing & Documentation](./04_PHASE_4_TESTING_DOCS.md)
**Timeline:** Week 2, Days 4-5
**Risk:** LOW (documentation)

**Key Changes:**
- Update ALL FraiseQL tests
- Write comprehensive migration guide
- Update status strings documentation
- Create code examples (before/after)

**Files Modified:**
- `tests/integration/test_graphql_cascade.py`
- `tests/integration/graphql/mutations/test_mutation_error_handling.py`
- `docs/migrations/v1.8.0.md` (new)
- `docs/mutations/status-strings.md`

---

### [Phase 5: Verification & Release](./05_PHASE_5_VERIFICATION_RELEASE.md)
**Timeline:** Week 3
**Risk:** LOW (verification)

**Key Tasks:**
- Run full test suite
- Performance benchmarking
- Beta release (v1.8.0-beta.1)
- Gather feedback (1 week)
- GA release (v1.8.0)

---

## 🚀 Getting Started

### Step 1: Read the Overview
Start with [`00_OVERVIEW.md`](./00_OVERVIEW.md) for architectural context.

### Step 2: Understand the Architecture
Review Tim Berners-Lee's feedback:
- `/tmp/fraiseql_tim_feedback_analysis.md`
- `/tmp/fraiseql_validation_error_architecture_review.md`

### Step 3: Begin Implementation
Follow phases in order:
1. [Phase 1: Rust Core](./01_PHASE_1_RUST_CORE.md)
2. [Phase 2: Python Layer](./02_PHASE_2_PYTHON_LAYER.md)
3. [Phase 3: Schema Generation](./03_PHASE_3_SCHEMA_GENERATION.md)
4. [Phase 4: Testing & Docs](./04_PHASE_4_TESTING_DOCS.md)
5. [Phase 5: Verification & Release](./05_PHASE_5_VERIFICATION_RELEASE.md)

---

## ⚠️ Breaking Changes

### User Code Changes Required

**1. Update Success Types**
```python
# OLD (v1.7.x)
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # ❌

# NEW (v1.8.0)
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Non-nullable
```

**2. Update Error Types**
```python
# OLD (v1.7.x)
@fraiseql.failure
class CreateMachineError:
    message: str
    errors: list[Error] | None = None

# NEW (v1.8.0)
@fraiseql.failure
class CreateMachineError:
    code: int          # ✅ NEW
    status: str        # ✅ NEW
    message: str
    cascade: Cascade | None = None
```

**3. Update Test Assertions**
```python
# OLD (v1.7.x)
assert result["__typename"] == "CreateMachineSuccess"
assert result["machine"] is None  # ❌

# NEW (v1.8.0)
assert result["__typename"] == "CreateMachineError"
assert result["code"] == 422  # ✅
assert result["status"] == "noop:invalid_contract_id"
```

**4. Update GraphQL Fragments**
```graphql
# OLD (v1.7.x)
mutation {
  createMachine(input: $input) {
    machine { id }
    message
  }
}

# NEW (v1.8.0)
mutation {
  createMachine(input: $input) {
    __typename
    ... on CreateMachineSuccess {
      machine { id }
    }
    ... on CreateMachineError {
      code
      status
      message
    }
  }
}
```

---

## 📊 Timeline

| Week | Deliverable |
|------|-------------|
| Week 1 | Rust + Python core changes complete |
| Week 2 | Schema + Testing + Docs complete |
| Week 3 | Beta release, feedback, GA release |
| Week 4+ | PrintOptim migration |

---

## 🎯 Success Criteria

### FraiseQL Core
- [ ] All Rust tests pass
- [ ] All Python tests pass
- [ ] Schema validates
- [ ] Performance acceptable (< 5% regression)
- [ ] Documentation complete
- [ ] Beta tested (1+ week)
- [ ] v1.8.0 released to PyPI

### PrintOptim Migration
- [ ] ~30-50 tests updated
- [ ] GraphQL fragments updated
- [ ] Frontend error handling updated
- [ ] Deployed to staging
- [ ] Deployed to production
- [ ] Zero regressions

---

## 📚 Resources

### Implementation Docs
- [Phase 1: Rust Core](./01_PHASE_1_RUST_CORE.md)
- [Phase 2: Python Layer](./02_PHASE_2_PYTHON_LAYER.md)
- [Phase 3: Schema Generation](./03_PHASE_3_SCHEMA_GENERATION.md)
- [Phase 4: Testing & Docs](./04_PHASE_4_TESTING_DOCS.md)
- [Phase 5: Verification & Release](./05_PHASE_5_VERIFICATION_RELEASE.md)

### Architecture Docs
- Tim's Feedback Analysis: `/tmp/fraiseql_tim_feedback_analysis.md`
- Architecture Review: `/tmp/fraiseql_validation_error_architecture_review.md`
- PrintOptim Issue: `/tmp/fraiseql_cascade_validation_error_handling.md`

### External References
- [GraphQL Best Practices](https://graphql.org/learn/best-practices/)
- [REST Status Codes](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status)
- [HTTP Semantics (RFC 9110)](https://www.rfc-editor.org/rfc/rfc9110.html)

---

## 🤝 Support

Questions during implementation?

1. **Review phase docs** - Each phase has detailed implementation steps
2. **Check code examples** - Before/after examples in each phase
3. **Consult architecture docs** - Rationale and design decisions
4. **Reach out** - GitHub Discussions for questions

---

## 📝 Notes

### Why Big Bang?
- Cleaner migration path (no hybrid state)
- Simpler mental model (one pattern, not two)
- Forces comprehensive testing
- Clear cutover point

### Why FraiseQL First?
- PrintOptim depends on FraiseQL
- FraiseQL changes are foundational
- Allows beta testing before PrintOptim migration
- Parallel migration not possible (dependency)

### Risk Mitigation
- Comprehensive test coverage
- Beta release period (1+ week)
- Performance benchmarking
- Migration guide with examples
- Direct PrintOptim team coordination

---

**Ready to implement?** Start with [Phase 1: Rust Core Changes](./01_PHASE_1_RUST_CORE.md)! 🚀
