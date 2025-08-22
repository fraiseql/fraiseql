-- App layer functions (new file)
-- Demonstrates app/core function split pattern
-- App layer handles JSONB input conversion and basic validation

-- App function: Create Post
CREATE OR REPLACE FUNCTION app.create_post(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_post_input;
BEGIN
    -- Convert JSONB to typed input (app layer responsibility)
    v_input := jsonb_populate_record(NULL::app.type_post_input, input_payload);

    -- Basic validation (app layer)
    IF v_input.title IS NULL OR length(trim(v_input.title)) < 3 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'post', NULL,
            'NOOP', 'noop:invalid_title', ARRAY[]::TEXT[],
            'Title must be at least 3 characters',
            NULL, NULL,
            jsonb_build_object('validation_layer', 'app', 'field', 'title')
        );
    END IF;

    IF v_input.content IS NULL OR length(trim(v_input.content)) < 50 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'post', NULL,
            'NOOP', 'noop:invalid_content', ARRAY[]::TEXT[],
            'Content must be at least 50 characters',
            NULL, NULL,
            jsonb_build_object('validation_layer', 'app', 'field', 'content')
        );
    END IF;

    -- Delegate to core layer
    RETURN core.create_post(input_pk_organization, input_created_by, v_input, input_payload);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- App function: Update Post with optimistic locking
CREATE OR REPLACE FUNCTION app.update_post(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_pk_post UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_post_update_input;
    v_expected_version INTEGER;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_post_update_input, input_payload);
    v_expected_version := (input_payload->>'_expected_version')::INTEGER;

    -- App layer validation
    IF v_input.title IS NOT NULL AND length(trim(v_input.title)) < 3 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_updated_by, 'post', input_pk_post,
            'NOOP', 'noop:invalid_title', ARRAY[]::TEXT[],
            'Title must be at least 3 characters',
            NULL, NULL,
            jsonb_build_object('validation_layer', 'app', 'field', 'title')
        );
    END IF;

    RETURN core.update_post(
        input_pk_organization, input_updated_by, input_pk_post,
        v_input, v_expected_version, input_payload
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- App function: Create User
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_user_input;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);

    -- App layer validation
    IF v_input.email IS NULL OR v_input.email !~ '^[^@]+@[^@]+\.[^@]+$' THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'user', NULL,
            'NOOP', 'noop:invalid_email', ARRAY[]::TEXT[],
            'Invalid email format',
            NULL, NULL,
            jsonb_build_object('validation_layer', 'app', 'field', 'email')
        );
    END IF;

    IF v_input.name IS NULL OR length(trim(v_input.name)) < 2 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'user', NULL,
            'NOOP', 'noop:invalid_name', ARRAY[]::TEXT[],
            'Name must be at least 2 characters',
            NULL, NULL,
            jsonb_build_object('validation_layer', 'app', 'field', 'name')
        );
    END IF;

    -- Delegate to core layer
    RETURN core.create_user(input_pk_organization, input_created_by, v_input, input_payload);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Type definitions for app layer
CREATE TYPE app.type_post_input AS (
    title TEXT,
    content TEXT,
    excerpt TEXT,
    tags TEXT[],
    is_published BOOLEAN
);

CREATE TYPE app.type_post_update_input AS (
    title TEXT,
    content TEXT,
    excerpt TEXT,
    tags TEXT[],
    is_published BOOLEAN
);

CREATE TYPE app.type_user_input AS (
    email TEXT,
    name TEXT,
    bio TEXT,
    avatar_url TEXT,
    password_hash TEXT
);

-- Mutation result type for standardized responses
CREATE TYPE app.mutation_result AS (
    success BOOLEAN,
    operation_type TEXT,
    entity_type TEXT,
    entity_id UUID,
    message TEXT,
    error_code TEXT,
    changed_fields TEXT[],
    old_data JSONB,
    new_data JSONB,
    metadata JSONB
);

-- Grant permissions
GRANT EXECUTE ON FUNCTION app.create_post(UUID, UUID, JSONB) TO blog_api_role;
GRANT EXECUTE ON FUNCTION app.update_post(UUID, UUID, UUID, JSONB) TO blog_api_role;
GRANT EXECUTE ON FUNCTION app.create_user(UUID, UUID, JSONB) TO blog_api_role;
