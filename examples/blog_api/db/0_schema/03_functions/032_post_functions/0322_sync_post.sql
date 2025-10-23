-- Post sync functions
-- Explicit synchronization of projection tables

-- Sync tv_post table from v_post view
CREATE OR REPLACE FUNCTION sync_tv_post() RETURNS VOID AS $$
BEGIN
    -- Clear and repopulate tv_post
    DELETE FROM tv_post;
    INSERT INTO tv_post (id, data)
    SELECT id, data FROM v_post;
END;
$$ LANGUAGE plpgsql;

-- Sync single post in tv_post
CREATE OR REPLACE FUNCTION sync_tv_post_single(post_id UUID) RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_post WHERE id = post_id;
    INSERT INTO tv_post (id, data)
    SELECT id, data FROM v_post WHERE id = post_id;
END;
$$ LANGUAGE plpgsql;
