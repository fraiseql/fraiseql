-- Update User Function (Multi-tenant)
-- Simplified version following app pattern with tenant isolation

CREATE OR REPLACE FUNCTION app.update_user(
    input_pk_user UUID,
    input_pk_organization UUID,
    input_updated_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_user_input;
    v_existing_user tenant.tb_user;
    v_result_data JSONB;
    v_conflict_id UUID;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);

    -- Validate tenant context and check if user exists
    SELECT * INTO v_existing_user
    FROM tenant.tb_user
    WHERE pk_user = input_pk_user
      AND fk_organization = input_pk_organization;

    IF v_existing_user.pk_user IS NULL THEN
        RETURN app.log_and_return_mutation(
            false,
            'User not found or access denied',
            NULL,
            'USER_NOT_FOUND',
            jsonb_build_object(
                'user_id', input_pk_user,
                'organization_id', input_pk_organization
            ),
            'update_user',
            input_updated_by,
            input_pk_organization
        );
    END IF;

    -- Check for identifier conflicts within tenant (if identifier is being updated)
    IF v_input.identifier IS NOT NULL AND v_input.identifier != v_existing_user.identifier THEN
        SELECT pk_user INTO v_conflict_id
        FROM tenant.tb_user
        WHERE fk_organization = input_pk_organization
          AND identifier = v_input.identifier
          AND pk_user != input_pk_user;

        IF v_conflict_id IS NOT NULL THEN
            RETURN app.log_and_return_mutation(
                false,
                'Username already exists in this organization',
                NULL,
                'DUPLICATE_IDENTIFIER',
                jsonb_build_object(
                    'identifier', v_input.identifier,
                    'conflict_id', v_conflict_id
                ),
                'update_user',
                input_updated_by,
                input_pk_organization
            );
        END IF;
    END IF;

    -- Check for email conflicts within tenant (if email is being updated)
    IF v_input.email IS NOT NULL AND v_input.email != v_existing_user.email THEN
        SELECT pk_user INTO v_conflict_id
        FROM tenant.tb_user
        WHERE fk_organization = input_pk_organization
          AND email = v_input.email
          AND pk_user != input_pk_user;

        IF v_conflict_id IS NOT NULL THEN
            RETURN app.log_and_return_mutation(
                false,
                'Email already exists in this organization',
                NULL,
                'DUPLICATE_EMAIL',
                jsonb_build_object(
                    'email', v_input.email,
                    'conflict_id', v_conflict_id
                ),
                'update_user',
                input_updated_by,
                input_pk_organization
            );
        END IF;
    END IF;

    -- Update user with only provided fields
    UPDATE tenant.tb_user SET
        identifier = COALESCE(v_input.identifier, identifier),
        email = COALESCE(v_input.email, email),
        password_hash = COALESCE(v_input.password_hash, password_hash),
        role = COALESCE(v_input.role, role),
        profile = COALESCE(profile || COALESCE(v_input.profile, '{}'), profile),
        preferences = COALESCE(preferences || COALESCE(v_input.preferences, '{}'), preferences),
        updated_by = input_updated_by
    WHERE pk_user = input_pk_user
      AND fk_organization = input_pk_organization;

    -- Build result data
    SELECT jsonb_build_object(
        'pk_user', u.pk_user,
        'id', u.pk_user,
        'identifier', u.identifier,
        'email', u.email,
        'role', u.role,
        'is_active', u.is_active,
        'email_verified', u.email_verified,
        'organizationId', u.fk_organization,
        'profile', u.profile,
        'preferences', u.preferences,
        'created_at', u.created_at,
        'updated_at', u.updated_at
    ) INTO v_result_data
    FROM tenant.tb_user u
    WHERE u.pk_user = input_pk_user;

    -- Return success
    RETURN app.log_and_return_mutation(
        true,
        'User updated successfully',
        v_result_data,
        NULL,
        jsonb_build_object(
            'user_id', input_pk_user,
            'organization_id', input_pk_organization
        ),
        'update_user',
        input_updated_by,
        input_pk_organization
    );

EXCEPTION
    WHEN OTHERS THEN
        RETURN app.log_and_return_mutation(
            false,
            'Failed to update user: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE
            ),
            'update_user',
            input_updated_by,
            input_pk_organization
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
