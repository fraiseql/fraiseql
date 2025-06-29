# FraiseQL Context Mocking Issue

## Problem
When trying to test GraphQL queries with FraiseQL context parameters, the following error occurs:

```
AttributeError: module 'printoptim_backend.entrypoints.api' has no attribute 'resolvers'
```

## Context
Tests are trying to mock the FraiseQL context using:
```python
with patch("printoptim_backend.entrypoints.api.resolvers.get_fraiseql_context") as mock_context:
    mock_context.return_value = test_context
```

## Expected Behavior
There should be a clear way to mock or inject FraiseQL context for testing purposes.

## Current Test Code
Located in: `tests/entrypoints/api_fraiseql/test_context_parameters.py`

## Questions for FraiseQL Team
1. What is the correct module path for mocking FraiseQL context?
2. Is there a recommended pattern for testing queries with custom context?
3. Should context be injected differently in tests vs production?

## Impact
All FraiseQL context parameter tests are failing (5 tests)