# FraiseQL v0.3.5 Release Notes 🔐

**Release Date**: August 17, 2025
**Type**: Security Patch Release
**Upgrade Priority**: **HIGH** (Security Fix)

## 🚨 Critical Security Fix

This release addresses a **GraphQL introspection vulnerability** that affected all production deployments of FraiseQL < 0.3.5.

### The Issue
Production environments were inadvertently exposing complete GraphQL schema information via introspection queries, allowing attackers to:
- Discover API structure and field names
- Map available queries, mutations, and subscriptions
- Understand business logic through schema analysis
- Identify potential attack vectors

### The Fix
- ✅ **Introspection now properly blocked in production**
- ✅ **Uses GraphQL's built-in security validation**
- ✅ **Zero breaking changes - automatic protection**
- ✅ **Development workflows unchanged**

## 🔧 What's Changed

### Security Enhancements
- **Production Protection**: Introspection automatically disabled when `environment="production"`
- **Robust Validation**: Uses `NoSchemaIntrospectionCustomRule` for comprehensive blocking
- **Security Logging**: Introspection attempts logged for monitoring
- **Clear Error Messages**: Non-revealing but informative error responses

### New Test Suite
- **9 comprehensive test cases** covering all security scenarios
- **Introspection blocking verification**
- **Error message validation**
- **Configuration override testing**
- **Backward compatibility confirmation**

## 📋 Migration Guide

### ✅ Zero-Action Upgrade
**Most users need to do nothing** - the security fix is automatic:

```bash
pip install --upgrade fraiseql==0.3.5
```

### Before/After Behavior

**Development Mode** (`environment="development"`):
```bash
# ✅ BEFORE: Introspection works
# ✅ AFTER:  Introspection works (unchanged)
```

**Production Mode** (`environment="production"`):
```bash
# ❌ BEFORE: Introspection leaked schema (vulnerability)
# ✅ AFTER:  Introspection properly blocked (secured)
```

### Configuration Options

**Automatic (Recommended)**:
```python
config = FraiseQLConfig(
    environment="production"  # Introspection auto-disabled
)
```

**Manual Override** (not recommended):
```python
config = FraiseQLConfig(
    environment="production",
    enable_introspection=True  # Force enable (security risk)
)
```

**Environment Variable**:
```bash
export FRAISEQL_ENABLE_INTROSPECTION=false
```

## 🧪 Verification

### Test the Fix
Verify introspection is blocked in your production environment:

```bash
curl -X POST https://your-api.com/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __schema { queryType { name } } }"}'
```

**Expected Response** (production):
```json
{
  "errors": [
    {
      "message": "GraphQL introspection has been disabled, but the requested query contained the field '__schema'."
    }
  ]
}
```

**Expected Response** (development):
```json
{
  "data": {
    "__schema": {
      "queryType": {
        "name": "Query"
      }
    }
  }
}
```

## 🔍 Impact Assessment

### Who Should Upgrade Immediately
- **All production deployments** of FraiseQL
- **Public-facing APIs** using FraiseQL
- **APIs with sensitive business logic** exposed through schema

### Risk Level by Environment
- **Production**: **HIGH** - Schema information exposed
- **Staging**: **MEDIUM** - Could reveal development patterns
- **Development**: **LOW** - Introspection expected and useful

## 📊 Technical Details

### Files Modified
- `src/fraiseql/graphql/execute.py` - Added introspection validation
- `src/fraiseql/fastapi/routers.py` - Pass introspection config
- `src/fraiseql/execution/unified_executor.py` - Config propagation

### Performance Impact
- **Zero performance overhead** - validation occurs early in request lifecycle
- **No memory increase** - uses GraphQL's built-in validation
- **No latency impact** - blocked queries fail faster

### Compatibility
- ✅ **Backward Compatible** - no API changes
- ✅ **Development Unchanged** - full introspection in dev mode
- ✅ **Existing Tests Pass** - no breaking changes

## 🚀 Deployment

### Rolling Deployment Safe
This fix can be deployed with **zero downtime**:
- No database migrations required
- No configuration changes required
- No API contract changes
- Immediate security protection upon deployment

### Rollback Plan
If rollback is needed (unlikely):
```bash
pip install fraiseql==0.3.4
```

## 📈 Monitoring

### Security Monitoring
Monitor your logs for introspection attempts:
```bash
grep -i "__schema\|__type" /var/log/app.log
```

### Recommended Alerts
Add monitoring for these log messages:
- `"Introspection disabled - validating query"`
- `"Introspection query blocked"`

## 🔮 Future Enhancements

Coming in future releases:
- **Rate limiting** for introspection attempts
- **Custom introspection policies** per endpoint
- **Advanced security metrics** and reporting

## 📞 Support

### Questions?
- **Documentation**: https://fraiseql.readthedocs.io/en/latest/advanced/security/
- **Issues**: https://github.com/fraiseql/fraiseql/issues
- **Security**: See `SECURITY_ADVISORY_0.3.5.md`

### Need Help Upgrading?
The upgrade should be seamless, but if you encounter issues:
1. Check that `environment="production"` in your config
2. Verify introspection behavior with test queries
3. Review logs for any unexpected errors
4. Create an issue with reproduction steps

---

**🛡️ Your production APIs are now properly secured against introspection attacks!**

*Thank you for using FraiseQL. Stay secure! 🔐*
