-- Sample Data for E-commerce API
-- This creates a realistic dataset for testing and demonstration

-- Categories
INSERT INTO categories (id, name, slug, description, parent_id) VALUES
('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'Electronics', 'electronics', 'Electronic devices and accessories', NULL),
('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a12', 'Computers', 'computers', 'Desktop and laptop computers', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'),
('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a13', 'Smartphones', 'smartphones', 'Mobile phones and tablets', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'),
('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a14', 'Clothing', 'clothing', 'Fashion and apparel', NULL),
('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a15', 'Men''s Clothing', 'mens-clothing', 'Clothing for men', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a14'),
('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a16', 'Women''s Clothing', 'womens-clothing', 'Clothing for women', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a14'),
('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a17', 'Books', 'books', 'Physical and digital books', NULL),
('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a18', 'Home & Garden', 'home-garden', 'Home improvement and gardening', NULL);

-- Products
INSERT INTO products (id, sku, name, slug, description, short_description, category_id, brand, tags, is_featured) VALUES
-- Laptops
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b11', 'LAPTOP-001', 'ProBook 15 Professional Laptop', 'probook-15-professional',
 'High-performance laptop with Intel Core i7 processor, 16GB RAM, and 512GB SSD. Perfect for professionals and power users.',
 'Professional laptop with i7, 16GB RAM, 512GB SSD',
 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a12', 'TechPro', ARRAY['laptop', 'computer', 'professional', 'intel'], true),

('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b12', 'LAPTOP-002', 'UltraBook Air 13', 'ultrabook-air-13',
 'Ultra-lightweight laptop with amazing battery life. Features Intel Core i5, 8GB RAM, and 256GB SSD.',
 'Lightweight laptop with 12-hour battery life',
 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a12', 'AirTech', ARRAY['laptop', 'ultrabook', 'portable', 'lightweight'], false),

-- Smartphones
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b13', 'PHONE-001', 'SmartPhone Pro Max', 'smartphone-pro-max',
 'Latest flagship smartphone with 6.7" OLED display, triple camera system, and 5G connectivity.',
 'Flagship phone with pro camera system',
 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a13', 'PhoneTech', ARRAY['smartphone', '5g', 'camera', 'flagship'], true),

('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b14', 'PHONE-002', 'Budget Phone Plus', 'budget-phone-plus',
 'Affordable smartphone with great features. 6.5" display, dual cameras, and all-day battery life.',
 'Affordable phone with premium features',
 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a13', 'ValueTech', ARRAY['smartphone', 'budget', 'value'], false),

-- Clothing
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b15', 'SHIRT-001', 'Classic Oxford Shirt', 'classic-oxford-shirt',
 'Timeless Oxford shirt made from 100% cotton. Perfect for business casual or smart casual occasions.',
 'Classic cotton Oxford shirt',
 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a15', 'StyleCo', ARRAY['shirt', 'oxford', 'cotton', 'business'], false),

('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b16', 'DRESS-001', 'Summer Floral Dress', 'summer-floral-dress',
 'Beautiful floral print dress perfect for summer. Made from breathable fabric with a flattering A-line cut.',
 'Floral summer dress with A-line cut',
 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a16', 'FashionForward', ARRAY['dress', 'summer', 'floral', 'casual'], true);

-- Product Variants
INSERT INTO product_variants (id, product_id, sku, name, price, compare_at_price, attributes) VALUES
-- Laptop variants
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c11', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b11', 'LAPTOP-001-16-512', '16GB RAM / 512GB SSD', 1299.99, 1499.99, '{"ram": "16GB", "storage": "512GB"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c12', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b11', 'LAPTOP-001-32-1TB', '32GB RAM / 1TB SSD', 1599.99, 1799.99, '{"ram": "32GB", "storage": "1TB"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c13', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b12', 'LAPTOP-002-8-256', '8GB RAM / 256GB SSD', 899.99, 999.99, '{"ram": "8GB", "storage": "256GB"}'),

-- Phone variants
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c14', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b13', 'PHONE-001-128', '128GB Storage', 999.99, NULL, '{"storage": "128GB", "color": "Black"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c15', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b13', 'PHONE-001-256', '256GB Storage', 1099.99, NULL, '{"storage": "256GB", "color": "Black"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c16', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b14', 'PHONE-002-64', '64GB Storage', 299.99, 399.99, '{"storage": "64GB", "color": "Blue"}'),

-- Clothing variants
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c17', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b15', 'SHIRT-001-M-BLUE', 'Medium - Blue', 59.99, 79.99, '{"size": "M", "color": "Blue"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c18', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b15', 'SHIRT-001-L-BLUE', 'Large - Blue', 59.99, 79.99, '{"size": "L", "color": "Blue"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c19', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b15', 'SHIRT-001-M-WHITE', 'Medium - White', 59.99, 79.99, '{"size": "M", "color": "White"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c20', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b16', 'DRESS-001-S', 'Small', 89.99, 119.99, '{"size": "S"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c21', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b16', 'DRESS-001-M', 'Medium', 89.99, 119.99, '{"size": "M"}'),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c22', 'b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b16', 'DRESS-001-L', 'Large', 89.99, 119.99, '{"size": "L"}');

-- Inventory
INSERT INTO inventory (variant_id, quantity, reserved_quantity, warehouse_location, low_stock_threshold) VALUES
-- Laptops
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c11', 50, 5, 'WAREHOUSE-A-15', 10),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c12', 20, 2, 'WAREHOUSE-A-15', 5),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c13', 100, 10, 'WAREHOUSE-A-16', 20),
-- Phones
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c14', 75, 8, 'WAREHOUSE-B-10', 15),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c15', 50, 5, 'WAREHOUSE-B-10', 10),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c16', 200, 20, 'WAREHOUSE-B-11', 30),
-- Clothing
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c17', 100, 5, 'WAREHOUSE-C-05', 20),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c18', 100, 8, 'WAREHOUSE-C-05', 20),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c19', 100, 10, 'WAREHOUSE-C-05', 20),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c20', 50, 3, 'WAREHOUSE-C-06', 10),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c21', 75, 5, 'WAREHOUSE-C-06', 15),
('c0eebc99-9c0b-4ef8-bb6d-6bb9bd380c22', 50, 2, 'WAREHOUSE-C-06', 10);

-- Product Images
INSERT INTO product_images (product_id, url, alt_text, position, is_primary) VALUES
-- Laptop images
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b11', 'https://images.example.com/laptop-pro-1.jpg', 'ProBook 15 front view', 0, true),
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b11', 'https://images.example.com/laptop-pro-2.jpg', 'ProBook 15 side view', 1, false),
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b12', 'https://images.example.com/ultrabook-1.jpg', 'UltraBook Air 13', 0, true),
-- Phone images
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b13', 'https://images.example.com/phone-pro-1.jpg', 'SmartPhone Pro Max', 0, true),
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b13', 'https://images.example.com/phone-pro-2.jpg', 'Camera detail', 1, false),
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b14', 'https://images.example.com/budget-phone-1.jpg', 'Budget Phone Plus', 0, true),
-- Clothing images
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b15', 'https://images.example.com/oxford-shirt-1.jpg', 'Classic Oxford Shirt', 0, true),
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b16', 'https://images.example.com/floral-dress-1.jpg', 'Summer Floral Dress', 0, true);

-- Sample Customers
INSERT INTO customers (id, email, password_hash, first_name, last_name, is_verified) VALUES
('d0eebc99-9c0b-4ef8-bb6d-6bb9bd380d11', 'john.doe@example.com', '$2b$12$KIXxPfGJiOmGKZLKlqq0bOV8.X3yYc0se/uyGWGd7AvpRnx7s4LFO', 'John', 'Doe', true),
('d0eebc99-9c0b-4ef8-bb6d-6bb9bd380d12', 'jane.smith@example.com', '$2b$12$KIXxPfGJiOmGKZLKlqq0bOV8.X3yYc0se/uyGWGd7AvpRnx7s4LFO', 'Jane', 'Smith', true),
('d0eebc99-9c0b-4ef8-bb6d-6bb9bd380d13', 'test.user@example.com', '$2b$12$KIXxPfGJiOmGKZLKlqq0bOV8.X3yYc0se/uyGWGd7AvpRnx7s4LFO', 'Test', 'User', false);

-- Sample Addresses
INSERT INTO addresses (customer_id, type, first_name, last_name, address_line1, city, state_province, postal_code, country_code, is_default) VALUES
('d0eebc99-9c0b-4ef8-bb6d-6bb9bd380d11', 'both', 'John', 'Doe', '123 Main St', 'New York', 'NY', '10001', 'US', true),
('d0eebc99-9c0b-4ef8-bb6d-6bb9bd380d12', 'both', 'Jane', 'Smith', '456 Oak Ave', 'Los Angeles', 'CA', '90001', 'US', true);

-- Sample Reviews
INSERT INTO reviews (product_id, customer_id, rating, title, comment, is_verified_purchase, status) VALUES
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b11', 'd0eebc99-9c0b-4ef8-bb6d-6bb9bd380d11', 5, 'Excellent laptop!', 'Great performance and build quality. Highly recommended for professionals.', true, 'approved'),
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b11', 'd0eebc99-9c0b-4ef8-bb6d-6bb9bd380d12', 4, 'Good but pricey', 'Solid laptop but a bit expensive for what you get.', true, 'approved'),
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b13', 'd0eebc99-9c0b-4ef8-bb6d-6bb9bd380d11', 5, 'Best phone ever!', 'Amazing camera and battery life. Worth every penny.', true, 'approved'),
('b0eebc99-9c0b-4ef8-bb6d-6bb9bd380b15', 'd0eebc99-9c0b-4ef8-bb6d-6bb9bd380d12', 4, 'Classic style', 'Good quality shirt, fits well. Would buy again.', true, 'approved');

-- Sample Coupons
INSERT INTO coupons (code, description, discount_type, discount_value, minimum_purchase_amount, usage_limit, valid_until) VALUES
('WELCOME10', 'Welcome discount - 10% off', 'percentage', 10, 50, 1000, CURRENT_TIMESTAMP + INTERVAL '30 days'),
('SAVE20', 'Save $20 on orders over $100', 'fixed_amount', 20, 100, 500, CURRENT_TIMESTAMP + INTERVAL '30 days'),
('FREESHIP', 'Free shipping on any order', 'fixed_amount', 10, 0, NULL, CURRENT_TIMESTAMP + INTERVAL '90 days');
