# Extracted from: docs/reference/database.md
# Block number: 6
# Find by ID
user = await db.find_one("v_user", where={"id": user_id})

# Using kwargs
user = await db.find_one("v_user", id=user_id)

# Find with complex filter
user = await db.find_one("v_user", where={"email": "user@example.com", "is_active": True})

# Returns None if not found
user = await db.find_one("v_user", where={"id": "nonexistent"})
if user is None:
    raise GraphQLError("User not found")
