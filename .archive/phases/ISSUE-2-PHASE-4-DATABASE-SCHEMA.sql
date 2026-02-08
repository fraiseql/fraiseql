-- Phase 4: Row-Level Authorization Database Schema
-- Created for FraiseQL v1.9.1
-- Issue #2: Row-Level Access Control Middleware

-- Create row-level access constraint table for storing row-level access rules
-- This table defines which rows users with specific roles can access
-- Named tb_row_constraint following FraiseQL framework table naming conventions

CREATE TABLE IF NOT EXISTS tb_row_constraint (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    table_name VARCHAR NOT NULL,
    role_id UUID NOT NULL,
    constraint_type VARCHAR NOT NULL CHECK (constraint_type IN ('ownership', 'tenant', 'expression')),
    field_name VARCHAR,                 -- For ownership/tenant constraints (e.g., 'owner_id', 'tenant_id')
    expression VARCHAR,                 -- For custom expression constraints (e.g., "status = 'published'")
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW(),

    -- Foreign key to roles table
    FOREIGN KEY (role_id) REFERENCES roles(id) ON DELETE CASCADE,

    -- Ensure unique constraints per table+role+type combination
    UNIQUE(table_name, role_id, constraint_type),

    -- Index for fast lookup by table and role
    INDEX idx_tb_row_constraint_table_role (table_name, role_id),
    INDEX idx_tb_row_constraint_role (role_id),
    INDEX idx_tb_row_constraint_table (table_name)
);

-- Create audit table for row constraint changes
CREATE TABLE IF NOT EXISTS tb_row_constraint_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    constraint_id UUID,
    user_id UUID,
    action VARCHAR NOT NULL CHECK (action IN ('CREATE', 'UPDATE', 'DELETE')),
    old_values JSONB,
    new_values JSONB,
    created_at TIMESTAMP DEFAULT NOW(),

    FOREIGN KEY (constraint_id) REFERENCES tb_row_constraint(id) ON DELETE SET NULL,
    INDEX idx_tb_row_constraint_audit_user (user_id),
    INDEX idx_tb_row_constraint_audit_created (created_at)
);

-- Example: Insert sample row constraints for demonstration
-- NOTE: Only add these if you want to test row-level auth. Remove for production.

-- Example 1: Admin role can see all rows (no constraint = no WHERE filter)
-- INSERT INTO tb_row_constraint (table_name, role_id, constraint_type)
-- VALUES ('documents', (SELECT id FROM roles WHERE name = 'admin'), 'ownership')
-- ON CONFLICT (table_name, role_id, constraint_type) DO NOTHING;

-- Example 2: Manager role can only see tenant's rows
-- INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, field_name)
-- VALUES ('documents', (SELECT id FROM roles WHERE name = 'manager'), 'tenant', 'tenant_id')
-- ON CONFLICT (table_name, role_id, constraint_type) DO NOTHING;

-- Example 3: User role can only see their own rows
-- INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, field_name)
-- VALUES ('documents', (SELECT id FROM roles WHERE name = 'user'), 'ownership', 'owner_id')
-- ON CONFLICT (table_name, role_id, constraint_type) DO NOTHING;

-- Example 4: Analyst role can see published docs in their tenant (complex expression)
-- INSERT INTO tb_row_constraint (table_name, role_id, constraint_type, expression)
-- VALUES ('documents', (SELECT id FROM roles WHERE name = 'analyst'), 'expression', 'status = ''published'' AND tenant_id = :user_tenant_id')
-- ON CONFLICT (table_name, role_id, constraint_type) DO NOTHING;

-- Create schema version tracking
CREATE TABLE IF NOT EXISTS schema_versions (
    id SERIAL PRIMARY KEY,
    module_name VARCHAR NOT NULL UNIQUE,
    version VARCHAR NOT NULL,
    applied_at TIMESTAMP DEFAULT NOW()
);

-- Track this schema version
INSERT INTO schema_versions (module_name, version)
VALUES ('row_constraints', '1.0')
ON CONFLICT (module_name) DO UPDATE SET version = '1.0', applied_at = NOW();
