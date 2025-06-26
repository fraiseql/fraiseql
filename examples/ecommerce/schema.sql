-- E-commerce Database Schema for FraiseQL Example
-- PostgreSQL 14+

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Create schema
CREATE SCHEMA IF NOT EXISTS ecommerce;
SET search_path TO ecommerce, public;

-- Enums
CREATE TYPE order_status AS ENUM (
    'pending', 'processing', 'shipped', 'delivered', 'cancelled', 'refunded'
);

CREATE TYPE payment_status AS ENUM (
    'pending', 'authorized', 'captured', 'failed', 'refunded'
);

CREATE TYPE product_category AS ENUM (
    'electronics', 'clothing', 'books', 'home', 'sports', 'toys', 'food', 'other'
);

-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    phone VARCHAR(50),
    is_active BOOLEAN DEFAULT true,
    is_verified BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_created_at ON users(created_at);

-- Addresses table
CREATE TABLE addresses (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label VARCHAR(100) NOT NULL,
    street1 VARCHAR(255) NOT NULL,
    street2 VARCHAR(255),
    city VARCHAR(100) NOT NULL,
    state VARCHAR(100) NOT NULL,
    postal_code VARCHAR(20) NOT NULL,
    country VARCHAR(2) DEFAULT 'US',
    is_default BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_addresses_user_id ON addresses(user_id);

-- Products table
CREATE TABLE products (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    sku VARCHAR(100) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT NOT NULL,
    category product_category NOT NULL,
    price DECIMAL(10,2) NOT NULL CHECK (price >= 0),
    compare_at_price DECIMAL(10,2) CHECK (compare_at_price >= price),
    cost DECIMAL(10,2) CHECK (cost >= 0),
    inventory_count INTEGER DEFAULT 0 CHECK (inventory_count >= 0),
    is_active BOOLEAN DEFAULT true,
    weight_grams INTEGER,
    images JSONB DEFAULT '[]'::jsonb,
    tags JSONB DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_products_sku ON products(sku);
CREATE INDEX idx_products_category ON products(category);
CREATE INDEX idx_products_price ON products(price);
CREATE INDEX idx_products_created_at ON products(created_at);
CREATE INDEX idx_products_tags ON products USING GIN(tags);

-- Carts table
CREATE TABLE carts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    session_id VARCHAR(255),
    items_count INTEGER DEFAULT 0,
    subtotal DECIMAL(10,2) DEFAULT 0.00,
    expires_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP + INTERVAL '7 days',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT cart_owner CHECK (user_id IS NOT NULL OR session_id IS NOT NULL)
);

CREATE INDEX idx_carts_user_id ON carts(user_id);
CREATE INDEX idx_carts_session_id ON carts(session_id);
CREATE INDEX idx_carts_expires_at ON carts(expires_at);

-- Cart items table
CREATE TABLE cart_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    cart_id UUID NOT NULL REFERENCES carts(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    price DECIMAL(10,2) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(cart_id, product_id)
);

CREATE INDEX idx_cart_items_cart_id ON cart_items(cart_id);

-- Orders table
CREATE TABLE orders (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_number VARCHAR(50) UNIQUE NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id),
    status order_status DEFAULT 'pending',
    payment_status payment_status DEFAULT 'pending',
    shipping_address_id UUID NOT NULL REFERENCES addresses(id),
    billing_address_id UUID NOT NULL REFERENCES addresses(id),
    subtotal DECIMAL(10,2) NOT NULL,
    tax_amount DECIMAL(10,2) NOT NULL DEFAULT 0.00,
    shipping_amount DECIMAL(10,2) NOT NULL DEFAULT 0.00,
    discount_amount DECIMAL(10,2) NOT NULL DEFAULT 0.00,
    total DECIMAL(10,2) NOT NULL,
    tracking_number VARCHAR(255),
    notes TEXT,
    placed_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    shipped_at TIMESTAMP WITH TIME ZONE,
    delivered_at TIMESTAMP WITH TIME ZONE,
    cancelled_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_orders_user_id ON orders(user_id);
CREATE INDEX idx_orders_order_number ON orders(order_number);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_placed_at ON orders(placed_at);

-- Order items table
CREATE TABLE order_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    price DECIMAL(10,2) NOT NULL,
    total DECIMAL(10,2) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_order_items_order_id ON order_items(order_id);

-- Reviews table
CREATE TABLE reviews (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id),
    order_id UUID REFERENCES orders(id),
    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
    title VARCHAR(255) NOT NULL,
    comment TEXT NOT NULL,
    is_verified BOOLEAN DEFAULT false,
    helpful_count INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(product_id, user_id, order_id)
);

CREATE INDEX idx_reviews_product_id ON reviews(product_id);
CREATE INDEX idx_reviews_user_id ON reviews(user_id);
CREATE INDEX idx_reviews_rating ON reviews(rating);
CREATE INDEX idx_reviews_created_at ON reviews(created_at);

-- Coupons table
CREATE TABLE coupons (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    code VARCHAR(50) UNIQUE NOT NULL,
    description TEXT NOT NULL,
    discount_type VARCHAR(20) NOT NULL CHECK (discount_type IN ('percentage', 'fixed')),
    discount_value DECIMAL(10,2) NOT NULL,
    minimum_amount DECIMAL(10,2),
    usage_limit INTEGER,
    usage_count INTEGER DEFAULT 0,
    is_active BOOLEAN DEFAULT true,
    valid_from TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    valid_until TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_coupons_code ON coupons(code);

-- Wishlist table
CREATE TABLE wishlist_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    added_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, product_id)
);

CREATE INDEX idx_wishlist_user_id ON wishlist_items(user_id);

-- Create views for FraiseQL

-- Users view
CREATE OR REPLACE VIEW v_users AS
SELECT
    id,                      -- For filtering
    email,                   -- For unique lookups
    is_active,               -- For filtering active users
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'phone', phone,
        'is_active', is_active,
        'is_verified', is_verified,
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM users;

-- Addresses view
CREATE OR REPLACE VIEW v_addresses AS
SELECT
    id,                      -- For filtering
    user_id,                 -- For user's addresses
    is_default,              -- For finding default address
    jsonb_build_object(
        'id', id,
        'user_id', user_id,
        'label', label,
        'street1', street1,
        'street2', street2,
        'city', city,
        'state', state,
        'postal_code', postal_code,
        'country', country,
        'is_default', is_default,
        'created_at', created_at
    ) as data
FROM addresses;

-- Products view
CREATE OR REPLACE VIEW v_products AS
SELECT
    id,                      -- For filtering
    sku,                     -- For SKU lookups
    category,                -- For category filtering
    price,                   -- For price range queries
    stock_quantity,          -- For availability checks
    is_active,               -- For active products only
    jsonb_build_object(
        'id', id,
        'sku', sku,
        'name', name,
        'description', description,
        'category', category,
        'price', price,
        'compare_at_price', compare_at_price,
        'cost', cost,
        'inventory_count', inventory_count,
        'is_active', is_active,
        'weight_grams', weight_grams,
        'images', images,
        'tags', tags,
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM products;

-- Carts view
CREATE OR REPLACE VIEW v_carts AS
SELECT
    jsonb_build_object(
        'id', id,
        'user_id', user_id,
        'session_id', session_id,
        'items_count', items_count,
        'subtotal', subtotal,
        'expires_at', expires_at,
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM carts;

-- Cart items view
CREATE OR REPLACE VIEW v_cart_items AS
SELECT
    jsonb_build_object(
        'id', id,
        'cart_id', cart_id,
        'product_id', product_id,
        'quantity', quantity,
        'price', price,
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM cart_items;

-- Orders view
CREATE OR REPLACE VIEW v_orders AS
SELECT
    jsonb_build_object(
        'id', id,
        'order_number', order_number,
        'user_id', user_id,
        'status', status,
        'payment_status', payment_status,
        'shipping_address_id', shipping_address_id,
        'billing_address_id', billing_address_id,
        'subtotal', subtotal,
        'tax_amount', tax_amount,
        'shipping_amount', shipping_amount,
        'discount_amount', discount_amount,
        'total', total,
        'tracking_number', tracking_number,
        'notes', notes,
        'placed_at', placed_at,
        'shipped_at', shipped_at,
        'delivered_at', delivered_at,
        'cancelled_at', cancelled_at
    ) as data
FROM orders;

-- Order items view
CREATE OR REPLACE VIEW v_order_items AS
SELECT
    jsonb_build_object(
        'id', id,
        'order_id', order_id,
        'product_id', product_id,
        'quantity', quantity,
        'price', price,
        'total', total,
        'created_at', created_at
    ) as data
FROM order_items;

-- Reviews view
CREATE OR REPLACE VIEW v_reviews AS
SELECT
    jsonb_build_object(
        'id', id,
        'product_id', product_id,
        'user_id', user_id,
        'order_id', order_id,
        'rating', rating,
        'title', title,
        'comment', comment,
        'is_verified', is_verified,
        'helpful_count', helpful_count,
        'created_at', created_at,
        'updated_at', updated_at
    ) as data
FROM reviews;

-- Coupons view
CREATE OR REPLACE VIEW v_coupons AS
SELECT
    jsonb_build_object(
        'id', id,
        'code', code,
        'description', description,
        'discount_type', discount_type,
        'discount_value', discount_value,
        'minimum_amount', minimum_amount,
        'usage_limit', usage_limit,
        'usage_count', usage_count,
        'is_active', is_active,
        'valid_from', valid_from,
        'valid_until', valid_until,
        'created_at', created_at
    ) as data
FROM coupons;

-- Wishlist items view
CREATE OR REPLACE VIEW v_wishlist_items AS
SELECT
    jsonb_build_object(
        'id', id,
        'user_id', user_id,
        'product_id', product_id,
        'added_at', added_at
    ) as data
FROM wishlist_items;

-- Helper functions

-- Update timestamps trigger
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply trigger to tables with updated_at
CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_addresses_updated_at BEFORE UPDATE ON addresses
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_products_updated_at BEFORE UPDATE ON products
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_carts_updated_at BEFORE UPDATE ON carts
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_cart_items_updated_at BEFORE UPDATE ON cart_items
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_reviews_updated_at BEFORE UPDATE ON reviews
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- Grant permissions
GRANT SELECT ON ALL TABLES IN SCHEMA ecommerce TO fraiseql_reader;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA ecommerce TO fraiseql_writer;
GRANT USAGE ON SCHEMA ecommerce TO fraiseql_reader, fraiseql_writer;
