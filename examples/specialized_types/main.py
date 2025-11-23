"""Specialized PostgreSQL Types Example for FraiseQL.

This example demonstrates FraiseQL's support for PostgreSQL's advanced types:
- IPv4/IPv6 addresses with CIDR notation
- Network operations (subnet checks, private IP detection)
- JSONB for flexible schemas
- Array types with containment operations

These specialized types provide type-safe operations that no other
GraphQL framework offers out of the box.
"""

from dataclasses import dataclass
from datetime import datetime
from ipaddress import IPv4Address, IPv6Address

from fraiseql import FraiseQL
from fraiseql.types.scalars.ip_address import IpAddressField

# Initialize FraiseQL
app = FraiseQL(database_url="postgresql://localhost/infrastructure")


@app.type
@dataclass
class NetworkDevice:
    """Network device in the infrastructure.

    Tracks servers, routers, switches, and other network equipment
    with their IP addresses and network configuration.
    """

    id: int
    """Unique device identifier"""

    hostname: str
    """Device hostname (e.g., 'web-server-01')"""

    ipv4_address: IpAddressField
    """IPv4 address in dot-decimal notation.

    Examples: '192.168.1.1', '10.0.0.5'
    Supports CIDR notation: '192.168.1.1/24'
    """

    ipv6_address: IpAddressField | None
    """IPv6 address if configured.

    Example: '2001:0db8:85a3:0000:0000:8a2e:0370:7334'
    Supports compressed notation and CIDR
    """

    subnet_mask: str
    """Subnet mask (e.g., '255.255.255.0' or CIDR '/24')"""

    device_type: str
    """Type of device: server, router, switch, firewall, load_balancer"""

    location: str
    """Physical location or data center"""

    vlan_ids: list[int]
    """VLANs this device belongs to"""

    tags: dict
    """Flexible metadata as JSONB.

    Example: {"environment": "production", "team": "platform", "monitored": true}
    """

    is_active: bool
    """Whether the device is currently active"""

    last_seen: datetime
    """Last successful ping/heartbeat (UTC)"""

    created_at: datetime
    """When the device was added to inventory (UTC)"""


@app.type
@dataclass
class SecurityRule:
    """Firewall or security group rule.

    Defines network access control rules with source/destination
    IP addresses or CIDR blocks.
    """

    id: int
    """Rule identifier"""

    name: str
    """Human-readable rule name"""

    source_cidr: str
    """Source IP address or CIDR block.

    Examples: '0.0.0.0/0' (any), '192.168.1.0/24' (subnet)
    """

    destination_cidr: str
    """Destination IP address or CIDR block"""

    port: int
    """Target port number"""

    protocol: str
    """Protocol: tcp, udp, icmp"""

    action: str
    """Action to take: allow, deny"""

    priority: int
    """Rule priority (lower numbers evaluated first)"""

    enabled: bool
    """Whether this rule is active"""


# =============================================================================
# GraphQL Queries with Network Operations
# =============================================================================

@app.query
async def devices(
    info,
    device_type: str | None = None,
    location: str | None = None,
    is_active: bool = True,
    vlan_id: int | None = None,
) -> list[NetworkDevice]:
    """Query network devices with filtering.

    Args:
        device_type: Filter by device type
        location: Filter by physical location
        is_active: Only return active devices (default: True)
        vlan_id: Filter by VLAN membership

    Returns:
        List of matching network devices

    Example:
        ```graphql
        {
          devices(device_type: "server", location: "us-east-1", is_active: true) {
            hostname
            ipv4_address
            ipv6_address
            vlan_ids
            tags
          }
        }
        ```
    """
    db = info.context["db"]
    filters = {"is_active": is_active}

    if device_type:
        filters["device_type"] = device_type
    if location:
        filters["location"] = location
    if vlan_id is not None:
        # Array containment check
        filters["vlan_ids__contains"] = [vlan_id]

    return await db.find("v_network_devices", **filters)


@app.query
async def devices_in_subnet(info, cidr: str) -> list[NetworkDevice]:
    """Find all devices in a specific subnet.

    Uses PostgreSQL's inet << operator for subnet containment.

    Args:
        cidr: CIDR notation (e.g., '192.168.1.0/24')

    Returns:
        List of devices whose IPv4 address is in the subnet

    Example:
        ```graphql
        {
          devices_in_subnet(cidr: "10.0.0.0/8") {
            hostname
            ipv4_address
            location
          }
        }
        ```
    """
    db = info.context["db"]
    # Use PostgreSQL's inet << operator via raw SQL
    # This would be wrapped in FraiseQL's network operator support
    return await db.find("v_network_devices", ipv4_address__in_subnet=cidr)


@app.query
async def private_ip_devices(info) -> list[NetworkDevice]:
    """Find all devices with private IP addresses.

    Private IP ranges:
    - 10.0.0.0/8
    - 172.16.0.0/12
    - 192.168.0.0/16

    Returns:
        List of devices with private IPv4 addresses

    Example:
        ```graphql
        {
          private_ip_devices {
            hostname
            ipv4_address
            device_type
          }
        }
        ```
    """
    db = info.context["db"]
    # Check if IP is in private ranges
    # This demonstrates network type awareness
    return await db.find("v_network_devices", ipv4_address__is_private=True)


@app.query
async def device(info, id: int) -> NetworkDevice | None:
    """Get a single device by ID.

    Args:
        id: Device ID

    Returns:
        Device details or null if not found
    """
    db = info.context["db"]
    return await db.find_one("v_network_devices", id=id)


@app.query
async def device_by_ip(info, ip_address: str) -> NetworkDevice | None:
    """Find device by its IP address.

    Supports both IPv4 and IPv6 addresses.

    Args:
        ip_address: IP address to search for

    Returns:
        Device with matching IP or null if not found

    Example:
        ```graphql
        {
          device_by_ip(ip_address: "192.168.1.100") {
            hostname
            device_type
            location
            tags
          }
        }
        ```
    """
    db = info.context["db"]
    # Try IPv4 first, then IPv6
    device = await db.find_one("v_network_devices", ipv4_address=ip_address)
    if not device and ":" in ip_address:
        # Might be IPv6
        device = await db.find_one("v_network_devices", ipv6_address=ip_address)
    return device


@app.query
async def security_rules(
    info,
    protocol: str | None = None,
    enabled_only: bool = True,
) -> list[SecurityRule]:
    """Query security rules.

    Args:
        protocol: Filter by protocol (tcp, udp, icmp)
        enabled_only: Only return enabled rules

    Returns:
        List of security rules sorted by priority
    """
    db = info.context["db"]
    filters = {"enabled": enabled_only}

    if protocol:
        filters["protocol"] = protocol

    return await db.find("v_security_rules", order_by="priority", **filters)


# =============================================================================
# Database Schema (for reference)
# =============================================================================
"""
-- Enable network types extension
CREATE EXTENSION IF NOT EXISTS postgis;  -- For advanced network operations

-- Network devices table
CREATE TABLE tb_network_devices (
    id SERIAL PRIMARY KEY,
    hostname VARCHAR(255) NOT NULL UNIQUE,
    ipv4_address INET NOT NULL,  -- PostgreSQL INET type for IPv4/IPv6
    ipv6_address INET,
    subnet_mask VARCHAR(50),
    device_type VARCHAR(50) NOT NULL,
    location VARCHAR(100) NOT NULL,
    vlan_ids INT[] NOT NULL DEFAULT '{}',  -- Array of integers
    tags JSONB NOT NULL DEFAULT '{}',  -- Flexible metadata
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_seen TIMESTAMP NOT NULL DEFAULT NOW(),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),

    -- Indexes for network operations
    CONSTRAINT valid_device_type CHECK (device_type IN ('server', 'router', 'switch', 'firewall', 'load_balancer'))
);

-- Index for fast IP lookups
CREATE INDEX idx_devices_ipv4 ON tb_network_devices USING gist (ipv4_address inet_ops);
CREATE INDEX idx_devices_ipv6 ON tb_network_devices USING gist (ipv6_address inet_ops);
CREATE INDEX idx_devices_location ON tb_network_devices(location);
CREATE INDEX idx_devices_type ON tb_network_devices(device_type);
CREATE INDEX idx_devices_tags ON tb_network_devices USING gin (tags);  -- JSONB index

-- View for GraphQL queries
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

-- Security rules table
CREATE TABLE tb_security_rules (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    source_cidr CIDR NOT NULL,  -- CIDR type for network blocks
    destination_cidr CIDR NOT NULL,
    port INT NOT NULL CHECK (port >= 1 AND port <= 65535),
    protocol VARCHAR(10) NOT NULL,
    action VARCHAR(10) NOT NULL,
    priority INT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,

    CONSTRAINT valid_protocol CHECK (protocol IN ('tcp', 'udp', 'icmp')),
    CONSTRAINT valid_action CHECK (action IN ('allow', 'deny'))
);

-- View for security rules
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

-- Example: Find devices in a subnet using PostgreSQL network operators
-- SELECT * FROM tb_network_devices WHERE ipv4_address << '192.168.1.0/24';

-- Example: Check if IP is private
-- SELECT * FROM tb_network_devices
-- WHERE ipv4_address << '10.0.0.0/8'::inet
--    OR ipv4_address << '172.16.0.0/12'::inet
--    OR ipv4_address << '192.168.0.0/16'::inet;

-- Example: JSONB queries
-- SELECT * FROM tb_network_devices WHERE tags->>'environment' = 'production';
-- SELECT * FROM tb_network_devices WHERE tags @> '{"monitored": true}';

-- Example: Array operations
-- SELECT * FROM tb_network_devices WHERE 100 = ANY(vlan_ids);  -- Device in VLAN 100
-- SELECT * FROM tb_network_devices WHERE vlan_ids && ARRAY[100, 200];  -- Overlap
"""

# =============================================================================
# Example Data
# =============================================================================
"""
-- Insert sample devices
INSERT INTO tb_network_devices (hostname, ipv4_address, device_type, location, vlan_ids, tags) VALUES
('web-server-01', '192.168.1.10', 'server', 'us-east-1', ARRAY[100, 200], '{"environment": "production", "app": "web"}'),
('web-server-02', '192.168.1.11', 'server', 'us-east-1', ARRAY[100, 200], '{"environment": "production", "app": "web"}'),
('db-server-01', '10.0.1.5', 'server', 'us-east-1', ARRAY[300], '{"environment": "production", "app": "database"}'),
('router-01', '192.168.1.1', 'router', 'us-east-1', ARRAY[100], '{"role": "gateway"}'),
('firewall-01', '192.168.1.2', 'firewall', 'us-east-1', ARRAY[100], '{"vendor": "palo-alto"}');

-- Insert sample security rules
INSERT INTO tb_security_rules (name, source_cidr, destination_cidr, port, protocol, action, priority) VALUES
('Allow HTTP from anywhere', '0.0.0.0/0', '192.168.1.0/24', 80, 'tcp', 'allow', 100),
('Allow HTTPS from anywhere', '0.0.0.0/0', '192.168.1.0/24', 443, 'tcp', 'allow', 101),
('Allow SSH from office', '203.0.113.0/24', '192.168.1.0/24', 22, 'tcp', 'allow', 200),
('Deny all others', '0.0.0.0/0', '0.0.0.0/0', 0, 'tcp', 'deny', 999);
"""

# =============================================================================
# Running the Example
# =============================================================================
if __name__ == "__main__":
    import uvicorn

    from fraiseql.fastapi import create_app

    # Create FastAPI app with FraiseQL
    fastapi_app = create_app(app, database_url="postgresql://localhost/infrastructure")

    print("Starting FraiseQL Specialized Types Example...")
    print("This example demonstrates:")
    print("  ✅ IPv4/IPv6 address types with validation")
    print("  ✅ CIDR notation support")
    print("  ✅ Network operators (subnet checks, private IP detection)")
    print("  ✅ JSONB for flexible metadata")
    print("  ✅ Array types with containment operations")
    print()
    print("Open http://localhost:8000/graphql to try queries like:")
    print('  - devices_in_subnet(cidr: "192.168.1.0/24")')
    print("  - private_ip_devices")
    print('  - device_by_ip(ip_address: "192.168.1.10")')

    uvicorn.run(fastapi_app, host="0.0.0.0", port=8000)
