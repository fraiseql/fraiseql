-- FraiseQL E-Commerce Example - Database Setup (Trinity Pattern)
-- PostgreSQL
-- Pattern: tb_* (table), pk_* (INTEGER primary key), fk_* (INTEGER foreign key), id (UUID), v_* (view)

-- Drop existing objects if present
DROP TABLE IF EXISTS tb_order_items CASCADE;
DROP TABLE IF EXISTS tb_orders CASCADE;
DROP TABLE IF EXISTS tb_products CASCADE;
DROP TABLE IF EXISTS tb_customers CASCADE;
DROP TABLE IF EXISTS tb_categories CASCADE;

-- Create categories table (Trinity Pattern)
CREATE TABLE tb_categories (
    pk_category SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create products table (Trinity Pattern)
CREATE TABLE tb_products (
    pk_product SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    price DECIMAL(10, 2) NOT NULL,
    inventory INTEGER NOT NULL DEFAULT 0,
    fk_category INTEGER NOT NULL REFERENCES tb_categories(pk_category),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create customers table (Trinity Pattern)
CREATE TABLE tb_customers (
    pk_customer SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    phone VARCHAR(20),
    joined_date TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create orders table (Trinity Pattern)
CREATE TABLE tb_orders (
    pk_order SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_customer INTEGER NOT NULL REFERENCES tb_customers(pk_customer),
    total_price DECIMAL(10, 2) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create order_items table (Trinity Pattern)
CREATE TABLE tb_order_items (
    pk_order_item SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_order INTEGER NOT NULL REFERENCES tb_orders(pk_order),
    fk_product INTEGER NOT NULL REFERENCES tb_products(pk_product),
    quantity INTEGER NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes for performance
CREATE INDEX idx_tb_products_fk_category ON tb_products(fk_category);
CREATE INDEX idx_tb_orders_fk_customer ON tb_orders(fk_customer);
CREATE INDEX idx_tb_orders_status ON tb_orders(status);
CREATE INDEX idx_tb_order_items_fk_order ON tb_order_items(fk_order);
CREATE INDEX idx_tb_order_items_fk_product ON tb_order_items(fk_product);
CREATE INDEX idx_tb_customers_email ON tb_customers(email);
CREATE INDEX idx_tb_categories_id ON tb_categories(id);
CREATE INDEX idx_tb_products_id ON tb_products(id);
CREATE INDEX idx_tb_customers_id ON tb_customers(id);
CREATE INDEX idx_tb_orders_id ON tb_orders(id);
CREATE INDEX idx_tb_order_items_id ON tb_order_items(id);

-- Create views (Trinity Pattern v_* naming)
CREATE VIEW v_categories AS
SELECT pk_category, id, name, description, created_at
FROM tb_categories;

CREATE VIEW v_products AS
SELECT
    pk_product, id, name, description, price, inventory,
    fk_category, c.id AS category_id, c.name AS category_name,
    tb_products.created_at, tb_products.updated_at
FROM tb_products
JOIN tb_categories c ON tb_products.fk_category = c.pk_category;

CREATE VIEW v_customers AS
SELECT pk_customer, id, name, email, phone, joined_date, created_at
FROM tb_customers;

CREATE VIEW v_orders AS
SELECT
    pk_order, id, fk_customer, cu.id AS customer_id, cu.name AS customer_name,
    total_price, status, tb_orders.created_at, tb_orders.updated_at
FROM tb_orders
JOIN tb_customers cu ON tb_orders.fk_customer = cu.pk_customer;

CREATE VIEW v_order_items AS
SELECT
    oi.pk_order_item, oi.id, oi.fk_order, oi.fk_product,
    pr.id AS product_id, pr.name AS product_name, pr.price AS product_price,
    oi.quantity, oi.price, oi.created_at
FROM tb_order_items oi
JOIN tb_products pr ON oi.fk_product = pr.pk_product;

-- Insert sample categories
INSERT INTO tb_categories (name, description) VALUES
    ('Electronics', 'Electronic devices and gadgets'),
    ('Clothing', 'Apparel and fashion items'),
    ('Books', 'Physical and digital books'),
    ('Home & Garden', 'Home improvement and garden supplies'),
    ('Sports', 'Sports equipment and accessories');

-- Insert sample products
INSERT INTO tb_products (name, description, price, inventory, fk_category) VALUES
    ('Laptop Computer', 'High-performance laptop with 16GB RAM', 999.99, 15, 1),
    ('Wireless Mouse', 'Ergonomic wireless mouse', 29.99, 50, 1),
    ('USB-C Cable', 'Fast charging USB-C cable', 14.99, 100, 1),
    ('Cotton T-Shirt', 'Comfortable 100% cotton t-shirt', 19.99, 75, 2),
    ('Denim Jeans', 'Classic blue denim jeans', 49.99, 40, 2),
    ('Running Shoes', 'Professional running shoes', 129.99, 30, 2),
    ('GraphQL Book', 'Learning GraphQL from basics to advanced', 39.99, 25, 3),
    ('Rust Programming', 'Systems programming with Rust', 49.99, 20, 3),
    ('Garden Tools Set', 'Complete garden tool collection', 79.99, 12, 4),
    ('Plant Pots', 'Set of ceramic plant pots', 34.99, 35, 4),
    ('Yoga Mat', 'Non-slip yoga exercise mat', 24.99, 45, 5),
    ('Dumbbells Set', 'Adjustable dumbbell set 5-25lbs', 89.99, 18, 5);

-- Insert sample customers
INSERT INTO tb_customers (name, email, phone, joined_date) VALUES
    ('Alice Johnson', 'alice@example.com', '555-0101', NOW() - INTERVAL '6 months'),
    ('Bob Smith', 'bob@example.com', '555-0102', NOW() - INTERVAL '5 months'),
    ('Charlie Brown', 'charlie@example.com', '555-0103', NOW() - INTERVAL '4 months'),
    ('Diana Prince', 'diana@example.com', '555-0104', NOW() - INTERVAL '3 months'),
    ('Eve Davis', 'eve@example.com', '555-0105', NOW() - INTERVAL '2 months');

-- Insert sample orders
INSERT INTO tb_orders (fk_customer, total_price, status, created_at) VALUES
    (1, 1359.97, 'delivered', NOW() - INTERVAL '3 months'),
    (1, 89.99, 'shipped', NOW() - INTERVAL '1 month'),
    (2, 299.97, 'delivered', NOW() - INTERVAL '2 months'),
    (2, 149.99, 'delivered', NOW() - INTERVAL '1 month'),
    (3, 129.99, 'pending', NOW() - INTERVAL '5 days'),
    (4, 549.98, 'delivered', NOW() - INTERVAL '1 month'),
    (5, 199.98, 'shipped', NOW() - INTERVAL '2 weeks');

-- Insert sample order items
INSERT INTO tb_order_items (fk_order, fk_product, quantity, price) VALUES
    -- Order 1: Alice's laptop bundle
    (1, 1, 1, 999.99),
    (1, 2, 1, 29.99),
    (1, 3, 1, 14.99),
    -- Order 2: Alice's fitness gear
    (2, 11, 1, 24.99),
    (2, 12, 1, 65.00),
    -- Order 3: Bob's books and mouse
    (3, 2, 2, 29.99),
    (3, 7, 1, 39.99),
    (3, 8, 1, 49.99),
    -- Order 4: Bob's clothing
    (4, 4, 1, 19.99),
    (4, 5, 2, 49.99),
    (4, 6, 1, 79.00),
    -- Order 5: Charlie's shoes
    (5, 6, 1, 129.99),
    -- Order 6: Diana's home and garden
    (6, 9, 1, 79.99),
    (6, 10, 3, 34.99),
    (6, 11, 2, 24.99),
    (6, 7, 1, 39.99),
    -- Order 7: Eve's office and fitness
    (7, 1, 1, 999.99),
    (7, 12, 1, 89.99);

-- Verify data
SELECT 'Categories:' AS info;
SELECT COUNT(*) as category_count FROM tb_categories;

SELECT 'Products:' AS info;
SELECT COUNT(*) as product_count FROM tb_products;

SELECT 'Customers:' AS info;
SELECT COUNT(*) as customer_count FROM tb_customers;

SELECT 'Orders:' AS info;
SELECT COUNT(*) as order_count FROM tb_orders;

SELECT 'Order Items:' AS info;
SELECT COUNT(*) as order_item_count FROM tb_order_items;

-- Sample queries to verify schema
SELECT 'Top customers by orders:' AS info;
SELECT c.name, COUNT(o.pk_order) as order_count, SUM(o.total_price) as total_spent
FROM tb_customers c
LEFT JOIN tb_orders o ON c.pk_customer = o.fk_customer
GROUP BY c.pk_customer, c.name
ORDER BY total_spent DESC;

SELECT 'Products by category:' AS info;
SELECT cat.name as category, COUNT(p.pk_product) as product_count, AVG(p.price) as avg_price
FROM tb_categories cat
LEFT JOIN tb_products p ON cat.pk_category = p.fk_category
GROUP BY cat.pk_category, cat.name
ORDER BY product_count DESC;
