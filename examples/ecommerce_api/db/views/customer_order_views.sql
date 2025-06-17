-- Customer and Order Views for E-commerce API
-- Optimized for GraphQL queries with FraiseQL

-- Customer profile view with stats
CREATE OR REPLACE VIEW customer_profile AS
SELECT
    c.*,
    -- Order statistics
    COUNT(DISTINCT o.id) as total_orders,
    COUNT(DISTINCT o.id) FILTER (WHERE o.status = 'completed') as completed_orders,
    COALESCE(SUM(o.total_amount) FILTER (WHERE o.status = 'completed'), 0) as lifetime_value,
    MAX(o.created_at) as last_order_date,
    -- Address count
    COUNT(DISTINCT a.id) as address_count,
    -- Wishlist stats
    COUNT(DISTINCT w.id) as wishlist_count,
    COUNT(DISTINCT wi.id) as wishlist_items_count,
    -- Review stats
    COUNT(DISTINCT r.id) as review_count,
    AVG(r.rating)::DECIMAL(3,2) as average_rating_given,
    -- Cart status
    EXISTS(
        SELECT 1 FROM carts
        WHERE customer_id = c.id
        AND status = 'active'
        AND expires_at > CURRENT_TIMESTAMP
    ) as has_active_cart
FROM customers c
LEFT JOIN orders o ON o.customer_id = c.id
LEFT JOIN addresses a ON a.customer_id = c.id
LEFT JOIN wishlists w ON w.customer_id = c.id
LEFT JOIN wishlist_items wi ON wi.wishlist_id = w.id
LEFT JOIN reviews r ON r.customer_id = c.id AND r.status = 'approved'
GROUP BY c.id;

-- Shopping cart view with items
CREATE OR REPLACE VIEW shopping_cart AS
SELECT
    c.*,
    -- Customer info if logged in
    CASE
        WHEN c.customer_id IS NOT NULL THEN
            json_build_object(
                'id', cust.id,
                'email', cust.email,
                'first_name', cust.first_name,
                'last_name', cust.last_name
            )
        ELSE NULL
    END as customer,
    -- Cart items with product details
    COALESCE(
        json_agg(
            json_build_object(
                'id', ci.id,
                'quantity', ci.quantity,
                'price_at_time', ci.price_at_time,
                'variant', json_build_object(
                    'id', pv.id,
                    'sku', pv.sku,
                    'name', pv.name,
                    'price', pv.price,
                    'attributes', pv.attributes,
                    'available_quantity', i.quantity - i.reserved_quantity
                ),
                'product', json_build_object(
                    'id', p.id,
                    'name', p.name,
                    'slug', p.slug,
                    'image_url', pi.url
                )
            ) ORDER BY ci.created_at
        ) FILTER (WHERE ci.id IS NOT NULL),
        '[]'::json
    ) as items,
    -- Cart totals
    COUNT(ci.id) as item_count,
    COALESCE(SUM(ci.quantity), 0) as total_quantity,
    COALESCE(SUM(ci.quantity * ci.price_at_time), 0) as subtotal,
    -- Check if all items are in stock
    BOOL_AND(ci.quantity <= (i.quantity - i.reserved_quantity)) as all_items_available
FROM carts c
LEFT JOIN customers cust ON c.customer_id = cust.id
LEFT JOIN cart_items ci ON ci.cart_id = c.id
LEFT JOIN product_variants pv ON ci.variant_id = pv.id
LEFT JOIN products p ON pv.product_id = p.id
LEFT JOIN inventory i ON i.variant_id = pv.id
LEFT JOIN LATERAL (
    SELECT url FROM product_images
    WHERE product_id = p.id
    ORDER BY is_primary DESC, position
    LIMIT 1
) pi ON true
WHERE c.status = 'active' AND c.expires_at > CURRENT_TIMESTAMP
GROUP BY c.id, cust.id, cust.email, cust.first_name, cust.last_name;

-- Order detail view
CREATE OR REPLACE VIEW order_detail AS
SELECT
    o.*,
    -- Customer info
    json_build_object(
        'id', c.id,
        'email', c.email,
        'first_name', c.first_name,
        'last_name', c.last_name,
        'phone', c.phone
    ) as customer,
    -- Shipping address
    CASE
        WHEN sa.id IS NOT NULL THEN
            json_build_object(
                'first_name', sa.first_name,
                'last_name', sa.last_name,
                'company', sa.company,
                'address_line1', sa.address_line1,
                'address_line2', sa.address_line2,
                'city', sa.city,
                'state_province', sa.state_province,
                'postal_code', sa.postal_code,
                'country_code', sa.country_code,
                'phone', sa.phone
            )
        ELSE NULL
    END as shipping_address,
    -- Billing address
    CASE
        WHEN ba.id IS NOT NULL THEN
            json_build_object(
                'first_name', ba.first_name,
                'last_name', ba.last_name,
                'company', ba.company,
                'address_line1', ba.address_line1,
                'address_line2', ba.address_line2,
                'city', ba.city,
                'state_province', ba.state_province,
                'postal_code', ba.postal_code,
                'country_code', ba.country_code,
                'phone', ba.phone
            )
        ELSE NULL
    END as billing_address,
    -- Order items
    COALESCE(
        json_agg(
            json_build_object(
                'id', oi.id,
                'quantity', oi.quantity,
                'unit_price', oi.unit_price,
                'total_price', oi.total_price,
                'discount_amount', oi.discount_amount,
                'tax_amount', oi.tax_amount,
                'variant', json_build_object(
                    'id', pv.id,
                    'sku', pv.sku,
                    'name', pv.name,
                    'attributes', pv.attributes
                ),
                'product', json_build_object(
                    'id', p.id,
                    'name', p.name,
                    'slug', p.slug,
                    'image_url', pi.url
                )
            ) ORDER BY oi.created_at
        ) FILTER (WHERE oi.id IS NOT NULL),
        '[]'::json
    ) as items
FROM orders o
JOIN customers c ON o.customer_id = c.id
LEFT JOIN addresses sa ON o.shipping_address_id = sa.id
LEFT JOIN addresses ba ON o.billing_address_id = ba.id
LEFT JOIN order_items oi ON oi.order_id = o.id
LEFT JOIN product_variants pv ON oi.variant_id = pv.id
LEFT JOIN products p ON pv.product_id = p.id
LEFT JOIN LATERAL (
    SELECT url FROM product_images
    WHERE product_id = p.id
    ORDER BY is_primary DESC, position
    LIMIT 1
) pi ON true
GROUP BY o.id, c.id, c.email, c.first_name, c.last_name, c.phone,
         sa.id, sa.first_name, sa.last_name, sa.company, sa.address_line1,
         sa.address_line2, sa.city, sa.state_province, sa.postal_code,
         sa.country_code, sa.phone,
         ba.id, ba.first_name, ba.last_name, ba.company, ba.address_line1,
         ba.address_line2, ba.city, ba.state_province, ba.postal_code,
         ba.country_code, ba.phone;

-- Customer order history
CREATE OR REPLACE VIEW customer_orders AS
SELECT
    o.customer_id,
    o.id as order_id,
    o.order_number,
    o.status,
    o.total_amount,
    o.created_at,
    COUNT(oi.id) as item_count,
    SUM(oi.quantity) as total_items,
    -- First item image for preview
    (
        SELECT pi.url
        FROM order_items oi2
        JOIN product_variants pv ON oi2.variant_id = pv.id
        JOIN product_images pi ON pi.product_id = pv.product_id
        WHERE oi2.order_id = o.id
        ORDER BY pi.is_primary DESC, pi.position
        LIMIT 1
    ) as preview_image
FROM orders o
LEFT JOIN order_items oi ON oi.order_id = o.id
GROUP BY o.id;

-- Review listing view
CREATE OR REPLACE VIEW product_reviews AS
SELECT
    r.*,
    -- Customer info (anonymized if needed)
    json_build_object(
        'id', c.id,
        'first_name', c.first_name,
        'last_name', LEFT(c.last_name, 1) || '.',
        'is_verified', c.is_verified
    ) as customer,
    -- Product info
    json_build_object(
        'id', p.id,
        'name', p.name,
        'slug', p.slug
    ) as product,
    -- Helpfulness ratio
    CASE
        WHEN r.helpful_count + r.not_helpful_count > 0 THEN
            r.helpful_count::FLOAT / (r.helpful_count + r.not_helpful_count)
        ELSE NULL
    END as helpfulness_ratio
FROM reviews r
JOIN customers c ON r.customer_id = c.id
JOIN products p ON r.product_id = p.id
WHERE r.status = 'approved';

-- Wishlist view
CREATE OR REPLACE VIEW customer_wishlists AS
SELECT
    w.*,
    -- Item count
    COUNT(wi.id) as item_count,
    -- Items with product details
    COALESCE(
        json_agg(
            json_build_object(
                'id', wi.id,
                'priority', wi.priority,
                'notes', wi.notes,
                'added_at', wi.created_at,
                'product', json_build_object(
                    'id', p.id,
                    'name', p.name,
                    'slug', p.slug,
                    'price', MIN(pv.price),
                    'image_url', pi.url,
                    'in_stock', COALESCE(SUM(i.quantity - i.reserved_quantity), 0) > 0
                )
            ) ORDER BY wi.priority DESC, wi.created_at DESC
        ) FILTER (WHERE wi.id IS NOT NULL),
        '[]'::json
    ) as items
FROM wishlists w
LEFT JOIN wishlist_items wi ON wi.wishlist_id = w.id
LEFT JOIN products p ON wi.product_id = p.id AND p.is_active = true
LEFT JOIN product_variants pv ON pv.product_id = p.id AND pv.is_active = true
LEFT JOIN inventory i ON i.variant_id = pv.id
LEFT JOIN LATERAL (
    SELECT url FROM product_images
    WHERE product_id = p.id
    ORDER BY is_primary DESC, position
    LIMIT 1
) pi ON true
GROUP BY w.id;

-- Customer addresses view
CREATE OR REPLACE VIEW customer_addresses AS
SELECT
    a.*,
    -- Format as single line
    CONCAT_WS(', ',
        NULLIF(CONCAT_WS(' ', a.first_name, a.last_name), ''),
        a.company,
        a.address_line1,
        a.address_line2,
        a.city,
        a.state_province,
        a.postal_code,
        a.country_code
    ) as formatted_address,
    -- Usage count
    COUNT(DISTINCT o1.id) as shipping_usage_count,
    COUNT(DISTINCT o2.id) as billing_usage_count
FROM addresses a
LEFT JOIN orders o1 ON o1.shipping_address_id = a.id
LEFT JOIN orders o2 ON o2.billing_address_id = a.id
GROUP BY a.id;

-- Order analytics view
CREATE OR REPLACE VIEW order_analytics AS
SELECT
    DATE_TRUNC('day', created_at) as order_date,
    COUNT(*) as order_count,
    COUNT(DISTINCT customer_id) as unique_customers,
    SUM(total_amount) as revenue,
    AVG(total_amount) as average_order_value,
    SUM(subtotal) as subtotal,
    SUM(tax_amount) as tax_collected,
    SUM(shipping_amount) as shipping_collected,
    SUM(discount_amount) as discounts_given,
    COUNT(*) FILTER (WHERE status = 'completed') as completed_orders,
    COUNT(*) FILTER (WHERE status = 'cancelled') as cancelled_orders,
    COUNT(*) FILTER (WHERE payment_status = 'paid') as paid_orders
FROM orders
GROUP BY DATE_TRUNC('day', created_at);

-- Inventory alerts view
CREATE OR REPLACE VIEW inventory_alerts AS
SELECT
    i.*,
    pv.sku as variant_sku,
    pv.name as variant_name,
    p.id as product_id,
    p.name as product_name,
    p.sku as product_sku,
    i.quantity - i.reserved_quantity as available_quantity,
    CASE
        WHEN i.quantity - i.reserved_quantity <= 0 THEN 'out_of_stock'
        WHEN i.quantity - i.reserved_quantity <= i.low_stock_threshold THEN 'low_stock'
        ELSE 'in_stock'
    END as stock_status
FROM inventory i
JOIN product_variants pv ON i.variant_id = pv.id
JOIN products p ON pv.product_id = p.id
WHERE p.is_active = true AND pv.is_active = true
  AND (i.quantity - i.reserved_quantity <= i.low_stock_threshold);
