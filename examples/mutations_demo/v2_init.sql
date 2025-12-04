-- Updated init.sql using mutation_response format
-- Demonstrates the new standardized mutation result format

-- =====================================================
-- SETUP FOR V2 MUTATION FORMAT
-- =====================================================

-- Create schema for GraphQL functions
CREATE SCHEMA IF NOT EXISTS graphql;

-- Create app schema if it doesn't exist
CREATE SCHEMA IF NOT EXISTS app;

-- Create user table with JSONB data
CREATE TABLE IF NOT EXISTS tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create index on email for uniqueness checks
CREATE UNIQUE INDEX idx_tb_user_email ON tb_user ((data->>'email'));

-- =====================================================
-- IMPORT V2 MUTATION HELPERS
-- =====================================================

-- Include the mutation_response type and helper functions
-- (In a real setup, these would be in a separate migration file)

-- Create the mutation_response composite type
CREATE TYPE mutation_response AS (
    status          text,                    -- Status: 'success', 'new', 'updated', 'deleted', 'noop:*', 'failed:*'
    message         text,                    -- Human-readable message
    entity_id       text,                    -- Optional entity ID (for updates/deletes)
    entity_type     text,                    -- Optional entity type name (e.g., 'User', 'Post')
    entity          jsonb,                   -- The entity data (for success cases)
    updated_fields  text[],                  -- Fields that were updated (for partial updates)
    cascade         jsonb,                   -- Cascade data for side effects
    metadata        jsonb                    -- Additional metadata
);

-- Success result helpers
CREATE OR REPLACE FUNCTION mutation_success(
    message_text text,
    entity_data jsonb,
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    entity_id_val text;
BEGIN
    entity_id_val := entity_data->>'id';
    RETURN ROW(
        'success'::text, message_text, entity_id_val, entity_type_name,
        entity_data, NULL::text[], cascade_data, metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION mutation_created(
    message_text text,
    entity_data jsonb,
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    entity_id_val text;
BEGIN
    entity_id_val := entity_data->>'id';
    RETURN ROW(
        'new'::text, message_text, entity_id_val, entity_type_name,
        entity_data, NULL::text[], cascade_data, metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION mutation_updated(
    message_text text,
    entity_data jsonb,
    updated_fields_list text[],
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    entity_id_val text;
BEGIN
    entity_id_val := entity_data->>'id';
    RETURN ROW(
        'updated'::text, message_text, entity_id_val, entity_type_name,
        entity_data, updated_fields_list, cascade_data, metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION mutation_deleted(
    message_text text,
    entity_id_val text,
    entity_type_name text DEFAULT NULL,
    cascade_data jsonb DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
BEGIN
    RETURN ROW(
        'deleted'::text, message_text, entity_id_val, entity_type_name,
        NULL::jsonb, NULL::text[], cascade_data, metadata_data
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Error result helpers
CREATE OR REPLACE FUNCTION mutation_validation_error(
    message_text text,
    field_name text DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    error_metadata jsonb;
BEGIN
    IF field_name IS NOT NULL THEN
        error_metadata := jsonb_build_object('field', field_name, 'type', 'validation');
        IF metadata_data IS NOT NULL THEN
            error_metadata := error_metadata || metadata_data;
        END IF;
    ELSE
        error_metadata := metadata_data;
    END IF;
    RETURN ROW(
        'failed:validation'::text, message_text, NULL::text, NULL::text,
        NULL::jsonb, NULL::text[], NULL::jsonb, error_metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION mutation_not_found(
    resource_type text DEFAULT 'Resource',
    resource_id text DEFAULT NULL,
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    message_text text;
    error_metadata jsonb;
BEGIN
    IF resource_id IS NOT NULL THEN
        message_text := resource_type || ' with ID ' || resource_id || ' not found';
    ELSE
        message_text := resource_type || ' not found';
    END IF;
    error_metadata := jsonb_build_object('resource_type', resource_type);
    IF resource_id IS NOT NULL THEN
        error_metadata := error_metadata || jsonb_build_object('resource_id', resource_id);
    END IF;
    IF metadata_data IS NOT NULL THEN
        error_metadata := error_metadata || metadata_data;
    END IF;
    RETURN ROW(
        'failed:not_found'::text, message_text, NULL::text, NULL::text,
        NULL::jsonb, NULL::text[], NULL::jsonb, error_metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION mutation_conflict(
    message_text text,
    conflict_type text DEFAULT 'duplicate',
    metadata_data jsonb DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    error_metadata jsonb;
BEGIN
    error_metadata := jsonb_build_object('conflict_type', conflict_type);
    IF metadata_data IS NOT NULL THEN
        error_metadata := error_metadata || metadata_data;
    END IF;
    RETURN ROW(
        'failed:' || conflict_type, message_text, NULL::text, NULL::text,
        NULL::jsonb, NULL::text[], NULL::jsonb, error_metadata
    )::mutation_response;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- =====================================================
-- CASCADE HELPERS
-- =====================================================
-- Per GraphQL Cascade spec (02_cascade_model.md):
-- - UpdatedEntity: { type_name, id, operation, entity } -> Rust converts to __typename
-- - DeletedEntity: { type_name, id, deleted_at } -> Rust converts to deletedAt
-- - CascadeOperation: CREATED | UPDATED | DELETED
--
-- NOTE: SQL uses snake_case. Rust transforms to camelCase for GraphQL.

-- Create cascade data for entity creation (operation = CREATED)
CREATE OR REPLACE FUNCTION cascade_entity_created(
    entity_type text, entity_id text, entity_data jsonb DEFAULT NULL
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object('updated', jsonb_build_array(
        jsonb_build_object(
            'type_name', entity_type,
            'id', entity_id,
            'operation', 'CREATED',
            'entity', COALESCE(entity_data, '{}'::jsonb)
        )
    ));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create cascade data for entity updates (operation = UPDATED)
CREATE OR REPLACE FUNCTION cascade_entity_update(
    entity_type text, entity_id text, entity_data jsonb
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object('updated', jsonb_build_array(
        jsonb_build_object(
            'type_name', entity_type,
            'id', entity_id,
            'operation', 'UPDATED',
            'entity', entity_data
        )
    ));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create cascade data for count updates (convenience wrapper)
CREATE OR REPLACE FUNCTION cascade_count_update(
    entity_type text, entity_id text, field_name text,
    previous_val int, current_val int
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object('updated', jsonb_build_array(
        jsonb_build_object(
            'type_name', entity_type,
            'id', entity_id,
            'operation', 'UPDATED',
            'entity', jsonb_build_object(
                'id', entity_id,
                field_name, current_val,
                '_previous_' || field_name, previous_val
            )
        )
    ));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Create cascade data for entity deletion
CREATE OR REPLACE FUNCTION cascade_entity_deleted(
    entity_type text, entity_id text, deleted_at timestamptz DEFAULT NOW()
) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object('deleted', jsonb_build_array(
        jsonb_build_object(
            'type_name', entity_type,
            'id', entity_id,
            'deleted_at', to_jsonb(deleted_at)
        )
    ));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Merge cascade data
CREATE OR REPLACE FUNCTION cascade_merge(c1 jsonb, c2 jsonb) RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'updated', COALESCE(c1->'updated', '[]'::jsonb) || COALESCE(c2->'updated', '[]'::jsonb),
        'deleted', COALESCE(c1->'deleted', '[]'::jsonb) || COALESCE(c2->'deleted', '[]'::jsonb),
        'invalidations', COALESCE(c1->'invalidations', '[]'::jsonb) || COALESCE(c2->'invalidations', '[]'::jsonb)
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- =====================================================
-- V2 MUTATION FUNCTIONS
-- =====================================================

-- Create user function using v2 format with cascade
CREATE OR REPLACE FUNCTION graphql.create_user(input_data JSONB)
RETURNS mutation_response AS $$
DECLARE
    v_user_data jsonb;
    v_user_id uuid;
    v_cascade_data jsonb;
BEGIN
    -- Check if email already exists
    IF EXISTS (SELECT 1 FROM tb_user WHERE data->>'email' = input_data->>'email') THEN
        RETURN mutation_conflict('Email address already exists', 'duplicate',
            jsonb_build_object('field', 'email', 'value', input_data->>'email'));
    END IF;

    -- Create user
    v_user_id := gen_random_uuid();
    v_user_data := jsonb_build_object(
        'id', v_user_id,
        'name', input_data->>'name',
        'email', input_data->>'email',
        'role', COALESCE(input_data->>'role', 'user'),
        'created_at', to_jsonb(now())
    );

    INSERT INTO tb_user (id, data, created_at)
    VALUES (v_user_id, v_user_data, now());

    -- Create cascade data: update organization user count
    v_cascade_data := cascade_count_update(
        'Organization',
        'org-123',  -- Would be from input_data
        'user_count',
        5, 6        -- Would be queried/calculated
    );

    RETURN mutation_created('User created successfully', v_user_data, 'User', v_cascade_data);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Update user function using v2 format
CREATE OR REPLACE FUNCTION graphql.update_user_account(input_data JSONB)
RETURNS mutation_response AS $$
DECLARE
    v_user_id uuid;
    v_current_user record;
    v_updated_fields text[] := ARRAY[]::text[];
    v_user_data jsonb;
BEGIN
    v_user_id := (input_data->>'id')::uuid;

    -- Get current user
    SELECT * INTO v_current_user FROM tb_user WHERE id = v_user_id;
    IF NOT FOUND THEN
        RETURN mutation_not_found('User', v_user_id::text);
    END IF;

    -- Check for email uniqueness if email is being updated
    IF input_data ? 'email' AND input_data->>'email' != v_current_user.data->>'email' THEN
        IF EXISTS (SELECT 1 FROM tb_user WHERE data->>'email' = input_data->>'email' AND id != v_user_id) THEN
            RETURN mutation_validation_error('Email address already exists', 'email');
        END IF;
        UPDATE tb_user SET data = jsonb_set(data, '{email}', input_data->'email') WHERE id = v_user_id;
        v_updated_fields := array_append(v_updated_fields, 'email');
    END IF;

    -- Update name if provided
    IF input_data ? 'name' AND input_data->>'name' != v_current_user.data->>'name' THEN
        UPDATE tb_user SET data = jsonb_set(data, '{name}', input_data->'name') WHERE id = v_user_id;
        v_updated_fields := array_append(v_updated_fields, 'name');
    END IF;

    -- Update role if provided
    IF input_data ? 'role' AND input_data->>'role' != v_current_user.data->>'role' THEN
        UPDATE tb_user SET data = jsonb_set(data, '{role}', input_data->'role') WHERE id = v_user_id;
        v_updated_fields := array_append(v_updated_fields, 'role');
    END IF;

    -- Check if anything was updated
    IF array_length(v_updated_fields, 1) = 0 THEN
        RETURN mutation_noop('unchanged', 'No fields were updated');
    END IF;

    -- Update timestamp
    UPDATE tb_user SET updated_at = now() WHERE id = v_user_id;

    -- Return updated user data
    SELECT jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'email', data->>'email',
        'role', data->>'role',
        'updated_at', to_jsonb(updated_at)
    ) INTO v_user_data FROM tb_user WHERE id = v_user_id;

    RETURN mutation_updated('User updated successfully', v_user_data, v_updated_fields, 'User');
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Delete user function using v2 format
CREATE OR REPLACE FUNCTION graphql.delete_user(input_data JSONB)
RETURNS mutation_response AS $$
DECLARE
    v_user_id uuid;
    v_user_data jsonb;
BEGIN
    v_user_id := (input_data->>'id')::uuid;

    -- Get user data before deletion
    SELECT data INTO v_user_data FROM tb_user WHERE id = v_user_id;
    IF NOT FOUND THEN
        RETURN mutation_not_found('User', v_user_id::text);
    END IF;

    -- Delete user
    DELETE FROM tb_user WHERE id = v_user_id;

    RETURN mutation_deleted('User deleted successfully', v_user_id::text, 'User');
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- =====================================================
-- TEST DATA
-- =====================================================

-- Create some test data
INSERT INTO tb_user (data) VALUES
    (jsonb_build_object(
        'id', gen_random_uuid(),
        'name', 'Alice Admin',
        'email', 'alice@example.com',
        'role', 'admin',
        'created_at', to_jsonb(now())
    )),
    (jsonb_build_object(
        'id', gen_random_uuid(),
        'name', 'Bob User',
        'email', 'bob@example.com',
        'role', 'user',
        'created_at', to_jsonb(now())
    ));

-- =====================================================
-- TESTING HELPERS
-- =====================================================

-- Helper to test mutation functions
CREATE OR REPLACE FUNCTION test_mutation_v2(result mutation_response)
RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'status', result.status,
        'is_success', CASE WHEN result.status NOT LIKE 'failed:%' AND result.status NOT LIKE 'noop:%' THEN true ELSE false END,
        'is_error', CASE WHEN result.status LIKE 'failed:%' THEN true ELSE false END,
        'is_noop', CASE WHEN result.status LIKE 'noop:%' THEN true ELSE false END,
        'message', result.message,
        'entity_type', result.entity_type,
        'entity_id', result.entity_id,
        'has_entity', result.entity IS NOT NULL,
        'has_cascade', result.cascade IS NOT NULL,
        'has_metadata', result.metadata IS NOT NULL,
        'updated_fields', result.updated_fields
    );
END;
$$ LANGUAGE plpgsql;

-- Example test queries:
/*
-- Test successful creation
SELECT test_mutation_v2(graphql.create_user('{"name": "John", "email": "john@test.com"}'));

-- Test validation error
SELECT test_mutation_v2(graphql.create_user('{"name": "John", "email": "alice@example.com"}'));

-- Test update
SELECT test_mutation_v2(graphql.update_user_account('{"id": "user-uuid", "name": "John Updated"}'));

-- Test not found
SELECT test_mutation_v2(graphql.delete_user('{"id": "00000000-0000-0000-0000-000000000000"}'));
*/
