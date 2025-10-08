# JSON Passthrough Production Mode Fix

## Issue Summary
FraiseQL v0.3.0 had a critical bug where it would force JSON passthrough mode in production environments, completely ignoring the `json_passthrough_in_production` configuration setting. This caused APIs to return snake_case field names instead of camelCase, breaking frontend compatibility.

## Root Cause
The bug was in `src/fraiseql/fastapi/routers.py` (lines 176 and 181) where the code unconditionally enabled passthrough for production/staging environments:

```python
# BUGGY CODE (before fix)
if mode in ("production", "staging"):
    json_passthrough = True  # Always enabled, ignoring config!

# ... and later ...
if is_production_env:
    json_passthrough = True  # Always enabled, ignoring config!
```

## The Fix
The fix ensures that both `json_passthrough_enabled` AND `json_passthrough_in_production` configuration settings are checked before enabling passthrough in production environments:

```python
# FIXED CODE
if mode in ("production", "staging"):
    # Respect json_passthrough configuration settings
    if config.json_passthrough_enabled and getattr(config, 'json_passthrough_in_production', True):
        json_passthrough = True

# ... and later ...
if is_production_env:
    # Respect json_passthrough configuration settings
    if config.json_passthrough_enabled and getattr(config, 'json_passthrough_in_production', True):
        json_passthrough = True
```

## Configuration Logic
The passthrough mode is now properly controlled by two configuration flags:

1. **`json_passthrough_enabled`**: General flag to enable/disable passthrough optimization
2. **`json_passthrough_in_production`**: Specific flag for production/staging environments

### Passthrough Behavior Matrix

| Environment | json_passthrough_enabled | json_passthrough_in_production | Result |
|------------|-------------------------|-------------------------------|---------|
| production | False | False | ❌ Disabled |
| production | False | True | ❌ Disabled |
| production | True | False | ❌ Disabled |
| production | True | True | ✅ Enabled |
| development | True/False | Any | ❌ Disabled |
| testing | True/False | Any | ❌ Disabled |

## Impact
This fix ensures that:

- APIs can properly disable JSON passthrough in production when needed
- Field name conversion (snake_case → camelCase) works correctly when passthrough is disabled
- Frontend applications receive the expected field format
- Configuration settings are properly respected

## Testing
Comprehensive tests have been added in:

- `tests/fastapi/test_router_passthrough_final.py` - Core logic verification
- `tests/fastapi/test_json_passthrough_production_fix.py` - Integration tests
- `tests/fastapi/test_passthrough_fix_verification.py` - Full configuration matrix testing

## Usage Example

To disable passthrough in production (ensuring proper field name conversion):

```python
from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app

config = FraiseQLConfig(
    database_url="postgresql://...",
    environment="production",
    json_passthrough_enabled=True,  # Can be enabled in general
    json_passthrough_in_production=False,  # But disabled for production
    auto_camel_case=True,  # Enable camelCase conversion
)

app = create_fraiseql_app(schema=schema, config=config)
```

## Migration Guide

If you were affected by this bug:

1. **Update FraiseQL** to version > 0.3.0 with this fix
2. **Set configuration explicitly**:
   ```python
   json_passthrough_in_production=False  # If you need field conversion
   # or
   json_passthrough_in_production=True   # If you want passthrough optimization
   ```
3. **Test your API** to ensure field names are in the expected format
4. **Use explicit headers** for fine-grained control:

   - `x-json-passthrough: true/false` - Override passthrough setting per request
   - `x-mode: production/staging/development` - Override environment mode per request

## Version History

- **v0.3.0**: Bug introduced - production forces passthrough
- **v0.3.1**: Bug fixed - configuration properly respected
