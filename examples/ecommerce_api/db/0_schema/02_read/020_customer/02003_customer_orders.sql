-- Customer orders view (query side)
-- Denormalized view of customers with their order history

CREATE OR REPLACE VIEW customer_orders AS
SELECT
    c.id as customer_id,
    c.email,
    c.first_name,
    c.last_name,
    o.id as order_id,
    o.order_number,
    o.status,
    o.total_amount,
    o.created_at as order_date,
    COUNT(oi.id) as item_count
FROM customers c
LEFT JOIN orders o ON c.id = o.customer_id
LEFT JOIN order_items oi ON o.id = oi.order_id
GROUP BY c.id, c.email, c.first_name, c.last_name, o.id, o.order_number, o.status, o.total_amount, o.created_at;
