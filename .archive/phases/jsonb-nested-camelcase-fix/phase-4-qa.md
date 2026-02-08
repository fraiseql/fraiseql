# Phase 4: QA - Comprehensive Validation

**Status**: Ready for Implementation (after Phase 3)
**Effort**: 30 minutes
**Type**: Quality Assurance

---

## Objective

Verify the fix works in **all scenarios** and is **production-ready**.

---

## Prerequisites

- [ ] Phase 3 completed
- [ ] All tests PASS
- [ ] Code is clean and documented
- [ ] Linters pass

---

## Validation Checklist

### 1. Run Full FraiseQL Test Suite

```bash
uv run pytest tests/ -v --tb=short
```

**Expected**: All tests pass, no regressions

**Pay attention to**:
- `tests/regression/` - All regression tests should pass
- `tests/integration/graphql/` - GraphQL execution tests
- `tests/integration/rust/` - Rust binding tests

### 2. Run Existing JSONB Tests

```bash
# Issue 112 - nested JSONB typename injection
uv run pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v

# JSONB GraphQL full execution
uv run pytest tests/integration/graphql/test_jsonb_graphql_full_execution.py -v

# JSONB FastAPI integration
uv run pytest tests/integration/fastapi/test_fastapi_jsonb_integration.py -v
```

**Expected**: All existing JSONB tests still pass

### 3. Run CamelCase Tests

```bash
# Rust camelCase bindings
uv run pytest tests/integration/rust/test_camel_case.py -v

# Mutation camelCase
uv run pytest tests/integration/graphql/mutations/test_unified_camel_case.py -v
```

**Expected**: All camelCase conversion tests pass

### 4. Run New Tests from Phase 1

```bash
# Unit tests
uv run pytest tests/unit/core/test_jsonb_camelcase_conversion.py -v

# Integration tests
uv run pytest tests/regression/test_jsonb_nested_camelcase.py -v
```

**Expected**: All 13+ new tests pass

### 5. Verify PrintOptim Compatibility (if available)

```bash
cd /home/lionel/code/printoptim_backend

# Run the failing tests that triggered this bug
uv run pytest tests/api/queries/dim/network/test_network_configuration_nested_arrays.py -v
uv run pytest tests/api/queries/dim/network/test_network_configuration_queries.py -v

# Return to FraiseQL
cd /home/lionel/code/fraiseql
```

**Expected**: PrintOptim tests that were failing now PASS

### 6. Manual Rust Function Verification

```bash
python -c "
from fraiseql._fraiseql_rs import to_camel_case, transform_json, build_graphql_response
import json

# Test to_camel_case
print('=== to_camel_case ===')
cases = ['smtp_server', 'dns_1', 'print_servers', 'ip_address', 'gateway']
for c in cases:
    print(f'{c} -> {to_camel_case(c)}')

# Test transform_json
print('\n=== transform_json ===')
data = {
    'smtp_server': {'ip_address': '1.2.3.4'},
    'dns_1': {'ip_address': '8.8.8.8'},
    'print_servers': [{'host_name': 'p1'}]
}
result = json.loads(transform_json(json.dumps(data)))
print(json.dumps(result, indent=2))

# Test build_graphql_response
print('\n=== build_graphql_response ===')
response = json.loads(build_graphql_response(
    [json.dumps(data)],
    'config',
    'Config',
    None,
    False
))
print(json.dumps(response, indent=2))
"
```

**Expected Output**:
```
=== to_camel_case ===
smtp_server -> smtpServer
dns_1 -> dns1
print_servers -> printServers
ip_address -> ipAddress
gateway -> gateway

=== transform_json ===
{
  "smtpServer": {"ipAddress": "1.2.3.4"},
  "dns1": {"ipAddress": "8.8.8.8"},
  "printServers": [{"hostName": "p1"}]
}

=== build_graphql_response ===
{
  "data": {
    "config": {
      "smtpServer": {"ipAddress": "1.2.3.4"},
      "dns1": {"ipAddress": "8.8.8.8"},
      "printServers": [{"hostName": "p1"}]
    }
  }
}
```

### 7. Edge Case Validation

Verify these patterns work correctly:

| Pattern | Input | Expected |
|---------|-------|----------|
| Single word | `gateway` | `gateway` |
| Underscore | `smtp_server` | `smtpServer` |
| Number suffix | `dns_1` | `dns1` |
| Double digit | `dns_10` | `dns10` |
| Number middle | `server_2_name` | `server2Name` |
| Multiple underscores | `user__name` | `userName` |
| Leading underscore | `_private` | `_private` |
| Array field | `print_servers` | `printServers` |
| Nested | `a.b_c.d_1` | each level converted |
| Empty string | `""` | `""` |
| Already camelCase | `smtpServer` | `smtpServer` |

```bash
python -c "
from fraiseql._fraiseql_rs import to_camel_case

cases = [
    ('gateway', 'gateway'),
    ('smtp_server', 'smtpServer'),
    ('dns_1', 'dns1'),
    ('dns_10', 'dns10'),
    ('server_2_name', 'server2Name'),
    ('user__name', 'userName'),
    ('_private', '_private'),
    ('print_servers', 'printServers'),
    ('', ''),
    ('smtpServer', 'smtpServer'),
]

all_pass = True
for input_val, expected in cases:
    result = to_camel_case(input_val)
    status = '✓' if result == expected else '✗'
    if result != expected:
        all_pass = False
    print(f'{status} {input_val!r} -> {result!r} (expected {expected!r})')

print(f'\n{\"All tests passed!\" if all_pass else \"Some tests FAILED\"}')
"
```

### 8. Performance Sanity Check

```bash
# Ensure no significant performance regression
time uv run pytest tests/regression/test_jsonb_nested_camelcase.py -v

# Compare with existing JSONB test
time uv run pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v
```

**Expected**: Similar execution times (within 2x)

### 9. Rust Extension Health Check

```bash
# Verify Rust extension compiles cleanly
cd fraiseql_rs
cargo build --release 2>&1 | grep -E "(warning|error)" || echo "No warnings or errors"
cargo test 2>&1 | tail -20
cd ..
```

**Expected**: No warnings, all Rust tests pass

---

## Validation Summary

Fill this out during QA:

| Check | Status | Notes |
|-------|--------|-------|
| Full test suite | ☐ | |
| Existing JSONB tests | ☐ | |
| CamelCase tests | ☐ | |
| New Phase 1 tests | ☐ | |
| PrintOptim tests | ☐ | N/A if not available |
| Manual function verification | ☐ | |
| Edge cases | ☐ | |
| Performance | ☐ | |
| Rust extension | ☐ | |

---

## Acceptance Criteria

- [ ] Full test suite passes (0 failures)
- [ ] All existing JSONB tests pass
- [ ] All new tests pass
- [ ] PrintOptim tests pass (if applicable)
- [ ] Manual verification shows correct output
- [ ] Edge cases handled correctly
- [ ] No performance degradation
- [ ] Rust extension builds without warnings

---

## If Issues Found

1. **Document the failure** - exact test, error message, expected vs actual
2. **Create additional test** for the failing case in Phase 1 test files
3. **Go back to Phase 2** to fix the issue
4. **Re-run Phase 3** (quick cleanup)
5. **Re-run Phase 4** (this phase)

---

## Commit Message

```
test(jsonb): comprehensive QA validation for nested JSONB fix [QA]

Verify fix works in all scenarios:
- Full test suite: X tests passed
- Existing JSONB tests: all pass
- New camelCase tests: all pass
- Edge cases: all handled correctly
- Performance: no degradation
- PrintOptim compatibility: confirmed

All validation checks pass. Ready for production.
```

---

**Next Phase**: Phase 5 - UNARCHEOLOGY (Achieve evergreen state)
