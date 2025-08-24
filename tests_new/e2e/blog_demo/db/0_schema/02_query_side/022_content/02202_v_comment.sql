-- Comment query view (v_comment)
-- Exposes comment data for GraphQL consumption

CREATE OR REPLACE VIEW v_comment AS
SELECT
    c.pk_comment AS id, -- Transform pk_comment -> id for GraphQL
    c.fk_post AS post_id, -- Keep as UUID reference
    c.fk_author AS author_id, -- Keep as UUID reference
    c.fk_parent_comment AS parent_id, -- Keep as UUID reference (nullable)
    c.content,
    c.status,
    c.created_at,
    c.updated_at,

    -- JSONB metadata
    COALESCE(c.moderation_data, '{}'::jsonb) AS moderation_data,

    -- Audit fields
    c.created_by,
    c.updated_by,
    c.version
FROM tb_comment c;

-- Grant permissions
GRANT SELECT ON v_comment TO PUBLIC;
