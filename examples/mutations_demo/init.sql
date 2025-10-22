-- Create schema for GraphQL functions
CREATE SCHEMA IF NOT EXISTS graphql;

-- Create the standardized mutation result type
CREATE TYPE mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);

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

-- Note: In a full implementation, you'd have tv_user table with generated JSONB
-- For this demo, we use tb_user.data directly (simplified approach)

-- Create mutation events table for simplified CDC logging
CREATE TABLE IF NOT EXISTS app.mutation_events (
    event_id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID,
    operation TEXT NOT NULL,

    -- What client receives (extracted by Rust)
    client_response JSONB NOT NULL,

    -- What CDC consumers need (before/after diff)
    before_state JSONB,
    after_state JSONB,

    -- Audit metadata
    metadata JSONB,
    source JSONB,

    event_timestamp TIMESTAMPTZ DEFAULT NOW(),
    transaction_id BIGINT
);

-- Create simplified log_mutation_event function
CREATE OR REPLACE FUNCTION app.log_mutation_event(
    p_event_type TEXT,
    p_entity_type TEXT,
    p_entity_id UUID,
    p_operation TEXT,
    p_client_response JSONB,    -- NEW: what client receives
    p_before_state JSONB,
    p_after_state JSONB,
    p_metadata JSONB
) RETURNS BIGINT AS $$
DECLARE
    v_event_id BIGINT;
BEGIN
    INSERT INTO app.mutation_events (
        event_type,
        entity_type,
        entity_id,
        operation,
        client_response,
        before_state,
        after_state,
        metadata,
        source,
        transaction_id
    ) VALUES (
        p_event_type,
        p_entity_type,
        p_entity_id,
        p_operation,
        p_client_response,
        p_before_state,
        p_after_state,
        p_metadata,
        jsonb_build_object(
            'db', current_database(),
            'schema', 'public',
            'table', p_entity_type || 's',
            'txId', txid_current()
        ),
        txid_current()
    )
    RETURNING event_id INTO v_event_id;

    RETURN v_event_id;
END;
$$ LANGUAGE plpgsql;

-- Create user function (simplified single-source CDC)
CREATE OR REPLACE FUNCTION graphql.create_user(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_user_id UUID;
    v_user_data JSONB;
    v_event_id BIGINT;
    v_existing_user RECORD;
BEGIN
    -- Check for existing email
    SELECT * INTO v_existing_user
    FROM tb_user
    WHERE data->>'email' = input_data->>'email';

    IF FOUND THEN
        -- Log error event and return result
        v_event_id := app.log_mutation_event(
            'USER_CREATION_FAILED',    -- event_type
            'user',                     -- entity_type
            NULL,                       -- entity_id (none for failed creation)
            'CREATE',                   -- operation
            jsonb_build_object(         -- client_response
                'success', false,
                'code', 'EMAIL_EXISTS',
                'message', 'Email already registered',
                'conflict_user', v_existing_user.data,
                'suggested_email', lower(replace(input_data->>'name', ' ', '.')) || '.' ||
                                   substring(gen_random_uuid()::text, 1, 4) ||
                                   '@' || split_part(input_data->>'email', '@', 2)
            ),
            NULL,                       -- before_state
            NULL,                       -- after_state
            jsonb_build_object(         -- metadata
                'error_type', 'duplicate_email',
                'input_email', input_data->>'email'
            )
        );

        -- Return mutation_result for FraiseQL compatibility
        RETURN (
            NULL,                     -- id
            NULL,                     -- updated_fields
            'email_exists',           -- status
            'Email already registered', -- message
            NULL,                     -- object_data
            jsonb_build_object(       -- extra_metadata
                'conflict_user', v_existing_user.data,
                'suggested_email', lower(replace(input_data->>'name', ' ', '.')) || '.' ||
                                   substring(gen_random_uuid()::text, 1, 4) ||
                                   '@' || split_part(input_data->>'email', '@', 2)
            )
        )::mutation_result;
    END IF;

    -- Create the user
    v_user_id := gen_random_uuid();
    v_user_data := jsonb_build_object(
        'id', v_user_id,
        'name', input_data->>'name',
        'email', input_data->>'email',
        'role', COALESCE(input_data->>'role', 'user'),
        'created_at', now()::text
    );

    INSERT INTO tb_user (id, data, created_at)
    VALUES (v_user_id, v_user_data, now());

    -- Log mutation event (single source of truth)
    v_event_id := app.log_mutation_event(
        'USER_CREATED',              -- event_type
        'user',                       -- entity_type
        v_user_id,                    -- entity_id
        'CREATE',                     -- operation
        jsonb_build_object(           -- client_response
            'success', true,
            'code', 'SUCCESS',
            'message', 'User created successfully',
            'user', v_user_data
        ),
        NULL,                         -- before_state
        v_user_data,                  -- after_state
        jsonb_build_object(           -- metadata
            'created_at', now(),
            'created_by', current_user,
            'source', 'graphql_api'
        )
    );

    -- Return mutation_result for FraiseQL compatibility
    RETURN (
        v_user_id,                    -- id
        NULL,                         -- updated_fields
        'success',                    -- status
        'User created successfully', -- message
        v_user_data,                  -- object_data
        NULL                          -- extra_metadata
    )::mutation_result;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Update user function (simplified single-source CDC)
CREATE OR REPLACE FUNCTION graphql.update_user_account(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_user_id UUID;
    v_before_data JSONB;
    v_after_data JSONB;
    v_updated_fields TEXT[] := ARRAY[]::TEXT[];
    v_event_id BIGINT;
BEGIN
    v_user_id := (input_data->>'id')::UUID;

    -- Get before state
    SELECT data INTO v_before_data
    FROM tb_user
    WHERE id = v_user_id;

    IF v_before_data IS NULL THEN
        -- Log error event
        v_event_id := app.log_mutation_event(
            'USER_UPDATE_FAILED',
            'user',
            v_user_id,
            'UPDATE',
            jsonb_build_object(
                'success', false,
                'code', 'NOT_FOUND',
                'message', 'User not found',
                'user_id', v_user_id
            ),
            NULL, NULL,
            jsonb_build_object('error', 'not_found')
        );

        -- Return mutation_result for FraiseQL compatibility
        RETURN (
            NULL,                     -- id
            NULL,                     -- updated_fields
            'not_found',              -- status
            'User not found',         -- message
            NULL,                     -- object_data
            jsonb_build_object('not_found', true)  -- extra_metadata
        )::mutation_result;
    END IF;

    -- Build updated data
    v_after_data := v_before_data;

    -- Update fields if provided
    IF input_data ? 'name' AND input_data->>'name' IS NOT NULL THEN
        v_after_data := jsonb_set(v_after_data, '{name}', input_data->'name');
        v_updated_fields := array_append(v_updated_fields, 'name');
    END IF;

    IF input_data ? 'email' AND input_data->>'email' IS NOT NULL THEN
        -- Check if email is already taken
        IF EXISTS (
            SELECT 1 FROM tb_user
            WHERE data->>'email' = input_data->>'email'
            AND id != v_user_id
        ) THEN
            v_event_id := app.log_mutation_event(
                'USER_UPDATE_FAILED',
                'user',
                v_user_id,
                'UPDATE',
                jsonb_build_object(
                    'success', false,
                    'code', 'VALIDATION_ERROR',
                    'message', 'Validation failed',
                    'validation_errors', jsonb_build_object('email', 'Email already taken')
                ),
                v_before_data,
                NULL,
                jsonb_build_object('validation_error', 'duplicate_email')
            );

            -- Return mutation_result for FraiseQL compatibility
            RETURN (
                NULL,                     -- id
                NULL,                     -- updated_fields
                'validation_error',       -- status
                'Validation failed',      -- message
                NULL,                     -- object_data
                jsonb_build_object(       -- extra_metadata
                    'validation_errors',
                    jsonb_build_object('email', 'Email already taken')
                )
            )::mutation_result;
        END IF;

        v_after_data := jsonb_set(v_after_data, '{email}', input_data->'email');
        v_updated_fields := array_append(v_updated_fields, 'email');
    END IF;

    IF input_data ? 'role' AND input_data->>'role' IS NOT NULL THEN
        v_after_data := jsonb_set(v_after_data, '{role}', input_data->'role');
        v_updated_fields := array_append(v_updated_fields, 'role');
    END IF;

    -- Update the user
    UPDATE tb_user
    SET data = v_after_data,
        updated_at = now()
    WHERE id = v_user_id;

    -- Log mutation event (success case)
    v_event_id := app.log_mutation_event(
        'USER_UPDATED',
        'user',
        v_user_id,
        'UPDATE',
        jsonb_build_object(
            'success', true,
            'code', 'SUCCESS',
            'message', 'User updated successfully',
            'user', v_after_data,
            'updated_fields', v_updated_fields
        ),
        v_before_data,
        v_after_data,
        jsonb_build_object(
            'updated_by', current_user,
            'fields_updated', v_updated_fields
        )
    );

    -- Return mutation_result for FraiseQL compatibility
    RETURN (
        v_user_id,                    -- id
        v_updated_fields,             -- updated_fields
        'success',                    -- status
        'User updated successfully', -- message
        v_after_data,                 -- object_data
        NULL                          -- extra_metadata
    )::mutation_result;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Delete user function (simplified single-source CDC)
CREATE OR REPLACE FUNCTION graphql.delete_user(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_user_id UUID;
    v_before_data JSONB;
    v_event_id BIGINT;
BEGIN
    v_user_id := (input_data->>'id')::UUID;

    -- Get before state
    SELECT data INTO v_before_data
    FROM tb_user
    WHERE id = v_user_id;

    IF v_before_data IS NULL THEN
        -- Log error event
        v_event_id := app.log_mutation_event(
            'USER_DELETION_FAILED',
            'user',
            v_user_id,
            'DELETE',
            jsonb_build_object(
                'success', false,
                'code', 'NOT_FOUND',
                'message', 'User not found',
                'user_id', v_user_id
            ),
            NULL, NULL,
            jsonb_build_object('error', 'not_found')
        );

        -- Return mutation_result for FraiseQL compatibility
        RETURN (
            NULL,                     -- id
            NULL,                     -- updated_fields
            'not_found',              -- status
            'User not found',         -- message
            NULL,                     -- object_data
            jsonb_build_object('not_found', true)  -- extra_metadata
        )::mutation_result;
    END IF;

    -- Delete the user
    DELETE FROM tb_user WHERE id = v_user_id;

    -- Log mutation event (success case)
    v_event_id := app.log_mutation_event(
        'USER_DELETED',
        'user',
        v_user_id,
        'DELETE',
        jsonb_build_object(
            'success', true,
            'code', 'SUCCESS',
            'message', 'User deleted successfully',
            'user', v_before_data
        ),
        v_before_data,
        NULL,
        jsonb_build_object(
            'deleted_by', current_user,
            'deleted_at', now()
        )
    );

    -- Return mutation_result for FraiseQL compatibility
    RETURN (
        v_user_id,                    -- id
        NULL,                         -- updated_fields
        'success',                    -- status
        'User deleted successfully', -- message
        v_before_data,                -- object_data
        NULL                          -- extra_metadata
    )::mutation_result;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Create some test data
INSERT INTO tb_user (data) VALUES
    (jsonb_build_object(
        'id', gen_random_uuid(),
        'name', 'Alice Admin',
        'email', 'alice@example.com',
        'role', 'admin',
        'created_at', now()::text
    )),
    (jsonb_build_object(
        'id', gen_random_uuid(),
        'name', 'Bob User',
        'email', 'bob@example.com',
        'role', 'user',
        'created_at', now()::text
    ));
