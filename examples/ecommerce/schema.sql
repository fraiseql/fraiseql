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
CREATE TABLE tb_user (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    phone VARCHAR(50),
    is_active BOOLEAN DEFAULT true,
    is_verified BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_created_at ON tb_user(created_at);

-- Addresses table
CREATE TABLE tb_address (
    pk_address INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
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

CREATE INDEX idx_tb_address_fk_user ON tb_address(fk_user);

-- Products table
CREATE TABLE tb_product (
    pk_product INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
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

CREATE INDEX idx_tb_product_sku ON tb_product(sku);
CREATE INDEX idx_tb_product_category ON tb_product(category);
CREATE INDEX idx_tb_product_price ON tb_product(price);
CREATE INDEX idx_tb_product_created_at ON tb_product(created_at);
CREATE INDEX idx_tb_product_tags ON tb_product USING GIN(tags);

-- Carts table
CREATE TABLE tb_cart (
    pk_cart INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_user INT REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    session_id VARCHAR(255),
    items_count INTEGER DEFAULT 0,
    subtotal DECIMAL(10,2) DEFAULT 0.00,
    expires_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP + INTERVAL '7 days',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT cart_owner CHECK (fk_user IS NOT NULL OR session_id IS NOT NULL)
);

CREATE INDEX idx_tb_cart_fk_user ON tb_cart(fk_user);
CREATE INDEX idx_tb_cart_session_id ON tb_cart(session_id);
CREATE INDEX idx_tb_cart_expires_at ON tb_cart(expires_at);

-- Cart items table
CREATE TABLE tb_cart_item (
    pk_cart_item INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_cart INT NOT NULL REFERENCES tb_cart(pk_cart) ON DELETE CASCADE,
    fk_product INT NOT NULL REFERENCES tb_product(pk_product),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    price DECIMAL(10,2) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(fk_cart, fk_product)
);

CREATE INDEX idx_tb_cart_item_fk_cart ON tb_cart_item(fk_cart);

-- Orders table
CREATE TABLE tb_order (
    pk_order INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    order_number VARCHAR(50) UNIQUE NOT NULL,
    fk_user INT NOT NULL REFERENCES tb_user(pk_user),
    status order_status DEFAULT 'pending',
    payment_status payment_status DEFAULT 'pending',
    fk_shipping_address INT NOT NULL REFERENCES tb_address(pk_address),
    fk_billing_address INT NOT NULL REFERENCES tb_address(pk_address),
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

CREATE INDEX idx_tb_order_fk_user ON tb_order(fk_user);
CREATE INDEX idx_tb_order_order_number ON tb_order(order_number);
CREATE INDEX idx_tb_order_status ON tb_order(status);
CREATE INDEX idx_tb_order_placed_at ON tb_order(placed_at);

-- Order items table
CREATE TABLE tb_order_item (
    pk_order_item INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_order INT NOT NULL REFERENCES tb_order(pk_order) ON DELETE CASCADE,
    fk_product INT NOT NULL REFERENCES tb_product(pk_product),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    price DECIMAL(10,2) NOT NULL,
    total DECIMAL(10,2) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tb_order_item_fk_order ON tb_order_item(fk_order);

-- Reviews table
CREATE TABLE tb_review (
    pk_review INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_product INT NOT NULL REFERENCES tb_product(pk_product) ON DELETE CASCADE,
    fk_user INT NOT NULL REFERENCES tb_user(pk_user),
    order_id UUID REFERENCES tb_order(id),
    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
    title VARCHAR(255) NOT NULL,
    comment TEXT NOT NULL,
    is_verified BOOLEAN DEFAULT false,
    helpful_count INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(fk_product, fk_user, order_id)
);

CREATE INDEX idx_tb_review_fk_product ON tb_review(fk_product);
CREATE INDEX idx_tb_review_fk_user ON tb_review(fk_user);
CREATE INDEX idx_tb_review_rating ON tb_review(rating);
CREATE INDEX idx_tb_review_created_at ON tb_review(created_at);

-- Coupons table
CREATE TABLE tb_coupon (
    pk_coupon INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
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

CREATE INDEX idx_tb_coupon_code ON tb_coupon(code);

-- Wishlist table
CREATE TABLE tb_wishlist_item (
    pk_wishlist_item INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_user INT NOT NULL REFERENCES tb_user(pk_user) ON DELETE CASCADE,
    fk_product INT NOT NULL REFERENCES tb_product(pk_product) ON DELETE CASCADE,
    added_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(fk_user, fk_product)
);

CREATE INDEX idx_tb_wishlist_item_fk_user ON tb_wishlist_item(fk_user);

-- Create views for FraiseQL

-- Users view
CREATE OR REPLACE VIEW v_user AS
SELECT
    pk_user,
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
FROM tb_user;

-- Addresses view
CREATE OR REPLACE VIEW v_address AS
SELECT
    pk_address,
    id,                      -- For filtering
    fk_user,                 -- For user's addresses
    is_default,              -- For finding default address
    jsonb_build_object(
        'id', id,
        'fk_user', fk_user,
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
FROM tb_address;

-- Products view
CREATE OR REPLACE VIEW v_product AS
SELECT
    pk_product,
    id,                      -- For filtering
    sku,                     -- For SKU lookups
    category,                -- For category filtering
    price,                   -- For price range queries
    inventory_count,         -- For availability checks
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
FROM tb_product;

-- Carts view
CREATE OR REPLACE VIEW v_cart AS
SELECT
    c.pk_cart,
    c.id,
    jsonb_build_object(
        'id', c.id,
        'user', u.data,
        'session_id', c.session_id,
        'items', COALESCE(
            jsonb_agg(ci.data) FILTER (WHERE ci.id IS NOT NULL),
            '[]'::jsonb
        ),
        'items_count', c.items_count,
        'subtotal', c.subtotal,
        'expires_at', c.expires_at,
        'created_at', c.created_at,
        'updated_at', c.updated_at
    ) as data
FROM tb_cart c
LEFT JOIN v_user u ON c.fk_user = u.pk_user
LEFT JOIN v_cart_item ci ON c.pk_cart = ci.fk_cart
GROUP BY c.pk_cart, c.id, c.fk_user, c.session_id, c.items_count, c.subtotal, c.expires_at, c.created_at, c.updated_at, u.data;

-- Cart items view
CREATE OR REPLACE VIEW v_cart_item AS
SELECT
    ci.pk_cart_item,
    ci.id,
    jsonb_build_object(
        'id', ci.id,
        'fk_cart', ci.fk_cart,
        'product', p.data,
        'quantity', ci.quantity,
        'price', ci.price,
        'created_at', ci.created_at,
        'updated_at', ci.updated_at
    ) as data
FROM tb_cart_item ci
LEFT JOIN v_product p ON ci.fk_product = p.pk_product;

-- Orders view
CREATE OR REPLACE VIEW v_order AS
SELECT
    o.pk_order,
    o.id,
    jsonb_build_object(
        'id', o.id,
        'order_number', o.order_number,
        'user', u.data,
        'status', o.status,
        'payment_status', o.payment_status,
        'shipping_address', sa.data,
        'billing_address', ba.data,
        'subtotal', o.subtotal,
        'tax_amount', o.tax_amount,
        'shipping_amount', o.shipping_amount,
        'discount_amount', o.discount_amount,
        'total', o.total,
        'tracking_number', o.tracking_number,
        'notes', o.notes,
        'placed_at', o.placed_at,
        'shipped_at', o.shipped_at,
        'delivered_at', o.delivered_at,
        'cancelled_at', o.cancelled_at
    ) as data
FROM tb_order o
LEFT JOIN v_user u ON o.fk_user = u.pk_user
LEFT JOIN v_address sa ON o.fk_shipping_address = sa.pk_address
LEFT JOIN v_address ba ON o.fk_billing_address = ba.pk_address;

-- Order items view
CREATE OR REPLACE VIEW v_order_item AS
SELECT
    oi.pk_order_item,
    oi.id,
    jsonb_build_object(
        'id', oi.id,
        'fk_order', oi.fk_order,
        'product', p.data,
        'quantity', oi.quantity,
        'price', oi.price,
        'total', oi.total,
        'created_at', oi.created_at
    ) as data
FROM tb_order_item oi
LEFT JOIN v_product p ON oi.fk_product = p.pk_product;

-- Reviews view
CREATE OR REPLACE VIEW v_review AS
SELECT
    r.pk_review,
    r.id,
    jsonb_build_object(
        'id', r.id,
        'product', p.data,
        'user', u.data,
        'order_id', r.order_id,
        'rating', r.rating,
        'title', r.title,
        'comment', r.comment,
        'is_verified', r.is_verified,
        'helpful_count', r.helpful_count,
        'created_at', r.created_at,
        'updated_at', r.updated_at
    ) as data
FROM tb_review r
LEFT JOIN v_product p ON r.fk_product = p.pk_product
LEFT JOIN v_user u ON r.fk_user = u.pk_user;

-- Coupons view
CREATE OR REPLACE VIEW v_coupon AS
SELECT
    pk_coupon,
    id,
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
FROM tb_coupon;

-- Wishlist items view
CREATE OR REPLACE VIEW v_wishlist_item AS
SELECT
    wi.pk_wishlist_item,
    wi.id,
    jsonb_build_object(
        'id', wi.id,
        'user', u.data,
        'product', p.data,
        'added_at', wi.added_at
    ) as data
FROM tb_wishlist_item wi
LEFT JOIN v_user u ON wi.fk_user = u.pk_user
LEFT JOIN v_product p ON wi.fk_product = p.pk_product;

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
CREATE TRIGGER update_tb_user_updated_at BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_tb_address_updated_at BEFORE UPDATE ON tb_address
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_tb_product_updated_at BEFORE UPDATE ON tb_product
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_tb_cart_updated_at BEFORE UPDATE ON tb_cart
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_tb_cart_item_updated_at BEFORE UPDATE ON tb_cart_item
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER update_tb_review_updated_at BEFORE UPDATE ON tb_review
    FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- Grant permissions
GRANT SELECT ON ALL TABLES IN SCHEMA ecommerce TO fraiseql_reader;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA ecommerce TO fraiseql_writer;
GRANT USAGE ON SCHEMA ecommerce TO fraiseql_reader, fraiseql_writer;
