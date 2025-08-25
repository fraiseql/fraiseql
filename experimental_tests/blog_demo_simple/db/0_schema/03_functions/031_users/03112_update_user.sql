-- Update User CRUD Functions
-- Following PrintOptim app/core pattern with JSONB input handling

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.update_user(
    input_pk_user UUID,
    input_updated_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_user_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);
    RETURN core.update_user(input_pk_user, input_updated_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.update_user(
    input_pk_user UUID,
    input_updated_by UUID,
    input_data app.type_user_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'user';
    v_fields TEXT[] := ARRAY(SELECT jsonb_object_keys(input_payload));

    v_payload_before JSONB;
    v_payload_after JSONB;
    v_existing_user tb_user;
    v_conflict_id UUID;

    v_op TEXT;
    v_status TEXT;
    v_message TEXT;
    v_reason TEXT;
    v_extra_metadata JSONB;
BEGIN
    -- Check if user exists and get current state
    SELECT * INTO v_existing_user
    FROM tb_user
    WHERE pk_user = input_pk_user;

    IF NOT FOUND THEN
        v_op := 'NOOP';
        v_status := 'noop:not_found';
        v_message := 'User not found.';
        v_reason := 'entity_not_found';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_update',
            'status', v_status,
            'reason', v_reason,
            'input_id', input_pk_user
        );

        RETURN core.log_and_return_mutation(
            input_updated_by,
            v_entity,
            input_pk_user,
            v_op,
            v_status,
            v_fields,
            v_message,
            NULL,
            NULL,
            v_extra_metadata
        );
    END IF;

    -- Get current payload before update
    SELECT row_to_json(v) INTO v_payload_before
    FROM v_user v
    WHERE v.id = input_pk_user;

    -- Check for identifier conflicts (if identifier is being changed)
    IF input_data.identifier IS NOT NULL AND input_data.identifier != v_existing_user.identifier THEN
        SELECT pk_user INTO v_conflict_id
        FROM tb_user
        WHERE identifier = input_data.identifier
          AND pk_user != input_pk_user
        LIMIT 1;

        IF v_conflict_id IS NOT NULL THEN
            v_op := 'NOOP';
            v_status := 'noop:identifier_conflict';
            v_message := 'Username already exists.';
            v_reason := 'unique_constraint_violation';
            v_fields := ARRAY[]::TEXT[];
            v_extra_metadata := jsonb_build_object(
                'trigger', 'api_update',
                'status', v_status,
                'reason', v_reason,
                'conflict', jsonb_build_object(
                    'identifier', input_data.identifier,
                    'conflict_id', v_conflict_id
                )
            );

            RETURN core.log_and_return_mutation(
                input_updated_by,
                v_entity,
                input_pk_user,
                v_op,
                v_status,
                v_fields,
                v_message,
                v_payload_before,
                v_payload_before,
                v_extra_metadata
            );
        END IF;
    END IF;

    -- Check for email conflicts (if email is being changed)
    IF input_data.email IS NOT NULL AND input_data.email != v_existing_user.email THEN
        SELECT pk_user INTO v_conflict_id
        FROM tb_user
        WHERE email = input_data.email
          AND pk_user != input_pk_user
        LIMIT 1;

        IF v_conflict_id IS NOT NULL THEN
            v_op := 'NOOP';
            v_status := 'noop:email_conflict';
            v_message := 'Email already exists.';
            v_reason := 'unique_constraint_violation';
            v_fields := ARRAY[]::TEXT[];
            v_extra_metadata := jsonb_build_object(
                'trigger', 'api_update',
                'status', v_status,
                'reason', v_reason,
                'conflict', jsonb_build_object(
                    'email', input_data.email,
                    'conflict_id', v_conflict_id
                )
            );

            RETURN core.log_and_return_mutation(
                input_updated_by,
                v_entity,
                input_pk_user,
                v_op,
                v_status,
                v_fields,
                v_message,
                v_payload_before,
                v_payload_before,
                v_extra_metadata
            );
        END IF;
    END IF;

    -- Update user with only provided fields
    UPDATE tb_user SET
        identifier = COALESCE(input_data.identifier, identifier),
        email = COALESCE(input_data.email, email),
        password_hash = COALESCE(input_data.password_hash, password_hash),
        role = COALESCE(input_data.role, role),
        is_active = COALESCE(input_data.is_active, is_active),
        email_verified = COALESCE(input_data.email_verified, email_verified),
        profile = COALESCE(input_data.profile, profile),
        preferences = COALESCE(input_data.preferences, preferences),
        metadata = COALESCE(input_data.metadata, metadata),
        updated_by = input_updated_by
    WHERE pk_user = input_pk_user;

    -- Get updated payload from view
    SELECT row_to_json(v) INTO v_payload_after
    FROM v_user v
    WHERE v.id = input_pk_user;

    v_op := 'UPDATE';
    v_status := 'updated';
    v_message := 'User updated successfully.';
    v_reason := 'entity_updated';
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_update',
        'status', v_status,
        'reason', v_reason,
        'input_payload', core.sanitize_jsonb_unset(input_payload),
        'updated_fields', v_fields
    );

    RETURN core.log_and_return_mutation(
        input_updated_by,
        v_entity,
        input_pk_user,
        v_op,
        v_status,
        v_fields,
        v_message,
        v_payload_before,
        v_payload_after,
        v_extra_metadata
    );
END;
$$;
