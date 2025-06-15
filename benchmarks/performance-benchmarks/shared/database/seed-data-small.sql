-- Small seed data for quick testing
-- Categories
TRUNCATE TABLE benchmark.categories CASCADE;
INSERT INTO benchmark.categories (name, description) VALUES
('Electronics', 'Electronic devices and accessories'),
('Books', 'Physical and digital books'),
('Clothing', 'Apparel and fashion items'),
('Home', 'Home and garden products'),
('Sports', 'Sports and outdoor equipment');

-- Generate users (only 1000)
DO $$
DECLARE
    i INTEGER;
BEGIN
    FOR i IN 1..1000 LOOP
        INSERT INTO benchmark.users (
            email,
            username,
            full_name,
            is_active
        ) VALUES (
            'user' || i || '@example.com',
            'user' || i,
            'Test User ' || i,
            i % 10 != 0  -- 90% active
        );
    END LOOP;
    RAISE NOTICE 'Generated 1000 users';
END $$;

-- Generate products (only 5000)
DO $$
DECLARE
    i INTEGER;
    cat_id UUID;
BEGIN
    FOR i IN 1..5000 LOOP
        -- Rotate through categories
        SELECT id INTO cat_id FROM benchmark.categories
        ORDER BY id LIMIT 1 OFFSET (i % 5);

        INSERT INTO benchmark.products (
            sku,
            name,
            description,
            price,
            stock_quantity,
            category_id,
            is_active
        ) VALUES (
            'SKU-' || LPAD(i::text, 6, '0'),
            'Product ' || i,
            'Description for product ' || i || '. This is a sample product in our catalog.',
            (RANDOM() * 1000 + 10)::DECIMAL(10,2),
            FLOOR(RANDOM() * 1000)::INTEGER,
            cat_id,
            i % 20 != 0  -- 95% active
        );
    END LOOP;
    RAISE NOTICE 'Generated 5000 products';
END $$;

-- Generate orders (only 2000)
DO $$
DECLARE
    i INTEGER;
    user_id UUID;
    order_id UUID;
    num_items INTEGER;
    j INTEGER;
    product_id UUID;
BEGIN
    FOR i IN 1..2000 LOOP
        -- Random user
        SELECT id INTO user_id FROM benchmark.users
        ORDER BY RANDOM() LIMIT 1;

        -- Create order
        INSERT INTO benchmark.orders (
            id,
            user_id,
            order_date,
            status,
            total_amount
        ) VALUES (
            gen_random_uuid(),
            user_id,
            CURRENT_TIMESTAMP - (RANDOM() * INTERVAL '365 days'),
            (ARRAY['pending', 'processing', 'shipped', 'delivered', 'cancelled'])[FLOOR(RANDOM() * 5 + 1)],
            0  -- Will update later
        ) RETURNING id INTO order_id;

        -- Add 1-5 items per order
        num_items := FLOOR(RANDOM() * 5 + 1)::INTEGER;

        FOR j IN 1..num_items LOOP
            SELECT id INTO product_id FROM benchmark.products
            WHERE is_active = true
            ORDER BY RANDOM() LIMIT 1;

            INSERT INTO benchmark.order_items (
                order_id,
                product_id,
                quantity,
                unit_price
            )
            SELECT
                order_id,
                product_id,
                FLOOR(RANDOM() * 5 + 1)::INTEGER,
                price
            FROM benchmark.products
            WHERE id = product_id;
        END LOOP;

        -- Update order total
        UPDATE benchmark.orders o
        SET total_amount = (
            SELECT SUM(oi.quantity * oi.unit_price)
            FROM benchmark.order_items oi
            WHERE oi.order_id = o.id
        )
        WHERE o.id = order_id;

        IF i % 500 = 0 THEN
            RAISE NOTICE 'Generated % orders', i;
        END IF;
    END LOOP;
END $$;

-- Add some reviews
DO $$
DECLARE
    i INTEGER;
    user_id UUID;
    product_id UUID;
BEGIN
    FOR i IN 1..1000 LOOP
        SELECT id INTO user_id FROM benchmark.users ORDER BY RANDOM() LIMIT 1;
        SELECT id INTO product_id FROM benchmark.products WHERE is_active = true ORDER BY RANDOM() LIMIT 1;

        BEGIN
            INSERT INTO benchmark.reviews (
                user_id,
                product_id,
                rating,
                title,
                comment
            ) VALUES (
                user_id,
                product_id,
                FLOOR(RANDOM() * 5 + 1)::INTEGER,
                'Review ' || i,
                'This is review number ' || i || ' for the product. ' ||
                CASE FLOOR(RANDOM() * 5 + 1)::INTEGER
                    WHEN 5 THEN 'Excellent product, highly recommended!'
                    WHEN 4 THEN 'Good product, works as expected.'
                    WHEN 3 THEN 'Average product, has some issues.'
                    WHEN 2 THEN 'Below average, needs improvement.'
                    ELSE 'Poor quality, would not recommend.'
                END
            );
        EXCEPTION WHEN unique_violation THEN
            -- Skip if user already reviewed this product
            NULL;
        END;
    END LOOP;
    RAISE NOTICE 'Generated reviews';
END $$;

-- Update indices
REINDEX SCHEMA benchmark;
