-- Enhanced Blog Functions - Clean FraiseQL Pattern Support
-- Demonstrates enterprise-ready PostgreSQL functions with comprehensive error handling
-- Following clean patterns without "Enhanced" or "Optimized" prefixes

-- ============================================================================
-- AUTHOR FUNCTIONS - User management
-- ============================================================================

-- Create author with comprehensive validation and duplicate handling
CREATE OR REPLACE FUNCTION app.create_author(
    input_user_id UUID,
    input_organization_id UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_author_id UUID;
    v_identifier TEXT;
    v_name TEXT;
    v_email TEXT;
    v_existing_author JSONB;
BEGIN
    -- Extract and validate input
    v_identifier := input_payload->>'identifier';
    v_name := input_payload->>'name';
    v_email := input_payload->>'email';

    -- Validation
    IF v_identifier IS NULL OR LENGTH(TRIM(v_identifier)) = 0 THEN
        RETURN core.log_and_return_mutation(
            'author', NULL, 'ERROR', 'validation_error', ARRAY[]::TEXT[],
            'Author identifier is required',
            NULL, NULL,
            jsonb_build_object('field', 'identifier', 'code', 'REQUIRED_FIELD')
        );
    END IF;

    IF v_email IS NULL OR v_email !~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$' THEN
        RETURN core.log_and_return_mutation(
            'author', NULL, 'ERROR', 'validation_error', ARRAY[]::TEXT[],
            'Valid email address is required',
            NULL, NULL,
            jsonb_build_object('field', 'email', 'code', 'INVALID_EMAIL', 'value', v_email)
        );
    END IF;

    -- Check for duplicate identifier
    SELECT jsonb_build_object(
        'id', pk_author,
        'identifier', identifier,
        'name', data->>'name',
        'email', data->>'email'
    ) INTO v_existing_author
    FROM blog.tb_author
    WHERE identifier = v_identifier;

    IF v_existing_author IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            'author', NULL, 'NOOP', 'noop:duplicate_identifier', ARRAY[]::TEXT[],
            format('Author with identifier "%s" already exists', v_identifier),
            NULL, v_existing_author,
            jsonb_build_object(
                'duplicate_field', 'identifier',
                'duplicate_value', v_identifier,
                'existing_author', v_existing_author
            )
        );
    END IF;

    -- Check for duplicate email
    SELECT jsonb_build_object(
        'id', pk_author,
        'identifier', identifier,
        'email', data->>'email'
    ) INTO v_existing_author
    FROM blog.tb_author
    WHERE data->>'email' = v_email;

    IF v_existing_author IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            'author', NULL, 'ERROR', 'duplicate_email', ARRAY[]::TEXT[],
            format('Email address "%s" is already registered', v_email),
            NULL, NULL,
            jsonb_build_object(
                'duplicate_field', 'email',
                'duplicate_value', v_email,
                'existing_author', v_existing_author,
                'suggestions', ARRAY['Use a different email address', 'Check if you already have an account']
            )
        );
    END IF;

    -- Create new author
    INSERT INTO blog.tb_author (
        identifier,
        data,
        created_by,
        updated_by
    ) VALUES (
        v_identifier,
        jsonb_build_object(
            'name', v_name,
            'email', v_email,
            'bio', input_payload->>'bio',
            'avatar_url', input_payload->>'avatar_url'
        ),
        input_user_id,
        input_user_id
    )
    RETURNING pk_author INTO v_author_id;

    -- Refresh materialized view
    PERFORM core.refresh_author(ARRAY[v_author_id]);

    -- Get complete author data
    SELECT jsonb_build_object(
        'id', pk_author,
        'identifier', identifier,
        'name', data->>'name',
        'email', data->>'email',
        'bio', data->>'bio',
        'avatar_url', data->>'avatar_url',
        'created_at', created_at,
        'updated_at', updated_at
    ) INTO v_existing_author
    FROM blog.tb_author
    WHERE pk_author = v_author_id;

    RETURN core.log_and_return_mutation(
        'author', v_author_id, 'INSERT', 'new',
        ARRAY['identifier', 'name', 'email', 'bio', 'avatar_url'],
        'Author created successfully',
        NULL, v_existing_author,
        jsonb_build_object('author_id', v_author_id)
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- POST FUNCTIONS - Content management with business rules
-- ============================================================================

-- Create post with comprehensive validation and author resolution
CREATE OR REPLACE FUNCTION app.create_post(
    input_user_id UUID,
    input_organization_id UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_post_id UUID;
    v_author_id UUID;
    v_identifier TEXT;
    v_title TEXT;
    v_content TEXT;
    v_author_identifier TEXT;
    v_existing_post JSONB;
    v_post_data JSONB;
    v_tag_array TEXT[];
BEGIN
    -- Extract input
    v_identifier := input_payload->>'identifier';
    v_title := input_payload->>'title';
    v_content := input_payload->>'content';
    v_author_identifier := input_payload->>'author_identifier';
    v_tag_array := ARRAY(SELECT jsonb_array_elements_text(input_payload->'tags'));

    -- Validation
    IF v_identifier IS NULL OR LENGTH(TRIM(v_identifier)) = 0 THEN
        RETURN core.log_and_return_mutation(
            'post', NULL, 'ERROR', 'validation_error', ARRAY[]::TEXT[],
            'Post identifier (slug) is required',
            NULL, NULL,
            jsonb_build_object('field', 'identifier', 'code', 'REQUIRED_FIELD')
        );
    END IF;

    IF v_title IS NULL OR LENGTH(TRIM(v_title)) < 5 THEN
        RETURN core.log_and_return_mutation(
            'post', NULL, 'ERROR', 'validation_error', ARRAY[]::TEXT[],
            'Post title must be at least 5 characters long',
            NULL, NULL,
            jsonb_build_object('field', 'title', 'code', 'TOO_SHORT', 'min_length', 5)
        );
    END IF;

    IF v_content IS NULL OR LENGTH(TRIM(v_content)) < 50 THEN
        RETURN core.log_and_return_mutation(
            'post', NULL, 'ERROR', 'validation_error', ARRAY[]::TEXT[],
            'Post content must be at least 50 characters long',
            NULL, NULL,
            jsonb_build_object('field', 'content', 'code', 'TOO_SHORT', 'min_length', 50)
        );
    END IF;

    -- Resolve author
    SELECT pk_author INTO v_author_id
    FROM blog.tb_author
    WHERE identifier = v_author_identifier;

    IF v_author_id IS NULL THEN
        RETURN core.log_and_return_mutation(
            'post', NULL, 'ERROR', 'author_not_found', ARRAY[]::TEXT[],
            format('Author with identifier "%s" not found', v_author_identifier),
            NULL, NULL,
            jsonb_build_object(
                'missing_author', jsonb_build_object('identifier', v_author_identifier),
                'suggestions', ARRAY['Check the author identifier', 'Create the author first']
            )
        );
    END IF;

    -- Check for duplicate identifier
    SELECT jsonb_build_object(
        'id', pk_post,
        'identifier', identifier,
        'title', data->>'title',
        'author_id', fk_author
    ) INTO v_existing_post
    FROM blog.tb_post
    WHERE identifier = v_identifier;

    IF v_existing_post IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            'post', NULL, 'ERROR', 'duplicate_identifier', ARRAY[]::TEXT[],
            format('Post with identifier "%s" already exists', v_identifier),
            NULL, NULL,
            jsonb_build_object(
                'duplicate_field', 'identifier',
                'duplicate_value', v_identifier,
                'conflict_post', v_existing_post,
                'suggestions', ARRAY['Choose a different identifier', 'Check if this is a duplicate post']
            )
        );
    END IF;

    -- Create post
    INSERT INTO blog.tb_post (
        identifier,
        fk_author,
        data,
        status,
        created_by,
        updated_by
    ) VALUES (
        v_identifier,
        v_author_id,
        jsonb_build_object(
            'title', v_title,
            'content', v_content,
            'excerpt', input_payload->>'excerpt',
            'featured_image_url', input_payload->>'featured_image_url',
            'meta_description', input_payload->>'meta_description',
            'tags', to_jsonb(v_tag_array),
            'reading_time_minutes', GREATEST(1, LENGTH(v_content) / 200)  -- Estimate reading time
        ),
        COALESCE(input_payload->>'status', 'draft'),
        input_user_id,
        input_user_id
    )
    RETURNING pk_post INTO v_post_id;

    -- Create tag associations if any
    IF array_length(v_tag_array, 1) > 0 THEN
        INSERT INTO blog.tb_post_tag (fk_post, fk_tag, created_by)
        SELECT v_post_id, t.pk_tag, input_user_id
        FROM unnest(v_tag_array) AS tag_id
        JOIN blog.tb_tag t ON t.identifier = tag_id
        ON CONFLICT (fk_post, fk_tag) DO NOTHING;
    END IF;

    -- Refresh materialized views
    PERFORM core.refresh_post(ARRAY[v_post_id]);
    PERFORM core.refresh_author(ARRAY[v_author_id]);

    -- Get complete post data with author info
    SELECT jsonb_build_object(
        'id', p.pk_post,
        'identifier', p.identifier,
        'title', p.data->>'title',
        'content', p.data->>'content',
        'excerpt', p.data->>'excerpt',
        'status', p.status,
        'author_id', p.fk_author,
        'author_name', a.data->>'name',
        'tags', p.data->'tags',
        'created_at', p.created_at,
        'updated_at', p.updated_at,
        'version', 1
    ) INTO v_post_data
    FROM blog.tb_post p
    JOIN blog.tb_author a ON p.fk_author = a.pk_author
    WHERE p.pk_post = v_post_id;

    RETURN core.log_and_return_mutation(
        'post', v_post_id, 'INSERT', 'new',
        ARRAY['identifier', 'title', 'content', 'excerpt', 'author', 'tags'],
        'Post created successfully',
        NULL, v_post_data,
        jsonb_build_object('post_id', v_post_id, 'author_id', v_author_id)
    );
END;
$$ LANGUAGE plpgsql;

-- Update post with change tracking
CREATE OR REPLACE FUNCTION app.update_post(
    input_user_id UUID,
    input_organization_id UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_post_id UUID;
    v_current_data JSONB;
    v_new_data JSONB;
    v_changed_fields TEXT[] := ARRAY[]::TEXT[];
    v_post_data JSONB;
BEGIN
    -- Extract post ID (would come from input in real implementation)
    v_post_id := (input_payload->>'post_id')::UUID;

    -- Get current post data
    SELECT data, status INTO v_current_data
    FROM blog.tb_post
    WHERE pk_post = v_post_id;

    IF v_current_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            'post', v_post_id, 'ERROR', 'not_found', ARRAY[]::TEXT[],
            'Post not found',
            NULL, NULL,
            jsonb_build_object('post_id', v_post_id)
        );
    END IF;

    -- Build updated data, only including changed fields
    v_new_data := v_current_data;

    IF input_payload ? 'title' AND input_payload->>'title' != v_current_data->>'title' THEN
        v_new_data := v_new_data || jsonb_build_object('title', input_payload->>'title');
        v_changed_fields := array_append(v_changed_fields, 'title');
    END IF;

    IF input_payload ? 'content' AND input_payload->>'content' != v_current_data->>'content' THEN
        v_new_data := v_new_data || jsonb_build_object('content', input_payload->>'content');
        v_changed_fields := array_append(v_changed_fields, 'content');
        -- Recalculate reading time
        v_new_data := v_new_data || jsonb_build_object(
            'reading_time_minutes',
            GREATEST(1, LENGTH(input_payload->>'content') / 200)
        );
    END IF;

    IF input_payload ? 'excerpt' AND input_payload->>'excerpt' != v_current_data->>'excerpt' THEN
        v_new_data := v_new_data || jsonb_build_object('excerpt', input_payload->>'excerpt');
        v_changed_fields := array_append(v_changed_fields, 'excerpt');
    END IF;

    -- Check if any changes were made
    IF array_length(v_changed_fields, 1) = 0 THEN
        -- No changes - return NOOP
        SELECT jsonb_build_object(
            'id', pk_post,
            'identifier', identifier,
            'title', data->>'title'
        ) INTO v_post_data
        FROM blog.tb_post
        WHERE pk_post = v_post_id;

        RETURN core.log_and_return_mutation(
            'post', v_post_id, 'NOOP', 'noop:no_changes', ARRAY[]::TEXT[],
            'No changes detected - post already in desired state',
            v_current_data, v_post_data,
            jsonb_build_object('current_post', v_post_data)
        );
    END IF;

    -- Update the post
    UPDATE blog.tb_post
    SET
        data = v_new_data,
        updated_at = NOW(),
        updated_by = input_user_id
    WHERE pk_post = v_post_id;

    -- Refresh materialized view
    PERFORM core.refresh_post(ARRAY[v_post_id]);

    -- Get updated post data
    SELECT jsonb_build_object(
        'id', p.pk_post,
        'identifier', p.identifier,
        'title', p.data->>'title',
        'content', p.data->>'content',
        'excerpt', p.data->>'excerpt',
        'status', p.status,
        'author_id', p.fk_author,
        'author_name', a.data->>'name',
        'updated_at', p.updated_at
    ) INTO v_post_data
    FROM blog.tb_post p
    JOIN blog.tb_author a ON p.fk_author = a.pk_author
    WHERE p.pk_post = v_post_id;

    RETURN core.log_and_return_mutation(
        'post', v_post_id, 'UPDATE', 'updated',
        v_changed_fields,
        format('Post updated successfully - %s fields changed', array_length(v_changed_fields, 1)),
        v_current_data, v_post_data,
        jsonb_build_object(
            'changed_fields', to_jsonb(v_changed_fields),
            'changes_count', array_length(v_changed_fields, 1)
        )
    );
END;
$$ LANGUAGE plpgsql;

-- Publish post with business rule validation
CREATE OR REPLACE FUNCTION app.publish_post(
    input_user_id UUID,
    input_organization_id UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
    v_post_id UUID;
    v_post_record RECORD;
    v_requirements JSONB;
    v_post_data JSONB;
    v_can_publish BOOLEAN := true;
    v_requirement_errors TEXT[] := ARRAY[]::TEXT[];
BEGIN
    -- Get post identifier
    v_post_id := (input_payload->>'post_id')::UUID;

    -- Get post details
    SELECT p.*, a.data->>'name' as author_name
    INTO v_post_record
    FROM blog.tb_post p
    JOIN blog.tb_author a ON p.fk_author = a.pk_author
    WHERE p.pk_post = v_post_id;

    IF v_post_record IS NULL THEN
        RETURN core.log_and_return_mutation(
            'post', v_post_id, 'ERROR', 'not_found', ARRAY[]::TEXT[],
            'Post not found',
            NULL, NULL,
            jsonb_build_object('post_id', v_post_id)
        );
    END IF;

    -- Check if already published
    IF v_post_record.status = 'published' THEN
        SELECT jsonb_build_object(
            'id', pk_post,
            'title', data->>'title',
            'published_at', published_at
        ) INTO v_post_data
        FROM blog.tb_post
        WHERE pk_post = v_post_id;

        RETURN core.log_and_return_mutation(
            'post', v_post_id, 'NOOP', 'noop:already_published', ARRAY[]::TEXT[],
            'Post is already published',
            NULL, v_post_data,
            jsonb_build_object('already_published', v_post_data)
        );
    END IF;

    -- Validate publication requirements
    v_requirements := jsonb_build_object();

    -- Title requirement
    IF LENGTH(TRIM(v_post_record.data->>'title')) < 5 THEN
        v_can_publish := false;
        v_requirement_errors := array_append(v_requirement_errors, 'Title must be at least 5 characters');
        v_requirements := v_requirements || jsonb_build_object('title_valid', false);
    ELSE
        v_requirements := v_requirements || jsonb_build_object('title_valid', true);
    END IF;

    -- Content requirement
    IF LENGTH(TRIM(v_post_record.data->>'content')) < 100 THEN
        v_can_publish := false;
        v_requirement_errors := array_append(v_requirement_errors, 'Content must be at least 100 characters');
        v_requirements := v_requirements || jsonb_build_object('content_valid', false);
    ELSE
        v_requirements := v_requirements || jsonb_build_object('content_valid', true);
    END IF;

    -- Author requirement
    IF v_post_record.fk_author IS NULL THEN
        v_can_publish := false;
        v_requirement_errors := array_append(v_requirement_errors, 'Post must have an assigned author');
        v_requirements := v_requirements || jsonb_build_object('author_valid', false);
    ELSE
        v_requirements := v_requirements || jsonb_build_object('author_valid', true);
    END IF;

    -- Return error if requirements not met
    IF NOT v_can_publish THEN
        RETURN core.log_and_return_mutation(
            'post', v_post_id, 'ERROR', 'publication_requirements_not_met', ARRAY[]::TEXT[],
            format('Post cannot be published: %s', array_to_string(v_requirement_errors, '; ')),
            NULL, NULL,
            jsonb_build_object(
                'requirements', v_requirements,
                'errors', to_jsonb(v_requirement_errors),
                'suggestions', ARRAY[
                    'Ensure title is at least 5 characters',
                    'Ensure content is at least 100 characters',
                    'Verify post has an assigned author'
                ]
            )
        );
    END IF;

    -- Publish the post
    UPDATE blog.tb_post
    SET
        status = 'published',
        published_at = NOW(),
        updated_at = NOW(),
        updated_by = input_user_id
    WHERE pk_post = v_post_id;

    -- Refresh materialized views
    PERFORM core.refresh_post(ARRAY[v_post_id]);
    PERFORM core.refresh_author(ARRAY[v_post_record.fk_author]);

    -- Get published post data
    SELECT jsonb_build_object(
        'id', p.pk_post,
        'identifier', p.identifier,
        'title', p.data->>'title',
        'status', p.status,
        'published_at', p.published_at,
        'author_name', v_post_record.author_name
    ) INTO v_post_data
    FROM blog.tb_post p
    WHERE p.pk_post = v_post_id;

    RETURN core.log_and_return_mutation(
        'post', v_post_id, 'UPDATE', 'updated',
        ARRAY['status', 'published_at'],
        'Post published successfully',
        NULL, v_post_data,
        jsonb_build_object(
            'published_at', NOW(),
            'requirements_met', v_requirements
        )
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- UTILITY FUNCTIONS - Support operations
-- ============================================================================

-- Increment post view count (for analytics)
CREATE OR REPLACE FUNCTION app.increment_post_view(
    post_id UUID
) RETURNS VOID AS $$
BEGIN
    -- Update view count in materialized table (async operation)
    UPDATE tv_post
    SET data = data || jsonb_build_object('view_count', COALESCE((data->>'view_count')::INT, 0) + 1)
    WHERE id = post_id;

    -- Could also log to analytics table here
END;
$$ LANGUAGE plpgsql;

-- Search posts with full-text search
CREATE OR REPLACE FUNCTION app.search_posts(
    search_query TEXT,
    limit_count INT DEFAULT 10,
    offset_count INT DEFAULT 0
) RETURNS SETOF JSONB AS $$
BEGIN
    RETURN QUERY
    SELECT row_to_json(search_results.*)::JSONB
    FROM (
        SELECT
            p.pk_post as id,
            p.identifier,
            p.data->>'title' as title,
            p.data->>'excerpt' as excerpt,
            p.status,
            p.published_at,
            a.data->>'name' as author_name,
            ts_rank(
                to_tsvector('english', p.data->>'title' || ' ' || p.data->>'content'),
                plainto_tsquery('english', search_query)
            ) as relevance_score
        FROM blog.tb_post p
        JOIN blog.tb_author a ON p.fk_author = a.pk_author
        WHERE
            p.status = 'published' AND
            (
                to_tsvector('english', p.data->>'title' || ' ' || p.data->>'content') @@
                plainto_tsquery('english', search_query)
            )
        ORDER BY relevance_score DESC, p.published_at DESC
        LIMIT limit_count OFFSET offset_count
    ) search_results;
END;
$$ LANGUAGE plpgsql;
