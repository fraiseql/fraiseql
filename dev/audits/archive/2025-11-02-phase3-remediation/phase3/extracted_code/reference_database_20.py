# Extracted from: docs/reference/database.md
# Block number: 20
# Simple query
results = await db.execute_raw("SELECT * FROM users")

# With parameters
results = await db.execute_raw("SELECT * FROM users WHERE id = $1", user_id)

# Complex aggregation
stats = await db.execute_raw(
    """
    SELECT
        count(*) as total_users,
        count(*) FILTER (WHERE is_active) as active_users
    FROM users
    WHERE created_at > $1
    """,
    datetime(2025, 1, 1),
)
