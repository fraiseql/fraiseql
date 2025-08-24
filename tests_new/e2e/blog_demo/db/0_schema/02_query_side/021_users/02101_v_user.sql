-- User query view (v_user)
-- Exposes user data for GraphQL consumption

CREATE OR REPLACE VIEW v_user AS
SELECT
    pk_user AS id, -- Transform pk_user -> id for GraphQL
    identifier AS username,
    email,
    role,
    is_active,
    email_verified,
    created_at,
    updated_at,
    last_login_at,

    -- JSONB profile data
    COALESCE(profile, '{}'::jsonb) AS profile,
    COALESCE(preferences, '{}'::jsonb) AS preferences,
    COALESCE(metadata, '{}'::jsonb) AS metadata,

    -- Audit fields
    created_by,
    updated_by,
    version
FROM tb_user
WHERE is_active = true;

-- Grant permissions
GRANT SELECT ON v_user TO PUBLIC;
