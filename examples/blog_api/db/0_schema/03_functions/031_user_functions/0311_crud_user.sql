-- User CRUD functions
-- Business logic for user management operations

-- Create user
CREATE OR REPLACE FUNCTION create_user(
    user_email TEXT,
    user_name TEXT,
    user_bio TEXT DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    new_user_id UUID;
BEGIN
    -- Validation
    IF user_email IS NULL OR user_name IS NULL THEN
        RAISE EXCEPTION 'Email and name are required';
    END IF;

    -- Check duplicate email
    IF EXISTS (SELECT 1 FROM tb_user WHERE email = user_email) THEN
        RAISE EXCEPTION 'User with email % already exists', user_email;
    END IF;

    -- Create user
    INSERT INTO tb_user (email, name, bio)
    VALUES (user_email, user_name, user_bio)
    RETURNING id INTO new_user_id;

    -- Sync Trinity table
    PERFORM sync_tv_user();

    RETURN new_user_id;
END;
$$ LANGUAGE plpgsql;

-- Update user
CREATE OR REPLACE FUNCTION update_user(
    user_id UUID,
    new_name TEXT DEFAULT NULL,
    new_bio TEXT DEFAULT NULL
) RETURNS BOOLEAN AS $$
BEGIN
    UPDATE tb_user
    SET
        name = COALESCE(new_name, name),
        bio = COALESCE(new_bio, bio),
        updated_at = NOW()
    WHERE id = user_id;

    -- Sync Trinity table
    PERFORM sync_tv_user();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

-- Delete user
CREATE OR REPLACE FUNCTION delete_user(user_id UUID) RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM tb_user WHERE id = user_id;

    -- Sync Trinity table
    PERFORM sync_tv_user();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
