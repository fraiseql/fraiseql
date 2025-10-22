CREATE OR REPLACE VIEW v_post AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id::text,
        'identifier', p.identifier,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'is_published', p.is_published,
        'published_at', p.published_at,
        'view_count', p.view_count,
        'created_at', p.created_at,
        'updated_at', p.updated_at,
        'author', vu.data
    ) AS data
FROM tb_post p
JOIN tb_user u ON u.pk_user = p.fk_user
JOIN v_user vu ON vu.id = u.id;
