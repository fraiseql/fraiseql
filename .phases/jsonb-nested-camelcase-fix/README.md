# JSONB Nested Field CamelCase Fix Implementation Plan

**Status**: Ready for Implementation
**Created**: 2025-12-15
**Priority**: P1 - Critical Bug Fix (Blocks PrintOptim Backend)

---

## Overview

Fix the bug where nested JSONB object fields are not converted to camelCase in GraphQL responses. This affects fields with snake_case names (e.g., `smtp_server` → `smtpServer`) and fields with underscore+number patterns (e.g., `dns_1` → `dns1`).

**Current Status**:
- All CI tests pass (no coverage for this scenario)
- PrintOptim backend has 8+ test failures due to this bug

**Goal**: Nested JSONB objects have their field names correctly converted to camelCase

---

## Problem Statement

### Current Behavior

When a JSONB column contains nested objects with snake_case field names:
```json
{
    "id": "...",
    "gateway": {"id": "...", "ip_address": "30.0.0.1"},
    "smtp_server": {"id": "...", "ip_address": "13.16.1.10"},
    "dns_1": {"id": "...", "ip_address": "120.0.0.1"},
    "print_servers": [{"id": "...", "hostname": "printserver01.local"}]
}
```

GraphQL response returns **snake_case keys** for nested objects:
```json
{
    "gateway": {"id": "...", "ipAddress": "30.0.0.1"},
    "smtp_server": {...},
    "print_servers": [...]
}
```

### Issues Identified

| Issue | Pattern | Expected | Actual |
|-------|---------|----------|--------|
| 1 | `smtp_server` | `smtpServer` | `smtp_server` |
| 2 | `print_servers` | `printServers` | `print_servers` |
| 3 | `dns_1` | `dns1` | May be missing |
| 4 | `dns_2` | `dns2` | May be missing |

---

## Root Cause Analysis

FraiseQL has **two JSON transformation code paths**:

### Path A: Schema-Aware Transformation
**Files**: `fraiseql_rs/src/json_transform.rs`, `fraiseql_rs/src/pipeline/builder.rs`

- Entry: `build_with_schema()` in `pipeline/builder.rs:86`
- Uses `transform_with_schema()` for type-aware recursion
- Relies on `SchemaRegistry` for nested type resolution
- **Issue**: When field not in registry, fallback path may not convert keys

### Path B: Zero-Copy Streaming
**Files**: `fraiseql_rs/src/core/transform.rs`

- Entry: `build_zero_copy()` in `pipeline/builder.rs:145`
- Uses `ZeroCopyTransformer::transform_bytes()`
- Applies `snake_to_camel()` to keys at line 174
- **Issue**: No schema awareness for nested types

### Most Likely Root Cause

1. **Schema registry lookup failure**: Fields like `dns_1` may not be registered, causing fallback to basic transformation
2. **Fallback path incomplete**: When schema lookup fails, `transform_value()` may not be called recursively
3. **Zero-copy path limitation**: Nested objects within JSONB may not be fully processed

---

## Architecture Context

### Exported Rust Functions (PyO3)

| Function | Signature | Purpose |
|----------|-----------|---------|
| `to_camel_case` | `(s: str) -> str` | Convert single snake_case string |
| `transform_json` | `(json_str: str) -> str` | Transform all keys in JSON string |
| `build_graphql_response` | `(json_strings, field_name, type_name, field_selections, is_list) -> bytes` | Build complete GraphQL response |
| `initialize_schema_registry` | `(schema_json: str)` | Initialize type registry |
| `reset_schema_registry_for_testing` | `()` | Clear registry for tests |

### Key Files

| File | Purpose |
|------|---------|
| `fraiseql_rs/src/json_transform.rs` | Value-based JSON transformation |
| `fraiseql_rs/src/core/transform.rs` | Zero-copy streaming transformation |
| `fraiseql_rs/src/pipeline/builder.rs` | GraphQL response building |
| `fraiseql_rs/src/camel_case.rs` | camelCase conversion utilities |
| `fraiseql_rs/src/schema_registry.rs` | Type metadata registry |

---

## Solution Design

### Test Strategy

**Regression Test** (integration):
- **Location**: `tests/regression/test_jsonb_nested_camelcase.py`
- **Scope**: Full GraphQL execution with database
- **Pattern**: Class-scoped fixtures, SchemaAwarePool wrapper

**Unit Test** (isolation):
- **Location**: `tests/unit/core/test_jsonb_camelcase_conversion.py`
- **Scope**: Test Rust functions directly via Python bindings

### Implementation Strategy

1. **Write failing tests** that reproduce the exact bug
2. **Investigate** with diagnostic commands to pinpoint the exact location
3. **Fix** with minimal code changes
4. **Verify** all tests pass with no regressions
5. **Clean up** to evergreen state

---

## Phases

| Phase | Name | Effort | Description |
|-------|------|--------|-------------|
| 1 | RED | 1h | Write failing tests reproducing the bug |
| 2 | GREEN | 2h | Make tests pass with minimal fix |
| 3 | REFACTOR | 30m | Clean up without changing behavior |
| 4 | QA | 30m | Comprehensive validation |
| 5 | UNARCHEOLOGY | 30m | Achieve evergreen state |

**Total Estimated Effort**: 4.5 hours

---

## Files to Create/Modify

### New Test Files
- `tests/regression/test_jsonb_nested_camelcase.py` - Integration tests
- `tests/unit/core/test_jsonb_camelcase_conversion.py` - Unit tests

### Likely Implementation Changes
- `fraiseql_rs/src/json_transform.rs` - Fix recursive transformation
- `fraiseql_rs/src/pipeline/builder.rs` - Ensure transform is applied
- `fraiseql_rs/src/core/transform.rs` - Fix nested object handling (if needed)

---

## Verification Commands

### Quick Check (during development)
```bash
uv run pytest tests/unit/core/test_jsonb_camelcase_conversion.py -v
uv run pytest tests/regression/test_jsonb_nested_camelcase.py -v
```

### Full Suite (before commit)
```bash
uv run pytest tests/ -v --tb=short
```

### Existing JSONB Tests (regression check)
```bash
uv run pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v
uv run pytest tests/integration/graphql/test_jsonb_graphql_full_execution.py -v
```

### PrintOptim Validation
```bash
cd /home/lionel/code/printoptim_backend
uv run pytest tests/api/queries/dim/network/ -v
```

---

## Success Metrics

### Must Have
- [ ] All new tests passing
- [ ] No regressions in existing tests
- [ ] PrintOptim test failures resolved

### Should Have
- [ ] Clean, documented code
- [ ] Consistent with FraiseQL patterns

### Nice to Have
- [ ] Repository in evergreen state
- [ ] No archaeological traces of the fix

---

## Commit Strategy

### Per-Phase Commits
```
test(jsonb): add tests for nested JSONB camelCase conversion [RED]
fix(jsonb): convert nested JSONB object fields to camelCase [GREEN]
refactor(jsonb): clean up nested JSONB camelCase implementation [REFACTOR]
test(jsonb): comprehensive QA validation for nested JSONB fix [QA]
chore(cleanup): achieve evergreen state for JSONB fix [UNARCHEOLOGY]
```

### Final Squashed Commit
```
fix(jsonb): convert nested JSONB object fields to camelCase

Nested objects within JSONB columns now have their field names
correctly converted from snake_case to camelCase in GraphQL responses.

This ensures consistent naming conventions across all levels of nested
JSONB structures, matching GraphQL schema expectations.

Features:
- Nested object fields: smtp_server → smtpServer
- Numbered fields: dns_1 → dns1
- Array items: print_servers[].host_name → printServers[].hostName

Includes comprehensive test coverage for all nested JSONB patterns.
```

---

## Next Steps

1. Start with **Phase 1**: Create failing tests
2. Execute phases sequentially: RED → GREEN → REFACTOR → QA → UNARCHEOLOGY
3. Verify PrintOptim tests pass after fix
4. Squash commits before merging
5. Delete `.phases/jsonb-nested-camelcase-fix/` directory after merge
