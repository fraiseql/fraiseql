# FraiseQL v0.1.0b30 Release Notes

## 🚀 Network Scalars Complete!

This release completes the network scalar type system by fixing the EmailAddress scalar in input types and adding comprehensive test coverage for all network scalars.

## 🐛 Bug Fixes

### EmailAddress Scalar in Input Types
- **Issue**: EmailAddress scalar type could not be used in GraphQL input types, throwing a `TypeError`
- **Root Cause**: Missing mapping from `EmailAddressField` to `EmailAddressScalar` in the type conversion system
- **Solution**: Added the missing import and mapping in `graphql_utils.py`
- **Impact**: EmailAddress can now be used in mutations and input types as intended

## ✨ Enhancements

### Comprehensive Network Scalar Testing
Added full integration test coverage for all network scalars:
- ✅ EmailAddress in input types
- ✅ IpAddress in mutation returns (verified working)
- ✅ All network scalars (EmailAddress, IpAddress, MacAddress, Port, Hostname) in input types
- ✅ All network scalars in output/query types
- ✅ Optional/nullable network scalar fields
- ✅ Schema registry cleanup to prevent type conflicts

### IpAddress Serialization Verification
- Confirmed IpAddress scalar properly serializes in mutation returns
- No code changes needed - already working correctly
- Added explicit tests to prevent future regression

## 📦 What's Included

### Changed Files
- `src/fraiseql/types/scalars/graphql_utils.py` - Added EmailAddressField mapping
- `tests/test_network_scalars_graphql_integration.py` - New comprehensive test suite

### Network Scalars Status
All network scalars now fully functional in both input and output types:
- ✅ **EmailAddress** - Email validation
- ✅ **IpAddress** - IPv4/IPv6 address validation
- ✅ **MacAddress** - MAC address validation
- ✅ **Port** - Network port validation (1-65535)
- ✅ **Hostname** - DNS hostname validation
- ✅ **CIDR** - CIDR notation validation

## 🔧 Migration Guide

If you were working around the EmailAddress input type issue:

```python
# Before (workaround)
@fraiseql.input
class CreateUserInput:
    email: str  # Had to use str instead of EmailAddress

# After (fixed)
@fraiseql.input
class CreateUserInput:
    email: EmailAddress  # Now works correctly!
```

## 🎯 Next Steps

With all network scalars fully functional, FraiseQL's type system is now complete for network infrastructure applications. The beta phase continues to focus on stability and production readiness.

## 📝 Full Changelog

See [CHANGELOG.md](CHANGELOG.md) for the complete history of changes.

---

**Thank you to the FraiseQL community for reporting these issues and helping make the framework better!**
