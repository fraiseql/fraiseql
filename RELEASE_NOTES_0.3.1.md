# FraiseQL v0.3.1 Release Notes

## Critical Bug Fix Release

This release fixes a critical bug where FraiseQL was forcing JSON passthrough mode in production environments, ignoring configuration settings and breaking API compatibility.

## 🐛 Bug Fixed

### JSON Passthrough Configuration Not Respected in Production

**Issue:** FraiseQL v0.3.0 was unconditionally enabling JSON passthrough mode when running in production or staging environments, completely ignoring the `json_passthrough_in_production` configuration setting.

**Impact:**
- APIs were returning snake_case field names instead of camelCase
- Frontend applications expecting camelCase fields were breaking
- The configuration `json_passthrough_in_production=False` had no effect

**Fix:** The router now properly checks both `json_passthrough_enabled` AND `json_passthrough_in_production` before enabling passthrough mode in production environments.

## 📦 Installation

```bash
pip install --upgrade fraiseql==0.3.1
```

## 🔧 Configuration

To disable JSON passthrough in production (ensuring proper field name conversion):

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

## 🔄 Migration from v0.3.0

If you were affected by this bug in v0.3.0:

1. **Update to v0.3.1** immediately
2. **Verify your configuration** - explicitly set `json_passthrough_in_production` based on your needs:
   - `False` if you need GraphQL field name conversion (snake_case → camelCase)
   - `True` if you want the performance optimization of JSON passthrough
3. **Test your API** to ensure field names are in the expected format

## 📊 Passthrough Behavior

The passthrough mode is now correctly controlled by two configuration flags:

| Environment | json_passthrough_enabled | json_passthrough_in_production | Passthrough Active |
|-------------|-------------------------|-------------------------------|-------------------|
| production  | False                   | Any                           | ❌ No             |
| production  | True                    | False                         | ❌ No             |
| production  | True                    | True                          | ✅ Yes            |
| development | Any                     | Any                           | ❌ No             |

## 🧪 Testing

Comprehensive tests have been added to prevent regression of this issue. The fix has been verified with:
- Unit tests for the router logic
- Integration tests for the full request flow
- Configuration matrix tests for all combinations

## 🙏 Acknowledgments

Thank you to the users who reported this critical issue affecting production deployments.

## 📚 Documentation

For more details, see:
- [JSON Passthrough Configuration Guide](https://fraiseql.dev/docs/configuration/json-passthrough)
- [Bug Fix Documentation](/docs/fixes/json-passthrough-production-fix.md)

---

**Full Changelog:** https://github.com/lhamayon/fraiseql/compare/v0.3.0...v0.3.1
