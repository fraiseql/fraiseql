# Extracted from: docs/reference/database.md
# Block number: 18
# With tenant isolation
result = await db.execute_function_with_context(
    "app.create_location", [tenant_id, user_id], {"name": "Office", "address": "123 Main St"}
)

# Function signature in PostgreSQL
# CREATE FUNCTION app.create_location(
#     p_tenant_id uuid,
#     p_user_id uuid,
#     input jsonb
# ) RETURNS jsonb
