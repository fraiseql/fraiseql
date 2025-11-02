# Extracted from: docs/core/types-and-schema.md
# Block number: 5
from uuid import UUID

from fraiseql import type


@type(sql_source="tv_machine", jsonb_column="machine_data")
class Machine:
    id: UUID
    identifier: str
    serial_number: str
