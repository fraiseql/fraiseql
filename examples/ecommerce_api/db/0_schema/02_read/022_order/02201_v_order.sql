CREATE OR REPLACE VIEW v_order AS
SELECT
    o.id,
    jsonb_build_object(
        'id', o.id::text,
        'order_number', o.order_number,
        'status', o.status,
        'total_amount', o.total_amount,
        'created_at', o.created_at,
        'customer', vc.data
    ) AS data
FROM orders o
JOIN v_customer vc ON vc.id = o.customer_id;
