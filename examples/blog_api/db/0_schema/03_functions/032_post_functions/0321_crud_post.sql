-- Post CRUD functions
-- Business logic for blog post management

-- Create post
CREATE OR REPLACE FUNCTION create_post(
    author_id UUID,
    post_title TEXT,
    post_content TEXT,
    post_slug TEXT DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    new_post_id UUID;
    generated_slug TEXT;
BEGIN
    -- Validation
    IF author_id IS NULL OR post_title IS NULL OR post_content IS NULL THEN
        RAISE EXCEPTION 'Author, title, and content are required';
    END IF;

    -- Check author exists
    IF NOT EXISTS (SELECT 1 FROM tb_user WHERE id = author_id) THEN
        RAISE EXCEPTION 'Author does not exist';
    END IF;

    -- Generate slug if not provided
    IF post_slug IS NULL THEN
        generated_slug := lower(regexp_replace(post_title, '[^a-zA-Z0-9]+', '-', 'g'));
        generated_slug := trim(both '-' from generated_slug);
    ELSE
        generated_slug := post_slug;
    END IF;

    -- Check duplicate slug
    IF EXISTS (SELECT 1 FROM tb_post WHERE slug = generated_slug) THEN
        RAISE EXCEPTION 'Post with slug % already exists', generated_slug;
    END IF;

    -- Create post
    INSERT INTO tb_post (fk_user, title, content, slug)
    VALUES (
        (SELECT pk_user FROM tb_user WHERE id = author_id),
        post_title,
        post_content,
        generated_slug
    )
    RETURNING id INTO new_post_id;

    -- Sync projection tables
    PERFORM sync_tv_user();
    PERFORM sync_tv_post();

    RETURN new_post_id;
END;
$$ LANGUAGE plpgsql;

-- Update post
CREATE OR REPLACE FUNCTION update_post(
    post_id UUID,
    new_title TEXT DEFAULT NULL,
    new_content TEXT DEFAULT NULL,
    new_slug TEXT DEFAULT NULL
) RETURNS BOOLEAN AS $$
DECLARE
    final_slug TEXT;
BEGIN
    -- Generate slug if title changed
    IF new_title IS NOT NULL AND new_slug IS NULL THEN
        final_slug := lower(regexp_replace(new_title, '[^a-zA-Z0-9]+', '-', 'g'));
        final_slug := trim(both '-' from final_slug);
    ELSE
        final_slug := new_slug;
    END IF;

    -- Check slug uniqueness if changing
    IF final_slug IS NOT NULL AND EXISTS (
        SELECT 1 FROM tb_post
        WHERE slug = final_slug AND id != post_id
    ) THEN
        RAISE EXCEPTION 'Post with slug % already exists', final_slug;
    END IF;

    UPDATE tb_post
    SET
        title = COALESCE(new_title, title),
        content = COALESCE(new_content, content),
        slug = COALESCE(final_slug, slug),
        updated_at = NOW()
    WHERE id = post_id;

    -- Sync projection tables
    PERFORM sync_tv_post();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

-- Delete post
CREATE OR REPLACE FUNCTION delete_post(post_id UUID) RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM tb_post WHERE id = post_id;

    -- Sync projection tables
    PERFORM sync_tv_post();
    PERFORM sync_tv_comment();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
