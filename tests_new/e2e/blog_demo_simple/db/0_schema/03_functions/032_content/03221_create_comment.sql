-- Create Comment CRUD Functions
-- Following PrintOptim app/core pattern with JSONB input handling

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.create_comment(
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_comment_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_comment_input, input_payload);
    RETURN core.create_comment(input_created_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.create_comment(
    input_created_by UUID,
    input_data app.type_comment_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'comment';
    v_id UUID := gen_random_uuid();
    v_fields TEXT[] := ARRAY(SELECT jsonb_object_keys(input_payload));

    v_payload_after JSONB;
    v_invalid_post_id BOOLEAN := FALSE;
    v_invalid_parent_id BOOLEAN := FALSE;
    v_invalid_author_id BOOLEAN := FALSE;

    v_op TEXT;
    v_status TEXT;
    v_message TEXT;
    v_reason TEXT;
    v_extra_metadata JSONB;
BEGIN
    -- Validate post exists
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
            'trigger', 'api_create',
            'status', v_status,
            'reason', v_reason,
            'invalid_post_id', input_data.fk_post
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
            NULL,
            v_extra_metadata
        );
    END IF;

    -- Validate author exists (if provided - can be anonymous)
    IF input_data.fk_author IS NOT NULL THEN
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
                'trigger', 'api_create',
                'status', v_status,
                'reason', v_reason,
                'invalid_author_id', input_data.fk_author
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
                NULL,
                v_extra_metadata
            );
        END IF;
    END IF;

    -- Validate parent comment exists and belongs to same post (if provided)
    IF input_data.fk_parent IS NOT NULL THEN
        IF NOT EXISTS (
            SELECT 1 FROM tb_comment
            WHERE pk_comment = input_data.fk_parent
              AND fk_post = input_data.fk_post
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
                'trigger', 'api_create',
                'status', v_status,
                'reason', v_reason,
                'invalid_parent_id', input_data.fk_parent,
                'post_id', input_data.fk_post
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
                NULL,
                v_extra_metadata
            );
        END IF;
    END IF;

    -- Insert new comment
    INSERT INTO tb_comment (
        pk_comment,
        fk_post,
        fk_parent,
        fk_author,
        content,
        status,
        metadata,
        created_by
    ) VALUES (
        v_id,
        input_data.fk_post,
        input_data.fk_parent,
        input_data.fk_author,
        input_data.content,
        COALESCE(input_data.status, 'pending'::comment_status),
        COALESCE(input_data.metadata, '{}'::JSONB),
        input_created_by
    );

    -- Get final payload from view
    SELECT row_to_json(v) INTO v_payload_after
    FROM v_comment v
    WHERE v.id = v_id;

    v_op := 'INSERT';
    v_status := 'new';
    v_message := 'Comment created successfully.';
    v_reason := 'new_entity_created';
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_create',
        'status', v_status,
        'reason', v_reason,
        'input_payload', core.sanitize_jsonb_unset(input_payload),
        'updated_fields', v_fields,
        'is_reply', input_data.fk_parent IS NOT NULL
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
