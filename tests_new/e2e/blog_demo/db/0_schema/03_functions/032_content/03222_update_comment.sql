-- Update Comment CRUD Functions
-- Following PrintOptim app/core pattern with JSONB input handling

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.update_comment(
    input_pk_comment UUID,
    input_updated_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_comment_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_comment_input, input_payload);
    RETURN core.update_comment(input_pk_comment, input_updated_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.update_comment(
    input_pk_comment UUID,
    input_updated_by UUID,
    input_data app.type_comment_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'comment';
    v_fields TEXT[] := ARRAY(SELECT jsonb_object_keys(input_payload));

    v_payload_before JSONB;
    v_payload_after JSONB;
    v_existing_comment tb_comment;
    v_invalid_post_id BOOLEAN := FALSE;
    v_invalid_parent_id BOOLEAN := FALSE;
    v_invalid_author_id BOOLEAN := FALSE;

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
            'trigger', 'api_update',
            'status', v_status,
            'reason', v_reason,
            'input_id', input_pk_comment
        );

        RETURN core.log_and_return_mutation(
            input_updated_by,
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

    -- Get current payload before update
    SELECT row_to_json(v) INTO v_payload_before
    FROM v_comment v
    WHERE v.id = input_pk_comment;

    -- Validate post exists (if being changed)
    IF input_data.fk_post IS NOT NULL AND input_data.fk_post != v_existing_comment.fk_post THEN
        IF NOT EXISTS (SELECT 1 FROM tb_post WHERE pk_post = input_data.fk_post) THEN
            v_invalid_post_id := TRUE;
        END IF;

        IF v_invalid_post_id THEN
            v_op := 'NOOP';
            v_status := 'noop:invalid_post';
            v_message := 'Post does not exist.';
            v_reason := 'referential_constraint';
            v_fields := ARRAY[]::TEXT[];
            v_extra_metadata := jsonb_build_object(
                'trigger', 'api_update',
                'status', v_status,
                'reason', v_reason,
                'invalid_post_id', input_data.fk_post
            );

            RETURN core.log_and_return_mutation(
                input_updated_by,
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
    END IF;

    -- Validate author exists (if being changed and not null)
    IF input_data.fk_author IS NOT NULL AND input_data.fk_author != COALESCE(v_existing_comment.fk_author, '00000000-0000-0000-0000-000000000000'::UUID) THEN
        IF NOT EXISTS (SELECT 1 FROM tb_user WHERE pk_user = input_data.fk_author) THEN
            v_invalid_author_id := TRUE;
        END IF;

        IF v_invalid_author_id THEN
            v_op := 'NOOP';
            v_status := 'noop:invalid_author';
            v_message := 'Author does not exist.';
            v_reason := 'referential_constraint';
            v_fields := ARRAY[]::TEXT[];
            v_extra_metadata := jsonb_build_object(
                'trigger', 'api_update',
                'status', v_status,
                'reason', v_reason,
                'invalid_author_id', input_data.fk_author
            );

            RETURN core.log_and_return_mutation(
                input_updated_by,
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
    END IF;

    -- Validate parent comment exists and belongs to same post (if being changed)
    IF input_data.fk_parent IS NOT NULL AND input_data.fk_parent != COALESCE(v_existing_comment.fk_parent, '00000000-0000-0000-0000-000000000000'::UUID) THEN
        IF NOT EXISTS (
            SELECT 1 FROM tb_comment
            WHERE pk_comment = input_data.fk_parent
              AND fk_post = COALESCE(input_data.fk_post, v_existing_comment.fk_post)
        ) THEN
            v_invalid_parent_id := TRUE;
        END IF;

        IF v_invalid_parent_id THEN
            v_op := 'NOOP';
            v_status := 'noop:invalid_parent';
            v_message := 'Parent comment does not exist or belongs to different post.';
            v_reason := 'referential_constraint';
            v_fields := ARRAY[]::TEXT[];
            v_extra_metadata := jsonb_build_object(
                'trigger', 'api_update',
                'status', v_status,
                'reason', v_reason,
                'invalid_parent_id', input_data.fk_parent,
                'post_id', COALESCE(input_data.fk_post, v_existing_comment.fk_post)
            );

            RETURN core.log_and_return_mutation(
                input_updated_by,
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
    END IF;

    -- Update comment with only provided fields
    UPDATE tb_comment SET
        fk_post = COALESCE(input_data.fk_post, fk_post),
        fk_parent = CASE
            WHEN 'fk_parent' = ANY(SELECT jsonb_object_keys(input_payload))
            THEN input_data.fk_parent
            ELSE fk_parent
        END,
        fk_author = CASE
            WHEN 'fk_author' = ANY(SELECT jsonb_object_keys(input_payload))
            THEN input_data.fk_author
            ELSE fk_author
        END,
        content = COALESCE(input_data.content, content),
        status = COALESCE(input_data.status, status),
        metadata = COALESCE(input_data.metadata, metadata),
        updated_by = input_updated_by
    WHERE pk_comment = input_pk_comment;

    -- Get updated payload from view
    SELECT row_to_json(v) INTO v_payload_after
    FROM v_comment v
    WHERE v.id = input_pk_comment;

    v_op := 'UPDATE';
    v_status := 'updated';
    v_message := 'Comment updated successfully.';
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
        input_pk_comment,
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
