-- CQRS Enterprise Pattern - Base Tables
-- Order Management System

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ============================================================================
-- CUSTOMERS TABLE
-- ============================================================================

CREATE TABLE tb_customers (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    phone VARCHAR(50),
    address TEXT,
    city VARCHAR(100),
    country VARCHAR(100),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    version INT NOT NULL DEFAULT 1  -- Optimistic locking
);

CREATE INDEX idx_customers_email ON tb_customers(email);
CREATE INDEX idx_customers_country ON tb_customers(country);

-- ============================================================================
-- PRODUCTS TABLE
-- ============================================================================

CREATE TABLE tb_products (
    id SERIAL PRIMARY KEY,
    sku VARCHAR(100) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    price DECIMAL(10, 2) NOT NULL CHECK (price >= 0),
    cost DECIMAL(10, 2) NOT NULL CHECK (cost >= 0),
    quantity_available INT NOT NULL DEFAULT 0 CHECK (quantity_available >= 0),
    quantity_reserved INT NOT NULL DEFAULT 0 CHECK (quantity_reserved >= 0),
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    version INT NOT NULL DEFAULT 1  -- Optimistic locking
);

CREATE INDEX idx_products_sku ON tb_products(sku);
CREATE INDEX idx_products_active ON tb_products(is_active) WHERE is_active = true;
CREATE INDEX idx_products_price ON tb_products(price);

-- ============================================================================
-- ORDERS TABLE
-- ============================================================================

CREATE TABLE tb_orders (
    id SERIAL PRIMARY KEY,
    order_number VARCHAR(50) UNIQUE NOT NULL,
    customer_id INT NOT NULL REFERENCES tb_customers(id),
    status VARCHAR(50) NOT NULL DEFAULT 'pending' CHECK (
        status IN ('pending', 'paid', 'processing', 'shipped', 'delivered', 'cancelled')
    ),
    subtotal DECIMAL(10, 2) NOT NULL DEFAULT 0 CHECK (subtotal >= 0),
    tax DECIMAL(10, 2) NOT NULL DEFAULT 0 CHECK (tax >= 0),
    shipping DECIMAL(10, 2) NOT NULL DEFAULT 0 CHECK (shipping >= 0),
    total DECIMAL(10, 2) NOT NULL DEFAULT 0 CHECK (total >= 0),
    notes TEXT,
    paid_at TIMESTAMP,
    shipped_at TIMESTAMP,
    delivered_at TIMESTAMP,
    cancelled_at TIMESTAMP,
    cancellation_reason TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    version INT NOT NULL DEFAULT 1  -- Optimistic locking
);

CREATE INDEX idx_orders_customer ON tb_orders(customer_id);
CREATE INDEX idx_orders_status ON tb_orders(status);
CREATE INDEX idx_orders_order_number ON tb_orders(order_number);
CREATE INDEX idx_orders_created ON tb_orders(created_at DESC);

-- Composite indexes for common queries
CREATE INDEX idx_orders_customer_status ON tb_orders(customer_id, status);
CREATE INDEX idx_orders_status_created ON tb_orders(status, created_at DESC);

-- ============================================================================
-- ORDER ITEMS TABLE
-- ============================================================================

CREATE TABLE tb_order_items (
    id SERIAL PRIMARY KEY,
    order_id INT NOT NULL REFERENCES tb_orders(id) ON DELETE CASCADE,
    product_id INT NOT NULL REFERENCES tb_products(id),
    quantity INT NOT NULL CHECK (quantity > 0),
    unit_price DECIMAL(10, 2) NOT NULL CHECK (unit_price >= 0),
    subtotal DECIMAL(10, 2) NOT NULL CHECK (subtotal >= 0),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),

    UNIQUE(order_id, product_id)  -- Can't add same product twice to an order
);

CREATE INDEX idx_order_items_order ON tb_order_items(order_id);
CREATE INDEX idx_order_items_product ON tb_order_items(product_id);

-- ============================================================================
-- PAYMENTS TABLE
-- ============================================================================

CREATE TABLE tb_payments (
    id SERIAL PRIMARY KEY,
    order_id INT NOT NULL REFERENCES tb_orders(id) ON DELETE CASCADE,
    amount DECIMAL(10, 2) NOT NULL CHECK (amount > 0),
    payment_method VARCHAR(50) NOT NULL CHECK (
        payment_method IN ('credit_card', 'debit_card', 'paypal', 'bank_transfer', 'cash')
    ),
    transaction_id VARCHAR(255) UNIQUE,
    status VARCHAR(50) NOT NULL DEFAULT 'pending' CHECK (
        status IN ('pending', 'completed', 'failed', 'refunded')
    ),
    processed_at TIMESTAMP,
    refunded_at TIMESTAMP,
    refund_amount DECIMAL(10, 2),
    notes TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_payments_order ON tb_payments(order_id);
CREATE INDEX idx_payments_status ON tb_payments(status);
CREATE INDEX idx_payments_transaction ON tb_payments(transaction_id);

-- ============================================================================
-- AUDIT LOG TABLE
-- ============================================================================

CREATE TABLE tb_audit_log (
    id BIGSERIAL PRIMARY KEY,
    operation VARCHAR(20) NOT NULL CHECK (operation IN ('INSERT', 'UPDATE', 'DELETE')),
    entity_type VARCHAR(50) NOT NULL,
    entity_id INT NOT NULL,
    changed_by VARCHAR(255),  -- User ID or system identifier
    old_values JSONB,
    new_values JSONB,
    changes JSONB,
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_log_entity ON tb_audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_log_created ON tb_audit_log(created_at DESC);
CREATE INDEX idx_audit_log_changed_by ON tb_audit_log(changed_by);

-- ============================================================================
-- TRIGGERS FOR AUTOMATIC TIMESTAMP UPDATES
-- ============================================================================

CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    NEW.version = OLD.version + 1;  -- Increment version for optimistic locking
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_customers_timestamp
    BEFORE UPDATE ON tb_customers
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_products_timestamp
    BEFORE UPDATE ON tb_products
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_orders_timestamp
    BEFORE UPDATE ON tb_orders
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- SAMPLE DATA
-- ============================================================================

-- Customers
INSERT INTO tb_customers (email, name, phone, address, city, country) VALUES
('alice@example.com', 'Alice Johnson', '+1-555-0101', '123 Main St', 'New York', 'USA'),
('bob@example.com', 'Bob Smith', '+1-555-0102', '456 Oak Ave', 'Los Angeles', 'USA'),
('carol@example.com', 'Carol Williams', '+44-20-1234-5678', '789 King St', 'London', 'UK'),
('david@example.com', 'David Brown', '+33-1-23-45-67-89', '321 Rue de Paris', 'Paris', 'France');

-- Products
INSERT INTO tb_products (sku, name, description, price, cost, quantity_available, quantity_reserved) VALUES
('LAPTOP-001', 'Professional Laptop', 'High-performance laptop for developers', 1299.99, 800.00, 50, 5),
('MOUSE-001', 'Wireless Mouse', 'Ergonomic wireless mouse', 49.99, 20.00, 200, 10),
('KEYBOARD-001', 'Mechanical Keyboard', 'RGB mechanical keyboard', 149.99, 80.00, 100, 8),
('MONITOR-001', '4K Monitor', '27-inch 4K display', 599.99, 350.00, 30, 3),
('HEADSET-001', 'Noise-Canceling Headset', 'Premium headset for calls', 199.99, 100.00, 75, 5),
('DOCK-001', 'USB-C Dock', '12-in-1 docking station', 249.99, 150.00, 40, 2),
('WEBCAM-001', '1080p Webcam', 'HD webcam with auto-focus', 89.99, 45.00, 150, 12),
('CABLE-001', 'USB-C Cable 2m', 'Premium braided cable', 19.99, 5.00, 500, 20);

-- Orders (Sample 1: Completed order)
INSERT INTO tb_orders (order_number, customer_id, status, subtotal, tax, shipping, total, paid_at, shipped_at, delivered_at, created_at)
VALUES (
    'ORD-2024-00001',
    1,
    'delivered',
    1549.97,
    124.00,
    15.00,
    1688.97,
    NOW() - INTERVAL '10 days',
    NOW() - INTERVAL '8 days',
    NOW() - INTERVAL '5 days',
    NOW() - INTERVAL '12 days'
);

INSERT INTO tb_order_items (order_id, product_id, quantity, unit_price, subtotal)
SELECT 1, id, qty, price, qty * price FROM (VALUES
    (1, 1, 1299.99),  -- 1 laptop
    (2, 5, 49.99),    -- 5 mice
    (5, 1, 199.99)    -- 1 headset
) AS items(product_id, qty, price);

INSERT INTO tb_payments (order_id, amount, payment_method, transaction_id, status, processed_at)
VALUES (1, 1688.97, 'credit_card', 'TXN-2024-001', 'completed', NOW() - INTERVAL '10 days');

-- Orders (Sample 2: Pending order)
INSERT INTO tb_orders (order_number, customer_id, status, subtotal, tax, shipping, total, created_at)
VALUES (
    'ORD-2024-00002',
    2,
    'pending',
    749.97,
    60.00,
    10.00,
    819.97,
    NOW() - INTERVAL '2 days'
);

INSERT INTO tb_order_items (order_id, product_id, quantity, unit_price, subtotal)
SELECT 2, id, qty, price, qty * price FROM (VALUES
    (4, 1, 599.99),   -- 1 monitor
    (3, 1, 149.99)    -- 1 keyboard
) AS items(product_id, qty, price);

-- Orders (Sample 3: Cancelled order)
INSERT INTO tb_orders (order_number, customer_id, status, subtotal, tax, shipping, total, cancelled_at, cancellation_reason, created_at)
VALUES (
    'ORD-2024-00003',
    3,
    'cancelled',
    299.97,
    24.00,
    8.00,
    331.97,
    NOW() - INTERVAL '3 days',
    'Customer requested cancellation',
    NOW() - INTERVAL '5 days'
);

INSERT INTO tb_order_items (order_id, product_id, quantity, unit_price, subtotal)
SELECT 3, id, qty, price, qty * price FROM (VALUES
    (6, 1, 249.99),   -- 1 dock
    (2, 1, 49.99)     -- 1 mouse
) AS items(product_id, qty, price);

-- Sample audit log entries
INSERT INTO tb_audit_log (operation, entity_type, entity_id, changed_by, changes, created_at) VALUES
('INSERT', 'order', 1, 'user:alice@example.com', '{"status": "pending", "total": 1688.97}'::jsonb, NOW() - INTERVAL '12 days'),
('UPDATE', 'order', 1, 'system:payment_processor', '{"status": "paid"}'::jsonb, NOW() - INTERVAL '10 days'),
('UPDATE', 'order', 1, 'system:shipping', '{"status": "shipped"}'::jsonb, NOW() - INTERVAL '8 days'),
('UPDATE', 'order', 1, 'system:shipping', '{"status": "delivered"}'::jsonb, NOW() - INTERVAL '5 days'),
('INSERT', 'order', 2, 'user:bob@example.com', '{"status": "pending", "total": 819.97}'::jsonb, NOW() - INTERVAL '2 days'),
('INSERT', 'order', 3, 'user:carol@example.com', '{"status": "pending", "total": 331.97}'::jsonb, NOW() - INTERVAL '5 days'),
('UPDATE', 'order', 3, 'user:carol@example.com', '{"status": "cancelled"}'::jsonb, NOW() - INTERVAL '3 days');

-- Update product reserved quantities based on pending orders
UPDATE tb_products SET quantity_reserved = 1 WHERE id = 4;  -- Monitor reserved for order 2
UPDATE tb_products SET quantity_reserved = 1 WHERE id = 3;  -- Keyboard reserved for order 2
