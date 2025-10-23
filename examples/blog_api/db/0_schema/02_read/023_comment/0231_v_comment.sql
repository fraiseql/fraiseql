CREATE OR REPLACE VIEW v_comment AS
WITH RECURSIVE comment_tree AS (
    SELECT
        c.pk_comment,
        c.id,
        c.identifier,
        c.content,
        c.is_edited,
        c.fk_post,
        c.fk_user,
        c.fk_parent_comment,
        c.created_at,
        c.updated_at,
        0 AS depth,
        ARRAY[c.pk_comment] AS path
    FROM tb_comment c
    WHERE c.fk_parent_comment IS NULL

    UNION ALL

    SELECT
        c.pk_comment,
        c.id,
        c.identifier,
        c.content,
        c.is_edited,
        c.fk_post,
        c.fk_user,
        c.fk_parent_comment,
        c.created_at,
        c.updated_at,
        ct.depth + 1,
        ct.path || c.pk_comment
    FROM tb_comment c
    JOIN comment_tree ct ON ct.pk_comment = c.fk_parent_comment
    WHERE NOT c.pk_comment = ANY(ct.path)
)
SELECT
    c.id,
    jsonb_build_object(
        'id', c.id::text,
        'identifier', c.identifier,
        'content', c.content,
        'is_edited', c.is_edited,
        'depth', c.depth,
        'created_at', c.created_at,
        'updated_at', c.updated_at,
        'post', vp.data,
        'author', vu.data,
        'parent_comment', CASE
            WHEN pc.id IS NOT NULL THEN jsonb_build_object(
                'id', pc.id::text,
                'content', pc.content,
                'author', vu_pc.data
            )
            ELSE NULL
        END
    ) AS data
FROM comment_tree c
JOIN tb_post p ON p.pk_post = c.fk_post
JOIN tb_user u ON u.pk_user = c.fk_user
LEFT JOIN tb_comment pc ON pc.pk_comment = c.fk_parent_comment
LEFT JOIN tb_user pu ON pu.pk_user = pc.fk_user
JOIN v_post vp ON vp.id = p.id
JOIN v_user vu ON vu.id = u.id
LEFT JOIN v_user vu_pc ON vu_pc.id = pu.id;
