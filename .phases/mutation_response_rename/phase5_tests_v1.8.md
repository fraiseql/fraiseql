# Phase 5: Test Files Updates (v1.8.0)

## Objective

Update test files to use `mutation_response` as the recommended pattern.

## Duration

1 hour

## Files to Modify

- `tests/fixtures/cascade/conftest.py`
- `tests/test_mutations/test_status_taxonomy.py`
- `tests/integration/graphql/mutations/test_unified_camel_case.py`

---

## Strategy for v1.8.0

**Tests should:**

1. Use `mutation_response` (new name) to demonstrate best practices
2. Old tests using `mutation_result_v2` don't need to change (backward compatible)
3. New tests should use `mutation_response`

---

## Task 5.1-5.3: Update Test Files

For EACH file:

1. Find SQL function definitions in test setup
2. Update `RETURNS mutation_result_v2` → `RETURNS mutation_response`
3. Update casts `::mutation_result_v2` → `::mutation_response`
4. Update docstrings/comments mentioning the type

### Example change

```python
# OLD
await db.execute("""
    CREATE FUNCTION test_func()
    RETURNS mutation_result_v2 AS $$
    BEGIN
        RETURN ROW(
            'success',
            'Test message',
            NULL, NULL, '{}'::jsonb, NULL, NULL, NULL
        )::mutation_result_v2;
    END;
    $$ LANGUAGE plpgsql;
""")

# NEW
await db.execute("""
    CREATE FUNCTION test_func()
    RETURNS mutation_response AS $$
    BEGIN
        RETURN ROW(
            'success',
            'Test message',
            NULL, NULL, '{}'::jsonb, NULL, NULL, NULL
        )::mutation_response;
    END;
    $$ LANGUAGE plpgsql;
""")
```

---

## Task 5.4: Run Tests

```bash
uv run pytest tests/test_mutations/ -v
uv run pytest tests/integration/graphql/mutations/ -v
uv run pytest tests/fixtures/cascade/ -v
```

**Expected**: All pass (no changes to test logic, only type names)

---

## Task 5.5: Add Backward Compatibility Test

**File**: `tests/test_mutations/test_mutation_response_alias.py` (NEW)

**Purpose**: Verify both type names work

```python
"""Test backward compatibility between mutation_response and mutation_result_v2."""

import pytest


@pytest.mark.asyncio
async def test_both_type_names_work(db_connection):
    """Verify both mutation_response and mutation_result_v2 work."""

    # Create function using NEW name
    await db_connection.execute("""
        CREATE FUNCTION test_new_name()
        RETURNS mutation_response AS $$
        BEGIN
            RETURN ROW(
                'success',
                'Using mutation_response',
                NULL, NULL, '{}'::jsonb, NULL, NULL, NULL
            )::mutation_response;
        END;
        $$ LANGUAGE plpgsql;
    """)

    # Create function using OLD name (deprecated)
    await db_connection.execute("""
        CREATE FUNCTION test_old_name()
        RETURNS mutation_result_v2 AS $$
        BEGIN
            RETURN ROW(
                'success',
                'Using mutation_result_v2',
                NULL, NULL, '{}'::jsonb, NULL, NULL, NULL
            )::mutation_result_v2;
        END;
        $$ LANGUAGE plpgsql;
    """)

    # Both should work
    result_new = await db_connection.fetchone("SELECT test_new_name()")
    result_old = await db_connection.fetchone("SELECT test_old_name()")

    assert result_new is not None
    assert result_old is not None

    # Both should have same structure
    assert result_new[0][0] == 'success'
    assert result_old[0][0] == 'success'


@pytest.mark.asyncio
async def test_helper_functions_return_mutation_response(db_connection):
    """Verify helper functions return mutation_response type."""

    # Helper functions should return mutation_response
    result = await db_connection.fetchone("""
        SELECT mutation_success('Test', '{"id": "1"}'::jsonb)
    """)

    assert result is not None
    assert result[0][0] == 'success'  # status field
    assert result[0][1] == 'Test'     # message field
```

---

## Acceptance Criteria

- [ ] All test files updated to use `mutation_response`
- [ ] No `mutation_result_v2` in new test code
- [ ] Backward compatibility test added
- [ ] All tests pass
- [ ] Both type names verified working

## Git Commit

```bash
git add tests/
git commit -m "test: update to use mutation_response

- Update test fixtures to use mutation_response
- Update test SQL functions to use mutation_response
- Add backward compatibility test for both type names
- All tests passing

Both mutation_response and mutation_result_v2 work in v1.8.0."
```

## Next: Phase 6 - Final Verification

---

**Phase Status**: ⏸️ Ready to Start
**Version**: v1.8.0 (alias strategy)
**Breaking**: No (backward compatible)
