# Phase 5: UNARCHEOLOGY - Evergreen Cleanup

**Status**: Ready for Implementation (after Phase 4)
**Effort**: 30 minutes
**Type**: Archaeological Cleanup

---

## Objective

Remove **all traces of the journey** to achieve an **evergreen codebase**. A developer reading this code in 2030 should see a clean, intentional implementation - not the archaeological layers of how we got here.

---

## Prerequisites

- [ ] Phase 4 completed
- [ ] All tests PASS
- [ ] Code is production-ready

---

## What to Remove

### 1. Inline Journey Comments

**In Test Files**:
```python
# REMOVE comments like:
# "BUG: smtp_server should return as smtpServer"
# "This test will FAIL initially"
# "RED PHASE: This assertion will fail"
# "GREEN PHASE: Now passes after fix"
# "Reproduces PrintOptim bug"

# KEEP only:
# Clear docstrings explaining WHAT the test does (not WHY it was created)
```

**In Implementation Files**:
```rust
// REMOVE comments like:
// "Fix for JSONB nested camelCase bug"
// "This was missing before"
// "Added to handle dns_1 pattern"

// KEEP only:
// Comments explaining non-obvious logic
```

### 2. Bug Reference Comments

**REMOVE**:
```python
"""Regression test for JSONB nested field camelCase conversion.

Bug Report: /tmp/FRAISEQL_JSONB_NESTED_FIELD_BUG.md

Issues:
1. Fields like `smtp_server` return as "smtp_server" instead of "smtpServer"
...
"""
```

**REPLACE WITH**:
```python
"""Test that nested JSONB objects have camelCase field names.

Validates that GraphQL responses correctly convert snake_case field names
from JSONB data to camelCase for:
- Single nested objects (e.g., smtpServer)
- Numbered fields (e.g., dns1, dns2)
- Arrays of nested objects (e.g., printServers)
"""
```

### 3. TDD Phase Markers

**REMOVE**:
```python
# RED: This test should fail initially
# GREEN: Now passes after implementation
# REFACTOR: Cleaned up
```

### 4. Temporary Test Assertions

**REMOVE overly defensive assertions**:
```python
# REMOVE:
assert "smtpServer" in config, (
    f"Expected 'smtpServer' in response, got keys: {list(config.keys())}. "
    f"BUG: Field is likely returned as 'smtp_server' (snake_case)"
)

# REPLACE WITH:
assert "smtpServer" in config
assert config["smtpServer"]["ipAddress"] == "13.16.1.10"
```

### 5. Temporary Files

**DELETE**:
```bash
# Bug report (no longer needed)
rm /tmp/FRAISEQL_JSONB_NESTED_FIELD_BUG.md

# Any other investigation artifacts
rm /tmp/fraiseql-*
```

### 6. Phase Plans (Optional)

**AFTER merge to main**, archive or delete:
```bash
# Option A: Archive for historical reference
mv .phases/jsonb-nested-camelcase-fix/ .phases/_archive/

# Option B: Delete (recommended for evergreen)
rm -rf .phases/jsonb-nested-camelcase-fix/
```

---

## What to Keep

### 1. Clear, Timeless Documentation

**Docstrings should read as if the feature always existed**:
```python
class TestJSONBNestedCamelCase:
    """Test camelCase conversion for nested JSONB objects.

    These tests verify that nested objects within JSONB columns have their
    field names correctly converted from snake_case to camelCase in GraphQL
    responses, matching the GraphQL schema conventions.
    """
```

### 2. Meaningful Test Names

**Test names should describe behavior, not history**:
```python
# GOOD
def test_nested_object_fields_convert_to_camelcase(self):
def test_numbered_fields_convert_correctly(self):
def test_array_field_names_convert_to_camelcase(self):

# BAD (references the bug)
def test_fix_for_printoptim_bug(self):
def test_dns1_no_longer_missing(self):
```

### 3. Essential Comments

**Keep comments that explain WHY, not WHAT**:
```rust
// Handle underscore before digit: dns_1 → dns1
// This differs from standard camelCase (dns_1 → dns1, not dnsOne)
if c.is_ascii_digit() {
    result.push(c);
}
```

---

## Cleanup Checklist

### Test Files

- [ ] `tests/regression/test_jsonb_nested_camelcase.py`
  - [ ] Remove "BUG:" from docstrings
  - [ ] Remove "Expected to FAIL" comments
  - [ ] Remove verbose error messages (keep simple assertions)
  - [ ] Ensure docstring describes behavior, not history

- [ ] `tests/unit/core/test_jsonb_camelcase_conversion.py`
  - [ ] Remove phase markers
  - [ ] Remove investigation comments
  - [ ] Keep only behavior-describing docstrings

### Implementation Files

- [ ] `fraiseql_rs/src/json_transform.rs`
  - [ ] Remove "fix for" comments
  - [ ] Ensure documentation is evergreen

- [ ] `fraiseql_rs/src/camel_case.rs`
  - [ ] Remove "added to handle" comments
  - [ ] Keep only explanatory comments for non-obvious logic

### External Files

- [ ] Delete `/tmp/FRAISEQL_JSONB_NESTED_FIELD_BUG.md`
- [ ] Archive or delete `.phases/jsonb-nested-camelcase-fix/`

---

## Verification

### After Cleanup

```bash
# Ensure tests still pass
uv run pytest tests/regression/test_jsonb_nested_camelcase.py -v
uv run pytest tests/unit/core/test_jsonb_camelcase_conversion.py -v

# Check for archaeological remnants
grep -r "BUG:\|RED\|GREEN\|FAIL initially\|PrintOptim" tests/regression/test_jsonb_nested_camelcase.py
grep -r "fix for\|was missing\|added to handle" fraiseql_rs/src/

# Should return NO matches
```

### Final Review

Read through the code as if you've never seen it before. Ask:
- Does every comment add value?
- Does the docstring make sense without knowing the bug history?
- Would a new developer understand this code?

---

## Final Commit

**Squash all phase commits into one clean commit**:

```bash
# Interactive rebase to squash
git rebase -i HEAD~5

# Squash all into first commit, rewrite message
```

**Final Commit Message**:
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

**Note**: The commit message describes WHAT the code does, not the journey to get there.

---

## Acceptance Criteria

- [ ] No "BUG:", "RED", "GREEN", "FAIL" comments in code
- [ ] No references to bug reports or investigation files
- [ ] Docstrings describe behavior, not history
- [ ] Test names describe behavior, not fixes
- [ ] All tests still PASS
- [ ] Code reads as if the feature was always there
- [ ] Temporary files deleted
- [ ] Phase plans archived or deleted (after merge)

---

## The Evergreen Test

Ask yourself:
> "If I read this code in 5 years, would I know there was ever a bug?"

**The answer should be NO.**

The code should look like it was designed this way from the start. Clean. Intentional. Timeless.

---

## Commit Message

```
chore(cleanup): achieve evergreen state for JSONB camelCase fix [UNARCHEOLOGY]

Remove all archaeological traces of the bug fix journey:
- Remove "BUG:" comments from test docstrings
- Remove phase markers (RED/GREEN/REFACTOR)
- Simplify verbose error messages
- Update docstrings to describe behavior, not history
- Delete temporary investigation files

The codebase now reads as if nested JSONB camelCase conversion
was always designed this way. Eternal sunshine achieved.
```

---

## After This Phase

1. **Squash commits** before merging to main
2. **Delete phase plans** from `.phases/` directory
3. **Verify PrintOptim tests pass**
4. **Celebrate** - the bug is fixed and the code is evergreen! 🎉
