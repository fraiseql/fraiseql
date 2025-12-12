# Phase 3: Integrate Input Conversion into Mutations

**Feature**: Input CamelCase → snake_case Conversion for PostgreSQL
**Phase**: 3/3 - Integrate utility into mutation executor
**Type**: Integration & Testing

---

## Objective

Integrate `dict_keys_to_snake_case()` into `rust_executor.py` to automatically convert mutation input keys before sending to PostgreSQL, and add integration tests to verify the fix.

---

## Context

**Prerequisites**:
- Phase 1 completed (utility tests exist)
- Phase 2 completed (utility function implemented and passing)

**Problem**: PostgreSQL functions receive camelCase JSON keys, causing `jsonb_populate_record()` to fail.

**Solution**: Apply `dict_keys_to_snake_case()` to `input_data` before `json.dumps()` when `auto_camel_case=True`.

---

## Files to Create

1. **Integration test**: `tests/integration/graphql/mutations/test_input_camelcase_to_snake_case.py`

---

## Files to Modify

1. **`src/fraiseql/mutations/rust_executor.py`**: Add input key conversion before serialization

---

## Implementation Steps

### Step 1: Write Integration Test (RED)

**File**: `tests/integration/graphql/mutations/test_input_camelcase_to_snake_case.py`

Create a test that reproduces the PrintOptim issue:

```python
"""Integration test for camelCase → snake_case conversion in mutation inputs.

This test verifies that FraiseQL correctly converts GraphQL input field names
(camelCase) to PostgreSQL field names (snake_case) before calling database functions,
ensuring jsonb_populate_record() works correctly with composite types.
"""

import pytest
from datetime import date
from uuid import UUID

import fraiseql
from fraiseql.mutations.decorators import success, failure
from fraiseql.mutations.mutation_decorator import mutation
from fraiseql.types.fraise_input import fraise_input

pytestmark = pytest.mark.integration


@pytest.mark.asyncio
class TestMutationInputCamelCaseConversion:
    """Test that mutation inputs are converted from camelCase to snake_case."""

    @pytest.fixture(scope="class")
    async def setup_test_schema(self, class_db_pool, test_schema, clear_registry_class):
        """Set up test schema with composite type and function using jsonb_populate_record."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")

            # Create mutation_response type
            await conn.execute(
                """
                CREATE TYPE mutation_response AS (
                    status TEXT,
                    message TEXT,
                    entity_id TEXT,
                    entity_type TEXT,
                    entity JSONB,
                    updated_fields TEXT[],
                    cascade JSONB,
                    metadata JSONB
                )
                """
            )

            # Create composite type with snake_case fields (like PostgreSQL convention)
            await conn.execute(
                """
                CREATE TYPE test_price_input AS (
                    contract_id UUID,
                    contract_item_id UUID,
                    start_date DATE,
                    end_date DATE,
                    amount DOUBLE PRECISION,
                    currency TEXT
                )
                """
            )

            # Create test table
            await conn.execute(
                """
                CREATE TABLE test_prices (
                    id TEXT PRIMARY KEY,
                    contract_id UUID NOT NULL,
                    contract_item_id UUID NOT NULL,
                    start_date DATE NOT NULL,
                    end_date DATE NOT NULL,
                    amount DOUBLE PRECISION NOT NULL,
                    currency TEXT NOT NULL,
                    created_at TIMESTAMPTZ DEFAULT NOW()
                )
                """
            )

            # Create function that uses jsonb_populate_record
            # This is the pattern that fails without input key conversion
            await conn.execute(
                f"""
                CREATE OR REPLACE FUNCTION {test_schema}.create_test_price(input_payload JSONB)
                RETURNS mutation_response AS $$
                DECLARE
                    v_input test_price_input;
                    new_id TEXT;
                    price_data JSONB;
                BEGIN
                    -- This is the critical line that fails without input conversion
                    -- If input_payload has camelCase keys, all fields will be NULL
                    v_input := jsonb_populate_record(NULL::test_price_input, input_payload);

                    -- Verify that fields were populated
                    IF v_input.contract_id IS NULL THEN
                        RETURN ROW(
                            'failed:validation',
                            'contract_id is NULL - input conversion failed',
                            NULL, NULL, NULL, NULL, NULL, NULL
                        )::mutation_response;
                    END IF;

                    new_id := gen_random_uuid()::TEXT;

                    INSERT INTO test_prices (
                        id, contract_id, contract_item_id,
                        start_date, end_date, amount, currency
                    )
                    VALUES (
                        new_id,
                        v_input.contract_id,
                        v_input.contract_item_id,
                        v_input.start_date,
                        v_input.end_date,
                        v_input.amount,
                        v_input.currency
                    )
                    RETURNING to_jsonb(test_prices.*) INTO price_data;

                    RETURN ROW(
                        'success',
                        'Price created successfully',
                        new_id,
                        'TestPrice',
                        price_data,
                        NULL,
                        NULL,
                        NULL
                    )::mutation_response;
                END;
                $$ LANGUAGE plpgsql;
                """
            )

            await conn.commit()

        yield

    async def test_mutation_converts_camelcase_input_to_snake_case(
        self, db_connection, setup_test_schema, clear_registry, test_schema
    ):
        """Verify that camelCase input is converted to snake_case before PostgreSQL call."""
        from types import SimpleNamespace

        # Create a mock config with auto_camel_case=True
        config = SimpleNamespace(auto_camel_case=True)

        # Execute mutation with camelCase input (as GraphQL would send)
        from fraiseql.mutations.rust_executor import execute_mutation_rust

        camelcase_input = {
            "contractId": "11111111-1111-1111-1111-111111111111",
            "contractItemId": "22222222-2222-2222-2222-222222222222",
            "startDate": "2025-02-15",
            "endDate": "2025-12-15",
            "amount": 99.99,
            "currency": "EUR",
        }

        result = await execute_mutation_rust(
            conn=db_connection,
            function_name=f"{test_schema}.create_test_price",
            input_data=camelcase_input,
            field_name="createTestPrice",
            success_type="CreateTestPriceSuccess",
            error_type="CreateTestPriceError",
            entity_field_name="price",
            entity_type="TestPrice",
            context_args=None,
            cascade_selections=None,
            config=config,
        )

        # Parse response
        data = result.to_json()

        # Verify no errors
        assert "errors" not in data or data["errors"] is None

        # Verify success
        mutation_result = data["data"]["createTestPrice"]
        assert mutation_result["__typename"] == "CreateTestPriceSuccess"
        assert mutation_result["message"] == "Price created successfully"

        # CRITICAL: Verify that the price entity was populated correctly
        # If input conversion didn't work, all fields would be NULL and the
        # function would have returned an error
        price = mutation_result["price"]
        assert price["contractId"] == "11111111-1111-1111-1111-111111111111"
        assert price["contractItemId"] == "22222222-2222-2222-2222-222222222222"
        assert price["startDate"] == "2025-02-15"
        assert price["endDate"] == "2025-12-15"
        assert price["amount"] == 99.99
        assert price["currency"] == "EUR"

    async def test_mutation_preserves_snake_case_when_auto_camel_case_false(
        self, db_connection, setup_test_schema, clear_registry, test_schema
    ):
        """Verify that snake_case input is NOT converted when auto_camel_case=False."""
        from types import SimpleNamespace

        config = SimpleNamespace(auto_camel_case=False)

        from fraiseql.mutations.rust_executor import execute_mutation_rust

        # Send snake_case input (as if auto_camel_case is disabled)
        snake_case_input = {
            "contract_id": "33333333-3333-3333-3333-333333333333",
            "contract_item_id": "44444444-4444-4444-4444-444444444444",
            "start_date": "2025-03-01",
            "end_date": "2025-11-30",
            "amount": 149.99,
            "currency": "USD",
        }

        result = await execute_mutation_rust(
            conn=db_connection,
            function_name=f"{test_schema}.create_test_price",
            input_data=snake_case_input,
            field_name="createTestPrice",
            success_type="CreateTestPriceSuccess",
            error_type="CreateTestPriceError",
            entity_field_name="price",
            entity_type="TestPrice",
            context_args=None,
            cascade_selections=None,
            config=config,
        )

        # Parse response
        data = result.to_json()

        # Verify success (snake_case input should work directly)
        mutation_result = data["data"]["createTestPrice"]
        assert mutation_result["__typename"] == "CreateTestPriceSuccess"

        # Verify entity fields remain snake_case (auto_camel_case=False)
        price = mutation_result["price"]
        assert "contract_id" in price
        assert "contract_item_id" in price
        assert price["contract_id"] == "33333333-3333-3333-3333-333333333333"

    async def test_nested_input_objects_are_converted(
        self, db_connection, setup_test_schema, clear_registry, test_schema
    ):
        """Verify that nested objects in input are also converted."""
        from types import SimpleNamespace

        config = SimpleNamespace(auto_camel_case=True)

        from fraiseql.mutations.rust_executor import execute_mutation_rust

        # Create function that accepts nested input
        async with db_connection.cursor() as cursor:
            await cursor.execute(
                f"""
                CREATE OR REPLACE FUNCTION {test_schema}.test_nested_input(input_payload JSONB)
                RETURNS mutation_response AS $$
                DECLARE
                    contract_id_value TEXT;
                BEGIN
                    -- Access nested field using snake_case path
                    contract_id_value := input_payload->'nested_data'->>'contract_id';

                    RETURN ROW(
                        'success',
                        'Nested input processed',
                        contract_id_value,
                        'NestedTest',
                        jsonb_build_object('result', contract_id_value),
                        NULL, NULL, NULL
                    )::mutation_response;
                END;
                $$ LANGUAGE plpgsql;
                """
            )
            await db_connection.commit()

        # Send nested camelCase input
        nested_input = {
            "nestedData": {
                "contractId": "55555555-5555-5555-5555-555555555555",
                "startDate": "2025-01-01",
            }
        }

        result = await execute_mutation_rust(
            conn=db_connection,
            function_name=f"{test_schema}.test_nested_input",
            input_data=nested_input,
            field_name="testNestedInput",
            success_type="TestNestedSuccess",
            error_type="TestNestedError",
            entity_field_name="result",
            entity_type="NestedTest",
            context_args=None,
            cascade_selections=None,
            config=config,
        )

        data = result.to_json()
        mutation_result = data["data"]["testNestedInput"]

        # Verify nested conversion worked
        assert mutation_result["__typename"] == "TestNestedSuccess"
        assert mutation_result["entityId"] == "55555555-5555-5555-5555-555555555555"
```

**Expected Result**: Test should FAIL initially because `rust_executor.py` doesn't convert input keys yet.

### Step 2: Integrate into rust_executor.py

**File**: `src/fraiseql/mutations/rust_executor.py`

**Location**: Line 82-83 (before `json.dumps()`)

**Change**:

```python
# Import at top of file (add to existing imports)
from fraiseql.utils.casing import dict_keys_to_snake_case

# ... (existing code) ...

async def execute_mutation_rust(
    conn: Any,
    function_name: str,
    input_data: dict[str, Any],
    # ... other params
) -> RustResponseBytes:
    """Execute mutation via Rust-first pipeline."""

    # ... (existing code up to line 80) ...

    # Extract auto_camel_case from config (default True for backward compatibility)
    auto_camel_case = getattr(config, "auto_camel_case", True) if config else True

    # Convert input keys to snake_case before serializing to JSON for PostgreSQL
    # This ensures jsonb_populate_record() works correctly with composite types
    # that use snake_case field names (PostgreSQL convention)
    if auto_camel_case:
        input_data = dict_keys_to_snake_case(input_data)

    # Convert input to JSON
    input_json = json.dumps(input_data, separators=(",", ":"))

    # ... (rest of function unchanged)
```

**Rationale**:
- Apply conversion only when `auto_camel_case=True` (GraphQL expects camelCase)
- Convert before `json.dumps()` to ensure PostgreSQL receives snake_case keys
- Preserves backward compatibility (when `auto_camel_case=False`, no conversion)

### Step 3: Run Integration Test

```bash
# Run the new integration test
uv run pytest tests/integration/graphql/mutations/test_input_camelcase_to_snake_case.py -v

# Run all mutation tests (regression check)
uv run pytest tests/integration/graphql/mutations/ -v
```

**Expected Output**:
- ✅ All 3 new integration tests pass
- ✅ No regressions in existing mutation tests

### Step 4: Run Full Test Suite

```bash
# Run all tests to ensure no regressions
uv run pytest tests/ -v

# Specifically check existing camelCase tests
uv run pytest tests/unit/utils/test_*camel*.py tests/integration/rust/test_camel_case.py -v
```

**Expected Output**: All tests pass

---

## Verification Commands

```bash
# Integration test (new)
uv run pytest tests/integration/graphql/mutations/test_input_camelcase_to_snake_case.py -v

# All mutation tests (regression)
uv run pytest tests/integration/graphql/mutations/ -v

# All camelCase tests (regression)
uv run pytest tests/unit/utils/test_*camel*.py tests/integration/rust/test_camel_case.py -v

# Linting
uv run ruff check src/fraiseql/mutations/rust_executor.py

# Full test suite
uv run pytest tests/ -k "not slow"
```

**Expected Output**:
- ✅ 3 new integration tests pass
- ✅ All existing tests pass (no regressions)
- ✅ Linting passes

---

## Acceptance Criteria

- [x] Integration test created in `tests/integration/graphql/mutations/test_input_camelcase_to_snake_case.py`
- [x] Test includes:
  - PostgreSQL composite type with snake_case fields
  - Function using `jsonb_populate_record()`
  - Mutation sending camelCase input
  - Verification that fields are populated correctly
  - Test for `auto_camel_case=False` (no conversion)
  - Test for nested input objects
- [x] `rust_executor.py` modified to call `dict_keys_to_snake_case()` before `json.dumps()`
- [x] Conversion only applied when `auto_camel_case=True`
- [x] All new tests pass
- [x] All existing tests pass (no regressions)
- [x] Linting passes

---

## DO NOT

- ❌ Modify the Rust pipeline (`fraiseql_rs/`)
- ❌ Change output conversion logic (it already works)
- ❌ Apply conversion when `auto_camel_case=False`
- ❌ Skip the integration test (critical for validation)
- ❌ Modify PostgreSQL function signatures

---

## Notes

### Why This Fix Works

1. **GraphQL Layer**: Clients send camelCase (e.g., `contractId`)
2. **Python Coercion**: Converts to snake_case Python attributes (e.g., `obj.contract_id`)
3. **Dict Conversion**: `_to_dict()` creates dict with snake_case keys
4. **NEW: Pre-Serialization**: `dict_keys_to_snake_case()` ensures all keys are snake_case
5. **JSON Serialization**: `json.dumps()` creates JSON with snake_case keys
6. **PostgreSQL**: `jsonb_populate_record()` works correctly with snake_case composite types

### Backward Compatibility

- When `auto_camel_case=False`: No conversion applied (legacy behavior preserved)
- When `auto_camel_case=True`: Input converted to snake_case (new behavior, fixes the bug)

### Performance Impact

- Minimal: Dict key conversion is O(n) where n = total number of keys in input
- Typical mutation input: 5-20 keys → negligible overhead
- No impact on query performance (queries don't use this path)

---

## Cleanup After Implementation

**Remove SQL workarounds in PrintOptim**:

After this fix is deployed, the `core.jsonb_camel_to_snake()` SQL function in PrintOptim can be removed. Update all wrapper functions to use the original pattern:

```sql
-- BEFORE (with workaround):
v_payload_snake := core.jsonb_camel_to_snake(input_payload);
v_input := jsonb_populate_record(NULL::app.type_price_input, v_payload_snake);

-- AFTER (with FraiseQL fix):
v_input := jsonb_populate_record(NULL::app.type_price_input, input_payload);
```

---

## Next Steps

After Phase 3 is complete:
1. Update FraiseQL documentation (`docs/features/auto-camel-case.md`)
2. Add changelog entry for v1.8.1
3. Create migration guide for users with SQL workarounds
4. Deploy to PrintOptim and remove SQL workaround functions
