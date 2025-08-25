-- Create Organization Function
-- Multi-tenant blog hosting organization creation

CREATE OR REPLACE FUNCTION app.create_organization(
    input_created_by UUID DEFAULT NULL,
    input_payload JSONB
) RETURNS mutation_result AS $$
DECLARE
    v_input app.type_organization_input;
    v_organization_id UUID;
    v_result_data JSONB;
    v_existing_count INTEGER;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_organization_input, input_payload);

    -- Validate required fields
    IF v_input.name IS NULL OR length(trim(v_input.name)) = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Organization name is required',
            NULL,
            'MISSING_NAME',
            jsonb_build_object('field', 'name'),
            'create_organization',
            input_created_by,
            NULL
        );
    END IF;

    IF v_input.identifier IS NULL OR length(trim(v_input.identifier)) = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Organization identifier is required',
            NULL,
            'MISSING_IDENTIFIER',
            jsonb_build_object('field', 'identifier'),
            'create_organization',
            input_created_by,
            NULL
        );
    END IF;

    IF v_input.contact_email IS NULL OR length(trim(v_input.contact_email)) = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Contact email is required',
            NULL,
            'MISSING_EMAIL',
            jsonb_build_object('field', 'contact_email'),
            'create_organization',
            input_created_by,
            NULL
        );
    END IF;

    -- Check for duplicate identifier
    SELECT COUNT(*) INTO v_existing_count
    FROM management.tb_organization
    WHERE identifier = v_input.identifier;

    IF v_existing_count > 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'An organization with this identifier already exists',
            NULL,
            'DUPLICATE_IDENTIFIER',
            jsonb_build_object('identifier', v_input.identifier),
            'create_organization',
            input_created_by,
            NULL
        );
    END IF;

    -- Generate new organization ID
    v_organization_id := uuid_generate_v4();

    -- Insert organization
    INSERT INTO management.tb_organization (
        pk_organization,
        name,
        identifier,
        contact_email,
        website_url,
        subscription_plan,
        status,
        settings,
        limits,
        created_by,
        updated_by
    ) VALUES (
        v_organization_id,
        trim(v_input.name),
        lower(trim(v_input.identifier)),
        lower(trim(v_input.contact_email)),
        v_input.website_url,
        COALESCE(v_input.subscription_plan, 'starter'),
        'active',
        COALESCE(v_input.settings, jsonb_build_object(
            'theme', 'default',
            'allow_user_registration', true,
            'moderation_required', false,
            'custom_domain', null
        )),
        COALESCE(v_input.limits, jsonb_build_object(
            'max_users', 5,
            'max_posts_per_month', 50,
            'max_storage_mb', 100,
            'max_api_requests_per_day', 1000
        )),
        input_created_by,
        input_created_by
    );

    -- Build result data
    SELECT jsonb_build_object(
        'pk_organization', o.pk_organization,
        'name', o.name,
        'identifier', o.identifier,
        'contact_email', o.contact_email,
        'website_url', o.website_url,
        'subscription_plan', o.subscription_plan,
        'status', o.status,
        'settings', o.settings,
        'limits', o.limits,
        'created_at', o.created_at,
        'updated_at', o.updated_at
    ) INTO v_result_data
    FROM management.tb_organization o
    WHERE o.pk_organization = v_organization_id;

    -- Return success
    RETURN app.log_and_return_mutation(
        true,
        'Organization created successfully',
        v_result_data,
        NULL,
        jsonb_build_object(
            'organization_id', v_organization_id,
            'identifier', v_input.identifier
        ),
        'create_organization',
        input_created_by,
        v_organization_id
    );

EXCEPTION
    WHEN OTHERS THEN
        -- Handle unexpected errors
        RETURN app.log_and_return_mutation(
            false,
            'Failed to create organization: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE
            ),
            'create_organization',
            input_created_by,
            NULL
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
