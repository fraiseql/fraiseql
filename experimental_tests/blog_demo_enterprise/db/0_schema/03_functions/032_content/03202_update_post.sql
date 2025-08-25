-- Update Post Function (Multi-tenant)
-- Update blog post with tenant isolation

CREATE OR REPLACE FUNCTION app.update_post(
    input_pk_post UUID,
    input_pk_organization UUID,
    input_updated_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_post_input;
    v_existing_post tenant.tb_post;
    v_result_data JSONB;
    v_conflict_id UUID;
    v_new_slug TEXT;
    v_post_data JSONB;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_post_input, input_payload);

    -- Validate tenant context and check if post exists
    SELECT * INTO v_existing_post
    FROM tenant.tb_post
    WHERE pk_post = input_pk_post
      AND fk_organization = input_pk_organization;

    IF v_existing_post.pk_post IS NULL THEN
        RETURN app.log_and_return_mutation(
            false,
            'Post not found or access denied',
            NULL,
            'POST_NOT_FOUND',
            jsonb_build_object(
                'post_id', input_pk_post,
                'organization_id', input_pk_organization
            ),
            'update_post',
            input_updated_by,
            input_pk_organization
        );
    END IF;

    -- Generate new slug if title is being updated
    IF v_input.title IS NOT NULL THEN
        v_new_slug := lower(regexp_replace(
            regexp_replace(trim(v_input.title), '[^a-zA-Z0-9\s-]', '', 'g'),
            '\s+', '-', 'g'
        ));

        -- Ensure slug is not empty
        IF v_new_slug IS NULL OR v_new_slug = '' THEN
            v_new_slug := 'post-' || extract(epoch from now())::text;
        END IF;

        -- Check for slug conflicts within tenant (if different from current)
        IF v_new_slug != v_existing_post.identifier THEN
            SELECT pk_post INTO v_conflict_id
            FROM tenant.tb_post
            WHERE fk_organization = input_pk_organization
              AND identifier = v_new_slug
              AND pk_post != input_pk_post;

            IF v_conflict_id IS NOT NULL THEN
                v_new_slug := v_new_slug || '-' || extract(epoch from now())::text;
            END IF;
        END IF;
    ELSE
        v_new_slug := v_existing_post.identifier;
    END IF;

    -- Build updated post data, merging with existing data
    v_post_data := v_existing_post.data;

    -- Update individual fields if provided
    IF v_input.title IS NOT NULL THEN
        v_post_data := v_post_data || jsonb_build_object('title', trim(v_input.title));
    END IF;

    IF v_input.content IS NOT NULL THEN
        v_post_data := v_post_data || jsonb_build_object('content', trim(v_input.content));
    END IF;

    IF v_input.excerpt IS NOT NULL THEN
        v_post_data := v_post_data || jsonb_build_object('excerpt', trim(v_input.excerpt));
    END IF;

    IF v_input.status IS NOT NULL THEN
        v_post_data := v_post_data || jsonb_build_object('status', v_input.status::text);

        -- Auto-set published_at when publishing
        IF v_input.status::text = 'published' AND (v_existing_post.data->>'status') != 'published' THEN
            v_post_data := v_post_data || jsonb_build_object('published_at', NOW());
        END IF;
    END IF;

    IF v_input.featured IS NOT NULL THEN
        v_post_data := v_post_data || jsonb_build_object('featured', v_input.featured);
    END IF;

    IF v_input.published_at IS NOT NULL THEN
        v_post_data := v_post_data || jsonb_build_object('published_at', v_input.published_at);
    END IF;

    IF v_input.seo_metadata IS NOT NULL THEN
        v_post_data := v_post_data || jsonb_build_object('seo_metadata', v_input.seo_metadata);
    END IF;

    IF v_input.custom_fields IS NOT NULL THEN
        v_post_data := v_post_data || jsonb_build_object('custom_fields', v_input.custom_fields);
    END IF;

    -- Update post
    UPDATE tenant.tb_post SET
        identifier = v_new_slug,
        data = v_post_data,
        updated_by = input_updated_by
    WHERE pk_post = input_pk_post
      AND fk_organization = input_pk_organization;

    -- Build result data
    SELECT jsonb_build_object(
        'pk_post', p.pk_post,
        'id', p.pk_post,
        'title', p.data->>'title',
        'slug', p.identifier,
        'content', p.data->>'content',
        'excerpt', p.data->>'excerpt',
        'status', p.data->>'status',
        'featured', (p.data->>'featured')::boolean,
        'published_at', p.data->>'published_at',
        'organizationId', p.fk_organization,
        'author', jsonb_build_object(
            'id', u.pk_user,
            'name', u.profile->>'display_name',
            'organizationId', u.fk_organization
        ),
        'seo_metadata', p.data->'seo_metadata',
        'custom_fields', p.data->'custom_fields',
        'created_at', p.created_at,
        'updated_at', p.updated_at
    ) INTO v_result_data
    FROM tenant.tb_post p
    JOIN tenant.tb_user u ON u.pk_user = p.fk_author
    WHERE p.pk_post = input_pk_post;

    -- Return success
    RETURN app.log_and_return_mutation(
        true,
        'Post updated successfully',
        v_result_data,
        NULL,
        jsonb_build_object(
            'post_id', input_pk_post,
            'organization_id', input_pk_organization,
            'slug', v_new_slug
        ),
        'update_post',
        input_updated_by,
        input_pk_organization
    );

EXCEPTION
    WHEN OTHERS THEN
        RETURN app.log_and_return_mutation(
            false,
            'Failed to update post: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE
            ),
            'update_post',
            input_updated_by,
            input_pk_organization
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
