# Extracted from: docs/reference/database.md
# Block number: 31
# Use specific fields instead of SELECT *
users = await db.find("v_user", where={"is_active": True}, limit=100)

# Use pagination for large datasets
result = await db.paginate("v_user", first=50)

# Use database views for complex queries
# Create view: CREATE VIEW v_user_stats AS SELECT ...
stats = await db.find("v_user_stats")
