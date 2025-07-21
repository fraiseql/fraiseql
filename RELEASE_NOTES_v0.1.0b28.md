# FraiseQL v0.1.0b28 Release Notes

## 🎯 Network Scalars Release

This release significantly expands FraiseQL's built-in validation capabilities with comprehensive network-related scalar types, making it ideal for infrastructure management, DevOps tools, and network automation applications.

## ✨ What's New

### 🌐 Network Scalar Types
Four powerful new scalar types for network data validation:

- **`Port`** - Network port validation (1-65535)
- **`CIDR`** - Network range validation (IPv4/IPv6)
- **`Hostname`** - RFC 1123 DNS hostname validation
- **`MacAddress`** - Hardware address with format normalization

### 🚀 Quick Usage

```python
from fraiseql.types import Port, IpAddress, CIDR, Hostname, MacAddress, EmailAddress

@fraiseql.type
class NetworkDevice:
    hostname: Hostname          # example.com
    ip_address: IpAddress      # 192.168.1.100 or 2001:db8::1
    mac_address: MacAddress    # 00:11:22:33:44:55 (multiple formats supported)
    subnet: CIDR              # 192.168.1.0/24
    ssh_port: Port           # 22 (1-65535)
    admin_email: EmailAddress # admin@company.com
```

### 🔧 Key Features

- **Multi-format support** - MAC addresses accept colon, hyphen, dot, or no separators
- **Automatic normalization** - MAC addresses normalized to uppercase colon format
- **Clear error messages** - Descriptive validation errors at GraphQL layer
- **Type safety** - Full Python type hints and IDE support
- **Comprehensive validation** - IPv4/IPv6 CIDR, RFC 1123 hostnames, proper port ranges

### 📝 Complete Scalar Suite

FraiseQL now includes validation for:

| Scalar | Purpose | Example |
|--------|---------|---------|
| `Port` | Network ports | `80`, `443`, `8080` |
| `IpAddress` | IP addresses | `192.168.1.1`, `2001:db8::1` |
| `CIDR` | Network ranges | `192.168.1.0/24`, `10.0.0.0/8` |
| `Hostname` | DNS hostnames | `api.example.com`, `server-01` |
| `MacAddress` | Hardware addresses | `00:11:22:33:44:55` |
| `EmailAddress` | Email validation | `user@domain.com` |
| `UUID` | Unique identifiers | `550e8400-e29b-41d4-a716-446655440000` |
| `DateTime` | ISO 8601 dates | `2024-01-15T10:30:00Z` |
| `JSON` | Arbitrary data | `{"key": "value"}` |

## 📖 Documentation Updates

- **New scalar reference**: Complete documentation at `docs/scalars.md`
- **Updated README**: Added network scalars section with examples
- **Usage examples**: Network device management patterns
- **Migration guidance**: How to upgrade from string-based validation

## 🔄 Migration Guide

### Before (String-based)
```python
@fraiseql.input
class DeviceInput:
    hostname: str           # No validation
    ip_address: str | None  # Could be invalid IP
    port: int | None       # Could be out of range
```

### After (Scalar-based)
```python
@fraiseql.input
class DeviceInput:
    hostname: Hostname           # Validates DNS rules
    ip_address: IpAddress | None # Validates IPv4/IPv6
    port: Port | None           # Validates 1-65535
```

## 🛠️ Technical Details

- **Production ready** - All scalars include comprehensive test coverage
- **GraphQL integration** - Proper schema descriptions and error handling  
- **Python integration** - Works with dataclasses, type hints, and IDEs
- **Performance optimized** - Validation uses efficient regex and built-in libraries

## 🎯 Use Cases

Perfect for applications requiring network data validation:

- **Infrastructure management** - Server and device configuration
- **Network automation** - Network device provisioning
- **DevOps tools** - Deployment and monitoring systems  
- **IoT applications** - Device registration and management
- **Security tools** - Network scanning and audit systems

## 📦 Installation

```bash
pip install fraiseql==0.1.0b28

# Or upgrade existing installation
pip install --upgrade fraiseql
```

## 🔗 Resources

- **[Complete Scalar Documentation](docs/scalars.md)** - Full reference and examples
- **[Getting Started Guide](docs/GETTING_STARTED.md)** - New to FraiseQL?
- **[Migration Guide](docs/MIGRATION_TO_JSONB_PATTERN.md)** - Upgrading from older versions

## 🙏 What's Next

Future scalar enhancements could include:
- **URL validation** - Full URL with protocol validation
- **IPv4/IPv6 specific** - Separate scalars for IP version specificity  
- **Private IP ranges** - RFC 1918 private network validation
- **Domain names** - Stricter domain vs hostname validation

---

## Full Changelog

### Added
- **New network scalar types**: `Port`, `CIDR`, `Hostname`, `MacAddress` with comprehensive validation
- **Enhanced scalar architecture**: All scalars follow established FraiseQL patterns with full test coverage
- **Documentation improvements**: New `docs/scalars.md` with complete scalar reference and usage examples

### Enhanced
- **Network data validation**: FraiseQL now provides built-in validation for most common network data types
- **Multi-format support**: MAC addresses accept multiple input formats with automatic normalization
- **Error handling**: Consistent, descriptive error messages across all network scalars

**Migration**: This release is fully backward compatible. Existing applications can adopt new scalars gradually.

---

*Released: 2025-07-21*
*Version: 0.1.0b28*