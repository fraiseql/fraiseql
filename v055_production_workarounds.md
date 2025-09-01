# FraiseQL v0.5.5 Network Filtering - Production Workarounds

## 🎯 Issue Summary

FraiseQL v0.5.5 has **partially fixed** network filtering:
- ✅ **Working**: `inSubnet`, `isPrivate`, `isPublic` operators
- ❌ **Broken**: `eq`, `ne` operators for IP addresses
- 🔧 **Root Cause**: NetworkOperatorStrategy missing basic comparison operators

## 🛠️ Production Workarounds

### 1. IP Equality Filtering Workaround

**❌ Broken Query:**
```graphql
query GetSpecificDNS {
  dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
    id
    identifier
    ipAddress
  }
}
```

**✅ Working Workaround - Use identifier field:**
```graphql
query GetSpecificDNS {
  dnsServers(where: { identifier: { eq: "Primary DNS Google" } }) {
    id
    identifier
    ipAddress
  }
}
```

**✅ Working Workaround - Use subnet with /32:**
```graphql
query GetSpecificDNS {
  dnsServers(where: { ipAddress: { inSubnet: "8.8.8.8/32" } }) {
    id
    identifier
    ipAddress
  }
}
```

### 2. IP Range Filtering Workarounds

**❌ Broken Query:**
```graphql
query GetPrivateIPs {
  dnsServers(where: { ipAddress: { eq: "192.168.1.1" } }) {
    id
    identifier
    ipAddress
  }
}
```

**✅ Working Workaround - Use private IP classification:**
```graphql
query GetPrivateIPs {
  dnsServers(where: { ipAddress: { isPrivate: true } }) {
    id
    identifier
    ipAddress
  }
}
```

**✅ Working Workaround - Use subnet filtering:**
```graphql
query GetSpecificSubnet {
  dnsServers(where: { ipAddress: { inSubnet: "192.168.0.0/16" } }) {
    id
    identifier
    ipAddress
  }
}
```

### 3. Multiple IP Filtering Workaround

**❌ Broken Query:**
```graphql
query GetMultipleIPs {
  dnsServers(where: {
    ipAddress: { in: ["8.8.8.8", "1.1.1.1", "9.9.9.9"] }
  }) {
    id
    identifier
    ipAddress
  }
}
```

**✅ Working Workaround - Use multiple subnet queries:**
```graphql
query GetMultipleIPs {
  googleDNS: dnsServers(where: { ipAddress: { inSubnet: "8.8.8.8/32" } }) {
    id
    identifier
    ipAddress
  }
  cloudflareDNS: dnsServers(where: { ipAddress: { inSubnet: "1.1.1.1/32" } }) {
    id
    identifier
    ipAddress
  }
  quadDNS: dnsServers(where: { ipAddress: { inSubnet: "9.9.9.9/32" } }) {
    id
    identifier
    ipAddress
  }
}
```

**✅ Working Workaround - Use identifier-based filtering:**
```graphql
query GetMultipleIPs {
  dnsServers(where: {
    identifier: {
      in: ["Primary DNS Google", "Cloudflare DNS Primary", "Quad9 DNS"]
    }
  }) {
    id
    identifier
    ipAddress
  }
}
```

## 📋 Client-Side Workarounds

### JavaScript/TypeScript Helper Functions

```javascript
// Helper function to convert IP equality to subnet filtering
function ipToSubnetFilter(ip) {
  return `${ip}/32`;
}

// Helper function to handle multiple IP filtering
function multipleIpsToSubnets(ips) {
  return ips.map(ip => ({ ipAddress: { inSubnet: `${ip}/32` } }));
}

// Usage examples
const singleIP = "8.8.8.8";
const multipleIPs = ["8.8.8.8", "1.1.1.1"];

// Single IP workaround
const singleIPFilter = {
  where: { ipAddress: { inSubnet: ipToSubnetFilter(singleIP) } }
};

// Multiple IPs workaround
const multipleIPFilters = multipleIpsToSubnets(multipleIPs).map(filter => ({
  dnsServers: { where: filter }
}));
```

### Python Helper Functions

```python
def ip_to_subnet_filter(ip: str) -> dict:
    """Convert IP equality to subnet filtering."""
    return {"ipAddress": {"inSubnet": f"{ip}/32"}}

def multiple_ips_query(ips: list[str]) -> str:
    """Generate GraphQL query for multiple IPs using subnet filtering."""
    queries = []
    for i, ip in enumerate(ips):
        alias = f"ip_{ip.replace('.', '_')}"
        queries.append(f'{alias}: dnsServers(where: {{ ipAddress: {{ inSubnet: "{ip}/32" }} }}) {{ id identifier ipAddress }}')

    return f"query GetMultipleIPs {{ {' '.join(queries)} }}"

# Usage
single_ip_filter = ip_to_subnet_filter("8.8.8.8")
multiple_ips_query_str = multiple_ips_query(["8.8.8.8", "1.1.1.1"])
```

## ⚠️ Limitations of Workarounds

### 1. Performance Impact
- **Multiple queries**: Using aliases for multiple IPs increases query complexity
- **Subnet filtering**: May be slower than direct equality for large datasets
- **Client-side filtering**: May require fetching more data than needed

### 2. Precision Issues
- **Subnet /32**: Functionally equivalent to IP equality but may have edge cases
- **Private IP classification**: Returns ALL private IPs, not specific addresses
- **Missing negation**: No workaround for `ne` (not equal) operations

### 3. Code Complexity
- **Query duplication**: Multiple similar queries instead of single parameterized query
- **Client logic**: Business logic moved from GraphQL to client code
- **Maintenance burden**: Workarounds need to be removed when FraiseQL is fixed

## 🚀 Recommended Migration Plan

### Phase 1: Immediate Workarounds (Current)
```javascript
// Use subnet filtering for specific IPs
const query = `
  query GetDNSServer {
    dnsServers(where: { ipAddress: { inSubnet: "8.8.8.8/32" } }) {
      id identifier ipAddress
    }
  }
`;
```

### Phase 2: Monitor FraiseQL Updates
- Track FraiseQL v0.5.6+ releases
- Test IP equality operators in new versions
- Prepare migration scripts to remove workarounds

### Phase 3: Clean Migration (Future)
```javascript
// After FraiseQL fix - clean syntax
const query = `
  query GetDNSServer {
    dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
      id identifier ipAddress
    }
  }
`;
```

## 📊 Testing Your Workarounds

### Verification Queries

```graphql
# Test 1: Verify subnet workaround works
query TestSubnetWorkaround {
  original: dnsServers(where: { ipAddress: { inSubnet: "8.8.8.8/32" } }) {
    count: _count
  }

  # This should match when FraiseQL is fixed
  # target: dnsServers(where: { ipAddress: { eq: "8.8.8.8" } }) {
  #   count: _count
  # }
}

# Test 2: Verify private IP classification works
query TestPrivateIPWorkaround {
  privateIPs: dnsServers(where: { ipAddress: { isPrivate: true } }) {
    identifier
    ipAddress
  }
}

# Test 3: Verify identifier-based workaround
query TestIdentifierWorkaround {
  byIdentifier: dnsServers(where: { identifier: { eq: "Primary DNS Google" } }) {
    identifier
    ipAddress
  }
}
```

## 🎯 Summary

- **Use subnet filtering** (`inSubnet: "IP/32"`) instead of IP equality
- **Use classification filtering** (`isPrivate: true`) for IP ranges
- **Use identifier filtering** when available for exact matches
- **Monitor FraiseQL updates** for v0.5.6+ with complete network filtering
- **Plan for migration** to remove workarounds when fixed

These workarounds provide full functionality while maintaining production stability until FraiseQL v0.5.6+ resolves the underlying IP equality issues.
