-- CQRS Enterprise Pattern - Read Model Views
-- Optimized views for query performance

-- ============================================================================
-- CUSTOMERS VIEW
-- ============================================================================

CREATE VIEW v_customers AS
SELECT
    id,
    email,
    name,
    phone,
    address,
    city,
    country,
    created_at,
    updated_at,
    version
FROM tb_customers;

-- ============================================================================
-- PRODUCTS VIEW WITH INVENTORY STATUS
-- ============================================================================

CREATE VIEW v_products AS
SELECT
    id,
    sku,
    name,
    description,
    price,
    cost,
    quantity_available,
    quantity_reserved,
    (quantity_available - quantity_reserved) as quantity_in_stock,
    CASE
        WHEN quantity_available - quantity_reserved <= 0 THEN 'out_of_stock'
        WHEN quantity_available - quantity_reserved < 10 THEN 'low_stock'
        ELSE 'in_stock'
    END as stock_status,
    is_active,
    created_at,
    updated_at,
    version
FROM tb_products;

-- ============================================================================
-- PRODUCT INVENTORY VIEW (Real-time inventory tracking)
-- ============================================================================

CREATE VIEW v_product_inventory AS
SELECT
    p.id as product_id,
    p.sku,
    p.name as product_name,
    p.quantity_available,
    p.quantity_reserved,
    (p.quantity_available - p.quantity_reserved) as quantity_in_stock,
    COALESCE(SUM(oi.quantity), 0) as quantity_in_orders,
    CASE
        WHEN p.quantity_available - p.quantity_reserved <= 0 THEN true
        ELSE false
    END as low_stock,
    p.is_active
FROM tb_products p
LEFT JOIN tb_order_items oi ON oi.product_id = p.id
LEFT JOIN tb_orders o ON oi.order_id = o.id AND o.status IN ('pending', 'paid', 'processing')
GROUP BY p.id, p.sku, p.name, p.quantity_available, p.quantity_reserved, p.is_active;

-- ============================================================================
-- ORDERS SUMMARY VIEW (Denormalized for performance)
-- ============================================================================

CREATE VIEW v_orders_summary AS
SELECT
    o.id,
    o.order_number,
    o.customer_id,
    c.name as customer_name,
    c.email as customer_email,
    c.country as customer_country,
    o.status,
    o.subtotal,
    o.tax,
    o.shipping,
    o.total,
    (SELECT COUNT(*) FROM tb_order_items WHERE order_id = o.id) as item_count,
    o.notes,
    o.paid_at,
    o.shipped_at,
    o.delivered_at,
    o.cancelled_at,
    o.cancellation_reason,
    o.created_at,
    o.updated_at,
    o.version
FROM tb_orders o
JOIN tb_customers c ON o.customer_id = c.id;

-- ============================================================================
-- ORDER DETAILS VIEW (Complete order information with items)
-- ============================================================================

CREATE VIEW v_order_items_details AS
SELECT
    oi.id,
    oi.order_id,
    o.order_number,
    oi.product_id,
    p.sku as product_sku,
    p.name as product_name,
    oi.quantity,
    oi.unit_price,
    oi.subtotal,
    p.price as current_price,  -- Compare with unit_price to see price changes
    oi.created_at
FROM tb_order_items oi
JOIN tb_orders o ON oi.order_id = o.id
JOIN tb_products p ON oi.product_id = p.id;

-- ============================================================================
-- CUSTOMER ORDERS VIEW (Customer order history)
-- ============================================================================

CREATE VIEW v_customer_orders AS
SELECT
    c.id as customer_id,
    c.name as customer_name,
    c.email as customer_email,
    o.id as order_id,
    o.order_number,
    o.status,
    o.total,
    (SELECT COUNT(*) FROM tb_order_items WHERE order_id = o.id) as item_count,
    o.created_at as order_date,
    o.paid_at,
    o.shipped_at,
    o.delivered_at,
    o.cancelled_at
FROM tb_customers c
JOIN tb_orders o ON o.customer_id = c.id;

-- ============================================================================
-- PAYMENTS VIEW
-- ============================================================================

CREATE VIEW v_payments AS
SELECT
    p.id,
    p.order_id,
    o.order_number,
    o.customer_id,
    c.name as customer_name,
    p.amount,
    p.payment_method,
    p.transaction_id,
    p.status,
    p.processed_at,
    p.refunded_at,
    p.refund_amount,
    p.notes,
    p.created_at
FROM tb_payments p
JOIN tb_orders o ON p.order_id = o.id
JOIN tb_customers c ON o.customer_id = c.id;

-- ============================================================================
-- REVENUE BY PRODUCT VIEW (Analytics)
-- ============================================================================

CREATE VIEW v_revenue_by_product AS
SELECT
    p.id as product_id,
    p.sku,
    p.name as product_name,
    COUNT(DISTINCT oi.order_id) as orders_count,
    SUM(oi.quantity) as units_sold,
    SUM(oi.subtotal) as total_revenue,
    AVG(oi.unit_price) as average_price,
    MIN(oi.unit_price) as min_price,
    MAX(oi.unit_price) as max_price,
    p.price as current_price,
    p.cost as current_cost,
    SUM(oi.subtotal) - (SUM(oi.quantity) * p.cost) as estimated_profit
FROM tb_products p
LEFT JOIN tb_order_items oi ON oi.product_id = p.id
LEFT JOIN tb_orders o ON oi.order_id = o.id AND o.status IN ('paid', 'processing', 'shipped', 'delivered')
GROUP BY p.id, p.sku, p.name, p.price, p.cost;

-- ============================================================================
-- CUSTOMER LIFETIME VALUE VIEW (Analytics)
-- ============================================================================

CREATE VIEW v_customer_lifetime_value AS
SELECT
    c.id as customer_id,
    c.email,
    c.name as customer_name,
    c.country,
    COUNT(o.id) as total_orders,
    COUNT(o.id) FILTER (WHERE o.status = 'delivered') as completed_orders,
    COUNT(o.id) FILTER (WHERE o.status = 'cancelled') as cancelled_orders,
    COALESCE(SUM(o.total) FILTER (WHERE o.status IN ('paid', 'processing', 'shipped', 'delivered')), 0) as lifetime_value,
    COALESCE(AVG(o.total) FILTER (WHERE o.status IN ('paid', 'processing', 'shipped', 'delivered')), 0) as average_order_value,
    MIN(o.created_at) as first_order_date,
    MAX(o.created_at) as last_order_date,
    c.created_at as customer_since
FROM tb_customers c
LEFT JOIN tb_orders o ON o.customer_id = c.id
GROUP BY c.id, c.email, c.name, c.country, c.created_at;

-- ============================================================================
-- AUDIT LOG VIEW (Enhanced with entity details)
-- ============================================================================

CREATE VIEW v_audit_log AS
SELECT
    al.id,
    al.operation,
    al.entity_type,
    al.entity_id,
    al.changed_by,
    al.old_values,
    al.new_values,
    al.changes,
    al.ip_address,
    al.user_agent,
    al.created_at,
    -- Entity-specific details (for orders)
    CASE
        WHEN al.entity_type = 'order' THEN (SELECT order_number FROM tb_orders WHERE id = al.entity_id)
        ELSE NULL
    END as order_number,
    CASE
        WHEN al.entity_type = 'customer' THEN (SELECT email FROM tb_customers WHERE id = al.entity_id)
        ELSE NULL
    END as customer_email
FROM tb_audit_log al;

-- ============================================================================
-- ORDER STATUS TIMELINE VIEW (Tracks order progression)
-- ============================================================================

CREATE VIEW v_order_status_timeline AS
SELECT
    o.id as order_id,
    o.order_number,
    o.created_at as order_created,
    o.paid_at,
    o.shipped_at,
    o.delivered_at,
    o.cancelled_at,
    -- Time to pay
    CASE
        WHEN o.paid_at IS NOT NULL THEN
            EXTRACT(EPOCH FROM (o.paid_at - o.created_at)) / 3600
        ELSE NULL
    END as hours_to_payment,
    -- Time to ship
    CASE
        WHEN o.shipped_at IS NOT NULL AND o.paid_at IS NOT NULL THEN
            EXTRACT(EPOCH FROM (o.shipped_at - o.paid_at)) / 3600
        ELSE NULL
    END as hours_to_shipment,
    -- Time to deliver
    CASE
        WHEN o.delivered_at IS NOT NULL AND o.shipped_at IS NOT NULL THEN
            EXTRACT(EPOCH FROM (o.delivered_at - o.shipped_at)) / 3600
        ELSE NULL
    END as hours_to_delivery,
    -- Total fulfillment time
    CASE
        WHEN o.delivered_at IS NOT NULL THEN
            EXTRACT(EPOCH FROM (o.delivered_at - o.created_at)) / 3600
        ELSE NULL
    END as total_fulfillment_hours,
    o.status
FROM tb_orders o;

-- ============================================================================
-- INDEXES ON FOREIGN KEYS FOR VIEW PERFORMANCE
-- (Already created in schema.sql, but listed here for documentation)
-- ============================================================================

-- CREATE INDEX idx_orders_customer ON tb_orders(customer_id);
-- CREATE INDEX idx_order_items_order ON tb_order_items(order_id);
-- CREATE INDEX idx_order_items_product ON tb_order_items(product_id);
-- CREATE INDEX idx_payments_order ON tb_payments(order_id);

-- ============================================================================
-- OPTIONAL: MATERIALIZED VIEWS FOR ANALYTICS
-- (Use for expensive queries that can tolerate slight staleness)
-- ============================================================================

-- Uncomment for production analytics workloads:

-- CREATE MATERIALIZED VIEW mv_revenue_by_product AS
-- SELECT * FROM v_revenue_by_product;
--
-- CREATE UNIQUE INDEX ON mv_revenue_by_product(product_id);
--
-- -- Refresh schedule (example: every 15 minutes via cron)
-- -- */15 * * * * psql -d cqrs_orders_demo -c "REFRESH MATERIALIZED VIEW CONCURRENTLY mv_revenue_by_product"

-- CREATE MATERIALIZED VIEW mv_customer_lifetime_value AS
-- SELECT * FROM v_customer_lifetime_value;
--
-- CREATE UNIQUE INDEX ON mv_customer_lifetime_value(customer_id);
