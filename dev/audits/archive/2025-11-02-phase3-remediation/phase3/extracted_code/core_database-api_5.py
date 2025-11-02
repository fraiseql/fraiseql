# Extracted from: docs/core/database-api.md
# Block number: 5
from fraiseql.db.pagination import (
    OrderByInstruction,
    OrderByInstructions,
    OrderDirection,
    PaginationInput,
)

from fraiseql.db import PsycopgRepository, QueryOptions

repo = PsycopgRepository(connection_pool)

options = QueryOptions(
    filters={"status": "active", "created_at__min": "2024-01-01", "price__max": 100.00},
    order_by=OrderByInstructions(
        instructions=[OrderByInstruction(field="created_at", direction=OrderDirection.DESC)]
    ),
    pagination=PaginationInput(limit=50, offset=0),
)

data, total = await repo.select_from_json_view(
    tenant_id=tenant_id, view_name="v_orders", options=options
)

print(f"Retrieved {len(data)} orders out of {total} total")
for order in data:
    print(f"Order {order['id']}: {order['status']}")
