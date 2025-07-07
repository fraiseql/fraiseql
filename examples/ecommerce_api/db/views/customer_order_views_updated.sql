-- Customer and Order Views for E-commerce API (Updated for FraiseQL v0.1.0a14+)
-- Uses JSONB data column pattern

-- Customer profile view with stats
CREATE OR REPLACE VIEW customer_profile AS
SELECT
    c.id,                    -- For filtering
    c.email,                 -- For email lookups
    c.status,                -- For active/inactive filter
    jsonb_build_object(
        'id', c.id,
        'email', c.email,
        'first_name', c.first_name,
        'last_name', c.last_name,
        'phone', c.phone,
        'date_of_birth', c.date_of_birth,
        'status', c.status,
        'email_verified', c.email_verified,
        'created_at', c.created_at,
        'updated_at', c.updated_at,
        'total_orders', COUNT(DISTINCT o.id),
        'completed_orders', COUNT(DISTINCT o.id) FILTER (WHERE o.status = 'completed'),
        'lifetime_value', COALESCE(SUM(o.total_amount) FILTER (WHERE o.status = 'completed'), 0),
        'last_order_date', MAX(o.created_at),
        'address_count', COUNT(DISTINCT a.id),
        'wishlist_count', COUNT(DISTINCT w.id),
        'wishlist_items_count', COUNT(DISTINCT wi.id),
        'review_count', COUNT(DISTINCT r.id),
        'average_rating_given', AVG(r.rating)::DECIMAL(3,2),
        'has_active_cart', EXISTS(
            SELECT 1 FROM carts
            WHERE customer_id = c.id
            AND status = 'active'
            AND expires_at > CURRENT_TIMESTAMP
        )
    ) as data
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
    c.id,                    -- For filtering
    c.customer_id,           -- For customer carts
    c.session_id,            -- For guest carts
    c.status,                -- For active/abandoned filter
    c.expires_at,            -- For cleanup queries
    jsonb_build_object(
        'id', c.id,
        'customer_id', c.customer_id,
        'session_id', c.session_id,
        'status', c.status,
        'created_at', c.created_at,
        'updated_at', c.updated_at,
        'expires_at', c.expires_at,
        'customer', CASE
            WHEN c.customer_id IS NOT NULL THEN
                jsonb_build_object(
                    'id', cust.id,
                    'email', cust.email,
                    'first_name', cust.first_name,
                    'last_name', cust.last_name
                )
            ELSE NULL
        END,
        'items', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', ci.id,
                    'quantity', ci.quantity,
                    'price', ci.price,
                    'added_at', ci.created_at,
                    'variant', jsonb_build_object(
                        'id', pv.id,
                        'sku', pv.sku,
                        'name', pv.name,
                        'price', pv.price,
                        'compare_at_price', pv.compare_at_price,
                        'available', i.quantity - i.reserved_quantity > 0
                    ),
                    'product', jsonb_build_object(
                        'id', p.id,
                        'name', p.name,
                        'slug', p.slug,
                        'image_url', pi.url
                    )
                ) ORDER BY ci.created_at
            ) FILTER (WHERE ci.id IS NOT NULL),
            '[]'::jsonb
        ),
        'subtotal', COALESCE(SUM(ci.price * ci.quantity), 0),
        'item_count', COALESCE(SUM(ci.quantity), 0),
        'unique_items', COUNT(DISTINCT ci.id)
    ) as data
FROM carts c
LEFT JOIN customers cust ON c.customer_id = cust.id
LEFT JOIN cart_items ci ON ci.cart_id = c.id
LEFT JOIN product_variants pv ON ci.variant_id = pv.id
LEFT JOIN products p ON pv.product_id = p.id
LEFT JOIN inventory i ON i.variant_id = pv.id
LEFT JOIN product_images pi ON pi.product_id = p.id AND pi.is_primary = true
GROUP BY c.id, cust.id, cust.email, cust.first_name, cust.last_name;

-- Order detail view
CREATE OR REPLACE VIEW order_detail AS
SELECT
    o.id,                    -- For filtering
    o.customer_id,           -- For customer orders
    o.status,                -- For status filtering
    o.created_at,            -- For date range queries
    jsonb_build_object(
        'id', o.id,
        'order_number', o.order_number,
        'status', o.status,
        'payment_status', o.payment_status,
        'shipping_status', o.shipping_status,
        'currency', o.currency,
        'subtotal', o.subtotal,
        'tax_amount', o.tax_amount,
        'shipping_amount', o.shipping_amount,
        'discount_amount', o.discount_amount,
        'total_amount', o.total_amount,
        'notes', o.notes,
        'created_at', o.created_at,
        'updated_at', o.updated_at,
        'customer', jsonb_build_object(
            'id', c.id,
            'email', c.email,
            'first_name', c.first_name,
            'last_name', c.last_name,
            'phone', c.phone
        ),
        'shipping_address', CASE
            WHEN sa.id IS NOT NULL THEN
                jsonb_build_object(
                    'id', sa.id,
                    'label', sa.label,
                    'street1', sa.street1,
                    'street2', sa.street2,
                    'city', sa.city,
                    'state', sa.state,
                    'postal_code', sa.postal_code,
                    'country', sa.country
                )
            ELSE NULL
        END,
        'billing_address', CASE
            WHEN ba.id IS NOT NULL THEN
                jsonb_build_object(
                    'id', ba.id,
                    'label', ba.label,
                    'street1', ba.street1,
                    'street2', ba.street2,
                    'city', ba.city,
                    'state', ba.state,
                    'postal_code', ba.postal_code,
                    'country', ba.country
                )
            ELSE NULL
        END,
        'items', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', oi.id,
                    'quantity', oi.quantity,
                    'unit_price', oi.unit_price,
                    'discount_amount', oi.discount_amount,
                    'tax_amount', oi.tax_amount,
                    'total_amount', oi.total_amount,
                    'variant', jsonb_build_object(
                        'id', pv.id,
                        'sku', pv.sku,
                        'name', pv.name
                    ),
                    'product', jsonb_build_object(
                        'id', p.id,
                        'name', p.name,
                        'slug', p.slug,
                        'image_url', pi.url
                    )
                ) ORDER BY oi.id
            ) FILTER (WHERE oi.id IS NOT NULL),
            '[]'::jsonb
        ),
        'payment', CASE
            WHEN pay.id IS NOT NULL THEN
                jsonb_build_object(
                    'id', pay.id,
                    'method', pay.method,
                    'status', pay.status,
                    'amount', pay.amount,
                    'transaction_id', pay.transaction_id,
                    'processed_at', pay.processed_at
                )
            ELSE NULL
        END,
        'shipment', CASE
            WHEN ship.id IS NOT NULL THEN
                jsonb_build_object(
                    'id', ship.id,
                    'carrier', ship.carrier,
                    'tracking_number', ship.tracking_number,
                    'status', ship.status,
                    'shipped_at', ship.shipped_at,
                    'delivered_at', ship.delivered_at
                )
            ELSE NULL
        END
    ) as data
FROM orders o
LEFT JOIN customers c ON o.customer_id = c.id
LEFT JOIN addresses sa ON o.shipping_address_id = sa.id
LEFT JOIN addresses ba ON o.billing_address_id = ba.id
LEFT JOIN order_items oi ON oi.order_id = o.id
LEFT JOIN product_variants pv ON oi.variant_id = pv.id
LEFT JOIN products p ON pv.product_id = p.id
LEFT JOIN product_images pi ON pi.product_id = p.id AND pi.is_primary = true
LEFT JOIN payments pay ON pay.order_id = o.id AND pay.is_primary = true
LEFT JOIN shipments ship ON ship.order_id = o.id
GROUP BY o.id, c.id, sa.id, ba.id, pay.id, ship.id;

-- Customer orders summary
CREATE OR REPLACE VIEW customer_orders AS
SELECT
    o.id,                    -- For filtering
    o.customer_id,           -- For customer filter
    o.status,                -- For status filter
    o.created_at,            -- For date ordering
    jsonb_build_object(
        'id', o.id,
        'order_number', o.order_number,
        'status', o.status,
        'payment_status', o.payment_status,
        'shipping_status', o.shipping_status,
        'total_amount', o.total_amount,
        'created_at', o.created_at,
        'item_count', COUNT(DISTINCT oi.id),
        'items_preview', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'product_name', p.name,
                    'variant_name', pv.name,
                    'quantity', oi.quantity,
                    'image_url', pi.url
                ) ORDER BY oi.id
            ) FILTER (WHERE oi.id IS NOT NULL),
            '[]'::jsonb
        )
    ) as data
FROM orders o
LEFT JOIN order_items oi ON oi.order_id = o.id
LEFT JOIN product_variants pv ON oi.variant_id = pv.id
LEFT JOIN products p ON pv.product_id = p.id
LEFT JOIN product_images pi ON pi.product_id = p.id AND pi.is_primary = true
GROUP BY o.id;
