CREATE OR REPLACE VIEW v_category AS
SELECT
    id,
    jsonb_build_object(
        'id', id::text,
        'name', name,
        'slug', slug,
        'description', description,
        'image_url', image_url,
        'is_active', is_active,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM categories;
