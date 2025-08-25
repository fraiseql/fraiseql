-- Create Tag CRUD Functions
-- Following PrintOptim app/core pattern with JSONB input handling

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.create_tag(
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_tag_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_tag_input, input_payload);
    RETURN core.create_tag(input_created_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.create_tag(
    input_created_by UUID,
    input_data app.type_tag_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'tag';
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
    -- Check for existing tag by identifier (slug)
    SELECT pk_tag INTO v_existing_id
    FROM tb_tag
    WHERE identifier = input_data.identifier
    LIMIT 1;

    IF v_existing_id IS NOT NULL THEN
        v_op := 'NOOP';
        v_status := 'noop:already_exists';
        v_message := 'Tag with this slug already exists.';
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

    -- Check for existing tag by name (case-insensitive)
    SELECT pk_tag INTO v_existing_id
    FROM tb_tag
    WHERE LOWER(name) = LOWER(input_data.name)
    LIMIT 1;

    IF v_existing_id IS NOT NULL THEN
        v_op := 'NOOP';
        v_status := 'noop:name_already_exists';
        v_message := 'Tag with this name already exists.';
        v_reason := 'unique_constraint_violation';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_create',
            'status', v_status,
            'reason', v_reason,
            'conflict', jsonb_build_object(
                'name', input_data.name,
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

    -- Insert new tag
    INSERT INTO tb_tag (
        pk_tag,
        identifier,
        name,
        description,
        color,
        metadata,
        created_by
    ) VALUES (
        v_id,
        input_data.identifier,
        input_data.name,
        input_data.description,
        input_data.color,
        COALESCE(input_data.metadata, '{}'::JSONB),
        input_created_by
    );

    -- Get final payload from view
    SELECT row_to_json(v) INTO v_payload_after
    FROM v_tag v
    WHERE v.id = v_id;

    v_op := 'INSERT';
    v_status := 'new';
    v_message := 'Tag created successfully.';
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
