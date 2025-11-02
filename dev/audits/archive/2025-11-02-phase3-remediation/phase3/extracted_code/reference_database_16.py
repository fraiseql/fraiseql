# Extracted from: docs/reference/database.md
# Block number: 16
# Execute mutation function
result = await db.execute_function(
    "graphql.create_user", {"name": "John", "email": "john@example.com"}
)

# With schema prefix
result = await db.execute_function(
    "auth.register_user", {"email": "user@example.com", "password": "secret"}
)
