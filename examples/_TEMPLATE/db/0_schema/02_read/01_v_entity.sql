-- Trinity Pattern View Template
-- Replace with your actual view logic

CREATE VIEW v_entity AS
SELECT
    e.id,  -- ✅ Required: Direct id column for WHERE filtering
    jsonb_build_object(
        'id', e.id::text,           -- ✅ Public UUID in JSONB
        'identifier', e.identifier, -- ✅ Human-readable identifier
        'name', e.name,
        'description', e.description,
        'created_at', e.created_at
        -- ❌ NEVER include pk_entity in JSONB (security!)
    ) AS data
FROM tb_entity e;
