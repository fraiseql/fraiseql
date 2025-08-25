-- Blog Demo Enterprise Multi-Tenant Setup
-- Core configuration for multi-tenant blog hosting platform

-- Create schemas for different domains
CREATE SCHEMA IF NOT EXISTS management;  -- Organizations, subscriptions, platform-level data
CREATE SCHEMA IF NOT EXISTS tenant;      -- Tenant-isolated data (users, posts, comments, tags)
CREATE SCHEMA IF NOT EXISTS app;         -- Application functions and procedures
CREATE SCHEMA IF NOT EXISTS public;      -- Public views and interfaces

-- Enable Row Level Security globally
-- This will be applied to tenant-specific tables
-- ALTER DEFAULT PRIVILEGES will be set per table

-- Create function to get current tenant context
-- This will be set by the application layer
CREATE OR REPLACE FUNCTION current_tenant_id() RETURNS UUID AS $$
BEGIN
    RETURN COALESCE(
        current_setting('app.tenant_id', true)::UUID,
        '00000000-0000-0000-0000-000000000000'::UUID
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Create function to set tenant context
CREATE OR REPLACE FUNCTION set_tenant_context(tenant_id UUID) RETURNS VOID AS $$
BEGIN
    PERFORM set_config('app.tenant_id', tenant_id::TEXT, false);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Create function to validate tenant access
CREATE OR REPLACE FUNCTION validate_tenant_access(required_tenant_id UUID) RETURNS BOOLEAN AS $$
BEGIN
    RETURN current_tenant_id() = required_tenant_id;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
