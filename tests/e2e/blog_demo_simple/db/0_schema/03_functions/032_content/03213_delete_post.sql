-- Delete Post CRUD Functions
-- Following PrintOptim app/core pattern

-- ===========================================================================
-- APP LAYER: Simple wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.delete_post(
    input_pk_post UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
BEGIN
    RETURN core.delete_post(input_pk_post, input_deleted_by);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.delete_post(
    input_pk_post UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'post';
    v_fields TEXT[] := ARRAY['deleted'];

    v_payload_before JSONB;
    v_existing_post tb_post;

    v_op TEXT;
    v_status TEXT;
    v_message TEXT;
    v_reason TEXT;
    v_extra_metadata JSONB;
BEGIN
    -- Check if post exists and get current state
    SELECT * INTO v_existing_post
    FROM tb_post
    WHERE pk_post = input_pk_post;

    IF NOT FOUND THEN
        v_op := 'NOOP';
        v_status := 'noop:not_found';
        v_message := 'Post not found.';
        v_reason := 'entity_not_found';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_delete',
            'status', v_status,
            'reason', v_reason,
            'input_id', input_pk_post
        );

        RETURN core.log_and_return_mutation(
            input_deleted_by,
            v_entity,
            input_pk_post,
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
    FROM v_post v
    WHERE v.id = input_pk_post;

    -- Delete post (CASCADE will handle comments and post_tag associations)
    DELETE FROM tb_post
    WHERE pk_post = input_pk_post;

    v_op := 'DELETE';
    v_status := 'deleted';
    v_message := 'Post deleted successfully.';
    v_reason := 'entity_deleted';
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_delete',
        'status', v_status,
        'reason', v_reason,
        'deleted_id', input_pk_post
    );

    RETURN core.log_and_return_mutation(
        input_deleted_by,
        v_entity,
        input_pk_post,
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
