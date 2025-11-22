-- Seed data for e-commerce development
-- This creates sample data for testing and development

SET search_path TO ecommerce, public;

-- Sample users
INSERT INTO tb_user (email, password_hash, name, phone, is_verified) VALUES
('john.doe@example.com', crypt('password123', gen_salt('bf', 8)), 'John Doe', '+1234567890', true),
('jane.smith@example.com', crypt('password123', gen_salt('bf', 8)), 'Jane Smith', '+1234567891', true),
('bob.wilson@example.com', crypt('password123', gen_salt('bf', 8)), 'Bob Wilson', '+1234567892', false);

-- Sample addresses
INSERT INTO addresses (user_id, label, street1, street2, city, state, postal_code, country, is_default)
SELECT
    u.id,
    'Home',
    '123 Main St',
    'Apt 4B',
    'New York',
    'NY',
    '10001',
    'US',
    true
FROM tb_user u WHERE u.email = 'john.doe@example.com';

INSERT INTO addresses (user_id, label, street1, city, state, postal_code, country, is_default)
SELECT
    u.id,
    'Work',
    '456 Office Blvd',
    'New York',
    'NY',
    '10002',
    'US',
    false
FROM tb_user u WHERE u.email = 'john.doe@example.com';

-- Sample products
INSERT INTO products (sku, name, description, category, price, compare_at_price, inventory_count, images, tags) VALUES
-- Electronics
('LAPTOP-001', 'UltraBook Pro 15"', 'High-performance laptop with 16GB RAM, 512GB SSD, and dedicated graphics', 'electronics', 1299.99, 1499.99, 25,
 '["https://example.com/laptop1.jpg", "https://example.com/laptop2.jpg"]'::jsonb,
 '["laptop", "computer", "ultrabook", "pro"]'::jsonb),

('PHONE-001', 'SmartPhone X', 'Latest flagship smartphone with 5G, triple camera system', 'electronics', 899.99, 999.99, 50,
 '["https://example.com/phone1.jpg", "https://example.com/phone2.jpg"]'::jsonb,
 '["smartphone", "5g", "camera", "flagship"]'::jsonb),

('HEADPHONE-001', 'Wireless ANC Headphones', 'Premium noise-canceling wireless headphones', 'electronics', 299.99, 349.99, 100,
 '["https://example.com/headphones1.jpg"]'::jsonb,
 '["headphones", "wireless", "anc", "audio"]'::jsonb),

-- Clothing
('SHIRT-001', 'Classic Cotton T-Shirt', 'Comfortable 100% cotton t-shirt in multiple colors', 'clothing', 19.99, 24.99, 200,
 '["https://example.com/shirt1.jpg", "https://example.com/shirt2.jpg"]'::jsonb,
 '["tshirt", "cotton", "casual", "basics"]'::jsonb),

('JEANS-001', 'Slim Fit Denim Jeans', 'Modern slim fit jeans with stretch fabric', 'clothing', 59.99, 79.99, 150,
 '["https://example.com/jeans1.jpg"]'::jsonb,
 '["jeans", "denim", "slim-fit", "casual"]'::jsonb),

-- Books
('BOOK-001', 'The Art of Programming', 'Comprehensive guide to software development', 'books', 49.99, 59.99, 75,
 '["https://example.com/book1.jpg"]'::jsonb,
 '["programming", "software", "education", "technology"]'::jsonb),

('BOOK-002', 'GraphQL in Action', 'Learn GraphQL from basics to advanced concepts', 'books', 39.99, 44.99, 60,
 '["https://example.com/book2.jpg"]'::jsonb,
 '["graphql", "api", "web-development", "programming"]'::jsonb),

-- Home
('LAMP-001', 'Modern Desk Lamp', 'LED desk lamp with adjustable brightness and color temperature', 'home', 79.99, 99.99, 80,
 '["https://example.com/lamp1.jpg", "https://example.com/lamp2.jpg"]'::jsonb,
 '["lamp", "desk", "led", "lighting"]'::jsonb),

('CHAIR-001', 'Ergonomic Office Chair', 'Comfortable office chair with lumbar support', 'home', 299.99, 399.99, 30,
 '["https://example.com/chair1.jpg"]'::jsonb,
 '["chair", "office", "ergonomic", "furniture"]'::jsonb),

-- Sports
('YOGA-001', 'Premium Yoga Mat', 'Non-slip yoga mat with carrying strap', 'sports', 39.99, 49.99, 120,
 '["https://example.com/yoga1.jpg"]'::jsonb,
 '["yoga", "fitness", "mat", "exercise"]'::jsonb);

-- Sample coupons
INSERT INTO coupons (code, description, discount_type, discount_value, minimum_amount, usage_limit, valid_until) VALUES
('WELCOME10', 'Welcome discount - 10% off', 'percentage', 10, 50.00, 1000, CURRENT_TIMESTAMP + INTERVAL '30 days'),
('SAVE20', 'Save $20 on orders over $100', 'fixed', 20, 100.00, 500, CURRENT_TIMESTAMP + INTERVAL '14 days'),
('FREESHIP', 'Free shipping on any order', 'fixed', 10, NULL, NULL, CURRENT_TIMESTAMP + INTERVAL '7 days'),
('VIP50', 'VIP customer - 50% off', 'percentage', 50, 200.00, 10, CURRENT_TIMESTAMP + INTERVAL '3 days');

-- Sample orders with items (for john.doe)
DO $$
DECLARE
    user_id UUID;
    order_id UUID;
    addr_id UUID;
BEGIN
    -- Get John's ID and address
    SELECT u.id INTO user_id FROM tb_user u WHERE u.email = 'john.doe@example.com';
    SELECT a.id INTO addr_id FROM addresses a WHERE a.user_id = user_id AND a.is_default = true;

    -- Create a delivered order
    INSERT INTO orders (
        order_number, user_id, status, payment_status,
        shipping_address_id, billing_address_id,
        subtotal, tax_amount, shipping_amount, total,
        placed_at, shipped_at, delivered_at
    ) VALUES (
        'ORD-20240101-000001', user_id, 'delivered', 'captured',
        addr_id, addr_id,
        359.98, 28.80, 10.00, 398.78,
        CURRENT_TIMESTAMP - INTERVAL '10 days',
        CURRENT_TIMESTAMP - INTERVAL '8 days',
        CURRENT_TIMESTAMP - INTERVAL '5 days'
    ) RETURNING id INTO order_id;

    -- Add order items
    INSERT INTO order_items (order_id, product_id, quantity, price, total)
    SELECT order_id, p.id, 1, p.price, p.price
    FROM products p WHERE p.sku = 'PHONE-001';

    INSERT INTO order_items (order_id, product_id, quantity, price, total)
    SELECT order_id, p.id, 2, p.price, p.price * 2
    FROM products p WHERE p.sku = 'SHIRT-001';

    -- Create a processing order
    INSERT INTO orders (
        order_number, user_id, status, payment_status,
        shipping_address_id, billing_address_id,
        subtotal, tax_amount, shipping_amount, total,
        placed_at
    ) VALUES (
        'ORD-20240115-000002', user_id, 'processing', 'captured',
        addr_id, addr_id,
        299.99, 24.00, 10.00, 333.99,
        CURRENT_TIMESTAMP - INTERVAL '1 day'
    ) RETURNING id INTO order_id;

    INSERT INTO order_items (order_id, product_id, quantity, price, total)
    SELECT order_id, p.id, 1, p.price, p.price
    FROM products p WHERE p.sku = 'HEADPHONE-001';
END $$;

-- Sample reviews
INSERT INTO reviews (product_id, user_id, rating, title, comment, is_verified)
SELECT
    p.id, u.id, 5,
    'Excellent laptop!',
    'This laptop exceeded my expectations. Fast, reliable, and great battery life.',
    true
FROM products p, tb_user u
WHERE p.sku = 'LAPTOP-001' AND u.email = 'john.doe@example.com';

INSERT INTO reviews (product_id, user_id, rating, title, comment, is_verified)
SELECT
    p.id, u.id, 4,
    'Good phone, but pricey',
    'Great features and camera quality. A bit expensive but worth it for the performance.',
    false
FROM products p, tb_user u
WHERE p.sku = 'PHONE-001' AND u.email = 'jane.smith@example.com';

INSERT INTO reviews (product_id, user_id, rating, title, comment, is_verified)
SELECT
    p.id, u.id, 5,
    'Amazing sound quality',
    'The noise cancellation is incredible. Perfect for long flights and work from home.',
    true
FROM products p, tb_user u
WHERE p.sku = 'HEADPHONE-001' AND u.email = 'john.doe@example.com';

-- Sample wishlist items
INSERT INTO wishlist_items (user_id, product_id)
SELECT u.id, p.id
FROM tb_user u, products p
WHERE u.email = 'john.doe@example.com' AND p.sku IN ('LAPTOP-001', 'CHAIR-001');

INSERT INTO wishlist_items (user_id, product_id)
SELECT u.id, p.id
FROM tb_user u, products p
WHERE u.email = 'jane.smith@example.com' AND p.sku IN ('YOGA-001', 'BOOK-001');

-- Update product inventory based on orders
UPDATE products p
SET inventory_count = inventory_count - COALESCE((
    SELECT SUM(oi.quantity)
    FROM order_items oi
    JOIN orders o ON oi.order_id = o.id
    WHERE oi.product_id = p.id
    AND o.status NOT IN ('cancelled', 'refunded')
), 0);

-- Output summary
SELECT 'Seed data created:' as message;
SELECT COUNT(*) as users_count FROM tb_user;
SELECT COUNT(*) as products_count FROM products;
SELECT COUNT(*) as orders_count FROM orders;
SELECT COUNT(*) as reviews_count FROM reviews;
SELECT COUNT(*) as coupons_count FROM coupons;
