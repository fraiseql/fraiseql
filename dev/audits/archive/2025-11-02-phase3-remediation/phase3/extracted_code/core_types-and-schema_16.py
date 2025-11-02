# Extracted from: docs/core/types-and-schema.md
# Block number: 16
from fraiseql import type
from fraiseql.types import CIDR, Hostname, IpAddress, LTree, MacAddress, Port


@type
class NetworkConfig:
    ip_address: IpAddress
    cidr_block: CIDR
    gateway: IpAddress
    mac_address: MacAddress
    port: Port
    hostname: Hostname


@type
class Category:
    path: LTree  # PostgreSQL ltree for hierarchical data
    name: str
