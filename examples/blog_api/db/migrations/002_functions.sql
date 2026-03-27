-- Blog API CQRS Schema - SQL Functions for Mutations
-- All functions return mutation_response type

-- Function to create a user
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSON)
RETURNS mutation_response AS $$
DECLARE
    new_user_id UUID;
BEGIN
    -- Validate required fields
    IF input_data->>'email' IS NULL OR input_data->>'name' IS NULL THEN
        RETURN ROW('failed:validation', 'Email and name are required', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Check if email already exists
    IF EXISTS (SELECT 1 FROM tb_user WHERE email = input_data->>'email') THEN
        RETURN ROW('failed:validation', 'Email already exists', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Insert new user
    INSERT INTO tb_user (email, name, bio, avatar_url)
    VALUES (
        input_data->>'email',
        input_data->>'name',
        input_data->>'bio',
        input_data->>'avatar_url'
    )
    RETURNING id INTO new_user_id;

    -- Return success with new user
    RETURN ROW(
        'new',
        'User created successfully',
        new_user_id::text,
        'User',
        jsonb_build_object('id', new_user_id, 'email', input_data->>'email', 'name', input_data->>'name'),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;

EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Function to create a post
CREATE OR REPLACE FUNCTION fn_create_post(input_data JSON)
RETURNS mutation_response AS $$
DECLARE
    new_post_id UUID;
    generated_slug VARCHAR(500);
BEGIN
    -- Validate required fields
    IF input_data->>'author_id' IS NULL OR input_data->>'title' IS NULL OR input_data->>'content' IS NULL THEN
        RETURN ROW('failed:validation', 'Author ID, title, and content are required', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Generate slug from title
    generated_slug := LOWER(REGEXP_REPLACE(input_data->>'title', '[^a-zA-Z0-9]+', '-', 'g'));
    generated_slug := TRIM(BOTH '-' FROM generated_slug);

    -- Ensure slug is unique
    WHILE EXISTS (SELECT 1 FROM tb_posts WHERE slug = generated_slug) LOOP
        generated_slug := generated_slug || '-' || EXTRACT(EPOCH FROM NOW())::INTEGER;
    END LOOP;

    -- Insert new post
    INSERT INTO tb_posts (
        author_id, title, slug, content, excerpt, tags, is_published, published_at
    )
    VALUES (
        (input_data->>'author_id')::UUID,
        input_data->>'title',
        generated_slug,
        input_data->>'content',
        input_data->>'excerpt',
        COALESCE(
            ARRAY(SELECT json_array_elements_text(input_data->'tags')),
            ARRAY[]::TEXT[]
        ),
        COALESCE((input_data->>'is_published')::BOOLEAN, false),
        CASE
            WHEN COALESCE((input_data->>'is_published')::BOOLEAN, false)
            THEN NOW()
            ELSE NULL
        END
    )
    RETURNING id INTO new_post_id;

    RETURN ROW(
        'new',
        'Post created successfully',
        new_post_id::text,
        'Post',
        jsonb_build_object('id', new_post_id, 'slug', generated_slug, 'title', input_data->>'title'),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;

EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Function to update a post
CREATE OR REPLACE FUNCTION fn_update_post(input_data JSON)
RETURNS mutation_response AS $$
DECLARE
    post_exists BOOLEAN;
BEGIN
    -- Validate required fields
    IF input_data->>'id' IS NULL THEN
        RETURN ROW('failed:validation', 'Post ID is required', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Check if post exists
    SELECT EXISTS (SELECT 1 FROM tb_posts WHERE id = (input_data->>'id')::UUID) INTO post_exists;

    IF NOT post_exists THEN
        RETURN ROW('failed:validation', 'Post not found', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Update post fields that are provided
    UPDATE tb_posts
    SET
        title = COALESCE(input_data->>'title', title),
        content = COALESCE(input_data->>'content', content),
        excerpt = COALESCE(input_data->>'excerpt', excerpt),
        tags = CASE
            WHEN input_data->'tags' IS NOT NULL
            THEN ARRAY(SELECT json_array_elements_text(input_data->'tags'))
            ELSE tags
        END,
        is_published = COALESCE((input_data->>'is_published')::BOOLEAN, is_published),
        published_at = CASE
            WHEN input_data->>'is_published' IS NOT NULL AND (input_data->>'is_published')::BOOLEAN AND published_at IS NULL
            THEN NOW()
            ELSE published_at
        END
    WHERE id = (input_data->>'id')::UUID;

    RETURN ROW(
        'success',
        'Post updated successfully',
        input_data->>'id',
        'Post',
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;

EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Function to create a comment
CREATE OR REPLACE FUNCTION fn_create_comment(input_data JSON)
RETURNS mutation_response AS $$
DECLARE
    new_comment_id UUID;
BEGIN
    -- Validate required fields
    IF input_data->>'post_id' IS NULL OR input_data->>'author_id' IS NULL OR input_data->>'content' IS NULL THEN
        RETURN ROW('failed:validation', 'Post ID, author ID, and content are required', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Validate parent comment if provided
    IF input_data->>'parent_id' IS NOT NULL THEN
        IF NOT EXISTS (SELECT 1 FROM tb_comments WHERE id = (input_data->>'parent_id')::UUID) THEN
            RETURN ROW('failed:validation', 'Parent comment not found', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
        END IF;
    END IF;

    -- Insert new comment
    INSERT INTO tb_comments (post_id, author_id, parent_id, content)
    VALUES (
        (input_data->>'post_id')::UUID,
        (input_data->>'author_id')::UUID,
        (input_data->>'parent_id')::UUID,
        input_data->>'content'
    )
    RETURNING id INTO new_comment_id;

    RETURN ROW(
        'new',
        'Comment created successfully',
        new_comment_id::text,
        'Comment',
        jsonb_build_object('id', new_comment_id, 'post_id', input_data->>'post_id'),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;

EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Function to delete a post
CREATE OR REPLACE FUNCTION fn_delete_post(input_data JSON)
RETURNS mutation_response AS $$
BEGIN
    -- Validate required fields
    IF input_data->>'id' IS NULL THEN
        RETURN ROW('failed:validation', 'Post ID is required', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Delete post (comments will cascade)
    DELETE FROM tb_posts WHERE id = (input_data->>'id')::UUID;

    IF NOT FOUND THEN
        RETURN ROW('failed:validation', 'Post not found', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    RETURN ROW(
        'success',
        'Post deleted successfully',
        input_data->>'id',
        'Post',
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;

EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Function to increment view count
CREATE OR REPLACE FUNCTION fn_increment_view_count(input_data JSON)
RETURNS mutation_response AS $$
BEGIN
    UPDATE tb_posts
    SET view_count = view_count + 1
    WHERE id = (input_data->>'post_id')::UUID;

    IF NOT FOUND THEN
        RETURN ROW('failed:validation', 'Post not found', NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    RETURN ROW(
        'success',
        'View count incremented',
        input_data->>'post_id',
        'Post',
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;

EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;
