-- FraiseQL E-Commerce Example - Database Setup
-- PostgreSQL

-- Drop existing objects if present
DROP TABLE IF EXISTS order_items CASCADE;
DROP TABLE IF EXISTS orders CASCADE;
DROP TABLE IF EXISTS products CASCADE;
DROP TABLE IF EXISTS customers CASCADE;
DROP TABLE IF EXISTS categories CASCADE;

-- Create categories table
CREATE TABLE categories (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create products table
CREATE TABLE products (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    price DECIMAL(10, 2) NOT NULL,
    inventory INTEGER NOT NULL DEFAULT 0,
    category_id INTEGER NOT NULL REFERENCES categories(id),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create customers table
CREATE TABLE customers (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    phone VARCHAR(20),
    joined_date TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create orders table
CREATE TABLE orders (
    id SERIAL PRIMARY KEY,
    customer_id INTEGER NOT NULL REFERENCES customers(id),
    total_price DECIMAL(10, 2) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create order_items table
CREATE TABLE order_items (
    id SERIAL PRIMARY KEY,
    order_id INTEGER NOT NULL REFERENCES orders(id),
    product_id INTEGER NOT NULL REFERENCES products(id),
    quantity INTEGER NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes for performance
CREATE INDEX idx_products_category_id ON products(category_id);
CREATE INDEX idx_orders_customer_id ON orders(customer_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_order_items_order_id ON order_items(order_id);
CREATE INDEX idx_order_items_product_id ON order_items(product_id);
CREATE INDEX idx_customers_email ON customers(email);

-- Insert sample categories
INSERT INTO categories (name, description) VALUES
    ('Electronics', 'Electronic devices and gadgets'),
    ('Clothing', 'Apparel and fashion items'),
    ('Books', 'Physical and digital books'),
    ('Home & Garden', 'Home improvement and garden supplies'),
    ('Sports', 'Sports equipment and accessories');

-- Insert sample products
INSERT INTO products (name, description, price, inventory, category_id) VALUES
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
INSERT INTO customers (name, email, phone, joined_date) VALUES
    ('Alice Johnson', 'alice@example.com', '555-0101', NOW() - INTERVAL '6 months'),
    ('Bob Smith', 'bob@example.com', '555-0102', NOW() - INTERVAL '5 months'),
    ('Charlie Brown', 'charlie@example.com', '555-0103', NOW() - INTERVAL '4 months'),
    ('Diana Prince', 'diana@example.com', '555-0104', NOW() - INTERVAL '3 months'),
    ('Eve Davis', 'eve@example.com', '555-0105', NOW() - INTERVAL '2 months');

-- Insert sample orders
INSERT INTO orders (customer_id, total_price, status, created_at) VALUES
    (1, 1359.97, 'delivered', NOW() - INTERVAL '3 months'),
    (1, 89.99, 'shipped', NOW() - INTERVAL '1 month'),
    (2, 299.97, 'delivered', NOW() - INTERVAL '2 months'),
    (2, 149.99, 'delivered', NOW() - INTERVAL '1 month'),
    (3, 129.99, 'pending', NOW() - INTERVAL '5 days'),
    (4, 549.98, 'delivered', NOW() - INTERVAL '1 month'),
    (5, 199.98, 'shipped', NOW() - INTERVAL '2 weeks');

-- Insert sample order items
INSERT INTO order_items (order_id, product_id, quantity, price) VALUES
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
SELECT COUNT(*) as category_count FROM categories;

SELECT 'Products:' AS info;
SELECT COUNT(*) as product_count FROM products;

SELECT 'Customers:' AS info;
SELECT COUNT(*) as customer_count FROM customers;

SELECT 'Orders:' AS info;
SELECT COUNT(*) as order_count FROM orders;

SELECT 'Order Items:' AS info;
SELECT COUNT(*) as order_item_count FROM order_items;

-- Sample queries to verify schema
SELECT 'Top customers by orders:' AS info;
SELECT c.name, COUNT(o.id) as order_count, SUM(o.total_price) as total_spent
FROM customers c
LEFT JOIN orders o ON c.id = o.customer_id
GROUP BY c.id, c.name
ORDER BY total_spent DESC;

SELECT 'Products by category:' AS info;
SELECT cat.name as category, COUNT(p.id) as product_count, AVG(p.price) as avg_price
FROM categories cat
LEFT JOIN products p ON cat.id = p.category_id
GROUP BY cat.id, cat.name
ORDER BY product_count DESC;
