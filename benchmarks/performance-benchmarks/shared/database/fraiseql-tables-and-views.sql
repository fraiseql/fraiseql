-- FraiseQL Table Views and Base Tables with Layered Architecture
-- Following the patterns from the documentation with tv_* for table views and tb_* for base tables

SET search_path TO benchmark, public;

-- Drop existing objects
DROP TABLE IF EXISTS tv_users CASCADE;
DROP TABLE IF EXISTS tv_products CASCADE;
DROP TABLE IF EXISTS tv_orders CASCADE;
DROP TABLE IF EXISTS tv_categories CASCADE;
DROP TABLE IF EXISTS tv_popular_products CASCADE;
DROP TABLE IF EXISTS tv_products_by_category CASCADE;
DROP TABLE IF EXISTS tv_user_stats CASCADE;

DROP VIEW IF EXISTS v_users CASCADE;
DROP VIEW IF EXISTS v_products CASCADE;
DROP VIEW IF EXISTS v_orders CASCADE;
DROP VIEW IF EXISTS v_categories CASCADE;

DROP MATERIALIZED VIEW IF EXISTS mv_popular_products CASCADE;
DROP MATERIALIZED VIEW IF EXISTS mv_products_by_category CASCADE;
DROP MATERIALIZED VIEW IF EXISTS mv_user_stats CASCADE;

-- Rename existing tables to tb_* prefix if not already done
DO $$
BEGIN
    -- Check and rename users table
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'users' AND schemaname = 'benchmark')
       AND NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'tb_users' AND schemaname = 'benchmark') THEN
        ALTER TABLE users RENAME TO tb_users;
    END IF;

    -- Check and rename products table
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'products' AND schemaname = 'benchmark')
       AND NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'tb_products' AND schemaname = 'benchmark') THEN
        ALTER TABLE products RENAME TO tb_products;
    END IF;

    -- Check and rename orders table
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'orders' AND schemaname = 'benchmark')
       AND NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'tb_orders' AND schemaname = 'benchmark') THEN
        ALTER TABLE orders RENAME TO tb_orders;
    END IF;

    -- Check and rename categories table
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'categories' AND schemaname = 'benchmark')
       AND NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'tb_categories' AND schemaname = 'benchmark') THEN
        ALTER TABLE categories RENAME TO tb_categories;
    END IF;

    -- Check and rename order_items table
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'order_items' AND schemaname = 'benchmark')
       AND NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'tb_order_items' AND schemaname = 'benchmark') THEN
        ALTER TABLE order_items RENAME TO tb_order_items;
    END IF;

    -- Check and rename product_categories table
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'product_categories' AND schemaname = 'benchmark')
       AND NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'tb_product_categories' AND schemaname = 'benchmark') THEN
        ALTER TABLE product_categories RENAME TO tb_product_categories;
    END IF;

    -- Check and rename product_reviews table
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'product_reviews' AND schemaname = 'benchmark')
       AND NOT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'tb_product_reviews' AND schemaname = 'benchmark') THEN
        ALTER TABLE product_reviews RENAME TO tb_product_reviews;
    END IF;
END $$;

-- Create table views (tv_*) - these are actual tables that store denormalized data
-- They enable precise node updates and better performance than traditional views

-- Table view for users with aggregated data
CREATE TABLE tv_users (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_tv_users_data ON tv_users USING gin (data);
CREATE INDEX idx_tv_users_updated ON tv_users (updated_at);

-- Table view for products with aggregated data
CREATE TABLE tv_products (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_tv_products_data ON tv_products USING gin (data);
CREATE INDEX idx_tv_products_updated ON tv_products (updated_at);

-- Table view for orders with items
CREATE TABLE tv_orders (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_tv_orders_data ON tv_orders USING gin (data);
CREATE INDEX idx_tv_orders_updated ON tv_orders (updated_at);

-- Table view for categories
CREATE TABLE tv_categories (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_tv_categories_data ON tv_categories USING gin (data);

-- Table view for popular products (replaces materialized view)
CREATE TABLE tv_popular_products (
    id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_tv_popular_products_data ON tv_popular_products USING gin (data);
CREATE INDEX idx_tv_popular_products_revenue ON tv_popular_products ((data->>'totalRevenue')::float DESC);

-- Table view for products by category
CREATE TABLE tv_products_by_category (
    id SERIAL PRIMARY KEY,  -- Using serial since this is a synthetic grouping
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_tv_products_by_category_data ON tv_products_by_category USING gin (data);

-- Table view for user statistics
CREATE TABLE tv_user_stats (
    id UUID PRIMARY KEY,  -- User ID
    data JSONB NOT NULL,
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_tv_user_stats_data ON tv_user_stats USING gin (data);
CREATE INDEX idx_tv_user_stats_spent ON tv_user_stats ((data->>'totalSpent')::float DESC);

-- Create simple views that read from table views for FraiseQL
-- These views provide the interface that FraiseQL expects (id + data columns)

CREATE VIEW v_users AS
SELECT id, data FROM tv_users;

CREATE VIEW v_products AS
SELECT id, data FROM tv_products;

CREATE VIEW v_orders AS
SELECT id, data FROM tv_orders;

CREATE VIEW v_categories AS
SELECT id, data FROM tv_categories;

-- For the specialized views, we use different names to match the model types
CREATE VIEW v_popular_products AS
SELECT id, data FROM tv_popular_products;

CREATE VIEW v_products_by_category AS
SELECT id, data FROM tv_products_by_category;

CREATE VIEW v_user_stats AS
SELECT id, data FROM tv_user_stats;
