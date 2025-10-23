-- ==============================================================================
-- READ SIDE (Query): All v_* views
-- ==============================================================================
-- File: 00_schema/02_read.sql
-- Layer: Read (query side)
-- Contains: v_user, v_post, v_comment - denormalized, read-optimized
-- ==============================================================================

-- User view (query side)
CREATE VIEW v_user AS
SELECT
    u.id,
    u.identifier,
    jsonb_build_object(
        'id', u.id::text,
        'identifier', u.identifier,
        'email', u.email,
        'name', u.name,
        'bio', u.bio,
        'createdAt', u.created_at,
        'updatedAt', u.updated_at
    ) AS data
FROM tb_user u;

-- Post view with author (query side)
CREATE VIEW v_post AS
SELECT
    p.id,
    p.identifier,
    jsonb_build_object(
        'id', p.id::text,
        'identifier', p.identifier,
        'title', p.title,
        'content', p.content,
        'excerpt', p.excerpt,
        'status', p.status,
        'publishedAt', p.published_at,
        'createdAt', p.created_at,
        'updatedAt', p.updated_at,
        'author', jsonb_build_object(
            'id', u.id::text,
            'identifier', u.identifier,
            'name', u.name
        )
    ) AS data
FROM tb_post p
JOIN tb_user u ON u.pk_user = p.fk_author;

-- Comment view with author and post (query side)
CREATE VIEW v_comment AS
SELECT
    c.id,
    jsonb_build_object(
        'id', c.id::text,
        'content', c.content,
        'status', c.status,
        'createdAt', c.created_at,
        'updatedAt', c.updated_at,
        'author', jsonb_build_object(
            'id', u.id::text,
            'name', u.name
        ),
        'post', jsonb_build_object(
            'id', p.id::text,
            'title', p.title
        ),
        'parentComment', CASE
            WHEN pc.id IS NOT NULL THEN jsonb_build_object(
                'id', pc.id::text,
                'content', pc.content
            )
            ELSE NULL
        END
    ) AS data
FROM tb_comment c
JOIN tb_user u ON u.pk_user = c.fk_author
JOIN tb_post p ON p.pk_post = c.fk_post
LEFT JOIN tb_comment pc ON pc.pk_comment = c.fk_parent;
