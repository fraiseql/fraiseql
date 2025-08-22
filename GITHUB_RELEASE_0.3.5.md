# 🔐 FraiseQL v0.3.5 - Critical Security Fix

**This is a security patch release that fixes a GraphQL introspection vulnerability.**

## 🚨 Security Advisory

**Vulnerability**: GraphQL schema introspection was exposed in production environments
**Severity**: Medium (CVSS 5.3)
**Impact**: Information disclosure - attackers could discover API structure
**Fix**: Introspection now properly blocked in production mode

**All users should upgrade immediately.**

---

## 🔧 What's Fixed

### Security Issue
- **Production environments** were inadvertently exposing GraphQL schema information
- **Introspection queries** (`__schema`, `__type`) revealed API structure and field names
- **Attack vector**: Unauthenticated introspection allowed reconnaissance

### Security Solution
- ✅ **Automatic Protection**: Introspection disabled by default in production
- ✅ **Robust Implementation**: Uses GraphQL's `NoSchemaIntrospectionCustomRule`
- ✅ **Zero Breaking Changes**: Development workflows unchanged
- ✅ **Clear Error Messages**: Non-revealing but informative responses

## 📦 Installation

```bash
pip install --upgrade fraiseql==0.3.5
```

## 🧪 Verification

Test that introspection is properly blocked in production:

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

## 🔄 Migration

### ✅ Zero-Action Upgrade
**No configuration changes required** - the security fix is automatic:

- **Production** (`environment="production"`): Introspection automatically blocked
- **Development** (`environment="development"`): Introspection works normally

### Configuration Options

**Default Behavior** (recommended):
```python
config = FraiseQLConfig(
    environment="production"  # Introspection auto-disabled
)
```

**Override** (not recommended for production):
```python
config = FraiseQLConfig(
    environment="production",
    enable_introspection=True  # ⚠️ Security risk
)
```

**Environment Variable**:
```bash
export FRAISEQL_ENABLE_INTROSPECTION=false
```

## 📋 Technical Details

### Modified Files
- `src/fraiseql/graphql/execute.py` - Added introspection validation
- `src/fraiseql/fastapi/routers.py` - Configuration integration
- `src/fraiseql/execution/unified_executor.py` - Config propagation

### Test Coverage
- **9 comprehensive test cases** in `tests/security/test_schema_introspection_security.py`
- **Introspection blocking** verification
- **Error message validation**
- **Configuration testing**
- **Backward compatibility** confirmation

### Performance Impact
- **Zero overhead** - validation occurs early in request lifecycle
- **No latency increase** - blocked queries fail faster
- **No memory impact** - uses GraphQL's built-in validation

## 🔍 Affected Versions

**All versions < 0.3.5** are vulnerable:
- 0.3.4 and earlier
- All 0.2.x versions
- All 0.1.x versions
- All beta versions

## 📚 Resources

- **Security Advisory**: [`SECURITY_ADVISORY_0.3.5.md`](./SECURITY_ADVISORY_0.3.5.md)
- **Release Notes**: [`RELEASE_NOTES_0.3.5.md`](./RELEASE_NOTES_0.3.5.md)
- **Changelog**: [`CHANGELOG.md`](./CHANGELOG.md)
- **Documentation**: https://fraiseql.readthedocs.io/en/latest/advanced/security/

## 🐛 Issues Fixed

- **Security**: Introspection exposed in production environments
- **Vulnerability**: Information disclosure through schema queries
- **Configuration**: `enable_introspection` setting not properly enforced

---

## 📁 Assets

- **Source Code**: `fraiseql-0.3.5.tar.gz`
- **Wheel Package**: `fraiseql-0.3.5-py3-none-any.whl`

**SHA256 Checksums** (will be generated during release):
```
# Will be populated during actual release
```

---

**🛡️ Upgrade immediately to secure your production APIs!**

For questions or issues with this release:
- **Bug Reports**: [Create an Issue](https://github.com/fraiseql/fraiseql/issues/new)
- **Security**: Follow responsible disclosure guidelines
- **Support**: Check documentation and existing issues first
