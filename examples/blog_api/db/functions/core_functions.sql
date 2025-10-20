-- Core layer functions (new file)
-- Demonstrates business logic separation
-- Core layer handles all business logic, rules, and data operations

CREATE OR REPLACE FUNCTION core.create_post(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_post_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_post_id UUID;
    v_slug TEXT;
    v_duplicate_check RECORD;
    v_identifier TEXT;
BEGIN
    -- Business logic: Generate slug from title
    v_slug := core.generate_post_slug(input_data.title);

    -- Business logic: Check for duplicate slug (NOOP handling)
    SELECT pk_post, data INTO v_duplicate_check
    FROM tenant.tb_post
    WHERE fk_customer_org = input_pk_organization
    AND data->>'slug' = v_slug
    AND deleted_at IS NULL;

    -- NOOP: Post with same slug exists
    IF v_duplicate_check.pk_post IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'post', v_duplicate_check.pk_post,
            'NOOP', 'noop:slug_exists', ARRAY[]::TEXT[],
            'Post with similar title already exists',
            v_duplicate_check.data, v_duplicate_check.data,
            jsonb_build_object(
                'business_rule', 'unique_slug',
                'generated_slug', v_slug,
                'title_attempted', input_data.title,
                'existing_post_id', v_duplicate_check.pk_post
            )
        );
    END IF;

    -- Business logic: Auto-generate identifier
    v_identifier := core.generate_post_identifier(input_pk_organization, input_data.title);

    -- Generate new UUID for post
    v_post_id := gen_random_uuid();

    -- Create post with full audit trail
    INSERT INTO tenant.tb_post (
        pk_post, pk_organization, data, created_by, created_at, updated_at, updated_by, version
    ) VALUES (
        v_post_id,
        input_pk_organization,
        jsonb_build_object(
            'title', input_data.title,
            'content', input_data.content,
            'slug', v_slug,
            'identifier', v_identifier,
            'excerpt', COALESCE(input_data.excerpt, core.generate_excerpt(input_data.content)),
            'tags', COALESCE(input_data.tags, ARRAY[]::TEXT[]),
            'is_published', COALESCE(input_data.is_published, false),
            'status', CASE WHEN COALESCE(input_data.is_published, false) THEN 'published' ELSE 'draft' END,
            'view_count', 0,
            'word_count', core.count_words(input_data.content)
        ),
        input_created_by, NOW(), NOW(), input_created_by, 1
    );

    -- Business logic: Create initial post stats
    PERFORM core.initialize_post_stats(v_post_id);

    -- Business logic: Update author post count
    PERFORM core.update_author_post_count(input_created_by);

    -- Business logic: Process tags for indexing
    PERFORM core.process_post_tags(v_post_id, input_data.tags);

    -- Return with full audit information
    RETURN core.log_and_return_mutation(
        input_pk_organization, input_created_by, 'post', v_post_id,
        'INSERT', 'new',
        ARRAY['title', 'content', 'slug', 'identifier', 'excerpt', 'tags', 'is_published', 'status'],
        'Post created successfully',
        NULL,
        (SELECT data FROM public.v_post WHERE id = v_post_id),
        jsonb_build_object(
            'business_actions', ARRAY['slug_generated', 'identifier_assigned', 'stats_initialized', 'tags_processed'],
            'generated_slug', v_slug,
            'assigned_identifier', v_identifier,
            'word_count', core.count_words(input_data.content)
        )
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.update_post(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_pk_post UUID,
    input_data app.type_post_update_input,
    input_expected_version INTEGER,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_current_post RECORD;
    v_updated_fields TEXT[] := ARRAY[]::TEXT[];
    v_new_data JSONB;
    v_slug TEXT;
    v_publishing_change BOOLEAN := false;
BEGIN
    -- Get current post with row lock
    SELECT * INTO v_current_post
    FROM tenant.tb_post
    WHERE pk_post = input_pk_post
    AND pk_organization = input_pk_organization
    AND deleted_at IS NULL
    FOR UPDATE;

    -- NOOP: Post not found
    IF v_current_post.pk_post IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_updated_by, 'post', input_pk_post,
            'NOOP', 'noop:not_found', ARRAY[]::TEXT[],
            'Post not found',
            NULL, NULL,
            jsonb_build_object('requested_post_id', input_pk_post)
        );
    END IF;

    -- Business logic: Optimistic locking check
    IF input_expected_version IS NOT NULL AND v_current_post.version != input_expected_version THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_updated_by, 'post', input_pk_post,
            'NOOP', 'noop:version_conflict', ARRAY[]::TEXT[],
            format('Version conflict: expected %s, current %s', input_expected_version, v_current_post.version),
            v_current_post.data, v_current_post.data,
            jsonb_build_object(
                'expected_version', input_expected_version,
                'current_version', v_current_post.version,
                'conflict_type', 'optimistic_lock'
            )
        );
    END IF;

    -- Start with current data
    v_new_data := v_current_post.data;

    -- Business logic: Update fields that changed
    IF input_data.title IS NOT NULL AND input_data.title != (v_current_post.data->>'title') THEN
        -- Regenerate slug when title changes
        v_slug := core.generate_post_slug(input_data.title);
        v_new_data := v_new_data || jsonb_build_object('title', input_data.title, 'slug', v_slug);
        v_updated_fields := v_updated_fields || ARRAY['title', 'slug'];
    END IF;

    IF input_data.content IS NOT NULL AND input_data.content != (v_current_post.data->>'content') THEN
        v_new_data := v_new_data || jsonb_build_object(
            'content', input_data.content,
            'word_count', core.count_words(input_data.content)
        );
        v_updated_fields := v_updated_fields || ARRAY['content', 'word_count'];
    END IF;

    IF input_data.excerpt IS NOT NULL AND input_data.excerpt != (v_current_post.data->>'excerpt') THEN
        v_new_data := v_new_data || jsonb_build_object('excerpt', input_data.excerpt);
        v_updated_fields := v_updated_fields || ARRAY['excerpt'];
    END IF;

    IF input_data.tags IS NOT NULL THEN
        -- Convert current tags to array for comparison
        DECLARE
            v_current_tags TEXT[];
        BEGIN
            SELECT ARRAY_AGG(value::TEXT) INTO v_current_tags
            FROM jsonb_array_elements_text(v_current_post.data->'tags');

            IF v_current_tags IS DISTINCT FROM input_data.tags THEN
                v_new_data := v_new_data || jsonb_build_object('tags', to_jsonb(input_data.tags));
                v_updated_fields := v_updated_fields || ARRAY['tags'];
                -- Update tag processing
                PERFORM core.process_post_tags(input_pk_post, input_data.tags);
            END IF;
        END;
    END IF;

    IF input_data.is_published IS NOT NULL AND input_data.is_published != (v_current_post.data->>'is_published')::BOOLEAN THEN
        v_new_data := v_new_data || jsonb_build_object(
            'is_published', input_data.is_published,
            'status', CASE WHEN input_data.is_published THEN 'published' ELSE 'draft' END
        );
        v_updated_fields := v_updated_fields || ARRAY['is_published', 'status'];
        v_publishing_change := true;

        -- Set published_at timestamp when publishing
        IF input_data.is_published AND (v_current_post.data->>'published_at') IS NULL THEN
            v_new_data := v_new_data || jsonb_build_object('published_at', to_jsonb(NOW()));
            v_updated_fields := v_updated_fields || ARRAY['published_at'];
        END IF;
    END IF;

    -- NOOP: No changes detected
    IF array_length(v_updated_fields, 1) = 0 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_updated_by, 'post', input_pk_post,
            'NOOP', 'noop:no_changes', ARRAY[]::TEXT[],
            'No changes detected',
            v_current_post.data, v_current_post.data,
            jsonb_build_object(
                'attempted_fields', jsonb_object_keys(to_jsonb(input_data)),
                'change_detection', 'field_by_field_comparison'
            )
        );
    END IF;

    -- Update the post
    UPDATE tenant.tb_post SET
        data = v_new_data,
        updated_at = NOW(),
        updated_by = input_updated_by,
        version = version + 1
    WHERE pk_post = input_pk_post;

    -- Business logic: Handle publishing state changes
    IF v_publishing_change THEN
        PERFORM core.handle_post_publishing_change(input_pk_post, input_data.is_published);
    END IF;

    -- Business logic: Update search index for content changes
    IF 'content' = ANY(v_updated_fields) OR 'title' = ANY(v_updated_fields) THEN
        PERFORM core.update_post_search_index(input_pk_post);
    END IF;

    RETURN core.log_and_return_mutation(
        input_pk_organization, input_updated_by, 'post', input_pk_post,
        'UPDATE', 'updated', v_updated_fields,
        format('Post updated successfully (%s fields changed)', array_length(v_updated_fields, 1)),
        v_current_post.data, v_new_data,
        jsonb_build_object(
            'version_change', jsonb_build_object(
                'from', v_current_post.version,
                'to', v_current_post.version + 1
            ),
            'business_actions', CASE
                WHEN v_publishing_change THEN ARRAY['publishing_state_changed', 'search_updated']
                WHEN 'content' = ANY(v_updated_fields) THEN ARRAY['search_updated']
                ELSE ARRAY[]::TEXT[]
            END,
            'updated_field_count', array_length(v_updated_fields, 1)
        )
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.create_user(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_user_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_user_id UUID;
    v_duplicate_check RECORD;
    v_identifier TEXT;
BEGIN
    -- Business logic: Check for duplicate email (NOOP case)
    SELECT pk_user, data INTO v_duplicate_check
    FROM tenant.tb_user
    WHERE pk_organization = input_pk_organization
    AND data->>'email' = lower(trim(input_data.email))
    AND deleted_at IS NULL;

    IF v_duplicate_check.pk_user IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization, input_created_by, 'user', v_duplicate_check.pk_user,
            'NOOP', 'noop:email_exists', ARRAY[]::TEXT[],
            'User with this email already exists',
            v_duplicate_check.data, v_duplicate_check.data,
            jsonb_build_object(
                'attempted_email', input_data.email,
                'normalized_email', lower(trim(input_data.email)),
                'existing_user_id', v_duplicate_check.pk_user
            )
        );
    END IF;

    -- Business logic: Generate user identifier
    v_identifier := core.generate_user_identifier(input_data.name, input_data.email);

    -- Generate new UUID
    v_user_id := gen_random_uuid();

    -- Create user with audit trail
    INSERT INTO tenant.tb_user (
        pk_user, pk_organization, data, created_by, created_at, updated_at, updated_by, version
    ) VALUES (
        v_user_id,
        input_pk_organization,
        jsonb_build_object(
            'email', lower(trim(input_data.email)),
            'name', trim(input_data.name),
            'bio', COALESCE(trim(input_data.bio), ''),
            'avatar_url', input_data.avatar_url,
            'identifier', v_identifier,
            'is_active', true,
            'roles', jsonb_build_array('user'),
            'password_hash', input_data.password_hash,
            'email_verified', false,
            'login_count', 0,
            'last_login_at', NULL
        ),
        input_created_by, NOW(), NOW(), input_created_by, 1
    );

    -- Business logic: Initialize user preferences
    PERFORM core.initialize_user_preferences(v_user_id);

    -- Business logic: Send welcome email (async)
    PERFORM core.queue_welcome_email(v_user_id, input_data.email, input_data.name);

    RETURN core.log_and_return_mutation(
        input_pk_organization, input_created_by, 'user', v_user_id,
        'INSERT', 'new',
        ARRAY['email', 'name', 'bio', 'avatar_url', 'identifier', 'is_active', 'roles'],
        'User created successfully',
        NULL,
        (SELECT data FROM public.v_user WHERE id = v_user_id),
        jsonb_build_object(
            'business_actions', ARRAY['identifier_generated', 'preferences_initialized', 'welcome_email_queued'],
            'assigned_identifier', v_identifier,
            'normalized_email', lower(trim(input_data.email))
        )
    );
END;
$$ LANGUAGE plpgsql;

-- Helper functions for business logic

CREATE OR REPLACE FUNCTION core.generate_post_slug(title TEXT) RETURNS TEXT AS $$
BEGIN
    RETURN lower(regexp_replace(regexp_replace(title, '[^a-zA-Z0-9\s]', '', 'g'), '\s+', '-', 'g'));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION core.generate_post_identifier(org_id UUID, title TEXT) RETURNS TEXT AS $$
BEGIN
    RETURN format('POST-%s-%s',
        to_char(NOW(), 'YYYY-MM-DD'),
        upper(substring(core.generate_post_slug(title), 1, 20))
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.generate_user_identifier(name TEXT, email TEXT) RETURNS TEXT AS $$
BEGIN
    RETURN format('USER-%s-%s',
        upper(substring(regexp_replace(name, '[^a-zA-Z]', '', 'g'), 1, 5)),
        upper(substring(split_part(email, '@', 1), 1, 5))
    );
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.generate_excerpt(content TEXT) RETURNS TEXT AS $$
BEGIN
    RETURN left(regexp_replace(content, '\s+', ' ', 'g'), 200) || CASE WHEN length(content) > 200 THEN '...' ELSE '' END;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

CREATE OR REPLACE FUNCTION core.count_words(content TEXT) RETURNS INTEGER AS $$
BEGIN
    RETURN array_length(string_to_array(trim(regexp_replace(content, '\s+', ' ', 'g')), ' '), 1);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Placeholder functions for complex business logic
CREATE OR REPLACE FUNCTION core.initialize_post_stats(post_id UUID) RETURNS VOID AS $$
BEGIN
    -- Initialize statistics tracking for the post
    INSERT INTO tenant.tb_post_stats (pk_post, view_count, like_count, share_count, created_at)
    VALUES (post_id, 0, 0, 0, NOW())
    ON CONFLICT (pk_post) DO NOTHING;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.update_author_post_count(user_id UUID) RETURNS VOID AS $$
BEGIN
    -- Update the author's post count
    UPDATE tenant.tb_user SET
        data = data || jsonb_build_object(
            'post_count', COALESCE((data->>'post_count')::INTEGER, 0) + 1
        )
    WHERE pk_user = user_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.process_post_tags(post_id UUID, tags TEXT[]) RETURNS VOID AS $$
BEGIN
    -- Process tags for search indexing and categorization
    -- Implementation would handle tag normalization, search index updates, etc.
    NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.handle_post_publishing_change(post_id UUID, is_published BOOLEAN) RETURNS VOID AS $$
BEGIN
    -- Handle business logic for publishing/unpublishing posts
    -- Implementation would handle notifications, RSS feeds, search indices, etc.
    NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.update_post_search_index(post_id UUID) RETURNS VOID AS $$
BEGIN
    -- Update full-text search index
    -- Implementation would update search vectors, elasticsearch, etc.
    NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.initialize_user_preferences(user_id UUID) RETURNS VOID AS $$
BEGIN
    -- Initialize default user preferences
    INSERT INTO tenant.tb_user_preferences (pk_user, preferences, created_at)
    VALUES (user_id, jsonb_build_object(
        'email_notifications', true,
        'theme', 'light',
        'language', 'en'
    ), NOW())
    ON CONFLICT (pk_user) DO NOTHING;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION core.queue_welcome_email(user_id UUID, email TEXT, name TEXT) RETURNS VOID AS $$
BEGIN
    -- Queue welcome email for new user
    INSERT INTO tenant.tb_email_queue (recipient_email, template_name, template_data, created_at)
    VALUES (email, 'welcome', jsonb_build_object(
        'user_id', user_id,
        'name', name
    ), NOW());
END;
$$ LANGUAGE plpgsql;

-- Unified audit logging function
-- Logs to audit_events table with automatic crypto chain integrity
CREATE OR REPLACE FUNCTION core.log_and_return_mutation(
    p_tenant_id UUID,
    p_user_id UUID,
    p_entity_type TEXT,
    p_entity_id UUID,
    p_operation_type TEXT,        -- INSERT, UPDATE, DELETE, NOOP
    p_operation_subtype TEXT,     -- new, updated, noop:duplicate, etc.
    p_changed_fields TEXT[],
    p_message TEXT,
    p_old_data JSONB,             -- CDC: before state
    p_new_data JSONB,             -- CDC: after state
    p_metadata JSONB              -- Business actions, rules, etc.
) RETURNS app.mutation_result AS $$
DECLARE
    v_result app.mutation_result;
BEGIN
    -- Insert into unified audit_events table
    -- Crypto fields auto-populated by populate_crypto_trigger
    INSERT INTO audit_events (
        tenant_id, user_id, entity_type, entity_id,
        operation_type, operation_subtype, changed_fields,
        old_data, new_data, metadata
    ) VALUES (
        p_tenant_id, p_user_id, p_entity_type, p_entity_id,
        p_operation_type, p_operation_subtype, p_changed_fields,
        p_old_data, p_new_data, p_metadata
    );

    -- Return standardized mutation result
    v_result.success := (p_operation_type IN ('INSERT', 'UPDATE', 'DELETE'));
    v_result.operation_type := p_operation_type;
    v_result.entity_type := p_entity_type;
    v_result.entity_id := p_entity_id;
    v_result.message := p_message;
    v_result.error_code := CASE WHEN p_operation_type = 'NOOP' THEN p_operation_subtype ELSE NULL END;
    v_result.changed_fields := p_changed_fields;
    v_result.old_data := p_old_data;
    v_result.new_data := p_new_data;
    v_result.metadata := p_metadata;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;

-- Grant permissions
GRANT EXECUTE ON FUNCTION core.create_post(UUID, UUID, app.type_post_input, JSONB) TO blog_api_role;
GRANT EXECUTE ON FUNCTION core.update_post(UUID, UUID, UUID, app.type_post_update_input, INTEGER, JSONB) TO blog_api_role;
GRANT EXECUTE ON FUNCTION core.create_user(UUID, UUID, app.type_user_input, JSONB) TO blog_api_role;
