-- Create Post Function (Multi-tenant)
-- Blog post creation with tenant isolation following PrintOptim patterns

CREATE OR REPLACE FUNCTION app.create_post(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_post_input;
    v_post_id UUID;
    v_slug TEXT;
    v_result_data JSONB;
    v_existing_count INTEGER;
    v_post_data JSONB;
    v_author_check INTEGER;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_post_input, input_payload);

    -- Validate required fields
    IF v_input.title IS NULL OR length(trim(v_input.title)) = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Post title is required',
            NULL,
            'MISSING_TITLE',
            jsonb_build_object('field', 'title'),
            'create_post',
            input_created_by,
            input_pk_organization
        );
    END IF;

    IF v_input.content IS NULL OR length(trim(v_input.content)) = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Post content is required',
            NULL,
            'MISSING_CONTENT',
            jsonb_build_object('field', 'content'),
            'create_post',
            input_created_by,
            input_pk_organization
        );
    END IF;

    -- Validate author belongs to organization
    SELECT COUNT(*) INTO v_author_check
    FROM tenant.tb_user
    WHERE pk_user = input_created_by
    AND fk_organization = input_pk_organization;

    IF v_author_check = 0 THEN
        RETURN app.log_and_return_mutation(
            false,
            'Author must belong to the organization',
            NULL,
            'INVALID_AUTHOR',
            jsonb_build_object('author_id', input_created_by, 'organization_id', input_pk_organization),
            'create_post',
            input_created_by,
            input_pk_organization
        );
    END IF;

    -- Generate slug from title
    v_slug := lower(regexp_replace(
        regexp_replace(trim(v_input.title), '[^a-zA-Z0-9\s-]', '', 'g'),
        '\s+', '-', 'g'
    ));

    -- Ensure slug is not empty
    IF v_slug IS NULL OR v_slug = '' THEN
        v_slug := 'post-' || extract(epoch from now())::text;
    END IF;

    -- Check for duplicate slug within tenant
    SELECT COUNT(*) INTO v_existing_count
    FROM tenant.tb_post
    WHERE fk_organization = input_pk_organization
    AND identifier = v_slug;

    -- Make slug unique if needed
    IF v_existing_count > 0 THEN
        v_slug := v_slug || '-' || extract(epoch from now())::text;
    END IF;

    -- Generate new post ID
    v_post_id := uuid_generate_v4();

    -- Build post data JSONB following PrintOptim pattern
    v_post_data := jsonb_build_object(
        'title', trim(v_input.title),
        'content', trim(v_input.content),
        'excerpt', COALESCE(trim(v_input.excerpt), ''),
        'status', COALESCE(v_input.status::text, 'draft'),
        'featured', COALESCE(v_input.featured, false),
        'published_at', CASE
            WHEN COALESCE(v_input.status::text, 'draft') = 'published'
            THEN COALESCE(v_input.published_at, NOW())
            ELSE v_input.published_at
        END,
        'seo_metadata', COALESCE(v_input.seo_metadata, '{}'),
        'custom_fields', COALESCE(v_input.custom_fields, '{}')
    );

    -- Insert post
    INSERT INTO tenant.tb_post (
        pk_post,
        fk_organization,
        fk_author,
        identifier,
        data,
        created_by,
        updated_by
    ) VALUES (
        v_post_id,
        input_pk_organization,
        input_created_by,
        v_slug,
        v_post_data,
        input_created_by,
        input_created_by
    );

    -- Build result data with GraphQL-friendly field names
    SELECT jsonb_build_object(
        'pk_post', p.pk_post,
        'id', p.pk_post, -- GraphQL uses 'id' field
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
    WHERE p.pk_post = v_post_id;

    -- Return success
    RETURN app.log_and_return_mutation(
        true,
        'Post created successfully',
        v_result_data,
        NULL,
        jsonb_build_object(
            'post_id', v_post_id,
            'slug', v_slug,
            'organization_id', input_pk_organization
        ),
        'create_post',
        input_created_by,
        input_pk_organization
    );

EXCEPTION
    WHEN OTHERS THEN
        -- Handle unexpected errors
        RETURN app.log_and_return_mutation(
            false,
            'Failed to create post: ' || SQLERRM,
            NULL,
            'INTERNAL_ERROR',
            jsonb_build_object(
                'sql_error', SQLERRM,
                'sql_state', SQLSTATE
            ),
            'create_post',
            input_created_by,
            input_pk_organization
        );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
