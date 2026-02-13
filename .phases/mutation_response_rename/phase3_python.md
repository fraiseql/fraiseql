# Phase 3: Python Layer Updates (v1.8.0)

## Objective

Update Python docstrings and comments to use `mutation_response` as the canonical name.

## Duration

1 hour

## Note on v1.8.0 Strategy

The Python layer doesn't interact with PostgreSQL type names directly - it receives JSON from Rust.
This phase only updates documentation/comments to reflect the new canonical name.

## Files to Modify

- `src/fraiseql/mutations/entity_flattener.py`
- `src/fraiseql/mutations/rust_executor.py`

---

## Task 3.1: Update Entity Flattener

**File**: `src/fraiseql/mutations/entity_flattener.py`

### Changes

Search for all docstrings/comments mentioning `mutation_result_v2` and update:

```python
# OLD
"""Parse mutation_result_v2 format from PostgreSQL."""

# NEW
"""Parse mutation_response format from PostgreSQL.

Note: mutation_result_v2 is deprecated but still supported (v1.8.0+).
"""
```

### Verification

```bash
! grep -i "mutation_result_v2" src/fraiseql/mutations/entity_flattener.py
```

---

## Task 3.2: Update Rust Executor

**File**: `src/fraiseql/mutations/rust_executor.py`

### Changes

Update any docstrings/comments mentioning the type.

### Verification

```bash
! grep -i "mutation_result_v2" src/fraiseql/mutations/rust_executor.py
```

---

## Acceptance Criteria

- [ ] Primary documentation uses `mutation_response`
- [ ] Backward compatibility notes added where relevant
- [ ] Imports still work
- [ ] Type hints unchanged (they reference JSON structure, not SQL type)

## Git Commit

```bash
git add src/fraiseql/mutations/
git commit -m "docs(py): update to use mutation_response terminology

- Update entity_flattener.py docstrings
- Update rust_executor.py comments
- Add backward compatibility notes
- No functional changes (Python receives JSON from Rust)"
```

## Next: Phase 4 - Documentation

---

**Phase Status**: ✅ Completed
**Version**: v1.8.0 (documentation only)
**Breaking**: No
