# FraiseQL v0.5.6 Release Notes

**Release Date**: September 1, 2025
**Type**: PATCH Release (Bug Fix + Enhancement)
**Priority**: HIGH - Critical network filtering bug fix

## 🔧 Critical Bug Fix

### Network Operator Support Enhancement

This release resolves a significant issue that was blocking users from performing basic network operations in GraphQL queries.

**Problem Resolved**:
- Fixed "Unsupported network operator: eq" error for IP address filtering
- Users could not perform equality checks on IP addresses in GraphQL queries

**Solution**:
- Added basic comparison operators (`eq`, `neq`, `in`, `notin`) to NetworkOperatorStrategy
- Proper PostgreSQL `::inet` type casting in generated SQL
- Maintains full backward compatibility with existing network operators

### Impact

#### Before v0.5.6 ❌
```graphql
# This failed with "Unsupported network operator: eq"
query GetDNSServers {
  dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
    id
    identifier
    ipAddress
  }
}
```

#### After v0.5.6 ✅
```graphql
# This now works perfectly
query GetDNSServers {
  dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
    id
    identifier
    ipAddress
  }
}

# All these operators now work:
query GetDNSServersAdvanced {
  # Not equal to specific IP
  excludeLocal: dnsServers(where: { ipAddress: { neq: "192.168.1.1" } }) {
    id identifier ipAddress
  }

  # Match multiple IPs
  publicDNS: dnsServers(where: { ipAddress: { in: ["8.8.8.8", "1.1.1.1"] } }) {
    id identifier ipAddress
  }

  # Exclude multiple IPs
  nonPrivate: dnsServers(where: { ipAddress: { notin: ["192.168.1.1", "10.0.0.1"] } }) {
    id identifier ipAddress
  }
}
```

### SQL Generation Quality

The fix ensures proper PostgreSQL type casting:

```sql
-- Before v0.5.6: ERROR - Unsupported network operator: eq

-- After v0.5.6: Works perfectly
(data->>'ip_address')::inet = '8.8.8.8'::inet
(data->>'ip_address')::inet != '192.168.1.1'::inet
(data->>'ip_address')::inet IN ('8.8.8.8'::inet, '1.1.1.1'::inet)
(data->>'ip_address')::inet NOT IN ('192.168.1.1'::inet)
```

## 🧪 Quality Assurance

### Comprehensive Testing
- **19 NetworkOperatorStrategy tests** covering all operators
- **10 production fix validation tests** ensuring real-world scenarios work
- **Edge cases covered**: IPv6 addresses, empty lists, error handling
- **Backward compatibility verified**: All existing network operators continue working
- **SQL generation quality**: Proper `::inet` casting validation

### Test Coverage Examples
```python
# IPv4 equality
test_eq_operator_sql_generation()

# IPv6 support
test_ipv6_addresses()

# List operations
test_in_operator_sql_generation()
test_notin_operator_sql_generation()

# Edge cases
test_empty_list_for_in_operator()
test_single_item_list_for_in_operator()

# Backward compatibility
test_all_original_operators_still_supported()
test_network_operators_still_work()  # inSubnet, isPrivate, etc.
```

## 🛠️ Technical Details

### Architecture Consistency
- Follows the same pattern established by other operator strategies
- No breaking changes to existing APIs
- No new dependencies added
- Zero performance impact on existing queries

### Files Modified
- `src/fraiseql/sql/operator_strategies.py` - Added eq, neq, in, notin operators
- `tests/unit/sql/test_network_operator_strategy_fix.py` - 19 comprehensive tests
- `tests/core/test_production_fix_validation.py` - Production scenario validation

### Security
- No security concerns introduced
- Proper input validation maintained
- SQL injection protections preserved

## 📈 Upgrade Instructions

### Quick Upgrade
```bash
pip install --upgrade fraiseql==0.5.6
```

### Compatibility
- **Backward Compatible**: All existing queries continue to work exactly as before
- **No Configuration Changes**: No changes needed to existing code
- **Enhanced Functionality**: New operators are available immediately after upgrade

### Migration Guide
No migration required! This is a pure enhancement that adds missing functionality without changing existing behavior.

#### What You Can Do After Upgrading
```python
# New capabilities available immediately:

# IP equality filtering (previously failed)
@fraiseql.query
async def servers_by_ip(info, ip: str) -> list[Server]:
    repo = info.context["repo"]
    return await repo.find("v_server", where={"ip_address": {"eq": ip}})

# IP exclusion filtering
@fraiseql.query
async def servers_excluding_ips(info, excluded_ips: list[str]) -> list[Server]:
    repo = info.context["repo"]
    return await repo.find("v_server", where={"ip_address": {"notin": excluded_ips}})
```

## 🎯 Production Impact

### Users Affected
This fix resolves issues for users who:
- Need to filter network equipment by IP addresses
- Perform DNS server management operations
- Query network infrastructure by specific IPs
- Use IP-based filtering in GraphQL queries

### Performance
- **Zero Performance Impact**: Existing queries perform identically
- **Enhanced Queries**: New IP filtering queries perform optimally with proper PostgreSQL indexing
- **SQL Optimization**: Generated SQL uses efficient `::inet` operations

## 🔍 Verification Steps

After upgrading to v0.5.6, you can verify the fix works:

1. **Test IP Equality**:
```graphql
query TestIPEquality {
  dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
    id
    identifier
    ipAddress
  }
}
```

2. **Test IP List Operations**:
```graphql
query TestIPLists {
  publicDNS: dnsServers(where: {
    ipAddress: { in: ["8.8.8.8", "1.1.1.1", "208.67.222.222"] }
  }) {
    id identifier ipAddress
  }
}
```

3. **Verify Existing Operators Still Work**:
```graphql
query TestExistingOperators {
  privateIPs: dnsServers(where: { ipAddress: { isPrivate: true } }) {
    id identifier ipAddress
  }

  subnetIPs: dnsServers(where: {
    ipAddress: { inSubnet: "192.168.1.0/24" }
  }) {
    id identifier ipAddress
  }
}
```

## 🚀 What's Next

This patch release focuses specifically on resolving the network filtering issue. Future releases will continue to enhance:
- Additional operator strategies for specialized PostgreSQL types
- Performance optimizations for complex queries
- Enhanced developer experience features

## 📞 Support & Feedback

- **GitHub Issues**: [fraiseql/fraiseql/issues](https://github.com/fraiseql/fraiseql/issues)
- **Documentation**: [FraiseQL Docs](https://github.com/fraiseql/fraiseql)
- **Community**: Join our discussions on GitHub

---

**Upgrade today** to resolve network filtering issues and unlock IP-based query capabilities in your GraphQL API!

**Full Changelog**: [v0.5.5...v0.5.6](https://github.com/fraiseql/fraiseql/compare/v0.5.5...v0.5.6)
