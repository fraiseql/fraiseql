# UNSET Serialization Fix - Complete Solution

**Date:** 2025-07-11  
**FraiseQL Version:** 0.1.0b10+ (fix applied)  
**Issue:** Object of type Unset is not JSON serializable in GraphQL error extensions

## Summary

The UNSET serialization issue was occurring because GraphQL error extensions were not being cleaned before JSON serialization. While FraiseQL has a custom JSON response class (`FraiseQLJSONResponse`) that handles UNSET values, the error extensions were being passed through directly from GraphQL-core without cleaning.

## Changes Made

### 1. **Updated `routers.py`** - Clean UNSET values from error extensions

- Imported `clean_unset_values` from `json_encoder.py`
- Applied cleaning to `error.extensions` in all error response paths:
  - Development router: GraphQL execution errors
  - Development router: N+1 detection errors  
  - Development router: General exception errors
  - Production router: All error paths (though less relevant as extensions are minimal)

### 2. **Updated `mutation_decorator.py`** - Handle UNSET in input conversion

- Added import for `UNSET` from `types.definitions`
- Modified `_to_dict` function to convert UNSET values to None when converting input objects to dictionaries
- This prevents UNSET values from propagating into error contexts

### 3. **Added comprehensive test coverage**

- Created `test_unset_error_extensions.py` with tests for:
  - GraphQL errors with UNSET in extensions
  - Mutation errors with UNSET in input data
  - Production mode error handling

## How It Works

1. When a GraphQL error occurs and includes extensions (e.g., with input data containing UNSET), the error extensions are cleaned using `clean_unset_values()` before being included in the response.

2. When mutations convert input objects to dictionaries for database calls, UNSET values are converted to None to prevent them from causing issues downstream.

3. The `FraiseQLJSONResponse` class continues to handle any remaining UNSET values in the final response serialization as a safety net.

## Impact

- Fixes the "Object of type Unset is not JSON serializable" error in PrintOptim Backend
- No breaking changes - UNSET values are transparently converted to null in JSON
- Works with both development and production modes
- No changes required in projects using FraiseQL

## Testing

The fix has been verified to:
- Convert UNSET to null in error extensions
- Handle nested structures with UNSET values
- Work with both direct GraphQL errors and mutation errors
- Maintain backward compatibility

## Migration

No migration needed. Projects using `create_fraiseql_app()` will automatically benefit from this fix once they update to the version containing these changes.