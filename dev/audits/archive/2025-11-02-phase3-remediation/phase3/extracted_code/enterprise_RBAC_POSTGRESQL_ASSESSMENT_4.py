# Extracted from: docs/enterprise/RBAC_POSTGRESQL_ASSESSMENT.md
# Block number: 4
# Register domains for RBAC tables
await cache.setup_table_trigger("roles", domain_name="role")
await cache.setup_table_trigger("permissions", domain_name="permission")
await cache.setup_table_trigger("role_permissions", domain_name="role_permission")
await cache.setup_table_trigger("user_roles", domain_name="user_role")

# Register CASCADE rules
# When roles change, invalidate user permissions
await cache.register_cascade_rule("role", "user_permissions")
await cache.register_cascade_rule("role_permission", "user_permissions")
await cache.register_cascade_rule("user_role", "user_permissions")
