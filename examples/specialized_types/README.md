# Specialized Types (UUID, INET, CIDR, JSONB)

Production-ready examples demonstrating PostgreSQL's specialized types for infrastructure, networking, and flexible data applications. Type-safe operations that no other GraphQL framework offers out of the box.

## What This Example Demonstrates

This is a **complete specialized types showcase** with:
- IPv4/IPv6 addresses with CIDR notation support
- Network operations (subnet containment, private IP detection)
- JSONB for flexible schemas and metadata
- Array types with containment operations
- Type-safe GraphQL operations
- Infrastructure/networking use cases
- Complete database schema with appropriate indexes

## Available Specialized Types

### 1. IP Address Types (INET/CIDR)

PostgreSQL's `INET` type stores IPv4 or IPv6 addresses with optional CIDR notation:

```python
from fraiseql.types.scalars.ip_address import IpAddressField

@app.type
@dataclass
class NetworkDevice:
    ipv4_address: IpAddressField  # "192.168.1.1" or "192.168.1.1/24"
    ipv6_address: IpAddressField | None  # "2001:db8::1"
```

**Benefits:**
- ✅ Automatic validation (invalid IPs rejected)
- ✅ CIDR notation support (`192.168.1.0/24`)
- ✅ Network operators (subnet containment, overlaps)
- ✅ Compact storage (4 bytes for IPv4, 16 for IPv6)
- ✅ Type safety at GraphQL level

**Comparison with storing as strings:**

| Feature | INET Type | VARCHAR Type |
|---------|-----------|--------------|
| Storage | 7-19 bytes | 15-45+ bytes |
| Validation | Automatic | Manual validation needed |
| CIDR support | Native | Must parse strings |
| Subnet queries | Fast (indexed) | Slow (string matching) |
| Type safety | GraphQL-level | None |

### 2. JSONB (Binary JSON)

PostgreSQL's `JSONB` type stores JSON data in binary format for fast queries:

```python
@app.type
@dataclass
class NetworkDevice:
    tags: dict  # {"environment": "prod", "team": "platform"}
    specifications: dict  # Nested objects, variable schema
```

**Benefits:**
- ✅ Flexible schema (add fields without migrations)
- ✅ Fast queries with GIN indexes
- ✅ Nested object support
- ✅ JSON path queries (`data->'nested'->>'field'`)
- ✅ Containment operators (`@>`, `<@`)

**When to use JSONB:**
- User preferences and settings
- Metadata that varies by type
- API responses to store
- Audit trail details
- Configuration objects

### 3. Array Types

PostgreSQL supports native arrays with type-safe operations:

```python
@app.type
@dataclass
class NetworkDevice:
    vlan_ids: list[int]  # [100, 200, 300]
    tags: list[str]  # ["production", "monitored"]
```

**Benefits:**
- ✅ Containment checks (`@>`, `<@`, `&&`)
- ✅ GIN indexes for fast queries
- ✅ Type-safe at database level
- ✅ No junction tables needed for simple lists

### 4. UUID Type

PostgreSQL's `UUID` type stores universally unique identifiers:

```python
from uuid import UUID

@app.type
@dataclass
class Device:
    id: UUID  # Generated with gen_random_uuid()
    organization_id: UUID  # For multi-tenant systems
```

**Benefits:**
- ✅ Globally unique (no collisions)
- ✅ Great for distributed systems
- ✅ 16 bytes (compact)
- ✅ No sequential ID leakage
- ✅ Perfect for multi-tenant apps

## Use Cases

### Infrastructure Monitoring

**Problem:** Track thousands of servers, IP addresses, and network configuration without rigid schema constraints.

**Solution:** Use INET for IP addresses, JSONB for flexible metadata:

```graphql
query InfrastructureInventory {
  devices(device_type: "server", location: "us-east-1") {
    hostname
    ipv4_address
    ipv6_address
    vlan_ids
    tags
    last_seen
  }
}
```

### IP Allowlisting/Blocklisting

**Problem:** Manage firewall rules with CIDR blocks and subnet operations.

**Solution:** Use CIDR type with network operators:

```graphql
query SecurityRules {
  security_rules(protocol: "tcp", enabled_only: true) {
    name
    source_cidr        # "0.0.0.0/0" or "192.168.1.0/24"
    destination_cidr
    port
    action
    priority
  }
}

query DevicesInSubnet {
  devices_in_subnet(cidr: "10.0.0.0/8") {
    hostname
    ipv4_address
    location
  }
}
```

### Network Monitoring

**Problem:** Monitor network traffic, detect private vs public IPs, track subnet usage.

**Solution:** Use PostgreSQL's network operators:

```graphql
query PrivateIPDevices {
  private_ip_devices {
    hostname
    ipv4_address
    device_type
    location
  }
}

query SubnetUtilization {
  devices_in_subnet(cidr: "192.168.1.0/24") {
    ipv4_address
    hostname
    is_active
  }
}
```

### Geolocation Services

**Problem:** Map IP addresses to locations for analytics or access control.

**Solution:** Store IP ranges with location metadata:

```sql
CREATE TABLE ip_geolocation (
    id SERIAL PRIMARY KEY,
    ip_range CIDR NOT NULL,
    country VARCHAR(2),
    city VARCHAR(100),
    latitude DECIMAL(10, 7),
    longitude DECIMAL(10, 7),
    metadata JSONB
);

-- Query: Find location for an IP
SELECT * FROM ip_geolocation
WHERE '203.0.113.45'::inet << ip_range
ORDER BY masklen(ip_range) DESC
LIMIT 1;
```

## IP Address Handling

### IPv4 Addresses

```python
@app.type
@dataclass
class Device:
    ipv4_address: IpAddressField  # "192.168.1.10"
```

**GraphQL Query:**
```graphql
query FindDevice {
  device_by_ip(ip_address: "192.168.1.10") {
    hostname
    ipv4_address
    device_type
  }
}
```

**Database:**
```sql
CREATE TABLE devices (
    id SERIAL PRIMARY KEY,
    hostname VARCHAR(255),
    ipv4_address INET NOT NULL,
    UNIQUE(ipv4_address)
);

-- Fast IP lookups with GiST index
CREATE INDEX idx_devices_ipv4
    ON devices USING gist (ipv4_address inet_ops);
```

### IPv6 Addresses

```python
@app.type
@dataclass
class Device:
    ipv6_address: IpAddressField | None
    # "2001:0db8:85a3:0000:0000:8a2e:0370:7334"
    # or compressed: "2001:db8::1"
```

**GraphQL Query:**
```graphql
query IPv6Devices {
  devices {
    hostname
    ipv4_address
    ipv6_address
  }
}
```

### CIDR Notation Support

CIDR notation allows specifying IP address ranges:

```python
# Query for devices in a subnet
@app.query
async def devices_in_subnet(info, cidr: str) -> list[NetworkDevice]:
    """
    Find all devices in a CIDR block.

    Examples:
    - "10.0.0.0/8" - All 10.x.x.x addresses
    - "192.168.1.0/24" - 192.168.1.0 to 192.168.1.255
    - "172.16.0.0/12" - Private network range
    """
    db = info.context["db"]
    # Uses PostgreSQL's << operator (contained by)
    return await db.find("v_network_devices", ipv4_address__in_subnet=cidr)
```

**GraphQL Query:**
```graphql
query SubnetDevices {
  devices_in_subnet(cidr: "192.168.1.0/24") {
    hostname
    ipv4_address
    location
    device_type
  }
}
```

**Database Implementation:**
```sql
-- Find devices in subnet using << operator
SELECT * FROM devices
WHERE ipv4_address << '192.168.1.0/24'::inet;

-- Check if IP is in private ranges
SELECT * FROM devices
WHERE ipv4_address << '10.0.0.0/8'::inet
   OR ipv4_address << '172.16.0.0/12'::inet
   OR ipv4_address << '192.168.0.0/16'::inet;

-- Find overlapping subnets
SELECT * FROM security_rules
WHERE source_cidr && '192.168.0.0/16'::inet;
```

## Network Operators

PostgreSQL provides powerful network operators:

| Operator | Description | Example |
|----------|-------------|---------|
| `<<` | Is contained by (subnet) | `'192.168.1.5' << '192.168.1.0/24'` |
| `<<=` | Is contained by or equals | `'192.168.1.0/24' <<= '192.168.0.0/16'` |
| `>>` | Contains (subnet) | `'192.168.1.0/24' >> '192.168.1.5'` |
| `>>=` | Contains or equals | `'192.168.0.0/16' >>= '192.168.1.0/24'` |
| `&&` | Overlaps | `'192.168.1.0/24' && '192.168.0.0/16'` |
| `~` | Bitwise NOT | `~'192.168.1.0'::inet` |
| `&` | Bitwise AND | `'192.168.1.5' & '255.255.255.0'` |
| `|` | Bitwise OR | `'192.168.1.5' | '0.0.0.255'` |

**Example Queries:**

```sql
-- Private IP detection
SELECT * FROM devices
WHERE ipv4_address << '10.0.0.0/8'::inet
   OR ipv4_address << '172.16.0.0/12'::inet
   OR ipv4_address << '192.168.0.0/16'::inet;

-- Find all subnets that contain a specific IP
SELECT * FROM subnets
WHERE subnet_cidr >> '192.168.1.100'::inet;

-- Find overlapping security rules
SELECT r1.name as rule1, r2.name as rule2
FROM security_rules r1, security_rules r2
WHERE r1.id < r2.id
  AND r1.source_cidr && r2.source_cidr;
```

## JSONB Advantages

### Flexible Schema

Add fields without migrations:

```sql
-- Initial data
INSERT INTO devices (hostname, ipv4_address, data) VALUES
('server-01', '192.168.1.10', '{"environment": "production"}');

-- Later: Add new fields without ALTER TABLE
UPDATE devices SET data = data || '{"monitoring": true, "alerts": ["cpu"]}'
WHERE hostname = 'server-01';
```

### Nested Objects

Store complex structures:

```python
@app.type
@dataclass
class NetworkDevice:
    tags: dict  # Nested objects supported
    # {
    #   "environment": "production",
    #   "team": "platform",
    #   "contact": {
    #     "name": "John Doe",
    #     "email": "john@example.com"
    #   },
    #   "monitoring": {
    #     "enabled": true,
    #     "intervals": [60, 300, 900]
    #   }
    # }
```

### JSON Path Queries

Query nested fields:

```sql
-- Get device environment
SELECT hostname, data->>'environment' as environment
FROM devices;

-- Get nested contact email
SELECT hostname, data->'contact'->>'email' as email
FROM devices;

-- Filter by nested field
SELECT * FROM devices
WHERE data->'monitoring'->>'enabled' = 'true';

-- Query array elements
SELECT * FROM devices
WHERE data->'monitoring'->'intervals' @> '[300]'::jsonb;
```

### Containment Operators

Check if JSONB contains specific data:

```sql
-- Contains specific key-value pair
SELECT * FROM devices
WHERE data @> '{"environment": "production"}';

-- Contained by (subset check)
SELECT * FROM devices
WHERE data <@ '{"environment": "production", "team": "platform"}';

-- Key exists
SELECT * FROM devices
WHERE data ? 'monitoring';

-- Any of these keys exist
SELECT * FROM devices
WHERE data ?| ARRAY['monitoring', 'logging'];

-- All of these keys exist
SELECT * FROM devices
WHERE data ?& ARRAY['environment', 'team'];
```

### GIN Indexes for Performance

```sql
-- Index entire JSONB column
CREATE INDEX idx_devices_data
    ON devices USING gin (data);

-- Query with containment (uses GIN index)
SELECT * FROM devices
WHERE data @> '{"environment": "production"}';
-- Fast: ~10-50ms on 1M rows with GIN index

-- Index specific JSONB path
CREATE INDEX idx_devices_environment
    ON devices ((data->>'environment'));

-- Query specific path (uses B-tree index)
SELECT * FROM devices
WHERE data->>'environment' = 'production';
-- Very fast: ~5ms on 1M rows
```

## Type Safety Benefits

### At GraphQL Level

FraiseQL automatically validates types:

```graphql
# ✅ Valid
mutation AddDevice {
  add_device(input: {
    hostname: "web-01"
    ipv4_address: "192.168.1.10"  # Valid IP
    vlan_ids: [100, 200]          # Valid array
  }) {
    id
    hostname
  }
}

# ❌ Rejected at GraphQL level
mutation InvalidDevice {
  add_device(input: {
    hostname: "web-01"
    ipv4_address: "999.999.999.999"  # Invalid IP - GraphQL error
    vlan_ids: "not-an-array"         # Wrong type - GraphQL error
  }) {
    id
  }
}
```

### At Database Level

PostgreSQL enforces constraints:

```sql
-- Type checking
INSERT INTO devices (ipv4_address) VALUES ('invalid-ip');
-- ERROR: invalid input syntax for type inet

-- Range validation
INSERT INTO security_rules (port) VALUES (99999);
-- ERROR: violates check constraint (port >= 1 AND port <= 65535)

-- Foreign key enforcement
INSERT INTO devices (category_id) VALUES (999);
-- ERROR: violates foreign key constraint
```

## Complete Database Schema

### Network Devices Table

```sql
-- Enable UUID extension (for UUIDs)
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Network devices with specialized types
CREATE TABLE tb_network_devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hostname VARCHAR(255) NOT NULL UNIQUE,

    -- IP address types (INET supports both IPv4 and IPv6)
    ipv4_address INET NOT NULL,
    ipv6_address INET,
    subnet_mask VARCHAR(50),

    -- Regular columns
    device_type VARCHAR(50) NOT NULL,
    location VARCHAR(100) NOT NULL,

    -- Array type
    vlan_ids INT[] NOT NULL DEFAULT '{}',

    -- JSONB for flexible metadata
    tags JSONB NOT NULL DEFAULT '{}',

    -- Status tracking
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_seen TIMESTAMP NOT NULL DEFAULT NOW(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT valid_device_type CHECK (
        device_type IN ('server', 'router', 'switch', 'firewall', 'load_balancer')
    )
);

-- Indexes for network operations
CREATE INDEX idx_devices_ipv4
    ON tb_network_devices USING gist (ipv4_address inet_ops);

CREATE INDEX idx_devices_ipv6
    ON tb_network_devices USING gist (ipv6_address inet_ops);

CREATE INDEX idx_devices_location
    ON tb_network_devices(location);

CREATE INDEX idx_devices_type
    ON tb_network_devices(device_type);

-- GIN index for JSONB queries
CREATE INDEX idx_devices_tags
    ON tb_network_devices USING gin (tags);

-- GIN index for array queries
CREATE INDEX idx_devices_vlans
    ON tb_network_devices USING gin (vlan_ids);

-- View for GraphQL
CREATE VIEW v_network_devices AS
SELECT
    id,
    hostname,
    ipv4_address::text as ipv4_address,
    ipv6_address::text as ipv6_address,
    subnet_mask,
    device_type,
    location,
    vlan_ids,
    tags,
    is_active,
    last_seen,
    created_at
FROM tb_network_devices;
```

### Security Rules Table

```sql
CREATE TABLE tb_security_rules (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,

    -- CIDR type for network blocks
    source_cidr CIDR NOT NULL,
    destination_cidr CIDR NOT NULL,

    -- Port and protocol
    port INT NOT NULL,
    protocol VARCHAR(10) NOT NULL,

    -- Rule action
    action VARCHAR(10) NOT NULL,
    priority INT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,

    -- Constraints
    CONSTRAINT valid_port CHECK (port >= 1 AND port <= 65535),
    CONSTRAINT valid_protocol CHECK (protocol IN ('tcp', 'udp', 'icmp', 'all')),
    CONSTRAINT valid_action CHECK (action IN ('allow', 'deny'))
);

-- Index for rule priority
CREATE INDEX idx_rules_priority
    ON tb_security_rules(priority)
    WHERE enabled = true;

-- View for GraphQL
CREATE VIEW v_security_rules AS
SELECT
    id,
    name,
    source_cidr::text as source_cidr,
    destination_cidr::text as destination_cidr,
    port,
    protocol,
    action,
    priority,
    enabled
FROM tb_security_rules
ORDER BY priority;
```

## Setup

### 1. Install Dependencies

```bash
cd examples/specialized_types
pip install -r requirements.txt
```

Or with uv:
```bash
uv pip install -r requirements.txt
```

### 2. Setup Database

```bash
# Create database
createdb infrastructure

# Apply schema
psql infrastructure << 'EOF'
-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Copy schema from above or use provided schema.sql
EOF
```

### 3. Load Sample Data

```sql
-- Insert sample devices
INSERT INTO tb_network_devices
    (hostname, ipv4_address, device_type, location, vlan_ids, tags)
VALUES
    ('web-server-01', '192.168.1.10', 'server', 'us-east-1',
     ARRAY[100, 200],
     '{"environment": "production", "app": "web", "team": "platform"}'::jsonb),

    ('web-server-02', '192.168.1.11', 'server', 'us-east-1',
     ARRAY[100, 200],
     '{"environment": "production", "app": "web", "team": "platform"}'::jsonb),

    ('db-server-01', '10.0.1.5', 'server', 'us-east-1',
     ARRAY[300],
     '{"environment": "production", "app": "database", "team": "data"}'::jsonb),

    ('router-01', '192.168.1.1', 'router', 'us-east-1',
     ARRAY[100],
     '{"role": "gateway", "vendor": "cisco"}'::jsonb),

    ('firewall-01', '192.168.1.2', 'firewall', 'us-east-1',
     ARRAY[100],
     '{"vendor": "palo-alto", "model": "PA-5220"}'::jsonb);

-- Insert sample security rules
INSERT INTO tb_security_rules
    (name, source_cidr, destination_cidr, port, protocol, action, priority)
VALUES
    ('Allow HTTP from anywhere', '0.0.0.0/0', '192.168.1.0/24', 80, 'tcp', 'allow', 100),
    ('Allow HTTPS from anywhere', '0.0.0.0/0', '192.168.1.0/24', 443, 'tcp', 'allow', 101),
    ('Allow SSH from office', '203.0.113.0/24', '192.168.1.0/24', 22, 'tcp', 'allow', 200),
    ('Block outbound to suspicious IPs', '192.168.1.0/24', '192.0.2.0/24', 0, 'all', 'deny', 500),
    ('Deny all others', '0.0.0.0/0', '0.0.0.0/0', 0, 'all', 'deny', 999);
```

### 4. Run the Application

```bash
python main.py
```

The API will be available at:
- **GraphQL Playground:** http://localhost:8000/graphql
- **API Documentation:** http://localhost:8000/docs

## GraphQL Queries

### Query All Devices

```graphql
query AllDevices {
  devices(device_type: "server", is_active: true) {
    hostname
    ipv4_address
    ipv6_address
    device_type
    location
    vlan_ids
    tags
    last_seen
  }
}
```

### Find Device by IP

```graphql
query FindByIP {
  device_by_ip(ip_address: "192.168.1.10") {
    hostname
    ipv4_address
    device_type
    location
    tags
  }
}
```

### Devices in Subnet

```graphql
query SubnetDevices {
  devices_in_subnet(cidr: "192.168.1.0/24") {
    hostname
    ipv4_address
    location
    device_type
  }
}
```

### Private IP Devices

```graphql
query PrivateIPs {
  private_ip_devices {
    hostname
    ipv4_address
    device_type
    location
  }
}
```

### Security Rules

```graphql
query FirewallRules {
  security_rules(protocol: "tcp", enabled_only: true) {
    name
    source_cidr
    destination_cidr
    port
    action
    priority
  }
}
```

## Related Examples

- [`../hybrid_tables/`](../hybrid_tables/) - Combining indexed columns with JSONB
- [`../filtering/`](../filtering/) - Type-aware filtering and where clauses
- [`../fastapi/`](../fastapi/) - Complete FastAPI integration

## Key Takeaways

1. **Use INET for IP addresses** - Native validation, CIDR support, network operators
2. **JSONB for flexible metadata** - No migrations needed, GIN indexes for fast queries
3. **Array types for simple lists** - No junction tables, type-safe operations
4. **UUID for distributed systems** - Globally unique, no ID leakage, multi-tenant ready
5. **Network operators are powerful** - Subnet containment, overlaps, private IP detection
6. **Type safety at all levels** - GraphQL, application, and database enforcement
7. **Specialized indexes matter** - GiST for network types, GIN for JSONB and arrays

---

**These specialized types give you database-level type safety and operations that are impossible with generic string storage. Perfect for infrastructure monitoring, networking applications, and flexible schemas!** 🚀
