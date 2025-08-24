-- Create Tag Function (Multi-tenant)
-- Blog tag creation with tenant isolation

CREATE OR REPLACE FUNCTION app.create_tag(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_tag_input;
    v_tag_id UUID;
    v_slug TEXT;
    v_result_data JSONB;
    v_existing_count INTEGER;
    v_tag_data JSONB;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_tag_input, input_payload);
    
    -- Validate required fields
    IF v_input.name IS NULL OR length(trim(v_input.name)) = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Tag name is required',
            NULL,
            'MISSING_NAME',
            jsonb_build_object('field', 'name'),
            'create_tag',
            input_created_by,
            input_pk_organization
        );
    END IF;
    
    -- Generate slug from name
    v_slug := lower(regexp_replace(
        regexp_replace(trim(v_input.name), '[^a-zA-Z0-9\s-]', '', 'g'),
        '\s+', '-', 'g'
    ));
    
    -- Ensure slug is not empty
    IF v_slug IS NULL OR v_slug = '' THEN
        v_slug := 'tag-' || extract(epoch from now())::text;
    END IF;
    
    -- Check for duplicate slug within tenant
    SELECT COUNT(*) INTO v_existing_count
    FROM tenant.tb_tag
    WHERE fk_organization = input_pk_organization
      AND identifier = v_slug;
    
    -- Handle existing tag (NOOP)
    IF v_existing_count > 0 THEN
        SELECT jsonb_build_object(
            'pk_tag', t.pk_tag,
            'id', t.pk_tag,
            'name', t.data->>'name',
            'slug', t.identifier,
            'description', t.data->>'description',
            'organizationId', t.fk_organization,
            'created_at', t.created_at,
            'updated_at', t.updated_at
        ) INTO v_result_data
        FROM tenant.tb_tag t
        WHERE fk_organization = input_pk_organization
          AND identifier = v_slug;
        
        RETURN app.log_and_return_mutation(
            true,
            'Tag already exists',
            v_result_data,
            NULL,
            jsonb_build_object(
                'noop', true,
                'reason', 'tag_already_exists',
                'slug', v_slug,
                'organization_id', input_pk_organization
            ),
            'create_tag',
            input_created_by,
            input_pk_organization
        );
    END IF;
    
    -- Generate new tag ID
    v_tag_id := uuid_generate_v4();
    
    -- Build tag data JSONB
    v_tag_data := jsonb_build_object(
        'name', trim(v_input.name),
        'description', COALESCE(trim(v_input.description), ''),
        'color', COALESCE(v_input.color, '#3b82f6'), -- Default blue color
        'metadata', '{}'
    );
    
    -- Insert tag
    INSERT INTO tenant.tb_tag (
        pk_tag,
        fk_organization,
        identifier,
        data,
        created_by,
        updated_by
    ) VALUES (
        v_tag_id,
        input_pk_organization,
        v_slug,
        v_tag_data,
        input_created_by,
        input_created_by
    );
    
    -- Build result data
    SELECT jsonb_build_object(
        'pk_tag', t.pk_tag,
        'id', t.pk_tag,
        'name', t.data->>'name',
        'slug', t.identifier,
        'description', t.data->>'description',
        'color', t.data->>'color',
        'organizationId', t.fk_organization,
        'created_at', t.created_at,
        'updated_at', t.updated_at
    ) INTO v_result_data
    FROM tenant.tb_tag t
    WHERE t.pk_tag = v_tag_id;
    
    -- Return success
    RETURN app.log_and_return_mutation(
        true,
        'Tag created successfully',
        v_result_data,
        NULL,
        jsonb_build_object(
            'tag_id', v_tag_id,
            'slug', v_slug,
            'organization_id', input_pk_organization
        ),
        'create_tag',
        input_created_by,
        input_pk_organization
    );
    
EXCEPTION
    WHEN OTHERS THEN
        RETURN app.log_and_return_mutation(
            false,
            'Failed to create tag: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE
            ),
            'create_tag',
            input_created_by,
            input_pk_organization
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;