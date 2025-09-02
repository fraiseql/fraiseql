---
← [Type System](./type-system.md) | [Core Concepts Index](./index.md) | [Database Views →](./database-views.md)
---

# Filtering and Where Clauses

> **In this section:** Master GraphQL filtering with FraiseQL's type-safe where clauses
> **Prerequisites:** Understanding of [Type System](./type-system.md) and basic GraphQL concepts
> **Time to complete:** 25 minutes

FraiseQL automatically generates type-safe GraphQL `WhereInput` types for all your data types, providing powerful filtering capabilities that map directly to efficient PostgreSQL queries.

## Automatic Where Input Generation

FraiseQL analyzes your type definitions and creates corresponding `WhereInput` types:

```python
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str
    age: int
    is_active: bool
    created_at: datetime

# Automatically generates UserWhereInput with type-appropriate filters
```

This generates a GraphQL input type like:

```graphql
input UserWhereInput {
  id: UUIDFilter
  name: StringFilter
  email: StringFilter
  age: IntFilter
  is_active: BooleanFilter
  created_at: DateTimeFilter
}
```

## Filter Types by Python Type

### String Filters
Available for `str` fields:

```graphql
input StringFilter {
  eq: String          # Exact match
  neq: String         # Not equal
  contains: String    # Substring match
  startswith: String  # Prefix match
  endswith: String    # Suffix match
  in: [String!]       # Match any of these values
  nin: [String!]      # Match none of these values
  isnull: Boolean     # Check if null/not null
}
```

**Example usage:**
```graphql
query {
  users(where: {
    name: {contains: "john"}
    email: {endswith: "@company.com"}
  }) {
    id
    name
    email
  }
}
```

### Numeric Filters
Available for `int`, `float`, `Decimal` fields:

```graphql
input IntFilter {
  eq: Int
  neq: Int
  gt: Int             # Greater than
  gte: Int            # Greater than or equal
  lt: Int             # Less than
  lte: Int            # Less than or equal
  in: [Int!]
  nin: [Int!]
  isnull: Boolean
}
```

**Example usage:**
```graphql
query {
  users(where: {
    age: {gte: 18, lt: 65}
  }) {
    name
    age
  }
}
```

### DateTime Filters
Available for `datetime`, `date` fields:

```graphql
input DateTimeFilter {
  eq: DateTime
  neq: DateTime
  gt: DateTime        # After this date
  gte: DateTime       # On or after this date
  lt: DateTime        # Before this date
  lte: DateTime       # On or before this date
  in: [DateTime!]
  nin: [DateTime!]
  isnull: Boolean
}
```

**Example usage:**
```graphql
query {
  posts(where: {
    created_at: {gte: "2024-01-01T00:00:00Z"}
    published_at: {isnull: false}
  }) {
    title
    created_at
  }
}
```

### Boolean Filters
Available for `bool` fields:

```graphql
input BooleanFilter {
  eq: Boolean
  neq: Boolean
  isnull: Boolean
}
```

### UUID Filters
Available for `UUID` fields:

```graphql
input UUIDFilter {
  eq: UUID
  neq: UUID
  in: [UUID!]
  nin: [UUID!]
  isnull: Boolean
}
```

## Restricted Filter Types for PostgreSQL Scalars

**🚨 Breaking Change in FraiseQL v0.3.7**: Exotic PostgreSQL scalar types now use restricted filter sets to prevent broken filtering operations.

### Why Restrictions Were Added

PostgreSQL automatically normalizes certain data types:
- **IP addresses**: `10.0.0.1` becomes `10.0.0.1/32` when converted to text
- **MAC addresses**: `aa:bb:cc:dd:ee:ff` becomes canonical form `aa:bb:cc:dd:ee:ff`
- **CIDR ranges**: Stored with network masks that break string pattern matching

This normalization meant operations like `contains: "10.0"` never worked correctly with IP addresses.

### Network Address Types (IP, CIDR)

```python
from fraiseql.types import IpAddress, CIDR

@fraiseql.type
class Server:
    ip_address: IpAddress  # Uses NetworkAddressFilter
    network: CIDR          # Uses NetworkAddressFilter
```

**NetworkAddressFilter** provides network-aware filtering operations:
```graphql
input NetworkAddressFilter {
  # Basic equality operations
  eq: String          # ✅ Exact IP match
  neq: String         # ✅ Not this IP
  in: [String!]       # ✅ Match any of these IPs
  nin: [String!]      # ✅ Match none of these IPs
  isnull: Boolean     # ✅ Check if null

  # Network-specific operations (v0.3.8+)
  inSubnet: String    # ✅ IP is in CIDR subnet
  inRange: IPRange    # ✅ IP is in range
  isPrivate: Boolean  # ✅ RFC 1918 private address
  isPublic: Boolean   # ✅ Non-private address
  isIPv4: Boolean     # ✅ IPv4 address
  isIPv6: Boolean     # ✅ IPv6 address
}

input IPRange {
  from: String!       # Starting IP address
  to: String!         # Ending IP address
}
```

**Network filtering examples:**
```graphql
# ✅ Subnet matching with CIDR notation
servers(where: {
  ip_address: {inSubnet: "192.168.1.0/24"}
})

# ✅ IP range queries
servers(where: {
  ip_address: {
    inRange: {from: "10.0.1.1", to: "10.0.1.100"}
  }
})

# ✅ Private network detection
servers(where: {
  ip_address: {isPrivate: true}
})

# ✅ Public-facing servers
servers(where: {
  ip_address: {isPublic: true}
})

# ✅ IPv4 vs IPv6 filtering
servers(where: {
  ip_address: {isIPv4: true}
})

# ✅ Combined conditions
servers(where: {
  ip_address: {
    isPrivate: true,
    isIPv4: true,
    inSubnet: "192.168.0.0/16"
  }
})
```

**For network range queries, use custom resolvers:**
```python
@fraiseql.query
async def servers_in_network(
    info,
    network_cidr: str
) -> list[Server]:
    """Find servers in a network range using PostgreSQL network operators."""
    repo = info.context["repo"]

    return await repo.raw_query("""
        SELECT jsonb_build_object(
            'id', id,
            'ip_address', ip_address,
            'hostname', hostname
        )
        FROM v_server
        WHERE ip_address <<= %s::inet
    """, [network_cidr])
```

### MAC Address Types

```python
from fraiseql.types import MacAddress

@fraiseql.type
class NetworkInterface:
    mac_address: MacAddress  # Uses MacAddressFilter
```

**MacAddressFilter** provides:
```graphql
input MacAddressFilter {
  eq: String          # ✅ Exact MAC match
  neq: String         # ✅ Not this MAC
  in: [String!]       # ✅ Match any of these MACs
  nin: [String!]      # ✅ Match none of these MACs
  isnull: Boolean     # ✅ Check if null
  # ❌ contains, startswith, endswith removed
}
```

### Hierarchical Path Types (LTree)

```python
from fraiseql.types import LTree

@fraiseql.type
class Category:
    path: LTree  # Uses LTreeFilter
```

**LTreeFilter** provides (most restrictive):
```graphql
input LTreeFilter {
  eq: String          # ✅ Exact path match
  neq: String         # ✅ Not this path
  isnull: Boolean     # ✅ Check if null
  # ❌ All other operators removed until proper ltree operators added
}
```

**Future enhancement**: Will add specialized ltree operators:
```graphql
# 🔄 Coming in future versions
input LTreeFilter {
  eq: String
  neq: String
  ancestor_of: String      # Path is ancestor of value
  descendant_of: String    # Path is descendant of value
  matches_lquery: String   # Matches ltree query pattern
  isnull: Boolean
}
```

### Date Range Types

```python
from fraiseql.types import DateRange

@fraiseql.type
class Event:
    date_range: DateRange  # Uses DateRangeFilter
```

**DateRangeFilter** provides:
```graphql
input DateRangeFilter {
  eq: String          # ✅ Exact range match
  neq: String         # ✅ Not this range
  isnull: Boolean     # ✅ Check if null
  # ❌ Pattern matching removed until proper range operators added
}
```

**Future enhancement**: Will add specialized range operators:
```graphql
# 🔄 Coming in future versions
input DateRangeFilter {
  eq: String
  neq: String
  contains_date: Date      # Range contains this date
  overlaps: String         # Ranges overlap
  adjacent: String         # Ranges are adjacent
  isnull: Boolean
}
```

## Complex Where Conditions

### Combining Filters
Use GraphQL's natural structure for complex conditions:

```graphql
query {
  users(where: {
    name: {contains: "john"}
    age: {gte: 18}
    is_active: {eq: true}
    created_at: {gte: "2024-01-01T00:00:00Z"}
  }) {
    id
    name
    email
  }
}
```

### Logical Operators (OR, AND, NOT)

**🆕 New in v0.6.0**: FraiseQL now supports logical operators for complex filtering conditions, similar to Hasura and Prisma.

Every `WhereInput` type automatically includes logical operators:

```graphql
input UserWhereInput {
  # Field filters
  name: StringFilter
  age: IntFilter
  is_active: BooleanFilter

  # Logical operators
  OR: [UserWhereInput!]
  AND: [UserWhereInput!]
  NOT: UserWhereInput
}
```

#### OR Operator
Match entities where ANY of the conditions are true:

```graphql
query {
  products(where: {
    OR: [
      {category: {eq: "electronics"}},
      {category: {eq: "computers"}},
      {price: {lt: 50}}
    ]
  }) {
    id
    name
    category
    price
  }
}
```

This finds products that are either electronics, computers, OR cost less than $50.

#### AND Operator
Match entities where ALL conditions are true (explicit AND):

```graphql
query {
  users(where: {
    AND: [
      {age: {gte: 18}},
      {is_active: {eq: true}},
      {created_at: {gte: "2024-01-01T00:00:00Z"}}
    ]
  }) {
    id
    name
    age
  }
}
```

**Note**: Fields at the same level are implicitly combined with AND, so this is equivalent to:

```graphql
query {
  users(where: {
    age: {gte: 18}
    is_active: {eq: true}
    created_at: {gte: "2024-01-01T00:00:00Z"}
  }) {
    id
    name
    age
  }
}
```

#### NOT Operator
Match entities where the condition is NOT true:

```graphql
query {
  users(where: {
    NOT: {
      is_active: {eq: false}
    }
  }) {
    id
    name
    is_active
  }
}
```

This finds all users that are NOT inactive (i.e., active users).

#### Complex Nested Logic
Combine logical operators for sophisticated queries:

```graphql
query {
  products(where: {
    # Must be in electronics category
    category: {eq: "electronics"}

    # AND (cheap OR high stock)
    OR: [
      {price: {lt: 100}},
      {stock: {gt: 50}}
    ]

    # AND NOT discontinued
    NOT: {
      status: {eq: "discontinued"}
    }
  }) {
    id
    name
    price
    stock
    status
  }
}
```

This query finds electronics that are either:
- Cheap (< $100) OR well-stocked (> 50 units)
- AND are not discontinued

#### Mixing Field and Logical Operators

```graphql
query {
  orders(where: {
    # Field-level filters (implicit AND)
    customer_id: {eq: "user-123"}
    status: {eq: "pending"}

    # Logical OR condition
    OR: [
      {total: {gt: 1000}},           # High-value orders
      {priority: {eq: "urgent"}}     # OR urgent priority
    ]
  }) {
    id
    total
    status
    priority
  }
}
```

#### Repository-Level Logical Operators

You can also use logical operators in repository queries:

```python
@fraiseql.query
async def complex_product_search(
    info,
    category: str,
    max_price: float | None = None,
    min_stock: int | None = None
) -> list[Product]:
    repo = info.context["repo"]

    # Build complex where condition
    where_conditions = [
        # Must match category
        {"category": category}
    ]

    # Add OR condition for price or stock
    or_conditions = []
    if max_price:
        or_conditions.append({"price__lt": max_price})
    if min_stock:
        or_conditions.append({"stock__gte": min_stock})

    if or_conditions:
        where_conditions.append({"OR": or_conditions})

    # Add NOT condition (exclude discontinued)
    where_conditions.append({
        "NOT": {"status": "discontinued"}
    })

    # Combine all with AND
    where = {"AND": where_conditions}

    return await repo.find("v_product", where=where, order_by="created_at DESC")
```

#### Performance Considerations

**Indexing for Logical Operators:**
Complex logical conditions may require composite indexes:

```sql
-- For OR conditions on multiple fields
CREATE INDEX idx_product_category_price ON tb_product
((data->>'category'), (data->>'price')::numeric);

-- For complex conditions
CREATE INDEX idx_product_complex ON tb_product
((data->>'category'), (data->>'status'), (data->>'price')::numeric);
```

**Query Planning:**
- OR conditions can be less efficient than AND conditions
- Put most selective filters first, even within OR clauses
- Consider using separate queries with UNION for complex OR conditions

**Testing Logical Operators:**

```python
import pytest

@pytest.mark.asyncio
async def test_logical_or_filtering(app_client):
    """Test OR operator with multiple conditions."""

    query = """
        query {
            products(where: {
                OR: [
                    {category: {eq: "electronics"}},
                    {price: {lt: 50}}
                ]
            }) {
                id
                category
                price
            }
        }
    """

    result = await app_client.post("/graphql", json={"query": query})

    assert result.status_code == 200
    products = result.json()["data"]["products"]

    # Verify OR logic: each product should match at least one condition
    for product in products:
        assert (
            product["category"] == "electronics" or
            product["price"] < 50
        )

@pytest.mark.asyncio
async def test_complex_nested_logic(app_client):
    """Test complex nested logical operators."""

    query = """
        query {
            products(where: {
                category: {eq: "electronics"}
                AND: [
                    {
                        OR: [
                            {price: {lt: 100}},
                            {stock: {gt: 50}}
                        ]
                    },
                    {
                        NOT: {
                            status: {eq: "discontinued"}
                        }
                    }
                ]
            }) {
                id
                category
                price
                stock
                status
            }
        }
    """

    result = await app_client.post("/graphql", json={"query": query})

    assert result.status_code == 200
    products = result.json()["data"]["products"]

    # Verify complex logic
    for product in products:
        assert product["category"] == "electronics"
        assert product["price"] < 100 or product["stock"] > 50
        assert product["status"] != "discontinued"
```

### Repository-Level Where Clauses

You can also build where clauses in your resolvers:

```python
@fraiseql.query
async def active_users(
    info,
    name_contains: str | None = None,
    min_age: int | None = None
) -> list[User]:
    repo = info.context["repo"]

    where = {
        "is_active": True  # Always filter for active users
    }

    if name_contains:
        where["name__contains"] = name_contains

    if min_age:
        where["age__gte"] = min_age

    return await repo.find("v_user", where=where, order_by="created_at DESC")
```

## Migration from v0.3.6 to v0.3.7

### Breaking Changes Summary

| Type | Before v0.3.7 | After v0.3.7 | Migration |
|------|---------------|--------------|-----------|
| `IpAddress` | All string operators | `eq`, `neq`, `in_`, `nin`, `isnull` | Use exact matching |
| `CIDR` | All string operators | `eq`, `neq`, `in_`, `nin`, `isnull` | Use exact matching |
| `MacAddress` | All string operators | `eq`, `neq`, `in_`, `nin`, `isnull` | Use exact matching |
| `LTree` | All string operators | `eq`, `neq`, `isnull` | Use exact matching or custom queries |
| `DateRange` | All string operators | `eq`, `neq`, `isnull` | Use exact matching or custom queries |
| Standard types (`str`, `int`, etc.) | No changes | No changes | No migration needed |

### Migration Examples

#### IP Address Filtering

```python
# ❌ Before v0.3.7 (broken but allowed)
servers = await repo.find("v_server", where={
    "ip_address__contains": "192.168"  # Never worked correctly
})

# ✅ v0.3.7+ (exact matching)
servers = await repo.find("v_server", where={
    "ip_address": "192.168.1.100"
})

# ✅ v0.3.8+ (network-aware filtering)
# Subnet matching
servers = await repo.find("v_server", where={
    "ip_address__inSubnet": "192.168.1.0/24"
})

# IP range queries
servers = await repo.find("v_server", where={
    "ip_address__inRange": {"from": "10.0.1.1", "to": "10.0.1.100"}
})

# Private network detection
private_servers = await repo.find("v_server", where={
    "ip_address__isPrivate": True
})

# Combined network conditions
corporate_servers = await repo.find("v_server", where={
    "ip_address__isPrivate": True,
    "ip_address__isIPv4": True,
    "ip_address__inSubnet": "192.168.0.0/16"
})
```

#### MAC Address Filtering

```python
# ❌ Before v0.3.7 (broken but allowed)
devices = await repo.find("v_network_device", where={
    "mac_address__startswith": "aa:bb"  # Never worked correctly
})

# ✅ After v0.3.7 (working solutions)
# Option 1: Exact matching
devices = await repo.find("v_network_device", where={
    "mac_address": "aa:bb:cc:dd:ee:ff"
})

# Option 2: Multiple MACs
devices = await repo.find("v_network_device", where={
    "mac_address__in": [
        "aa:bb:cc:dd:ee:ff",
        "11:22:33:44:55:66"
    ]
})
```

#### LTree Path Filtering

```python
# ❌ Before v0.3.7 (broken but allowed)
categories = await repo.find("v_category", where={
    "path__contains": "electronics"  # Never worked correctly
})

# ✅ After v0.3.7 (working solutions)
# Option 1: Exact path matching
categories = await repo.find("v_category", where={
    "path": "products.electronics.laptops"
})

# Option 2: Custom ltree query (most powerful)
@fraiseql.query
async def categories_under_path(info, parent_path: str) -> list[Category]:
    repo = info.context["repo"]
    return await repo.raw_query("""
        SELECT * FROM v_category
        WHERE path <@ %s::ltree
    """, [parent_path])
```

## Performance Considerations

### Indexing for Filters

Ensure your database has appropriate indexes for filtered fields:

```sql
-- For string contains/pattern matching
CREATE INDEX idx_user_name_gin ON tb_user USING gin(to_tsvector('english', data->>'name'));

-- For exact matching
CREATE INDEX idx_user_email ON tb_user ((data->>'email'));

-- For numeric ranges
CREATE INDEX idx_user_age ON tb_user ((data->>'age')::int);

-- For date ranges
CREATE INDEX idx_user_created_at ON tb_user ((data->>'created_at')::timestamptz);

-- For PostgreSQL network types
CREATE INDEX idx_server_ip ON tb_server ((data->>'ip_address')::inet);
```

### Filter Ordering

Put most selective filters first:

```graphql
# ✅ Good: Most selective filter first
users(where: {
  email: {eq: "specific@user.com"}    # Very selective
  name: {contains: "john"}            # Less selective
  is_active: {eq: true}               # Least selective
})

# ❌ Less optimal: Least selective first
users(where: {
  is_active: {eq: true}               # Least selective first
  name: {contains: "john"}
  email: {eq: "specific@user.com"}
})
```

## Testing Filtered Queries

```python
import pytest
from fraiseql import create_fraiseql_app

@pytest.mark.asyncio
async def test_user_filtering(app_client):
    """Test user filtering with various operators."""

    # Test string contains (works for standard string fields)
    query = """
        query($nameContains: String) {
            users(where: {name: {contains: $nameContains}}) {
                id
                name
                email
            }
        }
    """

    result = await app_client.post("/graphql", json={
        "query": query,
        "variables": {"nameContains": "john"}
    })

    assert result.status_code == 200
    data = result.json()["data"]
    assert all("john" in user["name"].lower() for user in data["users"])

@pytest.mark.asyncio
async def test_network_filtering_v3_7(app_client):
    """Test network address filtering with restricted operators."""

    # ✅ This works - exact matching
    query = """
        query($ipAddress: String) {
            servers(where: {ip_address: {eq: $ipAddress}}) {
                id
                ip_address
            }
        }
    """

    result = await app_client.post("/graphql", json={
        "query": query,
        "variables": {"ipAddress": "192.168.1.100"}
    })

    assert result.status_code == 200

    # ❌ This should fail - contains not available for IP addresses
    invalid_query = """
        query($ipContains: String) {
            servers(where: {ip_address: {contains: $ipContains}}) {
                id
            }
        }
    """

    result = await app_client.post("/graphql", json={
        "query": invalid_query,
        "variables": {"ipContains": "192.168"}
    })

    # Should return GraphQL validation error
    assert result.status_code == 400
    assert "contains" in result.json()["errors"][0]["message"]

@pytest.mark.asyncio
async def test_network_filtering_v3_8(app_client):
    """Test network-specific filtering operations in v0.3.8+."""

    # ✅ Subnet matching
    subnet_query = """
        query($subnet: String!) {
            servers(where: {ip_address: {inSubnet: $subnet}}) {
                id
                ip_address
            }
        }
    """

    result = await app_client.post("/graphql", json={
        "query": subnet_query,
        "variables": {"subnet": "192.168.1.0/24"}
    })

    assert result.status_code == 200
    servers = result.json()["data"]["servers"]
    # All IPs should be in the 192.168.1.x range
    for server in servers:
        ip = server["ip_address"]
        assert ip.startswith("192.168.1.")

    # ✅ IP range queries
    range_query = """
        query($from: String!, $to: String!) {
            servers(where: {
                ip_address: {
                    inRange: {from: $from, to: $to}
                }
            }) {
                id
                ip_address
            }
        }
    """

    result = await app_client.post("/graphql", json={
        "query": range_query,
        "variables": {"from": "192.168.1.1", "to": "192.168.1.100"}
    })

    assert result.status_code == 200

    # ✅ Private network detection
    private_query = """
        query {
            servers(where: {ip_address: {isPrivate: true}}) {
                id
                ip_address
            }
        }
    """

    result = await app_client.post("/graphql", json={"query": private_query})

    assert result.status_code == 200
    servers = result.json()["data"]["servers"]

    # All should be RFC 1918 private addresses
    for server in servers:
        ip = server["ip_address"]
        assert (ip.startswith("10.") or
                ip.startswith("172.") or
                ip.startswith("192.168."))

    # ✅ IPv4/IPv6 filtering
    ipv4_query = """
        query {
            servers(where: {ip_address: {isIPv4: true}}) {
                id
                ip_address
            }
        }
    """

    result = await app_client.post("/graphql", json={"query": ipv4_query})

    assert result.status_code == 200
    servers = result.json()["data"]["servers"]

    # All should be IPv4 addresses (contain dots, not colons)
    for server in servers:
        ip = server["ip_address"]
        assert "." in ip and ":" not in ip
```

## Best Practices

### 1. Use Appropriate Filter Types
- **Standard strings**: Use all available operators (`contains`, `startswith`, etc.)
- **Exotic types**: Stick to exact matching or implement custom resolvers
- **Numeric fields**: Leverage range operators (`gte`, `lt`) for efficient queries

### 2. Combine Filters Effectively
```python
# ✅ Good: Combine complementary filters
where = {
    "is_active": True,           # High selectivity boolean
    "created_at__gte": "2024-01-01",  # Date range
    "name__contains": search_term     # Text search
}
```

### 3. Provide Migration Path
When exotic types need complex filtering, provide custom resolvers:

```python
@fraiseql.query
async def servers_by_network_pattern(
    info,
    network_pattern: str
) -> list[Server]:
    """Custom network filtering with PostgreSQL operators."""
    repo = info.context["repo"]

    # Use proper PostgreSQL network operators
    return await repo.raw_query("""
        SELECT jsonb_build_object(
            'id', id,
            'ip_address', ip_address,
            'hostname', hostname
        )
        FROM v_server
        WHERE ip_address <<= %s::inet
           OR text(ip_address) LIKE %s
    """, [network_pattern, f"%{network_pattern}%"])
```

### 4. Document Breaking Changes
Always document when filter capabilities change:

```python
@fraiseql.type
class NetworkDevice:
    """Network device information.

    Note: As of FraiseQL v0.3.7, MAC address filtering only supports
    exact matching. Use custom resolvers for pattern-based queries.
    """
    mac_address: MacAddress
```

## Next Steps

- Learn about [Query Translation](./query-translation.md) to understand how filters become SQL
- Explore [Database Views](./database-views.md) to see how data is structured for efficient filtering
- See [Performance Guide](../advanced/performance.md) for indexing and optimization strategies
- Check out complete examples in [Blog API Tutorial](../tutorials/blog-api.md)
