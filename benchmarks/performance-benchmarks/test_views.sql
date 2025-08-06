-- Test FraiseQL views
SET search_path TO benchmark, public;

-- Check if views exist
SELECT table_name
FROM information_schema.views
WHERE table_schema = 'benchmark'
AND table_name LIKE 'v_%'
ORDER BY table_name;

-- Test v_users view
SELECT
    id,
    data->>'email' as email,
    data->>'username' as username,
    jsonb_typeof(data) as data_type
FROM v_users
LIMIT 5;

-- Test v_products view
SELECT
    id,
    data->>'name' as name,
    data->>'price' as price,
    jsonb_typeof(data) as data_type
FROM v_products
LIMIT 5;

-- Test v_orders view
SELECT
    id,
    data->>'orderNumber' as order_number,
    data->>'status' as status,
    jsonb_typeof(data) as data_type
FROM v_orders
LIMIT 5;
