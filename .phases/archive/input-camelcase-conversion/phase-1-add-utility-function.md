# Phase 1: Add Input Key Conversion Utility

**Feature**: Input CamelCase → snake_case Conversion for PostgreSQL
**Phase**: 1/3 - Add utility function for recursive key conversion
**Type**: TDD RED Phase

---

## Objective

Add a utility function `dict_keys_to_snake_case()` that recursively converts all dictionary keys from camelCase to snake_case, preserving nested structures and handling edge cases.

---

## Context

**Problem**: FraiseQL converts camelCase → snake_case for outputs but NOT for mutation inputs sent to PostgreSQL. This causes `jsonb_populate_record()` to fail silently when populating composite types.

**Solution**: Add a utility function to convert input dict keys before serializing to JSON in `rust_executor.py`.

**Reference**: `/tmp/fraiseql_input_conversion_verification.md`

---

## Files to Create

1. **Test file**: `tests/unit/utils/test_dict_keys_to_snake_case.py`

---

## Files to Modify

1. **`src/fraiseql/utils/casing.py`**: Add `dict_keys_to_snake_case()` function

---

## Implementation Steps

### Step 1: Write Failing Tests (RED Phase)

**File**: `tests/unit/utils/test_dict_keys_to_snake_case.py`

Create comprehensive tests covering:

```python
"""Test dict_keys_to_snake_case utility function."""

import pytest
from fraiseql.utils.casing import dict_keys_to_snake_case


class TestDictKeysToSnakeCase:
    """Test dictionary key conversion from camelCase to snake_case."""

    def test_simple_dict_conversion(self):
        """Test basic camelCase to snake_case conversion."""
        input_dict = {
            "firstName": "John",
            "lastName": "Doe",
            "emailAddress": "john@example.com",
        }
        expected = {
            "first_name": "John",
            "last_name": "Doe",
            "email_address": "john@example.com",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_nested_dict_conversion(self):
        """Test recursive conversion in nested dicts."""
        input_dict = {
            "userId": "123",
            "userProfile": {
                "firstName": "John",
                "phoneNumber": "555-1234",
            },
        }
        expected = {
            "user_id": "123",
            "user_profile": {
                "first_name": "John",
                "phone_number": "555-1234",
            },
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_list_of_dicts_conversion(self):
        """Test conversion in lists of dicts."""
        input_dict = {
            "userId": "123",
            "userTags": [
                {"tagName": "admin", "tagColor": "red"},
                {"tagName": "developer", "tagColor": "blue"},
            ],
        }
        expected = {
            "user_id": "123",
            "user_tags": [
                {"tag_name": "admin", "tag_color": "red"},
                {"tag_name": "developer", "tag_color": "blue"},
            ],
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_deeply_nested_structures(self):
        """Test conversion in deeply nested structures."""
        input_dict = {
            "contractData": {
                "lineItems": [
                    {
                        "itemId": "A1",
                        "priceInfo": {
                            "startDate": "2025-01-01",
                            "endDate": "2025-12-31",
                        },
                    }
                ]
            }
        }
        expected = {
            "contract_data": {
                "line_items": [
                    {
                        "item_id": "A1",
                        "price_info": {
                            "start_date": "2025-01-01",
                            "end_date": "2025-12-31",
                        },
                    }
                ]
            }
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_preserves_snake_case_keys(self):
        """Test that already snake_case keys are preserved."""
        input_dict = {
            "user_id": "123",
            "first_name": "John",
        }
        expected = {
            "user_id": "123",
            "first_name": "John",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_mixed_case_keys(self):
        """Test mixed camelCase and snake_case keys."""
        input_dict = {
            "userId": "123",
            "first_name": "John",
            "emailAddress": "john@example.com",
        }
        expected = {
            "user_id": "123",
            "first_name": "John",
            "email_address": "john@example.com",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_empty_dict(self):
        """Test empty dict returns empty dict."""
        assert dict_keys_to_snake_case({}) == {}

    def test_preserves_non_string_values(self):
        """Test that values are preserved as-is."""
        from datetime import date
        from uuid import UUID

        input_dict = {
            "userId": UUID("12345678-1234-5678-1234-567812345678"),
            "startDate": date(2025, 1, 1),
            "isActive": True,
            "retryCount": 42,
            "metadata": None,
        }
        expected = {
            "user_id": UUID("12345678-1234-5678-1234-567812345678"),
            "start_date": date(2025, 1, 1),
            "is_active": True,
            "retry_count": 42,
            "metadata": None,
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_handles_empty_strings(self):
        """Test empty string values are preserved."""
        input_dict = {
            "firstName": "",
            "lastName": "Doe",
        }
        expected = {
            "first_name": "",
            "last_name": "Doe",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_handles_lists_of_primitives(self):
        """Test lists of primitive values are preserved."""
        input_dict = {
            "userIds": ["123", "456", "789"],
            "statusCodes": [200, 404, 500],
        }
        expected = {
            "user_ids": ["123", "456", "789"],
            "status_codes": [200, 404, 500],
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_handles_mixed_lists(self):
        """Test lists containing both dicts and primitives."""
        input_dict = {
            "mixedData": [
                "string",
                123,
                {"itemName": "value"},
            ]
        }
        expected = {
            "mixed_data": [
                "string",
                123,
                {"item_name": "value"},
            ]
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_acronyms_in_keys(self):
        """Test handling of acronyms (e.g., 'IP', 'DNS', 'URL')."""
        input_dict = {
            "ipAddress": "192.168.1.1",
            "dnsServerName": "ns1.example.com",
            "apiURL": "https://api.example.com",
        }
        expected = {
            "ip_address": "192.168.1.1",
            "dns_server_name": "ns1.example.com",
            "api_url": "https://api.example.com",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_consecutive_capitals(self):
        """Test keys with consecutive capital letters."""
        input_dict = {
            "HTTPStatusCode": 200,
            "URLPath": "/api/v1/users",
        }
        expected = {
            "http_status_code": 200,
            "url_path": "/api/v1/users",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_single_letter_keys(self):
        """Test single-letter keys are preserved."""
        input_dict = {
            "x": 10,
            "y": 20,
            "z": 30,
        }
        expected = {
            "x": 10,
            "y": 20,
            "z": 30,
        }
        assert dict_keys_to_snake_case(input_dict) == expected
```

**Expected Result**: All tests should FAIL because the function doesn't exist yet.

### Step 2: Add Function Signature (Still RED)

**File**: `src/fraiseql/utils/casing.py`

Add the function signature (implementation in Phase 2):

```python
def dict_keys_to_snake_case(data: dict | list | Any) -> dict | list | Any:
    """Recursively convert dictionary keys from camelCase to snake_case.

    This function is used to convert GraphQL input (camelCase) to PostgreSQL-compatible
    format (snake_case) before serializing to JSONB.

    Args:
        data: Input data structure (dict, list, or primitive)

    Returns:
        Data structure with all dict keys converted to snake_case

    Examples:
        >>> dict_keys_to_snake_case({"firstName": "John", "lastName": "Doe"})
        {'first_name': 'John', 'last_name': 'Doe'}

        >>> dict_keys_to_snake_case({"user": {"emailAddress": "john@example.com"}})
        {'user': {'email_address': 'john@example.com'}}

        >>> dict_keys_to_snake_case({"items": [{"itemName": "A"}, {"itemName": "B"}]})
        {'items': [{'item_name': 'A'}, {'item_name': 'B'}]}
    """
    raise NotImplementedError("Will be implemented in Phase 2 (GREEN)")
```

---

## Verification Commands

```bash
# Run tests (should FAIL)
uv run pytest tests/unit/utils/test_dict_keys_to_snake_case.py -v

# Check linting
uv run ruff check src/fraiseql/utils/casing.py tests/unit/utils/test_dict_keys_to_snake_case.py
```

**Expected Output**:
- ❌ All tests fail with `NotImplementedError`
- ✅ Linting passes

---

## Acceptance Criteria

- [x] Test file created with 15+ test cases covering:
  - Simple dict conversion
  - Nested dicts
  - Lists of dicts
  - Deeply nested structures
  - Edge cases (empty dict, primitives, None values)
  - Acronyms and consecutive capitals
- [x] Function signature added to `casing.py` with docstring
- [x] All tests FAIL with `NotImplementedError`
- [x] Linting passes

---

## DO NOT

- ❌ Implement the function logic (that's Phase 2 GREEN)
- ❌ Skip any test cases
- ❌ Modify existing functions in `casing.py`
- ❌ Add dependencies or imports beyond what's needed for tests

---

## Notes

- This is a TDD RED phase - tests should fail
- Phase 2 (GREEN) will implement the function to make tests pass
- Phase 3 (INTEGRATION) will integrate into `rust_executor.py`
