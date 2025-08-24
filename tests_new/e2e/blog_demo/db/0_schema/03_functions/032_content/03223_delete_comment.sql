-- Delete Comment CRUD Functions
-- Following PrintOptim app/core pattern

-- ===========================================================================
-- APP LAYER: Simple wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.delete_comment(
    input_pk_comment UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
BEGIN
    RETURN core.delete_comment(input_pk_comment, input_deleted_by);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.delete_comment(
    input_pk_comment UUID,
    input_deleted_by UUID
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'comment';
    v_fields TEXT[] := ARRAY['deleted'];

    v_payload_before JSONB;
    v_existing_comment tb_comment;
    v_child_count INTEGER;

    v_op TEXT;
    v_status TEXT;
    v_message TEXT;
    v_reason TEXT;
    v_extra_metadata JSONB;
BEGIN
    -- Check if comment exists and get current state
    SELECT * INTO v_existing_comment
    FROM tb_comment
    WHERE pk_comment = input_pk_comment;

    IF NOT FOUND THEN
        v_op := 'NOOP';
        v_status := 'noop:not_found';
        v_message := 'Comment not found.';
        v_reason := 'entity_not_found';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_delete',
            'status', v_status,
            'reason', v_reason,
            'input_id', input_pk_comment
        );

        RETURN core.log_and_return_mutation(
            input_deleted_by,
            v_entity,
            input_pk_comment,
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
    FROM v_comment v
    WHERE v.id = input_pk_comment;

    -- Check if comment has child comments
    SELECT COUNT(*) INTO v_child_count
    FROM tb_comment
    WHERE fk_parent = input_pk_comment;

    IF v_child_count > 0 THEN
        v_op := 'NOOP';
        v_status := 'noop:has_replies';
        v_message := 'Cannot delete comment with replies.';
        v_reason := 'referential_constraint';
        v_fields := ARRAY[]::TEXT[];
        v_extra_metadata := jsonb_build_object(
            'trigger', 'api_delete',
            'status', v_status,
            'reason', v_reason,
            'child_count', v_child_count
        );

        RETURN core.log_and_return_mutation(
            input_deleted_by,
            v_entity,
            input_pk_comment,
            v_op,
            v_status,
            v_fields,
            v_message,
            v_payload_before,
            v_payload_before,
            v_extra_metadata
        );
    END IF;

    -- Delete comment
    DELETE FROM tb_comment
    WHERE pk_comment = input_pk_comment;

    v_op := 'DELETE';
    v_status := 'deleted';
    v_message := 'Comment deleted successfully.';
    v_reason := 'entity_deleted';
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_delete',
        'status', v_status,
        'reason', v_reason,
        'deleted_id', input_pk_comment,
        'post_id', v_existing_comment.fk_post
    );

    RETURN core.log_and_return_mutation(
        input_deleted_by,
        v_entity,
        input_pk_comment,
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
