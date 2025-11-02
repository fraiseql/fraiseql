# Extracted from: docs/reference/database.md
# Block number: 4
# Simple query
users = await db.find("v_user")

# With filter
active_users = await db.find("v_user", where={"is_active": True})

# With limit and offset
page_users = await db.find("v_user", limit=20, offset=40)

# With ordering
sorted_users = await db.find("v_user", order_by="created_at DESC")

# Complex filter (dict-based)
filtered_users = await db.find(
    "v_user", where={"name__icontains": "john", "created_at__gte": datetime(2025, 1, 1)}
)

# Using typed WhereInput
from fraiseql.types import UserWhere

filtered_users = await db.find(
    "v_user", where=UserWhere(name={"contains": "john"}, created_at={"gte": datetime(2025, 1, 1)})
)
