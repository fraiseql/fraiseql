-- Phase 11.6 Cycle 3: RBAC Schema
-- Role-based access control with roles, permissions, and assignments

-- Roles table (tenant-specific)
CREATE TABLE IF NOT EXISTS roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    level INT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

-- Indexes on roles
CREATE INDEX IF NOT EXISTS idx_roles_tenant_id ON roles(tenant_id);
CREATE INDEX IF NOT EXISTS idx_roles_name ON roles(name);
CREATE INDEX IF NOT EXISTS idx_roles_tenant_name ON roles(tenant_id, name);

-- Global permissions table (not tenant-specific)
CREATE TABLE IF NOT EXISTS permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource VARCHAR(255) NOT NULL,
    action VARCHAR(255) NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(resource, action)
);

-- Indexes on permissions
CREATE INDEX IF NOT EXISTS idx_permissions_resource ON permissions(resource);
CREATE INDEX IF NOT EXISTS idx_permissions_resource_action ON permissions(resource, action);

-- Role-Permission assignment (many-to-many junction table)
CREATE TABLE IF NOT EXISTS role_permissions (
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (role_id, permission_id)
);

-- Indexes on role_permissions
CREATE INDEX IF NOT EXISTS idx_role_permissions_role_id ON role_permissions(role_id);
CREATE INDEX IF NOT EXISTS idx_role_permissions_permission_id ON role_permissions(permission_id);

-- User-Role assignment (many-to-many junction table)
-- Assumes users table exists and has tenant_id column
DO $$
BEGIN
    IF EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'users') THEN
        IF NOT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'user_roles') THEN
            CREATE TABLE user_roles (
                user_id VARCHAR(255) NOT NULL,
                role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
                tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
                assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (user_id, role_id, tenant_id),
                UNIQUE(user_id, role_id)
            );

            -- Indexes on user_roles
            CREATE INDEX idx_user_roles_user_id ON user_roles(user_id);
            CREATE INDEX idx_user_roles_role_id ON user_roles(role_id);
            CREATE INDEX idx_user_roles_tenant_id ON user_roles(tenant_id);
            CREATE INDEX idx_user_roles_user_tenant ON user_roles(user_id, tenant_id);
        END IF;
    END IF;
END
$$;

-- Pre-populate default permissions for all system resources
INSERT INTO permissions (resource, action, description)
VALUES
    ('query', 'read', 'Execute GraphQL read queries'),
    ('mutation', 'write', 'Execute GraphQL mutations'),
    ('admin', 'read', 'Access admin read endpoints'),
    ('admin', 'write', 'Access admin write endpoints'),
    ('audit', 'read', 'Query audit logs'),
    ('audit', 'write', 'Create audit log entries'),
    ('rbac', 'read', 'View RBAC configuration'),
    ('rbac', 'write', 'Modify RBAC configuration'),
    ('cache', 'read', 'View cache status'),
    ('cache', 'write', 'Clear cache'),
    ('config', 'read', 'View server configuration'),
    ('config', 'write', 'Update server configuration'),
    ('federation', 'read', 'Access federation operations'),
    ('federation', 'write', 'Perform federation mutations')
ON CONFLICT DO NOTHING;
