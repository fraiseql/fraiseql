-- User sync functions
-- Explicit synchronization of Trinity tables

-- Sync tv_user table from v_user view
CREATE OR REPLACE FUNCTION sync_tv_user() RETURNS VOID AS $$
BEGIN
    -- Clear and repopulate tv_user
    DELETE FROM tv_user;
    INSERT INTO tv_user (id, data)
    SELECT id, data FROM v_user;
END;
$$ LANGUAGE plpgsql;

-- Sync single user in tv_user
CREATE OR REPLACE FUNCTION sync_tv_user_single(user_id UUID) RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_user WHERE id = user_id;
    INSERT INTO tv_user (id, data)
    SELECT id, data FROM v_user WHERE id = user_id;
END;
$$ LANGUAGE plpgsql;
