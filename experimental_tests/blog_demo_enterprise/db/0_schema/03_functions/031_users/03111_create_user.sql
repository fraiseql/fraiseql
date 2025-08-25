-- Create User CRUD Functions (Multi-tenant)
-- Following PrintOptim app/core pattern with JSONB input handling and tenant isolation

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_user_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_user_input, input_payload);
    RETURN core.create_user(input_pk_organization, input_created_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation with tenant isolation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.create_user(
    input_pk_organization UUID,
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
    -- Validate tenant organization exists
    IF NOT EXISTS (SELECT 1 FROM management.tb_organization WHERE pk_organization = input_pk_organization) THEN
        RETURN app.log_and_return_mutation(
            false,
            'Organization not found',
            NULL,
            'INVALID_ORGANIZATION',
            jsonb_build_object('organization_id', input_pk_organization),
            'create_user',
            input_created_by,
            input_pk_organization
        );
    END IF;

    -- Check for existing user by identifier (username) within tenant
    SELECT pk_user INTO v_existing_id
    FROM tenant.tb_user
    WHERE fk_organization = input_pk_organization
      AND identifier = input_data.identifier
    LIMIT 1;

    IF v_existing_id IS NOT NULL THEN
        v_op := 'NOOP';
        v_status := 'noop:already_exists';
        v_message := 'User with this username already exists in this organization.';
        v_reason := 'unique_constraint_violation';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_create',
            'status', v_status,
            'reason', v_reason,
            'organization_id', input_pk_organization,
            'conflict', jsonb_build_object(
                'identifier', input_data.identifier,
                'existing_id', v_existing_id
            )
        );

        RETURN app.log_and_return_mutation(
            true,  -- NOOP is success
            v_message,
            jsonb_build_object('pk_user', v_existing_id),
            NULL,
            v_extra_metadata,
            'create_user',
            input_created_by,
            input_pk_organization
        );
    END IF;

    -- Check for existing user by email within tenant
    SELECT pk_user INTO v_existing_id
    FROM tenant.tb_user
    WHERE fk_organization = input_pk_organization
      AND email = input_data.email
    LIMIT 1;

    IF v_existing_id IS NOT NULL THEN
        v_op := 'NOOP';
        v_status := 'noop:email_already_exists';
        v_message := 'User with this email already exists in this organization.';
        v_reason := 'unique_constraint_violation';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_create',
            'status', v_status,
            'reason', v_reason,
            'organization_id', input_pk_organization,
            'conflict', jsonb_build_object(
                'email', input_data.email,
                'existing_id', v_existing_id
            )
        );

        RETURN app.log_and_return_mutation(
            true,  -- NOOP is success
            v_message,
            jsonb_build_object('pk_user', v_existing_id),
            NULL,
            v_extra_metadata,
            'create_user',
            input_created_by,
            input_pk_organization
        );
    END IF;

    -- Insert new user with tenant isolation
    INSERT INTO tenant.tb_user (
        pk_user,
        fk_organization,
        identifier,
        email,
        password_hash,
        role,
        is_active,
        email_verified,
        profile,
        preferences,
        metadata,
        created_by,
        updated_by
    ) VALUES (
        v_id,
        input_pk_organization,
        input_data.identifier,
        input_data.email,
        input_data.password_hash,
        COALESCE(input_data.role, 'user'::user_role),
        true,
        false,
        COALESCE(input_data.profile, jsonb_build_object(
            'display_name', input_data.identifier,
            'first_name', '',
            'last_name', '',
            'bio', '',
            'avatar_url', '',
            'timezone', 'UTC',
            'language', 'en'
        )),
        COALESCE(input_data.preferences, jsonb_build_object(
            'email_notifications', true,
            'theme', 'auto',
            'posts_per_page', 10
        )),
        '{}',
        input_created_by,
        input_created_by
    );

    -- Build result data with GraphQL-friendly field names
    SELECT jsonb_build_object(
        'pk_user', u.pk_user,
        'id', u.pk_user, -- GraphQL uses 'id' field
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
    ) INTO v_payload_after
    FROM tenant.tb_user u
    WHERE u.pk_user = v_id;

    -- Return success
    RETURN app.log_and_return_mutation(
        true,
        'User created successfully',
        v_payload_after,
        NULL,
        jsonb_build_object(
            'user_id', v_id,
            'organization_id', input_pk_organization,
            'identifier', input_data.identifier,
            'trigger', 'api_create',
            'status', 'new',
            'reason', 'new_entity_created'
        ),
        'create_user',
        input_created_by,
        input_pk_organization
    );

EXCEPTION
    WHEN OTHERS THEN
        -- Handle unexpected errors
        RETURN app.log_and_return_mutation(
            false,
            'Failed to create user: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE,
                'organization_id', input_pk_organization
            ),
            'create_user',
            input_created_by,
            input_pk_organization
        );
END;
$$;
