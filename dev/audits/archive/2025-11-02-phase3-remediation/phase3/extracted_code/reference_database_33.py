# Extracted from: docs/reference/database.md
# Block number: 33
# Always use parameterized queries
results = await db.execute_raw(
    "SELECT * FROM users WHERE email = $1",  # Safe
    email,
)

# NEVER do this (SQL injection risk):
# results = await db.execute_raw(f"SELECT * FROM users WHERE email = '{email}'")
