-- Example SQL schema for context parameters demo
-- This shows how to create PostgreSQL functions that work with FraiseQL context parameters

-- Create the mutation result type
CREATE TYPE app.mutation_result AS (
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);

-- Organizations table (for multi-tenancy)
CREATE TABLE IF NOT EXISTS organizations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Users table
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    email VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Locations table (tenant-isolated)
CREATE TABLE IF NOT EXISTS locations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    created_by UUID NOT NULL REFERENCES users(id),
    updated_by UUID REFERENCES users(id),
    name VARCHAR(255) NOT NULL,
    address TEXT NOT NULL,
    latitude NUMERIC(10, 8),
    longitude NUMERIC(11, 8),
    active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

-- Categories table (legacy single-parameter example)
CREATE TABLE IF NOT EXISTS categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id),
    created_by UUID NOT NULL REFERENCES users(id),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Create location function with context parameters
-- This function receives tenant_id and user_id as separate parameters
CREATE OR REPLACE FUNCTION app.create_location(
    input_pk_organization UUID,  -- Tenant ID from GraphQL context
    input_created_by UUID,       -- User ID from GraphQL context
    input_json JSONB             -- Business data from mutation input
) RETURNS app.mutation_result
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_location_id UUID;
    v_result app.mutation_result;
BEGIN
    -- Validate organization exists and is active
    IF NOT EXISTS (
        SELECT 1 FROM organizations
        WHERE id = input_pk_organization
        AND active = true
    ) THEN
        v_result.status := 'error';
        v_result.message := 'Organization not found or inactive';
        v_result.object_data := jsonb_build_object('code', 'INVALID_ORGANIZATION');
        RETURN v_result;
    END IF;

    -- Validate user belongs to organization
    IF NOT EXISTS (
        SELECT 1 FROM users
        WHERE id = input_created_by
        AND organization_id = input_pk_organization
        AND active = true
    ) THEN
        v_result.status := 'error';
        v_result.message := 'User not authorized for this organization';
        v_result.object_data := jsonb_build_object('code', 'UNAUTHORIZED_USER');
        RETURN v_result;
    END IF;

    -- Validate required fields
    IF input_json->>'name' IS NULL OR input_json->>'address' IS NULL THEN
        v_result.status := 'error';
        v_result.message := 'Name and address are required';
        v_result.object_data := jsonb_build_object('code', 'MISSING_REQUIRED_FIELDS');
        RETURN v_result;
    END IF;

    -- Create the location with proper tenant isolation
    INSERT INTO locations (
        id,
        organization_id,    -- Ensures tenant isolation
        created_by,         -- Audit trail
        name,
        address,
        latitude,
        longitude,
        created_at
    ) VALUES (
        gen_random_uuid(),
        input_pk_organization,  -- From context parameter
        input_created_by,       -- From context parameter
        input_json->>'name',
        input_json->>'address',
        COALESCE((input_json->>'latitude')::NUMERIC, NULL),
        COALESCE((input_json->>'longitude')::NUMERIC, NULL),
        NOW()
    ) RETURNING id INTO v_location_id;

    -- Return success result
    v_result.status := 'success';
    v_result.message := 'Location created successfully';
    v_result.object_data := jsonb_build_object(
        'location_id', v_location_id
    );

    RETURN v_result;

EXCEPTION
    WHEN OTHERS THEN
        v_result.status := 'error';
        v_result.message := 'Failed to create location: ' || SQLERRM;
        v_result.object_data := jsonb_build_object('code', 'DATABASE_ERROR');
        RETURN v_result;
END;
$$;

-- Update location function with context parameters
CREATE OR REPLACE FUNCTION app.update_location(
    input_pk_organization UUID,  -- Tenant ID from context
    input_updated_by UUID,       -- User ID from context
    input_json JSONB             -- Update data
) RETURNS app.mutation_result
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_location_id UUID;
    v_updated_fields TEXT[] := '{}';
    v_result app.mutation_result;
BEGIN
    -- Get location ID
    v_location_id := (input_json->>'id')::UUID;

    -- Validate location exists and belongs to organization
    IF NOT EXISTS (
        SELECT 1 FROM locations
        WHERE id = v_location_id
        AND organization_id = input_pk_organization
        AND active = true
    ) THEN
        v_result.status := 'error';
        v_result.message := 'Location not found or access denied';
        v_result.object_data := jsonb_build_object('code', 'LOCATION_NOT_FOUND');
        RETURN v_result;
    END IF;

    -- Update fields that are provided
    UPDATE locations SET
        name = COALESCE(input_json->>'name', name),
        address = COALESCE(input_json->>'address', address),
        latitude = COALESCE((input_json->>'latitude')::NUMERIC, latitude),
        longitude = COALESCE((input_json->>'longitude')::NUMERIC, longitude),
        updated_by = input_updated_by,
        updated_at = NOW()
    WHERE id = v_location_id;

    -- Track which fields were updated
    IF input_json ? 'name' THEN v_updated_fields := array_append(v_updated_fields, 'name'); END IF;
    IF input_json ? 'address' THEN v_updated_fields := array_append(v_updated_fields, 'address'); END IF;
    IF input_json ? 'latitude' THEN v_updated_fields := array_append(v_updated_fields, 'latitude'); END IF;
    IF input_json ? 'longitude' THEN v_updated_fields := array_append(v_updated_fields, 'longitude'); END IF;

    -- Return success result
    v_result.status := 'success';
    v_result.message := 'Location updated successfully';
    v_result.object_data := jsonb_build_object(
        'location_id', v_location_id,
        'updated_fields', to_jsonb(v_updated_fields)
    );

    RETURN v_result;

EXCEPTION
    WHEN OTHERS THEN
        v_result.status := 'error';
        v_result.message := 'Failed to update location: ' || SQLERRM;
        v_result.object_data := jsonb_build_object('code', 'DATABASE_ERROR');
        RETURN v_result;
END;
$$;

-- Delete location function with context parameters
CREATE OR REPLACE FUNCTION app.delete_location(
    input_pk_organization UUID,  -- Tenant ID from context
    input_deleted_by UUID,       -- User ID from context
    input_json JSONB             -- Contains location ID
) RETURNS app.mutation_result
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_location_id UUID;
    v_result app.mutation_result;
BEGIN
    v_location_id := (input_json->>'id')::UUID;

    -- Validate location exists and belongs to organization
    IF NOT EXISTS (
        SELECT 1 FROM locations
        WHERE id = v_location_id
        AND organization_id = input_pk_organization
        AND active = true
    ) THEN
        v_result.status := 'error';
        v_result.message := 'Location not found or access denied';
        v_result.object_data := jsonb_build_object('code', 'LOCATION_NOT_FOUND');
        RETURN v_result;
    END IF;

    -- Soft delete the location (preserve audit trail)
    UPDATE locations SET
        active = false,
        updated_by = input_deleted_by,
        updated_at = NOW()
    WHERE id = v_location_id;

    -- Return success result
    v_result.status := 'success';
    v_result.message := 'Location deleted successfully';
    v_result.object_data := jsonb_build_object(
        'location_id', v_location_id
    );

    RETURN v_result;

EXCEPTION
    WHEN OTHERS THEN
        v_result.status := 'error';
        v_result.message := 'Failed to delete location: ' || SQLERRM;
        v_result.object_data := jsonb_build_object('code', 'DATABASE_ERROR');
        RETURN v_result;
END;
$$;

-- Legacy single-parameter function (for comparison)
-- This function expects tenant_id and user_id in the JSONB payload
CREATE OR REPLACE FUNCTION app.create_category(
    input_data JSONB  -- All data including context in single parameter
) RETURNS app.mutation_result
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_organization_id UUID;
    v_created_by UUID;
    v_category_id UUID;
    v_result app.mutation_result;
BEGIN
    -- Extract context from the input data (not ideal)
    v_organization_id := (input_data->>'tenant_id')::UUID;
    v_created_by := (input_data->>'created_by')::UUID;

    -- Validate organization
    IF NOT EXISTS (
        SELECT 1 FROM organizations
        WHERE id = v_organization_id
        AND active = true
    ) THEN
        v_result.status := 'error';
        v_result.message := 'Organization not found';
        RETURN v_result;
    END IF;

    -- Create category
    INSERT INTO categories (
        organization_id,
        created_by,
        name,
        description
    ) VALUES (
        v_organization_id,
        v_created_by,
        input_data->>'name',
        input_data->>'description'
    ) RETURNING id INTO v_category_id;

    v_result.status := 'success';
    v_result.message := 'Category created successfully';
    v_result.object_data := jsonb_build_object('category_id', v_category_id);

    RETURN v_result;

EXCEPTION
    WHEN OTHERS THEN
        v_result.status := 'error';
        v_result.message := 'Failed to create category: ' || SQLERRM;
        RETURN v_result;
END;
$$;

-- Create some sample data for testing
INSERT INTO organizations (id, name) VALUES
    ('550e8400-e29b-41d4-a716-446655440000', 'Acme Corporation'),
    ('550e8400-e29b-41d4-a716-446655440001', 'Widget Inc')
ON CONFLICT (id) DO NOTHING;

INSERT INTO users (id, organization_id, email, name) VALUES
    ('550e8400-e29b-41d4-a716-446655440010', '550e8400-e29b-41d4-a716-446655440000', 'admin@acme.com', 'John Admin'),
    ('550e8400-e29b-41d4-a716-446655440011', '550e8400-e29b-41d4-a716-446655440000', 'user@acme.com', 'Jane User'),
    ('550e8400-e29b-41d4-a716-446655440020', '550e8400-e29b-41d4-a716-446655440001', 'admin@widget.com', 'Bob Admin')
ON CONFLICT (id) DO NOTHING;
