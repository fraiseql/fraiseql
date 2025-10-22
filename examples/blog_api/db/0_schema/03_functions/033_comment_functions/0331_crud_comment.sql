-- Comment CRUD functions
-- Business logic for comment management

-- Create comment
CREATE OR REPLACE FUNCTION create_comment(
    post_id UUID,
    author_id UUID,
    comment_content TEXT,
    parent_comment_id UUID DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    new_comment_id UUID;
    post_pk INTEGER;
    author_pk INTEGER;
    parent_pk INTEGER;
BEGIN
    -- Validation
    IF post_id IS NULL OR author_id IS NULL OR comment_content IS NULL THEN
        RAISE EXCEPTION 'Post, author, and content are required';
    END IF;

    -- Get internal keys
    SELECT pk_post INTO post_pk FROM tb_post WHERE id = post_id;
    IF post_pk IS NULL THEN
        RAISE EXCEPTION 'Post does not exist';
    END IF;

    SELECT pk_user INTO author_pk FROM tb_user WHERE id = author_id;
    IF author_pk IS NULL THEN
        RAISE EXCEPTION 'Author does not exist';
    END IF;

    IF parent_comment_id IS NOT NULL THEN
        SELECT pk_comment INTO parent_pk FROM tb_comment WHERE id = parent_comment_id;
        IF parent_pk IS NULL THEN
            RAISE EXCEPTION 'Parent comment does not exist';
        END IF;
    END IF;

    -- Create comment
    INSERT INTO tb_comment (fk_post, fk_user, fk_parent_comment, content)
    VALUES (post_pk, author_pk, parent_pk, comment_content)
    RETURNING id INTO new_comment_id;

    -- Sync projection tables
    PERFORM sync_tv_comment();

    RETURN new_comment_id;
END;
$$ LANGUAGE plpgsql;

-- Update comment
CREATE OR REPLACE FUNCTION update_comment(
    comment_id UUID,
    new_content TEXT
) RETURNS BOOLEAN AS $$
BEGIN
    IF new_content IS NULL THEN
        RAISE EXCEPTION 'Content cannot be empty';
    END IF;

    UPDATE tb_comment
    SET
        content = new_content,
        is_edited = true,
        updated_at = NOW()
    WHERE id = comment_id;

    -- Sync projection tables
    PERFORM sync_tv_comment();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

-- Delete comment
CREATE OR REPLACE FUNCTION delete_comment(comment_id UUID) RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM tb_comment WHERE id = comment_id;

    -- Sync projection tables
    PERFORM sync_tv_comment();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
