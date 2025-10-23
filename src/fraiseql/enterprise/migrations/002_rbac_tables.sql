-- RBAC Tables Migration
-- Implements hierarchical role-based access control with PostgreSQL-native caching

-- Roles table with hierarchy support
CREATE TABLE IF NOT EXISTS roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    description TEXT,
    parent_role_id UUID REFERENCES roles(id) ON DELETE SET NULL,
    tenant_id UUID,  -- NULL for global roles
    is_system BOOLEAN DEFAULT FALSE,  -- System roles can't be deleted
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(name, tenant_id)  -- Unique per tenant
);

-- Permissions catalog
CREATE TABLE IF NOT EXISTS permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource VARCHAR(100) NOT NULL,  -- e.g., 'user', 'product', 'order'
    action VARCHAR(50) NOT NULL,     -- e.g., 'create', 'read', 'update', 'delete'
    description TEXT,
    constraints JSONB,  -- Optional constraints (e.g., {"own_data_only": true})
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(resource, action)
);

-- Role-Permission mapping (many-to-many)
CREATE TABLE IF NOT EXISTS role_permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    granted BOOLEAN DEFAULT TRUE,  -- TRUE = grant, FALSE = revoke (explicit deny)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(role_id, permission_id)
);

-- User-Role assignment
CREATE TABLE IF NOT EXISTS user_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,  -- References users table
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    tenant_id UUID,  -- Scoped to tenant
    granted_by UUID,  -- User who granted this role
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,  -- Optional expiration
    UNIQUE(user_id, role_id, tenant_id)
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_roles_parent ON roles(parent_role_id);
CREATE INDEX IF NOT EXISTS idx_roles_tenant ON roles(tenant_id);
CREATE INDEX IF NOT EXISTS idx_user_roles_user ON user_roles(user_id, tenant_id);
CREATE INDEX IF NOT EXISTS idx_user_roles_role ON user_roles(role_id);
CREATE INDEX IF NOT EXISTS idx_role_permissions_role ON role_permissions(role_id);

-- Function to compute role hierarchy (recursive)
CREATE OR REPLACE FUNCTION get_inherited_roles(p_role_id UUID)
RETURNS TABLE(role_id UUID, depth INT) AS $$
    WITH RECURSIVE role_hierarchy AS (
        -- Base case: the role itself
        SELECT id as role_id, 0 as depth
        FROM roles
        WHERE id = p_role_id

        UNION ALL

        -- Recursive case: parent roles
        SELECT r.parent_role_id as role_id, rh.depth + 1 as depth
        FROM roles r
        INNER JOIN role_hierarchy rh ON r.id = rh.role_id
        WHERE r.parent_role_id IS NOT NULL
        AND rh.depth < 10  -- Prevent infinite loops
    )
    SELECT DISTINCT role_id, MIN(depth) as depth
    FROM role_hierarchy
    WHERE role_id IS NOT NULL
    GROUP BY role_id
    ORDER BY depth;
$$ LANGUAGE SQL STABLE;

-- Seed common system roles
INSERT INTO roles (id, name, description, parent_role_id, is_system) VALUES
    ('00000000-0000-0000-0000-000000000001', 'super_admin', 'Full system access', NULL, TRUE),
    ('00000000-0000-0000-0000-000000000002', 'admin', 'Tenant administrator', '00000000-0000-0000-0000-000000000001', TRUE),
    ('00000000-0000-0000-0000-000000000003', 'manager', 'Department manager', '00000000-0000-0000-0000-000000000002', TRUE),
    ('00000000-0000-0000-0000-000000000004', 'user', 'Standard user', '00000000-0000-0000-0000-000000000003', TRUE),
    ('00000000-0000-0000-0000-000000000005', 'viewer', 'Read-only access', '00000000-0000-0000-0000-000000000004', TRUE)
ON CONFLICT (id) DO NOTHING;

-- Seed common permissions
INSERT INTO permissions (resource, action, description) VALUES
    ('user', 'create', 'Create new users'),
    ('user', 'read', 'View user data'),
    ('user', 'update', 'Modify user data'),
    ('user', 'delete', 'Delete users'),
    ('role', 'assign', 'Assign roles to users'),
    ('role', 'create', 'Create new roles'),
    ('audit', 'read', 'View audit logs'),
    ('settings', 'update', 'Modify system settings')
ON CONFLICT (resource, action) DO NOTHING;
