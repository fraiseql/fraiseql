-- Adaptive seed data generation for benchmark testing
-- This script generates data based on environment variables set by profile detection

SET search_path TO benchmark, public;

-- Get scale parameters from environment or use defaults
\set users_count `echo ${BENCHMARK_USERS:-1000}`
\set products_count `echo ${BENCHMARK_PRODUCTS:-5000}`
\set orders_count `echo ${BENCHMARK_ORDERS:-2000}`

-- Display configuration
\echo 'Benchmark Data Generation Configuration:'
\echo '  Users:    ' :users_count
\echo '  Products: ' :products_count
\echo '  Orders:   ' :orders_count

-- Function to generate random text
CREATE OR REPLACE FUNCTION random_text(length INTEGER) RETURNS TEXT AS $$
DECLARE
    chars TEXT := 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
    result TEXT := '';
    i INTEGER;
BEGIN
    FOR i IN 1..length LOOP
        result := result || substr(chars, floor(random() * length(chars) + 1)::INTEGER, 1);
    END LOOP;
    RETURN result;
END;
$$ LANGUAGE plpgsql;

-- Function to generate random email
CREATE OR REPLACE FUNCTION random_email() RETURNS TEXT AS $$
DECLARE
    domains TEXT[] := ARRAY['gmail.com', 'yahoo.com', 'outlook.com', 'example.com', 'test.com'];
    username TEXT;
BEGIN
    username := lower(random_text(8));
    RETURN username || '@' || domains[floor(random() * array_length(domains, 1) + 1)::INTEGER];
END;
$$ LANGUAGE plpgsql;

-- Function to generate random product name
CREATE OR REPLACE FUNCTION random_product_name() RETURNS TEXT AS $$
DECLARE
    adjectives TEXT[] := ARRAY['Premium', 'Professional', 'Advanced', 'Ultimate', 'Essential', 'Deluxe', 'Standard', 'Elite', 'Pro', 'Basic'];
    products TEXT[] := ARRAY['Widget', 'Gadget', 'Device', 'Tool', 'Appliance', 'Equipment', 'Instrument', 'Apparatus', 'Machine', 'System'];
    models TEXT[] := ARRAY['X', 'Pro', 'Plus', 'Max', 'Ultra', 'Lite', 'Mini', 'Air', 'Neo', 'One'];
BEGIN
    RETURN adjectives[floor(random() * array_length(adjectives, 1) + 1)::INTEGER] || ' ' ||
           products[floor(random() * array_length(products, 1) + 1)::INTEGER] || ' ' ||
           models[floor(random() * array_length(models, 1) + 1)::INTEGER];
END;
$$ LANGUAGE plpgsql;

-- Clear existing data
TRUNCATE TABLE cart_items, addresses, reviews, order_items, orders, products, categories, users CASCADE;

-- Generate categories (adaptive: 5-100 based on product count)
WITH category_count AS (
    SELECT LEAST(100, GREATEST(5, :products_count::INTEGER / 100)) as count
)
INSERT INTO categories (name, slug, description)
SELECT
    'Category ' || i,
    'category-' || i,
    'Description for category ' || i
FROM generate_series(1, (SELECT count FROM category_count)) i;

-- Generate users with progress tracking
DO $$
DECLARE
    batch_size INTEGER := 1000;
    total_users INTEGER := :users_count;
    current_batch INTEGER := 0;
    start_time TIMESTAMP := clock_timestamp();
BEGIN
    RAISE NOTICE 'Starting user generation: % users', total_users;

    FOR i IN 0..(total_users / batch_size) LOOP
        INSERT INTO users (username, email, password_hash, full_name, created_at)
        SELECT
            'user_' || (i * batch_size + s),
            random_email(),
            md5(random()::TEXT),
            'User ' || (i * batch_size + s),
            NOW() - INTERVAL '2 years' + (random() * INTERVAL '2 years')
        FROM generate_series(1, LEAST(batch_size, total_users - i * batch_size)) s;

        current_batch := current_batch + LEAST(batch_size, total_users - i * batch_size);

        IF current_batch % 10000 = 0 OR current_batch = total_users THEN
            RAISE NOTICE 'Generated % users (%.1f%%)', current_batch, (current_batch::FLOAT / total_users * 100);
        END IF;
    END LOOP;

    RAISE NOTICE 'User generation completed in % seconds',
        EXTRACT(EPOCH FROM (clock_timestamp() - start_time));
END $$;

-- Generate products with progress tracking
DO $$
DECLARE
    batch_size INTEGER := 1000;
    total_products INTEGER := :products_count;
    current_batch INTEGER := 0;
    category_count INTEGER;
    start_time TIMESTAMP := clock_timestamp();
BEGIN
    SELECT COUNT(*) INTO category_count FROM categories;
    RAISE NOTICE 'Starting product generation: % products', total_products;

    FOR i IN 0..(total_products / batch_size) LOOP
        INSERT INTO products (name, slug, description, price, category_id, stock_quantity, sku, created_at)
        SELECT
            random_product_name(),
            'product-' || (i * batch_size + s),
            'Description for product ' || (i * batch_size + s),
            (random() * 990 + 10)::DECIMAL(10,2),
            floor(random() * category_count + 1)::INTEGER,
            floor(random() * 1000)::INTEGER,
            'SKU-' || (i * batch_size + s),
            NOW() - INTERVAL '1 year' + (random() * INTERVAL '1 year')
        FROM generate_series(1, LEAST(batch_size, total_products - i * batch_size)) s;

        current_batch := current_batch + LEAST(batch_size, total_products - i * batch_size);

        IF current_batch % 10000 = 0 OR current_batch = total_products THEN
            RAISE NOTICE 'Generated % products (%.1f%%)', current_batch, (current_batch::FLOAT / total_products * 100);
        END IF;
    END LOOP;

    RAISE NOTICE 'Product generation completed in % seconds',
        EXTRACT(EPOCH FROM (clock_timestamp() - start_time));
END $$;

-- Generate orders with progress tracking
DO $$
DECLARE
    batch_size INTEGER := 1000;
    total_orders INTEGER := :orders_count;
    current_batch INTEGER := 0;
    user_count INTEGER;
    start_time TIMESTAMP := clock_timestamp();
BEGIN
    SELECT COUNT(*) INTO user_count FROM users;
    RAISE NOTICE 'Starting order generation: % orders', total_orders;

    FOR i IN 0..(total_orders / batch_size) LOOP
        INSERT INTO orders (user_id, status, total_amount, created_at, updated_at)
        SELECT
            floor(random() * user_count + 1)::INTEGER,
            (ARRAY['pending', 'processing', 'shipped', 'delivered', 'cancelled'])[floor(random() * 5 + 1)::INTEGER],
            0, -- Will be updated after order items
            NOW() - INTERVAL '6 months' + (random() * INTERVAL '6 months'),
            NOW() - INTERVAL '6 months' + (random() * INTERVAL '6 months')
        FROM generate_series(1, LEAST(batch_size, total_orders - i * batch_size)) s;

        current_batch := current_batch + LEAST(batch_size, total_orders - i * batch_size);

        IF current_batch % 10000 = 0 OR current_batch = total_orders THEN
            RAISE NOTICE 'Generated % orders (%.1f%%)', current_batch, (current_batch::FLOAT / total_orders * 100);
        END IF;
    END LOOP;

    RAISE NOTICE 'Order generation completed in % seconds',
        EXTRACT(EPOCH FROM (clock_timestamp() - start_time));
END $$;

-- Generate order items (average 2-3 items per order)
DO $$
DECLARE
    batch_size INTEGER := 5000;
    order_count INTEGER;
    product_count INTEGER;
    items_generated INTEGER := 0;
    estimated_total INTEGER;
    start_time TIMESTAMP := clock_timestamp();
BEGIN
    SELECT COUNT(*) INTO order_count FROM orders;
    SELECT COUNT(*) INTO product_count FROM products;
    estimated_total := order_count * 2.5; -- Average 2.5 items per order

    RAISE NOTICE 'Starting order items generation: ~% items', estimated_total;

    -- Generate items in batches
    FOR i IN 0..(order_count / batch_size) LOOP
        INSERT INTO order_items (order_id, product_id, quantity, price_at_time)
        SELECT
            o.id,
            floor(random() * product_count + 1)::INTEGER,
            floor(random() * 5 + 1)::INTEGER,
            p.price
        FROM (
            SELECT id
            FROM orders
            ORDER BY id
            LIMIT batch_size
            OFFSET i * batch_size
        ) o
        CROSS JOIN LATERAL (
            SELECT price, generate_series(1, floor(random() * 3 + 1)::INTEGER)
            FROM products
            WHERE id = floor(random() * product_count + 1)::INTEGER
            LIMIT 1
        ) p;

        items_generated := items_generated + batch_size * 2;

        IF items_generated % 50000 = 0 THEN
            RAISE NOTICE 'Generated ~% order items', items_generated;
        END IF;
    END LOOP;

    -- Update order totals
    UPDATE orders o
    SET total_amount = (
        SELECT COALESCE(SUM(oi.quantity * oi.price_at_time), 0)
        FROM order_items oi
        WHERE oi.order_id = o.id
    );

    RAISE NOTICE 'Order items generation completed in % seconds',
        EXTRACT(EPOCH FROM (clock_timestamp() - start_time));
END $$;

-- Generate reviews (adaptive: ~10-20% of orders)
DO $$
DECLARE
    review_count INTEGER;
    user_count INTEGER;
    product_count INTEGER;
    start_time TIMESTAMP := clock_timestamp();
BEGIN
    review_count := GREATEST(100, :orders_count::INTEGER / 5);
    SELECT COUNT(*) INTO user_count FROM users;
    SELECT COUNT(*) INTO product_count FROM products;

    RAISE NOTICE 'Generating % reviews...', review_count;

    INSERT INTO reviews (user_id, product_id, rating, comment, created_at)
    SELECT
        floor(random() * user_count + 1)::INTEGER,
        floor(random() * product_count + 1)::INTEGER,
        floor(random() * 5 + 1)::INTEGER,
        'Review comment ' || i,
        NOW() - INTERVAL '6 months' + (random() * INTERVAL '6 months')
    FROM generate_series(1, review_count) i
    ON CONFLICT (user_id, product_id) DO NOTHING;

    RAISE NOTICE 'Review generation completed in % seconds',
        EXTRACT(EPOCH FROM (clock_timestamp() - start_time));
END $$;

-- Generate some cart items for active users
DO $$
DECLARE
    active_user_count INTEGER;
    product_count INTEGER;
BEGIN
    active_user_count := GREATEST(10, :users_count::INTEGER / 10);
    SELECT COUNT(*) INTO product_count FROM products;

    RAISE NOTICE 'Generating cart items for % active users...', active_user_count;

    INSERT INTO cart_items (user_id, product_id, quantity, added_at)
    SELECT
        user_id,
        floor(random() * product_count + 1)::INTEGER,
        floor(random() * 3 + 1)::INTEGER,
        NOW() - (random() * INTERVAL '7 days')
    FROM (
        SELECT id as user_id, generate_series(1, floor(random() * 5 + 1)::INTEGER)
        FROM users
        ORDER BY created_at DESC
        LIMIT active_user_count
    ) u
    ON CONFLICT (user_id, product_id) DO NOTHING;
END $$;

-- Generate addresses for some users
DO $$
DECLARE
    address_user_count INTEGER;
BEGIN
    address_user_count := GREATEST(50, :users_count::INTEGER / 2);

    RAISE NOTICE 'Generating addresses for % users...', address_user_count;

    INSERT INTO addresses (user_id, street, city, state, postal_code, country, is_default)
    SELECT
        id,
        floor(random() * 9999 + 1)::TEXT || ' ' ||
            (ARRAY['Main St', 'Oak Ave', 'First St', 'Park Rd', 'Elm St'])[floor(random() * 5 + 1)::INTEGER],
        (ARRAY['New York', 'Los Angeles', 'Chicago', 'Houston', 'Phoenix'])[floor(random() * 5 + 1)::INTEGER],
        (ARRAY['NY', 'CA', 'IL', 'TX', 'AZ'])[floor(random() * 5 + 1)::INTEGER],
        lpad(floor(random() * 99999 + 1)::TEXT, 5, '0'),
        'USA',
        random() > 0.5
    FROM users
    ORDER BY created_at DESC
    LIMIT address_user_count;
END $$;

-- Display summary statistics
DO $$
DECLARE
    user_count INTEGER;
    product_count INTEGER;
    order_count INTEGER;
    order_item_count INTEGER;
    review_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO user_count FROM users;
    SELECT COUNT(*) INTO product_count FROM products;
    SELECT COUNT(*) INTO order_count FROM orders;
    SELECT COUNT(*) INTO order_item_count FROM order_items;
    SELECT COUNT(*) INTO review_count FROM reviews;

    RAISE NOTICE '';
    RAISE NOTICE '=== Data Generation Summary ===';
    RAISE NOTICE 'Users:       %', user_count;
    RAISE NOTICE 'Products:    %', product_count;
    RAISE NOTICE 'Orders:      %', order_count;
    RAISE NOTICE 'Order Items: %', order_item_count;
    RAISE NOTICE 'Reviews:     %', review_count;
    RAISE NOTICE '==============================';
END $$;

-- Create indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_orders_user_id ON orders(user_id);
CREATE INDEX IF NOT EXISTS idx_orders_created_at ON orders(created_at);
CREATE INDEX IF NOT EXISTS idx_order_items_order_id ON order_items(order_id);
CREATE INDEX IF NOT EXISTS idx_order_items_product_id ON order_items(product_id);
CREATE INDEX IF NOT EXISTS idx_products_category_id ON products(category_id);
CREATE INDEX IF NOT EXISTS idx_reviews_product_id ON reviews(product_id);
CREATE INDEX IF NOT EXISTS idx_reviews_user_id ON reviews(user_id);

-- Analyze tables for query optimization
ANALYZE users;
ANALYZE products;
ANALYZE orders;
ANALYZE order_items;
ANALYZE reviews;
