-- Update Tag CRUD Functions
-- Following PrintOptim app/core pattern with JSONB input handling

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.update_tag(
    input_pk_tag UUID,
    input_updated_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_tag_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_tag_input, input_payload);
    RETURN core.update_tag(input_pk_tag, input_updated_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.update_tag(
    input_pk_tag UUID,
    input_updated_by UUID,
    input_data app.type_tag_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'tag';
    v_fields TEXT[] := ARRAY(SELECT jsonb_object_keys(input_payload));

    v_payload_before JSONB;
    v_payload_after JSONB;
    v_existing_tag tb_tag;
    v_conflict_id UUID;

    v_op TEXT;
    v_status TEXT;
    v_message TEXT;
    v_reason TEXT;
    v_extra_metadata JSONB;
BEGIN
    -- Check if tag exists and get current state
    SELECT * INTO v_existing_tag
    FROM tb_tag
    WHERE pk_tag = input_pk_tag;

    IF NOT FOUND THEN
        v_op := 'NOOP';
        v_status := 'noop:not_found';
        v_message := 'Tag not found.';
        v_reason := 'entity_not_found';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_update',
            'status', v_status,
            'reason', v_reason,
            'input_id', input_pk_tag
        );

        RETURN core.log_and_return_mutation(
            input_updated_by,
            v_entity,
            input_pk_tag,
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
    FROM v_tag v
    WHERE v.id = input_pk_tag;

    -- Check for identifier conflicts (if identifier is being changed)
    IF input_data.identifier IS NOT NULL AND input_data.identifier != v_existing_tag.identifier THEN
        SELECT pk_tag INTO v_conflict_id
        FROM tb_tag
        WHERE identifier = input_data.identifier
          AND pk_tag != input_pk_tag
        LIMIT 1;

        IF v_conflict_id IS NOT NULL THEN
            v_op := 'NOOP';
            v_status := 'noop:identifier_conflict';
            v_message := 'Tag slug already exists.';
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
                input_pk_tag,
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

    -- Check for name conflicts (if name is being changed)
    IF input_data.name IS NOT NULL AND LOWER(input_data.name) != LOWER(v_existing_tag.name) THEN
        SELECT pk_tag INTO v_conflict_id
        FROM tb_tag
        WHERE LOWER(name) = LOWER(input_data.name)
          AND pk_tag != input_pk_tag
        LIMIT 1;

        IF v_conflict_id IS NOT NULL THEN
            v_op := 'NOOP';
            v_status := 'noop:name_conflict';
            v_message := 'Tag name already exists.';
            v_reason := 'unique_constraint_violation';
            v_fields := ARRAY[]::TEXT[];
            v_extra_metadata := jsonb_build_object(
                'trigger', 'api_update',
                'status', v_status,
                'reason', v_reason,
                'conflict', jsonb_build_object(
                    'name', input_data.name,
                    'conflict_id', v_conflict_id
                )
            );

            RETURN core.log_and_return_mutation(
                input_updated_by,
                v_entity,
                input_pk_tag,
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

    -- Update tag with only provided fields
    UPDATE tb_tag SET
        identifier = COALESCE(input_data.identifier, identifier),
        name = COALESCE(input_data.name, name),
        description = COALESCE(input_data.description, description),
        color = COALESCE(input_data.color, color),
        metadata = COALESCE(input_data.metadata, metadata),
        updated_by = input_updated_by
    WHERE pk_tag = input_pk_tag;

    -- Get updated payload from view
    SELECT row_to_json(v) INTO v_payload_after
    FROM v_tag v
    WHERE v.id = input_pk_tag;

    v_op := 'UPDATE';
    v_status := 'updated';
    v_message := 'Tag updated successfully.';
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
        input_pk_tag,
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
