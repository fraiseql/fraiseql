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

-- Create users table with JSONB data
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    data JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create index on email for uniqueness checks
CREATE UNIQUE INDEX idx_users_email ON users ((data->>'email'));

-- Create user function
CREATE OR REPLACE FUNCTION graphql.create_user(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_user_id UUID;
    v_existing_user RECORD;
BEGIN
    -- Check for existing email
    SELECT * INTO v_existing_user
    FROM users
    WHERE data->>'email' = input_data->>'email';

    IF FOUND THEN
        -- Return error with conflict info
        v_result.status := 'email_exists';
        v_result.message := 'Email already registered';
        v_result.extra_metadata := jsonb_build_object(
            'conflict_user', v_existing_user.data,
            'suggested_email', lower(replace(input_data->>'name', ' ', '.')) || '.' ||
                               substring(gen_random_uuid()::text, 1, 4) ||
                               '@' || split_part(input_data->>'email', '@', 2)
        );
        RETURN v_result;
    END IF;

    -- Create the user
    v_user_id := gen_random_uuid();
    INSERT INTO users (id, data, created_at)
    VALUES (
        v_user_id,
        jsonb_build_object(
            'id', v_user_id,
            'name', input_data->>'name',
            'email', input_data->>'email',
            'role', COALESCE(input_data->>'role', 'user'),
            'created_at', now()::text
        ),
        now()
    );

    -- Return success
    v_result.id := v_user_id;
    v_result.status := 'success';
    v_result.message := 'User created successfully';

    SELECT data INTO v_result.object_data
    FROM users WHERE id = v_user_id;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;

-- Update user function
CREATE OR REPLACE FUNCTION graphql.update_user_account(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_user_id UUID;
    v_current_data JSONB;
    v_new_data JSONB;
    v_updated_fields TEXT[] := ARRAY[]::TEXT[];
BEGIN
    v_user_id := (input_data->>'id')::UUID;

    -- Get current user data
    SELECT data INTO v_current_data
    FROM users
    WHERE id = v_user_id;

    IF NOT FOUND THEN
        v_result.status := 'not_found';
        v_result.message := 'User not found';
        v_result.extra_metadata := jsonb_build_object('not_found', true);
        RETURN v_result;
    END IF;

    -- Build updated data
    v_new_data := v_current_data;

    -- Update fields if provided
    IF input_data ? 'name' AND input_data->>'name' IS NOT NULL THEN
        v_new_data := jsonb_set(v_new_data, '{name}', input_data->'name');
        v_updated_fields := array_append(v_updated_fields, 'name');
    END IF;

    IF input_data ? 'email' AND input_data->>'email' IS NOT NULL THEN
        -- Check if email is already taken
        IF EXISTS (
            SELECT 1 FROM users
            WHERE data->>'email' = input_data->>'email'
            AND id != v_user_id
        ) THEN
            v_result.status := 'validation_error';
            v_result.message := 'Validation failed';
            v_result.extra_metadata := jsonb_build_object(
                'validation_errors',
                jsonb_build_object('email', 'Email already taken')
            );
            RETURN v_result;
        END IF;

        v_new_data := jsonb_set(v_new_data, '{email}', input_data->'email');
        v_updated_fields := array_append(v_updated_fields, 'email');
    END IF;

    IF input_data ? 'role' AND input_data->>'role' IS NOT NULL THEN
        v_new_data := jsonb_set(v_new_data, '{role}', input_data->'role');
        v_updated_fields := array_append(v_updated_fields, 'role');
    END IF;

    -- Update the user
    UPDATE users
    SET data = v_new_data,
        updated_at = now()
    WHERE id = v_user_id;

    -- Return success
    v_result.id := v_user_id;
    v_result.updated_fields := v_updated_fields;
    v_result.status := 'success';
    v_result.message := 'User updated successfully';
    v_result.object_data := v_new_data;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;

-- Delete user function (bonus example)
CREATE OR REPLACE FUNCTION graphql.delete_user(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_user_id UUID;
    v_user_data JSONB;
BEGIN
    v_user_id := (input_data->>'id')::UUID;

    -- Get user data before deletion
    SELECT data INTO v_user_data
    FROM users
    WHERE id = v_user_id;

    IF NOT FOUND THEN
        v_result.status := 'not_found';
        v_result.message := 'User not found';
        v_result.extra_metadata := jsonb_build_object('not_found', true);
        RETURN v_result;
    END IF;

    -- Delete the user
    DELETE FROM users WHERE id = v_user_id;

    -- Return success with deleted user data
    v_result.id := v_user_id;
    v_result.status := 'success';
    v_result.message := 'User deleted successfully';
    v_result.object_data := v_user_data;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;

-- Create some test data
INSERT INTO users (data) VALUES
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
