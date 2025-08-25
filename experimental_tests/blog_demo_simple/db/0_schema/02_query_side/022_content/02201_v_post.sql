-- Post query view (v_post)
-- Exposes post data for GraphQL consumption

CREATE OR REPLACE VIEW v_post AS
SELECT
    p.pk_post AS id, -- Transform pk_post -> id for GraphQL
    p.identifier AS slug,
    p.title,
    p.content,
    p.excerpt,
    p.fk_author AS author_id, -- Keep as UUID reference
    p.status,
    p.featured,
    p.created_at,
    p.updated_at,
    p.published_at,

    -- JSONB metadata
    COALESCE(p.seo_metadata, '{}'::jsonb) AS seo_metadata,
    COALESCE(p.custom_fields, '{}'::jsonb) AS custom_fields,

    -- Audit fields
    p.created_by,
    p.updated_by,
    p.version
FROM tb_post p
WHERE p.status != 'deleted';

-- Grant permissions
GRANT SELECT ON v_post TO PUBLIC;
