# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 5
from fraiseql.types import DateRange, IpAddress, LTree


@fraise_type(sql_source="network_devices")
@dataclass
class NetworkDevice:
    id: UUID
    ip_address: IpAddress  # Custom type hint
    path: LTree  # Hierarchical path
    availability: DateRange  # Date range
