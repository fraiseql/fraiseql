# Extracted from: docs/guides/nested-array-filtering.md
# Block number: 2
from datetime import datetime
from enum import Enum
from typing import Optional
from uuid import UUID

import fraiseql
from fraiseql.fields import fraise_field


# Define enums
@fraiseql.enum
class ServerStatus(str, Enum):
    ACTIVE = "active"
    MAINTENANCE = "maintenance"
    OFFLINE = "offline"


# Define nested types
@fraiseql.type
class Server:
    id: UUID
    hostname: str
    ip_address: Optional[str] = None
    status: ServerStatus = ServerStatus.ACTIVE
    last_check: datetime
    cpu_usage: float
    memory_gb: int


@fraiseql.type(sql_source="v_datacenter", jsonb_column="data")
class Datacenter:
    id: UUID
    name: str
    location: str

    # Enable where filtering
    servers: list[Server] = fraise_field(
        default_factory=list,
        supports_where_filtering=True,
        nested_where_type=Server,
        description="Servers in this datacenter",
    )


# Define query
@fraiseql.query
async def datacenter(id: UUID) -> Datacenter:
    """Get datacenter by ID."""
    # Your implementation here
