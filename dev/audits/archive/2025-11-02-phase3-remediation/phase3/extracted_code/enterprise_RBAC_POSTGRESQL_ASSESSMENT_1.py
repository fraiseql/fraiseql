# Extracted from: docs/enterprise/RBAC_POSTGRESQL_ASSESSMENT.md
# Block number: 1
# Domain versioning - auto-invalidate when roles change
await cache.get_domain_versions(tenant_id, ["role", "permission", "user_role"])

# CASCADE rules - when roles change, invalidate user permissions
await cache.register_cascade_rule("role", "user_permissions")

# Table triggers - auto-invalidate on INSERT/UPDATE/DELETE
await cache.setup_table_trigger("roles", domain_name="role")
await cache.setup_table_trigger("user_roles", domain_name="user_role")
