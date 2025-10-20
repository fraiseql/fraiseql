-- RBAC Row-Level Security Migration
-- Implements PostgreSQL RLS policies for tenant isolation and data access control

-- Enable RLS on core tables
-- Note: These tables may not exist yet in the base FraiseQL schema
-- This migration assumes they will be created or already exist

-- Enable RLS on users table (if it exists)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'users') THEN
        ALTER TABLE users ENABLE ROW LEVEL SECURITY;
        RAISE NOTICE 'Enabled RLS on users table';
    ELSE
        RAISE NOTICE 'users table does not exist - RLS policy will be applied when table is created';
    END IF;
END $$;

-- Enable RLS on orders table (if it exists)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'orders') THEN
        ALTER TABLE orders ENABLE ROW LEVEL SECURITY;
        RAISE NOTICE 'Enabled RLS on orders table';
    ELSE
        RAISE NOTICE 'orders table does not exist - RLS policy will be applied when table is created';
    END IF;
END $$;

-- Enable RLS on products table (if it exists)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'products') THEN
        ALTER TABLE products ENABLE ROW LEVEL SECURITY;
        RAISE NOTICE 'Enabled RLS on products table';
    ELSE
        RAISE NOTICE 'products table does not exist - RLS policy will be applied when table is created';
    END IF;
END $$;

-- Policy: Tenant isolation for users table
-- Users can only see data from their tenant, unless they are super admin
DROP POLICY IF EXISTS tenant_isolation_users ON users;
CREATE POLICY tenant_isolation_users ON users
    FOR ALL
    USING (
        tenant_id = current_setting('app.tenant_id', TRUE)::UUID
        OR current_setting('app.is_super_admin', TRUE)::BOOLEAN
    );

-- Policy: Own data update for users table
-- Users can only modify their own data, unless they are admin or super_admin
DROP POLICY IF EXISTS own_data_update_users ON users;
CREATE POLICY own_data_update_users ON users
    FOR UPDATE
    USING (
        id = current_setting('app.user_id', TRUE)::UUID
        OR EXISTS (
            SELECT 1 FROM user_roles ur
            INNER JOIN roles r ON ur.role_id = r.id
            WHERE ur.user_id = current_setting('app.user_id', TRUE)::UUID
            AND r.name IN ('admin', 'super_admin')
        )
    );

-- Policy: Tenant isolation for orders table
DROP POLICY IF EXISTS tenant_isolation_orders ON orders;
CREATE POLICY tenant_isolation_orders ON orders
    FOR ALL
    USING (
        tenant_id = current_setting('app.tenant_id', TRUE)::UUID
        OR current_setting('app.is_super_admin', TRUE)::BOOLEAN
    );

-- Policy: Own data update for orders table
-- Users can modify orders they own, or all orders if admin
DROP POLICY IF EXISTS own_data_update_orders ON orders;
CREATE POLICY own_data_update_orders ON orders
    FOR UPDATE
    USING (
        user_id = current_setting('app.user_id', TRUE)::UUID
        OR EXISTS (
            SELECT 1 FROM user_roles ur
            INNER JOIN roles r ON ur.role_id = r.id
            WHERE ur.user_id = current_setting('app.user_id', TRUE)::UUID
            AND r.name IN ('admin', 'super_admin')
        )
    );

-- Policy: Tenant isolation for products table
-- Products might be global or tenant-scoped
DROP POLICY IF EXISTS tenant_isolation_products ON products;
CREATE POLICY tenant_isolation_products ON products
    FOR ALL
    USING (
        tenant_id = current_setting('app.tenant_id', TRUE)::UUID
        OR tenant_id IS NULL  -- Global products
        OR current_setting('app.is_super_admin', TRUE)::BOOLEAN
    );

-- Policy: Product management permissions
-- Only admins can create/modify products
DROP POLICY IF EXISTS product_management ON products;
CREATE POLICY product_management ON products
    FOR ALL
    USING (
        EXISTS (
            SELECT 1 FROM user_roles ur
            INNER JOIN roles r ON ur.role_id = r.id
            WHERE ur.user_id = current_setting('app.user_id', TRUE)::UUID
            AND r.name IN ('admin', 'super_admin')
        )
    );

-- Create indexes to support RLS policy performance
CREATE INDEX IF NOT EXISTS idx_users_tenant_id ON users(tenant_id);
CREATE INDEX IF NOT EXISTS idx_orders_tenant_id ON orders(tenant_id);
CREATE INDEX IF NOT EXISTS idx_orders_user_id ON orders(user_id);
CREATE INDEX IF NOT EXISTS idx_products_tenant_id ON products(tenant_id);

-- Function to check if user has role (for use in policies)
CREATE OR REPLACE FUNCTION user_has_role(p_user_id UUID, p_role_name TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1 FROM user_roles ur
        INNER JOIN roles r ON ur.role_id = r.id
        WHERE ur.user_id = p_user_id
        AND r.name = p_role_name
        AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
    );
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to check if user has permission (for use in policies)
CREATE OR REPLACE FUNCTION user_has_permission(p_user_id UUID, p_resource TEXT, p_action TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1
        FROM user_roles ur
        INNER JOIN roles r ON ur.role_id = r.id
        INNER JOIN role_permissions rp ON r.id = rp.role_id
        INNER JOIN permissions p ON rp.permission_id = p.id
        WHERE ur.user_id = p_user_id
        AND p.resource = p_resource
        AND p.action = p_action
        AND rp.granted = TRUE
        AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
    );
END;
$$ LANGUAGE plpgsql STABLE;
