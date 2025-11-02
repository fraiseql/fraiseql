# Extracted from: docs/guides/nested-array-filtering.md
# Block number: 1
from typing import Optional
from uuid import UUID

import fraiseql
from fraiseql.fields import fraise_field


@fraiseql.type
class PrintServer:
    id: UUID
    hostname: str
    ip_address: Optional[str] = None
    operating_system: str
    n_total_allocations: int = 0


@fraiseql.type(sql_source="v_network", jsonb_column="data")
class NetworkConfiguration:
    id: UUID
    name: str
    # Enable where filtering on this field
    print_servers: list[PrintServer] = fraise_field(
        default_factory=list,
        supports_where_filtering=True,
        nested_where_type=PrintServer,
        description="Network print servers with optional filtering",
    )
