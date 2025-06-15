-- E-commerce benchmark schema for GraphQL performance testing
-- Designed to test various query patterns and complexities

-- Create required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Drop existing schema if exists (but preserve extensions in public schema)
DROP SCHEMA IF EXISTS benchmark CASCADE;
CREATE SCHEMA benchmark;
SET search_path TO benchmark, public;

-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    username VARCHAR(100) UNIQUE NOT NULL,
    full_name VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT true,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Categories table
CREATE TABLE categories (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) UNIQUE NOT NULL,
    description TEXT,
    parent_id UUID REFERENCES categories(id),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Products table
CREATE TABLE products (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    sku VARCHAR(100) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    price DECIMAL(10, 2) NOT NULL CHECK (price >= 0),
    stock_quantity INTEGER NOT NULL DEFAULT 0 CHECK (stock_quantity >= 0),
    category_id UUID REFERENCES categories(id),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT true,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Orders table
CREATE TABLE orders (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_number VARCHAR(50) UNIQUE NOT NULL,
    user_id UUID REFERENCES users(id) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    total_amount DECIMAL(10, 2) NOT NULL CHECK (total_amount >= 0),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    shipped_at TIMESTAMP WITH TIME ZONE,
    delivered_at TIMESTAMP WITH TIME ZONE,
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Order items table
CREATE TABLE order_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID REFERENCES orders(id) NOT NULL,
    product_id UUID REFERENCES products(id) NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price DECIMAL(10, 2) NOT NULL CHECK (unit_price >= 0),
    total_price DECIMAL(10, 2) NOT NULL CHECK (total_price >= 0),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Reviews table
CREATE TABLE reviews (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID REFERENCES products(id) NOT NULL,
    user_id UUID REFERENCES users(id) NOT NULL,
    rating INTEGER NOT NULL CHECK (rating >= 1 AND rating <= 5),
    title VARCHAR(255),
    comment TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_verified_purchase BOOLEAN DEFAULT false,
    helpful_count INTEGER DEFAULT 0
);

-- Shopping cart table
CREATE TABLE cart_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users(id) NOT NULL,
    product_id UUID REFERENCES products(id) NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    added_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, product_id)
);

-- Addresses table
CREATE TABLE addresses (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users(id) NOT NULL,
    type VARCHAR(50) NOT NULL DEFAULT 'shipping',
    street_address VARCHAR(255) NOT NULL,
    city VARCHAR(100) NOT NULL,
    state_province VARCHAR(100),
    postal_code VARCHAR(20),
    country VARCHAR(2) NOT NULL,
    is_default BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for better query performance
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_created_at ON users(created_at);

CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_products_price ON products(price);
CREATE INDEX idx_products_sku ON products(sku);
CREATE INDEX idx_products_name ON products(name);
CREATE INDEX idx_products_created_at ON products(created_at);

CREATE INDEX idx_orders_user ON orders(user_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created_at ON orders(created_at);
CREATE INDEX idx_orders_number ON orders(order_number);

CREATE INDEX idx_order_items_order ON order_items(order_id);
CREATE INDEX idx_order_items_product ON order_items(product_id);

CREATE INDEX idx_reviews_product ON reviews(product_id);
CREATE INDEX idx_reviews_user ON reviews(user_id);
CREATE INDEX idx_reviews_rating ON reviews(rating);
CREATE INDEX idx_reviews_created_at ON reviews(created_at);

CREATE INDEX idx_cart_items_user ON cart_items(user_id);
CREATE INDEX idx_addresses_user ON addresses(user_id);

-- Create JSONB indexes for metadata fields
CREATE INDEX idx_users_metadata ON users USING gin(metadata);
CREATE INDEX idx_products_metadata ON products USING gin(metadata);
CREATE INDEX idx_orders_metadata ON orders USING gin(metadata);

-- Create views for FraiseQL (JSONB-based)
CREATE VIEW user_view AS
SELECT
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'username', u.username,
        'fullName', u.full_name,
        'createdAt', u.created_at,
        'isActive', u.is_active,
        'orderCount', (SELECT COUNT(*) FROM orders WHERE user_id = u.id),
        'totalSpent', COALESCE((SELECT SUM(total_amount) FROM orders WHERE user_id = u.id), 0),
        'reviewCount', (SELECT COUNT(*) FROM reviews WHERE user_id = u.id),
        'averageRating', (SELECT AVG(rating) FROM reviews WHERE user_id = u.id)
    ) as data
FROM users u;

CREATE VIEW product_view AS
SELECT
    jsonb_build_object(
        'id', p.id,
        'sku', p.sku,
        'name', p.name,
        'description', p.description,
        'price', p.price,
        'stockQuantity', p.stock_quantity,
        'categoryId', p.category_id,
        'category', (
            SELECT jsonb_build_object(
                'id', c.id,
                'name', c.name,
                'slug', c.slug
            )
            FROM categories c
            WHERE c.id = p.category_id
        ),
        'averageRating', (SELECT AVG(rating) FROM reviews WHERE product_id = p.id),
        'reviewCount', (SELECT COUNT(*) FROM reviews WHERE product_id = p.id),
        'reviews', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', r.id,
                    'rating', r.rating,
                    'title', r.title,
                    'comment', r.comment,
                    'createdAt', r.created_at,
                    'user', (
                        SELECT jsonb_build_object(
                            'id', u.id,
                            'username', u.username,
                            'fullName', u.full_name
                        )
                        FROM users u
                        WHERE u.id = r.user_id
                    )
                )
                ORDER BY r.created_at DESC
            )
            FROM reviews r
            WHERE r.product_id = p.id
        )
    ) as data
FROM products p;

CREATE VIEW order_view AS
SELECT
    jsonb_build_object(
        'id', o.id,
        'orderNumber', o.order_number,
        'userId', o.user_id,
        'status', o.status,
        'totalAmount', o.total_amount,
        'createdAt', o.created_at,
        'user', (
            SELECT jsonb_build_object(
                'id', u.id,
                'email', u.email,
                'username', u.username,
                'fullName', u.full_name
            )
            FROM users u
            WHERE u.id = o.user_id
        ),
        'items', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', oi.id,
                    'quantity', oi.quantity,
                    'unitPrice', oi.unit_price,
                    'totalPrice', oi.total_price,
                    'product', (
                        SELECT jsonb_build_object(
                            'id', p.id,
                            'sku', p.sku,
                            'name', p.name,
                            'price', p.price
                        )
                        FROM products p
                        WHERE p.id = oi.product_id
                    )
                )
                ORDER BY oi.created_at
            )
            FROM order_items oi
            WHERE oi.order_id = o.id
        ),
        'itemCount', (SELECT COUNT(*) FROM order_items WHERE order_id = o.id)
    ) as data
FROM orders o;

-- Create update trigger for updated_at columns
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_products_updated_at BEFORE UPDATE ON products
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_orders_updated_at BEFORE UPDATE ON orders
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_reviews_updated_at BEFORE UPDATE ON reviews
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Grant permissions
GRANT ALL ON SCHEMA benchmark TO benchmark;
GRANT ALL ON ALL TABLES IN SCHEMA benchmark TO benchmark;
GRANT ALL ON ALL SEQUENCES IN SCHEMA benchmark TO benchmark;
