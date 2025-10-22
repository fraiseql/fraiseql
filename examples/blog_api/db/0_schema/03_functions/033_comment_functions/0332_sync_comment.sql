-- Comment sync functions
-- Explicit synchronization of projection tables

-- Sync tv_comment table from v_comment view
CREATE OR REPLACE FUNCTION sync_tv_comment() RETURNS VOID AS $$
BEGIN
    -- Clear and repopulate tv_comment
    DELETE FROM tv_comment;
    INSERT INTO tv_comment (id, data)
    SELECT id, data FROM v_comment;
END;
$$ LANGUAGE plpgsql;

-- Sync single comment in tv_comment
CREATE OR REPLACE FUNCTION sync_tv_comment_single(comment_id UUID) RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_comment WHERE id = comment_id;
    INSERT INTO tv_comment (id, data)
    SELECT id, data FROM v_comment WHERE id = comment_id;
END;
$$ LANGUAGE plpgsql;
