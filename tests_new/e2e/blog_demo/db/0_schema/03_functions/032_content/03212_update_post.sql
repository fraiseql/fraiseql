-- Update Post CRUD Functions
-- Following PrintOptim app/core pattern with JSONB input handling

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.update_post(
    input_pk_post UUID,
    input_updated_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_post_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_post_input, input_payload);
    RETURN core.update_post(input_pk_post, input_updated_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.update_post(
    input_pk_post UUID,
    input_updated_by UUID,
    input_data app.type_post_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'post';
    v_fields TEXT[] := ARRAY(SELECT jsonb_object_keys(input_payload));

    v_payload_before JSONB;
    v_payload_after JSONB;
    v_existing_post tb_post;
    v_conflict_id UUID;
    v_invalid_author_id BOOLEAN := FALSE;
    v_invalid_tag_ids UUID[];

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
            'trigger', 'api_update',
            'status', v_status,
            'reason', v_reason,
            'input_id', input_pk_post
        );

        RETURN core.log_and_return_mutation(
            input_updated_by,
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

    -- Get current payload before update
    SELECT row_to_json(v) INTO v_payload_before
    FROM v_post v
    WHERE v.id = input_pk_post;

    -- Validate author exists (if being changed)
    IF input_data.fk_author IS NOT NULL AND input_data.fk_author != v_existing_post.fk_author THEN
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
                input_pk_post,
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

    -- Check for identifier conflicts (if identifier is being changed)
    IF input_data.identifier IS NOT NULL AND input_data.identifier != v_existing_post.identifier THEN
        SELECT pk_post INTO v_conflict_id
        FROM tb_post
        WHERE identifier = input_data.identifier
          AND pk_post != input_pk_post
        LIMIT 1;

        IF v_conflict_id IS NOT NULL THEN
            v_op := 'NOOP';
            v_status := 'noop:identifier_conflict';
            v_message := 'Post slug already exists.';
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
                input_pk_post,
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

    -- Validate tag IDs if provided
    IF input_data.tag_ids IS NOT NULL AND array_length(input_data.tag_ids, 1) > 0 THEN
        WITH valid_tags AS (
            SELECT pk_tag
            FROM tb_tag
            WHERE pk_tag = ANY(input_data.tag_ids)
        ),
        invalid_tags AS (
            SELECT unnest(input_data.tag_ids) AS id
            EXCEPT
            SELECT pk_tag FROM valid_tags
        )
        SELECT array_agg(id) INTO v_invalid_tag_ids FROM invalid_tags;

        IF v_invalid_tag_ids IS NOT NULL THEN
            v_op := 'NOOP';
            v_status := 'noop:invalid_tags';
            v_message := 'Some tag IDs are invalid.';
            v_reason := 'referential_constraint';
            v_fields := ARRAY[]::TEXT[];
            v_extra_metadata := jsonb_build_object(
                'trigger', 'api_update',
                'status', v_status,
                'reason', v_reason,
                'invalid_tag_ids', v_invalid_tag_ids
            );

            RETURN core.log_and_return_mutation(
                input_updated_by,
                v_entity,
                input_pk_post,
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

    -- Handle published_at logic
    IF input_data.status IS NOT NULL THEN
        -- If changing to published and no published_at provided, set to now
        IF input_data.status = 'published' AND v_existing_post.status != 'published' AND input_data.published_at IS NULL THEN
            input_data.published_at := NOW();
        -- If changing from published to draft, clear published_at unless explicitly provided
        ELSIF input_data.status != 'published' AND v_existing_post.status = 'published' AND input_data.published_at IS NULL THEN
            input_data.published_at := NULL;
        END IF;
    END IF;

    -- Update post with only provided fields
    UPDATE tb_post SET
        identifier = COALESCE(input_data.identifier, identifier),
        fk_author = COALESCE(input_data.fk_author, fk_author),
        title = COALESCE(input_data.title, title),
        content = COALESCE(input_data.content, content),
        excerpt = COALESCE(input_data.excerpt, excerpt),
        status = COALESCE(input_data.status, status),
        featured = COALESCE(input_data.featured, featured),
        published_at = CASE
            WHEN 'published_at' = ANY(SELECT jsonb_object_keys(input_payload))
            THEN input_data.published_at
            ELSE published_at
        END,
        seo_metadata = COALESCE(input_data.seo_metadata, seo_metadata),
        custom_fields = COALESCE(input_data.custom_fields, custom_fields),
        updated_by = input_updated_by
    WHERE pk_post = input_pk_post;

    -- Update tag associations if provided
    IF 'tag_ids' = ANY(SELECT jsonb_object_keys(input_payload)) THEN
        -- Remove existing associations
        DELETE FROM tb_post_tag WHERE fk_post = input_pk_post;

        -- Add new associations
        IF input_data.tag_ids IS NOT NULL AND array_length(input_data.tag_ids, 1) > 0 THEN
            INSERT INTO tb_post_tag (fk_post, fk_tag)
            SELECT input_pk_post, unnest(input_data.tag_ids);
        END IF;
    END IF;

    -- Get updated payload from view
    SELECT row_to_json(v) INTO v_payload_after
    FROM v_post v
    WHERE v.id = input_pk_post;

    v_op := 'UPDATE';
    v_status := 'updated';
    v_message := 'Post updated successfully.';
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
        input_pk_post,
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
