-- Tag query view (v_tag)
-- Exposes tag data for GraphQL consumption

CREATE OR REPLACE VIEW v_tag AS
SELECT
    t.pk_tag AS id, -- Transform pk_tag -> id for GraphQL
    t.identifier AS slug,
    t.name,
    t.description,
    t.color,
    t.fk_parent_tag AS parent_id, -- Keep as UUID reference (nullable)
    t.sort_order,
    t.is_active,
    t.created_at,

    -- Audit fields
    t.created_by,
    t.updated_by,
    t.version
FROM tb_tag t
WHERE t.is_active = true;

-- Grant permissions
GRANT SELECT ON v_tag TO PUBLIC;
