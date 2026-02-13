# Phase 5: Test Files Updates

## Objective

Update test files and fixtures to use `mutation_response`.

## Duration

1 hour

## Files to Modify

- `tests/fixtures/cascade/conftest.py`
- `tests/test_mutations/test_status_taxonomy.py`
- `tests/integration/graphql/mutations/test_unified_camel_case.py`

---

## Task 5.1-5.3: Update Test Files

For EACH file:

1. Find SQL function definitions
2. Replace `mutation_result_v2` → `mutation_response`
3. Update docstrings/comments

### Example change

```python
# OLD
await db.execute("""
    CREATE FUNCTION test_func()
    RETURNS mutation_result_v2 AS $$
    ...
    )::mutation_result_v2;
""")

# NEW
await db.execute("""
    CREATE FUNCTION test_func()
    RETURNS mutation_response AS $$
    ...
    )::mutation_response;
""")
```

---

## Task 5.4: Run Tests

```bash
uv run pytest tests/test_mutations/ -v
uv run pytest tests/integration/graphql/mutations/ -v
uv run pytest tests/fixtures/cascade/ -v
```

**Expected**: All pass

---

## Acceptance Criteria

- [ ] All test files updated
- [ ] No `mutation_result_v2` in tests/
- [ ] All tests pass

## Git Commit

```bash
git add tests/
git commit -m "test: update mutation_response references

- Update test fixtures
- Update test SQL functions
- All tests passing"
```

## Next: Phase 6 - Final Verification

---

**Phase Status**: ✅ Completed
**Version**: v1.8.0
**Breaking**: No (test files only)
