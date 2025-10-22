CREATE OR REPLACE VIEW v_product AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id::text,
        'sku', p.sku,
        'name', p.name,
        'slug', p.slug,
        'description', p.description,
        'brand', p.brand,
        'is_active', p.is_active,
        'is_featured', p.is_featured,
        'category', vc.data
    ) AS data
FROM products p
LEFT JOIN v_category vc ON vc.id = p.category_id;
