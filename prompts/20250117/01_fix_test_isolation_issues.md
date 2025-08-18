# Fix Test Isolation Issues in FraiseQL

## Problem ✅ FIXED
Several tests are failing when run as part of the full test suite but pass when run individually. This indicates test isolation issues where tests are interfering with each other through shared state.

## Failing Tests ✅ FIXED
1. `tests/database/test_developer_experience.py::TestValidationUtilities::test_validate_where_input_type_mismatch`
2. `tests/gql/test_validation_extended.py::TestValidateWhereInput::test_operator_type_validation`

## Root Cause Identified ✅
The issue was that these two test files used the global `clear_registry` fixture from `tests/conftest.py`, but it was NOT configured with `autouse=True`. This meant registry cleanup only happened when explicitly requested via fixture parameters, but not automatically between tests.

Other test files in the codebase had their own `autouse=True` clear_registry fixtures, but these two files relied on the global one, creating test isolation issues when run as part of the full suite.

## Solution Applied ✅
Added `autouse=True` clear_registry fixtures to both failing test files:
- `tests/database/test_developer_experience.py`
- `tests/gql/test_validation_extended.py`

Each fixture clears:
- `SchemaRegistry.get_instance()`
- `_graphql_type_cache`

## Verification ✅
- Tests now pass individually: ✅
- Tests now pass when run together: ✅
- Tests pass when run with other registry-using tests: ✅

## Expected Outcome ✅ ACHIEVED
All tests should pass consistently regardless of execution order or whether run individually vs in suite.
