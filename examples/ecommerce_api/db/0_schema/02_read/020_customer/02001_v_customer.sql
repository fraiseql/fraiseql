CREATE OR REPLACE VIEW v_customer AS
SELECT
    id,
    jsonb_build_object(
        'id', id::text,
        'email', email,
        'first_name', first_name,
        'last_name', last_name,
        'phone', phone,
        'is_verified', is_verified,
        'is_active', is_active,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM customers;
