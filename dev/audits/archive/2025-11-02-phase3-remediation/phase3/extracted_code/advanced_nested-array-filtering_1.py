# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 1

from fraiseql.fields import fraise_field
from fraiseql.nested_array_filters import (
    auto_nested_array_filters,
    nested_array_filterable,
    register_nested_array_filter,
)
from fraiseql.types import fraise_type


@fraise_type
class PrintServer:
    id: UUID
    hostname: str
    ip_address: str | None = None
    operating_system: str
    n_total_allocations: int = 0


# Option 1: Automatic detection (recommended)
@auto_nested_array_filters
@fraise_type
class NetworkConfiguration:
    id: UUID
    name: str
    print_servers: list[PrintServer] = fraise_field(default_factory=list)


# Option 2: Selective fields
@nested_array_filterable("print_servers", "dns_servers")
@fraise_type
class NetworkConfiguration:
    id: UUID
    name: str
    print_servers: list[PrintServer] = fraise_field(default_factory=list)
    dns_servers: list[DnsServer] = fraise_field(default_factory=list)


# Option 3: Manual registration (maximum control)
@fraise_type
class NetworkConfiguration:
    id: UUID
    name: str
    print_servers: list[PrintServer] = fraise_field(default_factory=list)


register_nested_array_filter(NetworkConfiguration, "print_servers", PrintServer)
