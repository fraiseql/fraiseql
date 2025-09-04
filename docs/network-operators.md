# Network Operators Documentation

## Overview

FraiseQL provides comprehensive network operators for IP address classification and filtering. This document outlines the implemented operators and explains the design decisions behind operator selection.

## Implemented Operators

### Core Operations (v0.6.0+)
- **Basic**: `eq`, `neq`, `in`, `notin`, `nin`
- **Subnet**: `inSubnet`, `inRange`
- **Classification**: `isPrivate`, `isPublic`, `isIPv4`, `isIPv6`

### Enhanced Operations (v0.7.3+)
- **`isLoopback`** - RFC 3330 (IPv4) / RFC 4291 (IPv6)
  - IPv4: `127.0.0.0/8`
  - IPv6: `::1/128`

- **`isLinkLocal`** - RFC 3927 (IPv4) / RFC 4291 (IPv6)
  - IPv4: `169.254.0.0/16` (APIPA)
  - IPv6: `fe80::/10`

- **`isMulticast`** - RFC 3171 (IPv4) / RFC 4291 (IPv6)
  - IPv4: `224.0.0.0/4`
  - IPv6: `ff00::/8`

- **`isDocumentation`** - RFC 5737 (IPv4) / RFC 3849 (IPv6)
  - IPv4: `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`
  - IPv6: `2001:db8::/32`

- **`isCarrierGrade`** - RFC 6598
  - IPv4: `100.64.0.0/10` (Carrier-Grade NAT)
  - IPv6: No equivalent

## Design Decisions: Excluded Operators

The following operators were intentionally **excluded** during development for the reasons listed:

### ❌ `isBroadcast`
**Problem**: Ambiguous definition
- IPv4: Only `255.255.255.255` or include subnet broadcasts?
- IPv6: No broadcast concept exists
- **Decision**: Too ambiguous to implement reliably

### ❌ `isSiteLocal`
**Problem**: Deprecated functionality
- IPv6: `fec0::/10` deprecated per RFC 3879
- **Decision**: Should not implement deprecated standards

### ❌ `isUniqueLocal`
**Problem**: Limited applicability
- IPv6: `fc00::/7`
- IPv4: No equivalent
- **Decision**: Very IPv6-specific, limited use case

### ❌ `isReserved`
**Problem**: Too vague
- Multiple ranges with unclear definition
- What constitutes "reserved" varies by context
- **Decision**: Too broad and ambiguous

### ❌ `isGlobalUnicast`
**Problem**: Complex negative definition
- Defined as "not private, not multicast, not special-use"
- Complex logic prone to errors
- **Decision**: Too complex for reliable implementation

### ❌ `isnull`
**Note**: This is handled by `NullOperatorStrategy`, not `NetworkOperatorStrategy`

## Usage Examples

### Basic IP Classification
```graphql
query {
  devices(where: {
    ipAddress: { isPrivate: true }
  }) {
    name
    ipAddress
  }
}
```

### Advanced Network Analysis
```graphql
query {
  servers(where: {
    ipAddress: { isLoopback: false, isDocumentation: false }
  }) {
    name
    ipAddress
  }
}
```

### Subnet Filtering
```graphql
query {
  devices(where: {
    ipAddress: { inSubnet: "192.168.1.0/24" }
  }) {
    name
    ipAddress
  }
}
```

## Implementation Notes

### IPv4/IPv6 Support
All operators support both IPv4 and IPv6 addresses where applicable. IPv4-specific operators (like `isCarrierGrade`) gracefully handle IPv6 addresses by returning appropriate results.

### Boolean Logic
All classification operators accept boolean values:
- `true`: IP address matches the classification
- `false`: IP address does NOT match the classification

### PostgreSQL Integration
All operators use PostgreSQL's native `inet` type with subnet containment (`<<=`) and equality operators for optimal performance and correctness.

## Testing
Comprehensive test coverage ensures:
- Correct SQL generation for all operators
- Proper IPv4/IPv6 handling
- Backward compatibility
- Error handling for invalid input

See:
- `tests/unit/sql/test_enhanced_network_operators.py`
- `tests/unit/sql/test_network_operator_strategy_fix.py`
