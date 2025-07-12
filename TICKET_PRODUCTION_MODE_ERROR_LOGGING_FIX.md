# Production Mode Error Logging Fix for PrintOptim Backend

**Date:** 2025-07-12  
**Issue:** PrintOptim Backend experiencing "Internal server error" in production mode with no debugging details  
**FraiseQL Version:** v0.1.0b13+  
**Status:** Fixed  

## Issue Summary

PrintOptim Backend reported that production mode in FraiseQL v0.1.0b13 fails with generic "Internal server error" messages, making it impossible to debug the actual cause. Development mode works perfectly, but production mode completely suppresses all exception details.

## Root Cause Analysis

The production router in `src/fraiseql/fastapi/routers.py` was designed for security by hiding all error details from clients. However, this made debugging impossible because:

1. **No server-side logging**: Exceptions were caught but not logged
2. **Generic error responses**: All errors returned the same "Internal server error" message
3. **UNSET serialization issues**: The most likely cause was UNSET values in JSONB data that weren't properly cleaned

## Solution Implemented

### 1. Enhanced Production Router Exception Logging

**File:** `src/fraiseql/fastapi/routers.py` (lines 338-360)

```python
except Exception as e:
    # In production, log the actual error for debugging but don't expose details to client
    error_msg = str(e)
    logger.exception("Production GraphQL execution error: %s", error_msg)
    
    # Special logging for UNSET serialization issues
    if "Unset is not JSON serializable" in error_msg:
        logger.error(
            "UNSET serialization error in production mode. "
            "This may be caused by UNSET values in JSONB data that weren't properly cleaned. "
            "Query: %s, Variables: %s", 
            request.query[:200] if request.query else "None",
            str(request.variables)[:200] if request.variables else "None"
        )
    
    return {
        "errors": [
            {
                "message": "Internal server error",
                "extensions": {"code": "INTERNAL_SERVER_ERROR"},
            },
        ],
    }
```

**Benefits:**
- ✅ Server logs contain full exception details for debugging
- ✅ Client still receives generic error message for security
- ✅ Special handling for UNSET serialization errors with query context
- ✅ Query and variables are truncated to 200 characters to prevent log spam

### 2. UNSET Cleaning in Production JSONB Extraction

**File:** `src/fraiseql/db.py` (lines 288-295, 323-330)

Added UNSET cleaning to the JSONB extraction feature introduced in v0.1.0b12:

```python
if self.mode == "production":
    # Production: Extract JSONB data if present, otherwise return raw dicts
    if rows and len(rows) > 0 and "data" in rows[0]:
        # Clean UNSET values from extracted JSONB data before returning
        from fraiseql.fastapi.json_encoder import clean_unset_values
        return [clean_unset_values(row["data"]) for row in rows]
    return rows
```

**Benefits:**
- ✅ UNSET values in JSONB data are automatically converted to None
- ✅ Prevents "Object of type Unset is not JSON serializable" errors
- ✅ Maintains performance benefits of production mode
- ✅ Works for both `find()` and `find_one()` methods

### 3. Comprehensive Test Coverage

**File:** `tests/fastapi/test_production_mode_unset_simple.py`

Added tests to verify:
- ✅ UNSET cleaning works correctly in nested data structures
- ✅ Production mode error logging captures exception details
- ✅ UNSET serialization errors are specifically identified
- ✅ Query truncation works for large queries

## Impact on PrintOptim Backend

This fix directly addresses PrintOptim's reported issues:

1. **Debugging Capability**: Server logs will now contain actual exception details
2. **UNSET Serialization**: The most likely cause of the "Internal server error" is fixed
3. **Performance**: Production mode retains its performance benefits
4. **Security**: Client-facing errors remain generic for security

## How PrintOptim Can Use This Fix

1. **Update to Latest Version**: Upgrade to FraiseQL v0.1.0b14+ (when released)
2. **Check Server Logs**: Production errors will now be logged with full details
3. **Monitor for UNSET Issues**: Special logging will identify any remaining UNSET problems

## Testing Recommendations

PrintOptim Backend should:

1. Test with production mode enabled
2. Monitor server logs for "Production GraphQL execution error" messages
3. Look for "UNSET serialization error" specific logs if issues persist
4. Compare behavior between development and production modes

## Technical Details

- **Security Maintained**: Client responses still show generic "Internal server error"
- **Performance Preserved**: Production mode optimizations remain intact
- **Debugging Enabled**: Server-side logging provides full exception context
- **UNSET Handling**: Automatic cleaning prevents JSON serialization errors

This fix ensures PrintOptim Backend can effectively debug production issues while maintaining FraiseQL's security and performance characteristics.