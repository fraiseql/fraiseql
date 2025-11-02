# Extracted from: docs/core/database-api.md
# Block number: 50
import uuid

from fraiseql.db.pagination import (
    OrderByInstruction,
    OrderByInstructions,
    OrderDirection,
    PaginationInput,
)
from psycopg_pool import AsyncConnectionPool

from fraiseql.db import PsycopgRepository, QueryOptions

# Initialize repository
pool = AsyncConnectionPool(conninfo="postgresql://localhost/mydb", min_size=5, max_size=20)
repo = PsycopgRepository(pool)

# Query with filtering, pagination, and ordering
tenant_id = uuid.uuid4()
options = QueryOptions(
    filters={
        "status__in": ["active", "pending"],
        "created_at__min": "2024-01-01",
        "total_amount__min": 100.00,
    },
    order_by=OrderByInstructions(
        instructions=[OrderByInstruction(field="created_at", direction=OrderDirection.DESC)]
    ),
    pagination=PaginationInput(limit=20, offset=0),
)

data, total = await repo.select_from_json_view(
    tenant_id=tenant_id, view_name="v_orders", options=options
)

print(f"Retrieved {len(data)} of {total} orders")
for order in data:
    print(f"Order {order['id']}: ${order['total_amount']}")
