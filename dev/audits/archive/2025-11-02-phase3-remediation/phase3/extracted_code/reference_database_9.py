# Extracted from: docs/reference/database.md
# Block number: 9
# Forward pagination
result = await db.paginate("v_user", first=20)

# With cursor
result = await db.paginate("v_user", first=20, after="cursor_xyz")

# Backward pagination
result = await db.paginate("v_user", last=10, before="cursor_abc")

# With filters
result = await db.paginate("v_user", first=20, filters={"is_active": True}, order_by="created_at")

# Convert to typed Connection
from fraiseql.types import create_connection

connection = create_connection(result, User)
