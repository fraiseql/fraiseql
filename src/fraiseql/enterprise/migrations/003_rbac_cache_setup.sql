-- RBAC Cache Setup Migration
-- Sets up PostgreSQL-native caching domains and CASCADE rules for automatic invalidation
-- Requires pg_fraiseql_cache extension

-- Setup table triggers for automatic cache invalidation
-- These triggers automatically increment domain versions when RBAC tables change

-- Domain for roles table
SELECT fraiseql_cache.setup_table_invalidation('roles', 'role', 'tenant_id');

-- Domain for permissions table (no tenant_id - global)
SELECT fraiseql_cache.setup_table_invalidation('permissions', 'permission', NULL);

-- Domain for role_permissions table (no tenant_id - inherits from role)
SELECT fraiseql_cache.setup_table_invalidation('role_permissions', 'role_permission', NULL);

-- Domain for user_roles table
SELECT fraiseql_cache.setup_table_invalidation('user_roles', 'user_role', 'tenant_id');

-- CASCADE rules: when RBAC tables change, invalidate user permissions
-- This ensures user permission caches are invalidated when roles/permissions change

INSERT INTO fraiseql_cache.cascade_rules (source_domain, target_domain, rule_type) VALUES
    ('role', 'user_permissions', 'invalidate'),
    ('permission', 'user_permissions', 'invalidate'),
    ('role_permission', 'user_permissions', 'invalidate'),
    ('user_role', 'user_permissions', 'invalidate')
ON CONFLICT (source_domain, target_domain) DO NOTHING;
