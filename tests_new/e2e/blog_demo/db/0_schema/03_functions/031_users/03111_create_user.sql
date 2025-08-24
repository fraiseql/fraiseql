-- Create User CRUD Functions
-- Following PrintOptim app/core pattern with JSONB input handling

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.create_user(
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_user_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);
    RETURN core.create_user(input_created_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.create_user(
    input_created_by UUID,
    input_data app.type_user_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'user';
    v_id UUID := gen_random_uuid();
    v_fields TEXT[] := ARRAY(SELECT jsonb_object_keys(input_payload));

    v_payload_after JSONB;
    v_existing_id UUID;
    v_op TEXT;
    v_status TEXT;
    v_message TEXT;
    v_reason TEXT;
    v_extra_metadata JSONB;
BEGIN
    -- Check for existing user by identifier (username)
    SELECT pk_user INTO v_existing_id
    FROM tb_user
    WHERE identifier = input_data.identifier
    LIMIT 1;

    IF v_existing_id IS NOT NULL THEN
        v_op := 'NOOP';
        v_status := 'noop:already_exists';
        v_message := 'User with this username already exists.';
        v_reason := 'unique_constraint_violation';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_create',
            'status', v_status,
            'reason', v_reason,
            'conflict', jsonb_build_object(
                'identifier', input_data.identifier,
                'existing_id', v_existing_id
            )
        );

        RETURN core.log_and_return_mutation(
            input_created_by,
            v_entity,
            v_existing_id,
            v_op,
            v_status,
            v_fields,
            v_message,
            NULL,
            NULL,
            v_extra_metadata
        );
    END IF;

    -- Check for existing user by email
    SELECT pk_user INTO v_existing_id
    FROM tb_user
    WHERE email = input_data.email
    LIMIT 1;

    IF v_existing_id IS NOT NULL THEN
        v_op := 'NOOP';
        v_status := 'noop:email_already_exists';
        v_message := 'User with this email already exists.';
        v_reason := 'unique_constraint_violation';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_create',
            'status', v_status,
            'reason', v_reason,
            'conflict', jsonb_build_object(
                'email', input_data.email,
                'existing_id', v_existing_id
            )
        );

        RETURN core.log_and_return_mutation(
            input_created_by,
            v_entity,
            v_existing_id,
            v_op,
            v_status,
            v_fields,
            v_message,
            NULL,
            NULL,
            v_extra_metadata
        );
    END IF;

    -- Insert new user
    INSERT INTO tb_user (
        pk_user,
        identifier,
        email,
        password_hash,
        role,
        is_active,
        email_verified,
        profile,
        preferences,
        metadata,
        created_by
    ) VALUES (
        v_id,
        input_data.identifier,
        input_data.email,
        input_data.password_hash,
        COALESCE(input_data.role, 'user'::user_role),
        COALESCE(input_data.is_active, true),
        COALESCE(input_data.email_verified, false),
        COALESCE(input_data.profile, '{}'::JSONB),
        COALESCE(input_data.preferences, '{}'::JSONB),
        COALESCE(input_data.metadata, '{}'::JSONB),
        input_created_by
    );

    -- Get final payload from view
    SELECT row_to_json(v) INTO v_payload_after
    FROM v_user v
    WHERE v.id = v_id;

    v_op := 'INSERT';
    v_status := 'new';
    v_message := 'User created successfully.';
    v_reason := 'new_entity_created';
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_create',
        'status', v_status,
        'reason', v_reason,
        'input_payload', core.sanitize_jsonb_unset(input_payload),
        'updated_fields', v_fields
    );

    RETURN core.log_and_return_mutation(
        input_created_by,
        v_entity,
        v_id,
        v_op,
        v_status,
        v_fields,
        v_message,
        NULL,
        v_payload_after,
        v_extra_metadata
    );
END;
$$;
