-- Example PostgreSQL functions using mutation_response format
-- Demonstrates the new standardized mutation result format

-- =====================================================
-- EXAMPLE MUTATION FUNCTIONS USING V2 FORMAT
-- =====================================================

-- Example: Create User with v2 format
CREATE OR REPLACE FUNCTION app.create_user_v2(input jsonb)
RETURNS mutation_response AS $$
DECLARE
    user_data jsonb;
    user_id uuid;
BEGIN
    -- Check if email already exists
    IF EXISTS (SELECT 1 FROM users WHERE email = input->>'email') THEN
        RETURN mutation_conflict('Email address already exists', 'duplicate',
            jsonb_build_object('field', 'email', 'value', input->>'email'));
    END IF;

    -- Create user
    user_id := gen_random_uuid();
    INSERT INTO users (id, name, email, created_at)
    VALUES (user_id, input->>'name', input->>'email', now());

    -- Build response entity
    user_data := jsonb_build_object(
        'id', user_id,
        'name', input->>'name',
        'email', input->>'email',
        'created_at', to_jsonb(now())
    );

    RETURN mutation_created('User created successfully', user_data, 'User');
END;
$$ LANGUAGE plpgsql;

-- Example: Update User with v2 format
CREATE OR REPLACE FUNCTION app.update_user_v2(user_id uuid, input jsonb)
RETURNS mutation_response AS $$
DECLARE
    updated_fields text[] := ARRAY[]::text[];
    user_data jsonb;
    current_user record;
BEGIN
    -- Get current user
    SELECT * INTO current_user FROM users WHERE id = user_id;
    IF NOT FOUND THEN
        RETURN mutation_not_found('User', user_id::text);
    END IF;

    -- Check for email uniqueness if email is being updated
    IF input ? 'email' AND input->>'email' != current_user.email THEN
        IF EXISTS (SELECT 1 FROM users WHERE email = input->>'email' AND id != user_id) THEN
            RETURN mutation_validation_error('Email address already exists', 'email');
        END IF;
        UPDATE users SET email = input->>'email' WHERE id = user_id;
        updated_fields := array_append(updated_fields, 'email');
    END IF;

    -- Update name if provided
    IF input ? 'name' AND input->>'name' != current_user.name THEN
        UPDATE users SET name = input->>'name' WHERE id = user_id;
        updated_fields := array_append(updated_fields, 'name');
    END IF;

    -- Check if anything was updated
    IF array_length(updated_fields, 1) = 0 THEN
        RETURN mutation_noop('unchanged', 'No fields were updated');
    END IF;

    -- Return updated user data
    SELECT jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'updated_at', to_jsonb(updated_at)
    ) INTO user_data FROM users WHERE id = user_id;

    RETURN mutation_updated('User updated successfully', user_data, updated_fields, 'User');
END;
$$ LANGUAGE plpgsql;

-- Example: Delete User with v2 format
CREATE OR REPLACE FUNCTION app.delete_user_v2(user_id uuid)
RETURNS mutation_response AS $$
DECLARE
    user_exists boolean;
BEGIN
    -- Check if user exists
    SELECT EXISTS(SELECT 1 FROM users WHERE id = user_id) INTO user_exists;
    IF NOT user_exists THEN
        RETURN mutation_not_found('User', user_id::text);
    END IF;

    -- Delete user
    DELETE FROM users WHERE id = user_id;

    RETURN mutation_deleted('User deleted successfully', user_id::text, 'User');
END;
$$ LANGUAGE plpgsql;

-- Example: Create Post with v2 format and cascade data
CREATE OR REPLACE FUNCTION app.create_post_v2(input jsonb)
RETURNS mutation_response AS $$
DECLARE
    post_data jsonb;
    post_id uuid;
    cascade_data jsonb;
BEGIN
    -- Create post
    post_id := gen_random_uuid();
    INSERT INTO posts (id, title, content, author_id, created_at)
    VALUES (post_id, input->>'title', input->>'content', (input->>'author_id')::uuid, now());

    -- Build response entity
    post_data := jsonb_build_object(
        'id', post_id,
        'title', input->>'title',
        'content', input->>'content',
        'author_id', input->>'author_id',
        'created_at', to_jsonb(now())
    );

    -- Build cascade data for side effects (e.g., update author's post count)
    -- Uses snake_case - Rust transforms to camelCase/__typename for GraphQL
    cascade_data := jsonb_build_object(
        'updated', jsonb_build_array(
            jsonb_build_object(
                'type_name', 'User',
                'id', input->>'author_id',
                'operation', 'UPDATED',
                'entity', jsonb_build_object(
                    'id', input->>'author_id',
                    'post_count', 6,
                    '_previous_post_count', 5  -- Would be queried from database
                )
            )
        )
    );

    RETURN mutation_created(
        'Post created successfully',
        post_data,
        'Post',
        cascade_data,
        jsonb_build_object('word_count', json_length(post_data->'content', '$.words'))
    );
END;
$$ LANGUAGE plpgsql;

-- =====================================================
-- MIGRATION EXAMPLE: Converting existing functions
-- =====================================================

-- Before: Old format function
CREATE OR REPLACE FUNCTION app.create_user_old(input jsonb)
RETURNS jsonb AS $$
BEGIN
    -- Old logic returning custom JSON
    RETURN jsonb_build_object(
        'success', true,
        'data', jsonb_build_object('id', gen_random_uuid(), 'name', input->>'name'),
        'message', 'User created'
    );
END;
$$ LANGUAGE plpgsql;

-- After: New v2 format function
CREATE OR REPLACE FUNCTION app.create_user_new(input jsonb)
RETURNS mutation_response AS $$
DECLARE
    user_data jsonb;
    user_id uuid;
BEGIN
    user_id := gen_random_uuid();

    -- Insert user (same logic)
    INSERT INTO users (id, name, email, created_at)
    VALUES (user_id, input->>'name', input->>'email', now());

    -- Build entity data (snake_case - Rust transforms to camelCase for GraphQL)
    user_data := jsonb_build_object(
        'id', user_id,
        'name', input->>'name',
        'email', input->>'email',
        'created_at', to_jsonb(now())
    );

    -- Return standardized v2 result
    RETURN mutation_created('User created successfully', user_data, 'User');
END;
$$ LANGUAGE plpgsql;

-- =====================================================
-- TESTING HELPERS
-- =====================================================

-- Helper to test mutation functions
CREATE OR REPLACE FUNCTION test_mutation_result(result mutation_response)
RETURNS jsonb AS $$
BEGIN
    RETURN jsonb_build_object(
        'status', result.status,
        'is_success', mutation_is_success(result),
        'is_error', mutation_is_error(result),
        'is_noop', mutation_is_noop(result),
        'message', result.message,
        'entity_type', result.entity_type,
        'entity_id', result.entity_id,
        'has_entity', result.entity IS NOT NULL,
        'has_cascade', result.cascade IS NOT NULL,
        'has_metadata', result.metadata IS NOT NULL
    );
END;
$$ LANGUAGE plpgsql;

-- Example test queries:
/*
-- Test successful creation
SELECT test_mutation_result(app.create_user_v2('{"name": "John", "email": "john@test.com"}'));

-- Test validation error
SELECT test_mutation_result(app.create_user_v2('{"name": "John", "email": "existing@test.com"}'));

-- Test update
SELECT test_mutation_result(app.update_user_v2(user_id, '{"name": "John Updated"}'));

-- Test not found
SELECT test_mutation_result(app.delete_user_v2('00000000-0000-0000-0000-000000000000'::uuid));
*/
