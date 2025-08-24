-- Delete Tag CRUD Functions
-- Following PrintOptim app/core pattern

-- ===========================================================================
-- APP LAYER: Simple wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.delete_tag(
    input_pk_tag UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
BEGIN
    RETURN core.delete_tag(input_pk_tag, input_deleted_by);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.delete_tag(
    input_pk_tag UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'tag';
    v_fields TEXT[] := ARRAY['deleted'];

    v_payload_before JSONB;
    v_existing_tag tb_tag;
    v_post_count INTEGER;

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
            'trigger', 'api_delete',
            'status', v_status,
            'reason', v_reason,
            'input_id', input_pk_tag
        );

        RETURN core.log_and_return_mutation(
            input_deleted_by,
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

    -- Get current payload before deletion
    SELECT row_to_json(v) INTO v_payload_before
    FROM v_tag v
    WHERE v.id = input_pk_tag;

    -- Check if tag is used by posts
    SELECT COUNT(*) INTO v_post_count
    FROM tb_post_tag
    WHERE fk_tag = input_pk_tag;

    IF v_post_count > 0 THEN
        v_op := 'NOOP';
        v_status := 'noop:has_posts';
        v_message := 'Cannot delete tag that is used by posts.';
        v_reason := 'referential_constraint';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_delete',
            'status', v_status,
            'reason', v_reason,
            'post_count', v_post_count
        );

        RETURN core.log_and_return_mutation(
            input_deleted_by,
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

    -- Delete tag
    DELETE FROM tb_tag
    WHERE pk_tag = input_pk_tag;

    v_op := 'DELETE';
    v_status := 'deleted';
    v_message := 'Tag deleted successfully.';
    v_reason := 'entity_deleted';
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_delete',
        'status', v_status,
        'reason', v_reason,
        'deleted_id', input_pk_tag
    );

    RETURN core.log_and_return_mutation(
        input_deleted_by,
        v_entity,
        input_pk_tag,
        v_op,
        v_status,
        v_fields,
        v_message,
        v_payload_before,
        NULL,
        v_extra_metadata
    );
END;
$$;
