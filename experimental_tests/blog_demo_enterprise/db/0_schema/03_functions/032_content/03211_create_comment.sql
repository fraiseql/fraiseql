-- Create Comment Function (Multi-tenant)
-- Blog comment creation with tenant isolation and hierarchy support

CREATE OR REPLACE FUNCTION app.create_comment(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_comment_input;
    v_comment_id UUID;
    v_post_check INTEGER;
    v_parent_check INTEGER;
    v_result_data JSONB;
    v_comment_data JSONB;
    v_post_id UUID;
    v_parent_comment_id UUID;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_comment_input, input_payload);

    -- Extract post ID and parent comment ID from payload
    v_post_id := (input_payload->>'fk_post')::UUID;
    v_parent_comment_id := (input_payload->>'parent_comment_id')::UUID;

    -- Validate required fields
    IF v_input.content IS NULL OR length(trim(v_input.content)) = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Comment content is required',
            NULL,
            'MISSING_CONTENT',
            jsonb_build_object('field', 'content'),
            'create_comment',
            input_created_by,
            input_pk_organization
        );
    END IF;

    IF v_post_id IS NULL THEN
        RETURN app.log_and_return_mutation(
            false,
            'Post ID is required',
            NULL,
            'MISSING_POST_ID',
            jsonb_build_object('field', 'fk_post'),
            'create_comment',
            input_created_by,
            input_pk_organization
        );
    END IF;

    -- Validate post exists within tenant
    SELECT COUNT(*) INTO v_post_check
    FROM tenant.tb_post
    WHERE pk_post = v_post_id
      AND fk_organization = input_pk_organization;

    IF v_post_check = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Post not found or access denied',
            NULL,
            'INVALID_POST',
            jsonb_build_object('post_id', v_post_id, 'organization_id', input_pk_organization),
            'create_comment',
            input_created_by,
            input_pk_organization
        );
    END IF;

    -- Validate parent comment exists within same tenant and post (if provided)
    IF v_parent_comment_id IS NOT NULL THEN
        SELECT COUNT(*) INTO v_parent_check
        FROM tenant.tb_comment
        WHERE pk_comment = v_parent_comment_id
          AND fk_organization = input_pk_organization
          AND fk_post = v_post_id;

        IF v_parent_check = 0 THEN
            RETURN app.log_and_return_mutation(
                false,
                'Parent comment not found or invalid',
                NULL,
                'INVALID_PARENT_COMMENT',
                jsonb_build_object('parent_comment_id', v_parent_comment_id),
                'create_comment',
                input_created_by,
                input_pk_organization
            );
        END IF;
    END IF;

    -- Validate author belongs to organization
    IF NOT EXISTS (
        SELECT 1 FROM tenant.tb_user
        WHERE pk_user = input_created_by
          AND fk_organization = input_pk_organization
    ) THEN
        RETURN app.log_and_return_mutation(
            false,
            'Author must belong to the organization',
            NULL,
            'INVALID_AUTHOR',
            jsonb_build_object('author_id', input_created_by),
            'create_comment',
            input_created_by,
            input_pk_organization
        );
    END IF;

    -- Generate new comment ID
    v_comment_id := uuid_generate_v4();

    -- Build comment data JSONB
    v_comment_data := jsonb_build_object(
        'content', trim(v_input.content),
        'status', 'pending',  -- Default to pending for moderation
        'metadata', COALESCE(v_input.metadata, '{}')
    );

    -- Insert comment
    INSERT INTO tenant.tb_comment (
        pk_comment,
        fk_organization,
        fk_post,
        fk_author,
        fk_parent_comment,
        identifier,
        data,
        created_by,
        updated_by
    ) VALUES (
        v_comment_id,
        input_pk_organization,
        v_post_id,
        input_created_by,
        v_parent_comment_id,
        'comment-' || extract(epoch from now())::text,
        v_comment_data,
        input_created_by,
        input_created_by
    );

    -- Build result data
    SELECT jsonb_build_object(
        'pk_comment', c.pk_comment,
        'id', c.pk_comment,
        'content', c.data->>'content',
        'status', c.data->>'status',
        'organizationId', c.fk_organization,
        'post', jsonb_build_object(
            'id', p.pk_post,
            'title', p.data->>'title',
            'organizationId', p.fk_organization
        ),
        'author', jsonb_build_object(
            'id', u.pk_user,
            'name', u.profile->>'display_name',
            'organizationId', u.fk_organization
        ),
        'parent_comment_id', c.fk_parent_comment,
        'created_at', c.created_at,
        'updated_at', c.updated_at
    ) INTO v_result_data
    FROM tenant.tb_comment c
    JOIN tenant.tb_post p ON p.pk_post = c.fk_post
    JOIN tenant.tb_user u ON u.pk_user = c.fk_author
    WHERE c.pk_comment = v_comment_id;

    -- Return success
    RETURN app.log_and_return_mutation(
        true,
        'Comment created successfully',
        v_result_data,
        NULL,
        jsonb_build_object(
            'comment_id', v_comment_id,
            'post_id', v_post_id,
            'organization_id', input_pk_organization,
            'parent_comment_id', v_parent_comment_id
        ),
        'create_comment',
        input_created_by,
        input_pk_organization
    );

EXCEPTION
    WHEN OTHERS THEN
        RETURN app.log_and_return_mutation(
            false,
            'Failed to create comment: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE
            ),
            'create_comment',
            input_created_by,
            input_pk_organization
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
