-- Create Post CRUD Functions
-- Following PrintOptim app/core pattern with JSONB input handling

-- ===========================================================================
-- APP LAYER: JSONB input wrapper
-- ===========================================================================
CREATE OR REPLACE FUNCTION app.create_post(
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_input app.type_post_input;
BEGIN
    v_input := jsonb_populate_record(NULL::app.type_post_input, input_payload);
    RETURN core.create_post(input_created_by, v_input, input_payload);
END;
$$;

-- ===========================================================================
-- CORE LAYER: Business logic implementation
-- ===========================================================================
CREATE OR REPLACE FUNCTION core.create_post(
    input_created_by UUID,
    input_data app.type_post_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_entity TEXT := 'post';
    v_id UUID := gen_random_uuid();
    v_fields TEXT[] := ARRAY(SELECT jsonb_object_keys(input_payload));

    v_payload_after JSONB;
    v_existing_id UUID;
    v_invalid_author_id BOOLEAN := FALSE;
    v_invalid_tag_ids UUID[];

    v_op TEXT;
    v_status TEXT;
    v_message TEXT;
    v_reason TEXT;
    v_extra_metadata JSONB;
BEGIN
    -- Validate author exists
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
                'trigger', 'api_create',
                'status', v_status,
                'reason', v_reason,
                'invalid_tag_ids', v_invalid_tag_ids
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

    -- Check for existing post by identifier (slug)
    SELECT pk_post INTO v_existing_id
    FROM tb_post
    WHERE identifier = input_data.identifier
    LIMIT 1;

    IF v_existing_id IS NOT NULL THEN
        v_op := 'NOOP';
        v_status := 'noop:already_exists';
        v_message := 'Post with this slug already exists.';
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

    -- Set published_at if status is published and not provided
    IF input_data.status = 'published' AND input_data.published_at IS NULL THEN
        input_data.published_at := NOW();
    END IF;

    -- Insert new post
    INSERT INTO tb_post (
        pk_post,
        identifier,
        fk_author,
        title,
        content,
        excerpt,
        status,
        featured,
        published_at,
        seo_metadata,
        custom_fields,
        created_by
    ) VALUES (
        v_id,
        input_data.identifier,
        input_data.fk_author,
        input_data.title,
        input_data.content,
        input_data.excerpt,
        COALESCE(input_data.status, 'draft'::post_status),
        COALESCE(input_data.featured, false),
        input_data.published_at,
        COALESCE(input_data.seo_metadata, '{}'::JSONB),
        COALESCE(input_data.custom_fields, '{}'::JSONB),
        input_created_by
    );

    -- Insert tag associations if provided
    IF input_data.tag_ids IS NOT NULL AND array_length(input_data.tag_ids, 1) > 0 THEN
        INSERT INTO tb_post_tag (fk_post, fk_tag)
        SELECT v_id, unnest(input_data.tag_ids);
    END IF;

    -- Get final payload from view
    SELECT row_to_json(v) INTO v_payload_after
    FROM v_post v
    WHERE v.id = v_id;

    v_op := 'INSERT';
    v_status := 'new';
    v_message := 'Post created successfully.';
    v_reason := 'new_entity_created';
    v_extra_metadata := jsonb_build_object(
        'trigger', 'api_create',
        'status', v_status,
        'reason', v_reason,
        'input_payload', core.sanitize_jsonb_unset(input_payload),
        'updated_fields', v_fields,
        'tag_count', COALESCE(array_length(input_data.tag_ids, 1), 0)
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
