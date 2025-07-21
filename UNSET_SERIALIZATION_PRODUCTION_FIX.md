# UNSET Serialization Fix for Production Mode

**Date:** 2025-07-11
**FraiseQL Version:** 0.1.0b11+ (this fix)
**Issue:** UNSET values in GraphQL error extensions not being cleaned in production mode

## Problem

The UNSET serialization fix in v0.1.0b10 only applied to the development router. The production router and other GraphQL endpoints (GraphNoteRouter, WebSocket subscriptions) were not cleaning UNSET values from error extensions, causing "Object of type Unset is not JSON serializable" errors in production environments like PrintOptim Backend.

## Root Cause

While the development router correctly used `clean_unset_values()` on error extensions, the production router had different error handling paths that didn't apply this cleaning:
1. Production router's GraphQL execution errors
2. Production router's validation errors
3. Alternative GraphQL endpoints (GraphNoteRouter)
4. WebSocket subscription error handling

## Solution

Applied `clean_unset_values()` to all error paths:

### 1. **Production Router** (`src/fraiseql/fastapi/routers.py`)
- Added cleaning to GraphQL execution errors (line 317)
- Added cleaning to validation errors (line 281)
- Both paths now properly handle UNSET values in error extensions

### 2. **GraphNoteRouter** (`src/fraiseql/gql/graphql_entrypoint.py`)
- Added import for `clean_unset_values`
- Updated error response construction to clean extensions (lines 86-92)
- Fixed return type annotation to use `FraiseQLJSONResponse`

### 3. **WebSocket Handler** (`src/fraiseql/subscriptions/websocket.py`)
- Added import for `clean_unset_values`
- Updated `_send_error` method to clean extensions from GraphQL errors
- Handles both single errors and lists of errors

### 4. **Comprehensive Tests** (`tests/test_unset_production_error_extensions.py`)
- Added tests specifically for production mode error handling
- Tests cover GraphQL execution errors with UNSET in extensions
- Tests cover validation errors in production mode
- Tests cover both hidden and detailed error modes

## Impact

- Fixes the "Object of type Unset is not JSON serializable" error in production environments
- No breaking changes - transparent conversion of UNSET to null
- Works with all GraphQL endpoints (HTTP, WebSocket, alternative routers)
- Maintains backward compatibility

## Testing

Run the new tests to verify the fix:
```bash
pytest tests/test_unset_production_error_extensions.py -v
```

## Migration

No migration needed. Projects using `create_fraiseql_app()` will automatically benefit from this fix once they update to the version containing these changes.
