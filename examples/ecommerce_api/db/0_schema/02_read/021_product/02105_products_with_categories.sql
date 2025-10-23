-- Products with categories view (query side)
-- Denormalized view of products including category information

CREATE OR REPLACE VIEW products_with_categories AS
SELECT
    p.id,
    p.sku,
    p.name,
    p.slug,
    p.description,
    p.short_description,
    p.brand,
    p.tags,
    p.is_active,
    p.is_featured,
    c.name as category_name,
    c.slug as category_slug,
    p.created_at,
    p.updated_at
FROM products p
LEFT JOIN categories c ON p.category_id = c.id;
