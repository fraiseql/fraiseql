# FraiseQL v0.1.0b29 Release Notes

## 🐛 Critical Bug Fix Release

This hotfix release addresses a critical issue with the IpAddress scalar type introduced in v0.1.0b28.

## 🔧 What's Fixed

### IpAddress Scalar in Input Types
The `IpAddress` scalar type could not be used in GraphQL input types, causing a `TypeError` during schema generation.

**Before (v0.1.0b28):**
```python
@fraiseql.input
class CreateSmtpServerInput:
    ip_address: IpAddress  # ❌ TypeError: Invalid type passed to convert_type_to_graphql_input
```

**After (v0.1.0b29):**
```python
@fraiseql.input
class CreateSmtpServerInput:
    ip_address: IpAddress  # ✅ Works correctly!
```

## 📋 Technical Details

The fix involved adding the missing scalar type mapping:
- Added `IpAddressField` import to `graphql_utils.py`
- Added `IpAddressField: IpAddressScalar` mapping to the scalar conversion system

## ✅ What's New

- **Comprehensive IpAddress tests**: Added full test coverage including input type integration tests
- **All network scalars verified**: Port, IpAddress, CIDR, Hostname, and MacAddress all work in input types

## 📦 Installation

```bash
pip install fraiseql==0.1.0b29
```

## 🙏 Thank You

Thanks to the community member who reported this issue with detailed reproduction steps in `FRAISEQL_IPADDRESS_SCALAR_BUG.md`.

---

*Released: 2025-07-21*  
*Version: 0.1.0b29*