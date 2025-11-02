# Extracted from: docs/enterprise/RBAC_POSTGRESQL_ASSESSMENT.md
# Block number: 2
# Automatic invalidation when roles change
# 1. User updates role via GraphQL mutation
# 2. PostgreSQL function fn_assign_role() executes
# 3. Database trigger increments "user_role" domain version
# 4. All user permission caches auto-invalidate
# 5. Next query fetches fresh permissions

# CASCADE invalidation
# 1. Admin modifies a role's permissions
# 2. "role" domain version increments
# 3. CASCADE rule triggers "user_permissions" invalidation
# 4. All users with that role get fresh permissions
