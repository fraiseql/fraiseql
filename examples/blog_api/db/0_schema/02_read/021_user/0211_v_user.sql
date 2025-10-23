CREATE OR REPLACE VIEW v_user AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id::text,
        'identifier', u.identifier,
        'email', u.email,
        'name', u.name,
        'bio', u.bio,
        'avatar_url', u.avatar_url,
        'is_active', u.is_active,
        'roles', u.roles,
        'created_at', u.created_at,
        'updated_at', u.updated_at
    ) AS data
FROM tb_user u;
