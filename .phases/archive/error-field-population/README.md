# Error Field Population - Implementation Plan

## Overview

This directory contains a **4-phase TDD implementation plan** to restore error field population functionality in FraiseQL v1.8.0+.

## Background

**Issue**: Custom error class fields are not populated from database `entity`/`metadata` fields in v1.8.0

**Root Cause**: v1.8.0 Rust pipeline rewrite omitted the field extraction logic that existed in v1.7.1 Python pipeline

**Impact**: Error responses only return 5 hardcoded fields (`__typename`, `message`, `status`, `code`, `errors`), losing valuable error context

**Reference Issue**: `/tmp/fraiseql_issue_error_field_population.md`

## Implementation Phases

### Phase 1: RED - Write Failing Tests
**Duration**: ~1 hour
**File**: `phase-1-red.md`

Write comprehensive tests that define expected behavior:
- Error field population from `entity`
- Error field population from `metadata`
- CamelCase transformation
- Nested entity `__typename` addition
- Reserved field protection

**Goal**: 5 failing tests, 1 passing (reserved fields already work)

### Phase 2: GREEN - Implement Functionality
**Duration**: ~2-3 hours
**File**: `phase-2-green.md`

Implement minimum Rust code to make tests pass:
- Extend `build_error_response()` signature
- Extract fields from `entity` and `metadata`
- Add helper functions for type inference
- Update Python caller to pass error class fields

**Goal**: All 6 tests passing

### Phase 3: REFACTOR - Polish Code
**Duration**: ~1-2 hours
**File**: `phase-3-refactor.md`

Improve code quality and extract shared utilities:
- Create `field_extractor.rs` for shared extraction logic
- Create `type_inference.rs` for entity type inference
- Refactor both success and error builders to use shared code
- Add validation and warnings
- Improve error messages

**Goal**: Clean, maintainable code with no duplication

### Phase 4: QA - Validate & Document
**Duration**: ~1 hour
**File**: `phase-4-qa.md`

Validate implementation and prepare for release:
- Backward compatibility tests (v1.7.1 patterns)
- Cross-version integration tests
- Performance validation
- Update documentation
- Update CHANGELOG
- Manual QA checklist

**Goal**: Ready for v1.8.1 release

## Total Effort

**Estimated**: 5-7 hours for complete implementation

**Breakdown**:
- RED: 1 hour
- GREEN: 2-3 hours
- REFACTOR: 1-2 hours
- QA: 1 hour

## Getting Started

### Prerequisites

```bash
# Ensure development environment is ready
cd /home/lionel/code/fraiseql
uv sync

# Ensure Rust toolchain is available
cd fraiseql_rs
cargo --version
```

### Execution

Run phases sequentially:

```bash
# Phase 1: Write tests
# Follow: phase-1-red.md
uv run pytest tests/integration/mutations/test_error_field_population.py -v
# Expected: 5 failed, 1 passed

# Phase 2: Implement
# Follow: phase-2-green.md
# (make Rust changes, rebuild, test)
uv run pytest tests/integration/mutations/test_error_field_population.py -v
# Expected: 6 passed

# Phase 3: Refactor
# Follow: phase-3-refactor.md
# (extract shared code, add tests)
uv run pytest tests/integration/mutations/test_error_field_population.py -v
# Expected: 6+ passed (all tests)

# Phase 4: QA
# Follow: phase-4-qa.md
# (backward compat tests, docs, changelog)
uv run pytest -v  # Full suite
```

## Key Files Modified

### Rust (fraiseql_rs/)
- `src/mutation/response_builder.rs` - Extend error response builder
- `src/mutation/field_extractor.rs` - NEW: Shared field extraction
- `src/mutation/type_inference.rs` - NEW: Entity type inference
- `src/lib.rs` - Update PyO3 bindings

### Python (src/fraiseql/)
- `mutations/rust_executor.py` - Pass error class fields to Rust

### Tests
- `tests/integration/mutations/test_error_field_population.py` - NEW: Core tests
- `tests/regression/test_v1_7_1_error_compatibility.py` - NEW: Compatibility tests

### Documentation
- `docs/mutations/error-handling.md` - Document custom error fields
- `CHANGELOG.md` - Document v1.8.1 fix

## Success Criteria

- [ ] All tests pass (integration + regression)
- [ ] No performance regression (< 10% slower)
- [ ] Backward compatible with v1.7.1 patterns
- [ ] Code is clean and maintainable (no duplication)
- [ ] Documentation is comprehensive
- [ ] Ready for PyPI release

## Expected Outcome

After implementation, error responses will populate custom fields:

```python
@fraiseql.failure
class CreateDnsServerError:
    message: str
    conflict_dns_server: DnsServer | None = None  # ✅ Now populated!
```

```json
{
  "createDnsServer": {
    "__typename": "CreateDnsServerError",
    "message": "DNS Server with this IP already exists",
    "conflictDnsServer": {
      "__typename": "DnsServer",
      "id": "123e4567-e89b-12d3-a456-426614174000",
      "ipAddress": "192.168.1.1"
    }
  }
}
```

## Questions?

- **Technical details**: See individual phase plan files
- **Database patterns**: Current patterns are correct, no changes needed
- **Migration**: No breaking changes, purely additive

## References

- **Issue Document**: `/tmp/fraiseql_issue_error_field_population.md`
- **Response Document**: `/tmp/fraiseql_error_field_population_response.md`
- **v1.7.1 Implementation**: `git show v1.7.1:src/fraiseql/mutations/result_processor.py`
- **Current Implementation**: `fraiseql_rs/src/mutation/response_builder.rs:190-256`
