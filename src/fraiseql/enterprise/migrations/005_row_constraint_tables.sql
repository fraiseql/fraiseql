-- Row-Level Authorization Constraints Migration
-- Implements table-level row-level access control using tb_row_constraint
-- Issue #2: Row-Level Access Control Middleware
-- Created for FraiseQL v1.9.1

-- Main row constraint table
-- Defines which rows users with specific roles can access based on table + role combination
CREATE TABLE IF NOT EXISTS tb_row_constraint (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    table_name VARCHAR NOT NULL,
    role_id UUID NOT NULL,
    constraint_type VARCHAR NOT NULL CHECK (constraint_type IN ('ownership', 'tenant', 'expression')),
    field_name VARCHAR,  -- For ownership/tenant constraints (e.g., 'owner_id', 'tenant_id')
    expression VARCHAR,  -- For custom expression constraints (future implementation)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE,
    UNIQUE (table_name, role_id, constraint_type)
);

-- Audit table for tracking row constraint changes
-- Records all CREATE/UPDATE/DELETE operations on row constraints for compliance
CREATE TABLE IF NOT EXISTS tb_row_constraint_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    constraint_id UUID REFERENCES tb_row_constraint (id) ON DELETE SET NULL,
    user_id UUID,  -- Who made the change
    action VARCHAR NOT NULL CHECK (action IN ('CREATE', 'UPDATE', 'DELETE')),
    old_values JSONB,  -- Previous constraint state
    new_values JSONB,  -- New constraint state
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for performance
-- Primary query: lookup constraints by table + role
CREATE INDEX IF NOT EXISTS idx_tb_row_constraint_table_role ON tb_row_constraint (table_name, role_id);

-- Secondary queries
CREATE INDEX IF NOT EXISTS idx_tb_row_constraint_role ON tb_row_constraint (role_id);
CREATE INDEX IF NOT EXISTS idx_tb_row_constraint_table ON tb_row_constraint (table_name);

-- Audit indexes
CREATE INDEX IF NOT EXISTS idx_tb_row_constraint_audit_constraint ON tb_row_constraint_audit (constraint_id);
CREATE INDEX IF NOT EXISTS idx_tb_row_constraint_audit_user ON tb_row_constraint_audit (user_id);
CREATE INDEX IF NOT EXISTS idx_tb_row_constraint_audit_created ON tb_row_constraint_audit (created_at);

-- Function to audit row constraint changes
-- Automatically called by triggers on INSERT/UPDATE/DELETE
CREATE OR REPLACE FUNCTION audit_row_constraint_change()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO tb_row_constraint_audit (constraint_id, user_id, action, new_values)
        VALUES (NEW.id, current_setting('app.user_id', TRUE)::UUID, 'CREATE', row_to_json(NEW));
        RETURN NEW;
    ELSIF TG_OP = 'UPDATE' THEN
        INSERT INTO tb_row_constraint_audit (constraint_id, user_id, action, old_values, new_values)
        VALUES (
            NEW.id,
            current_setting('app.user_id', TRUE)::UUID,
            'UPDATE',
            row_to_json(OLD),
            row_to_json(NEW)
        );
        RETURN NEW;
    ELSIF TG_OP = 'DELETE' THEN
        INSERT INTO tb_row_constraint_audit (constraint_id, user_id, action, old_values)
        VALUES (OLD.id, current_setting('app.user_id', TRUE)::UUID, 'DELETE', row_to_json(OLD));
        RETURN OLD;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Trigger to audit all row constraint modifications
DROP TRIGGER IF EXISTS tr_audit_row_constraint ON tb_row_constraint;
CREATE TRIGGER tr_audit_row_constraint
AFTER INSERT OR UPDATE OR DELETE ON tb_row_constraint
FOR EACH ROW
EXECUTE FUNCTION audit_row_constraint_change();

-- Function to get all row constraints for a user on a table
-- Used by Rust resolver for efficient constraint lookup
CREATE OR REPLACE FUNCTION get_user_row_constraints(
    p_user_id UUID,
    p_table_name VARCHAR,
    p_tenant_id UUID DEFAULT NULL
)
RETURNS TABLE (
    constraint_id UUID,
    table_name VARCHAR,
    role_id UUID,
    constraint_type VARCHAR,
    field_name VARCHAR,
    expression VARCHAR
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        rc.id,
        rc.table_name,
        rc.role_id,
        rc.constraint_type,
        rc.field_name,
        rc.expression
    FROM tb_row_constraint rc
    INNER JOIN user_roles ur ON rc.role_id = ur.role_id
    WHERE ur.user_id = p_user_id
        AND rc.table_name = p_table_name
        AND (p_tenant_id IS NULL OR ur.tenant_id = p_tenant_id)
        AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
    ORDER BY rc.constraint_type DESC
    LIMIT 1;  -- Return first applicable constraint (ownership > tenant > expression)
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to check if user has row-level constraint on table
-- Faster boolean check for permission validation
CREATE OR REPLACE FUNCTION user_has_row_constraint(
    p_user_id UUID,
    p_table_name VARCHAR
)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN EXISTS (
        SELECT 1
        FROM tb_row_constraint rc
        INNER JOIN user_roles ur ON rc.role_id = ur.role_id
        WHERE ur.user_id = p_user_id
            AND rc.table_name = p_table_name
            AND (ur.expires_at IS NULL OR ur.expires_at > NOW())
    );
END;
$$ LANGUAGE plpgsql STABLE;

-- Sample data: comment out for production, uncomment for development/testing
-- These examples demonstrate the three constraint types

-- Example 1: Ownership constraint - User can only see their own records
-- INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, field_name)
-- SELECT 'documents', id, 'ownership', 'owner_id'
-- FROM roles WHERE name = 'user'
-- ON CONFLICT (table_name, role_id, constraint_type) DO NOTHING;

-- Example 2: Tenant constraint - User can only see their tenant's records
-- INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, field_name)
-- SELECT 'documents', id, 'tenant', 'tenant_id'
-- FROM roles WHERE name = 'manager'
-- ON CONFLICT (table_name, role_id, constraint_type) DO NOTHING;

-- Example 3: No constraint - Admin can see all records
-- Admins have no row constraint, so they see all rows
-- (Constraint lookup returns NULL, no WHERE filter injected)

-- Track migration version
INSERT INTO schema_versions (module_name, version)
VALUES ('row_constraints', '1.0')
ON CONFLICT (module_name) DO UPDATE SET version = '1.0', applied_at = NOW();
