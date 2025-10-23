-- Admin Panel Database Schema
-- Complete schema for customer support, operations, and sales dashboards

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";  -- For full-text search

-- ============================================================================
-- CORE TABLES
-- ============================================================================

-- Customers table
CREATE TABLE customers (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    subscription_status VARCHAR(50) NOT NULL DEFAULT 'active',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Support tickets table
CREATE TABLE support_tickets (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID NOT NULL REFERENCES customers(id),
    subject VARCHAR(500) NOT NULL,
    description TEXT NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'open',
    priority VARCHAR(20) NOT NULL DEFAULT 'medium',
    assigned_to_id UUID,
    resolution_notes TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Orders table
CREATE TABLE orders (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_number VARCHAR(50) UNIQUE NOT NULL,
    customer_id UUID NOT NULL REFERENCES customers(id),
    total DECIMAL(10, 2) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    tracking_number VARCHAR(100),
    refund_amount DECIMAL(10, 2),
    refund_reason TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    shipped_at TIMESTAMP,
    delivered_at TIMESTAMP
);

-- Order items table
CREATE TABLE order_items (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_name VARCHAR(255) NOT NULL,
    product_sku VARCHAR(100) NOT NULL,
    quantity INT NOT NULL,
    unit_price DECIMAL(10, 2) NOT NULL,
    total_price DECIMAL(10, 2) NOT NULL
);

-- Deals/opportunities table
CREATE TABLE deals (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    company_name VARCHAR(255) NOT NULL,
    contact_name VARCHAR(255) NOT NULL,
    contact_email VARCHAR(255) NOT NULL,
    stage VARCHAR(50) NOT NULL DEFAULT 'prospecting',
    amount DECIMAL(12, 2) NOT NULL,
    probability INT NOT NULL DEFAULT 10 CHECK (probability >= 0 AND probability <= 100),
    expected_close_date DATE NOT NULL,
    assigned_to_id UUID NOT NULL,
    notes TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Admin users table
CREATE TABLE admin_users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Audit log table (critical for compliance)
CREATE TABLE admin_audit_log (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    admin_user_id UUID NOT NULL REFERENCES admin_users(id),
    action VARCHAR(100) NOT NULL,
    target_type VARCHAR(50),
    target_id UUID,
    details JSONB,
    ip_address INET,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- ============================================================================
-- INDEXES FOR PERFORMANCE
-- ============================================================================

-- Customer search indexes
CREATE INDEX idx_customers_email ON customers USING gin(email gin_trgm_ops);
CREATE INDEX idx_customers_name ON customers USING gin(name gin_trgm_ops);
CREATE INDEX idx_customers_status ON customers(subscription_status);
CREATE INDEX idx_customers_created ON customers(created_at DESC);

-- Support tickets indexes
CREATE INDEX idx_tickets_customer ON support_tickets(customer_id);
CREATE INDEX idx_tickets_status ON support_tickets(status);
CREATE INDEX idx_tickets_priority ON support_tickets(priority);
CREATE INDEX idx_tickets_assigned ON support_tickets(assigned_to_id);
CREATE INDEX idx_tickets_created ON support_tickets(created_at DESC);

-- Orders indexes
CREATE INDEX idx_orders_customer ON orders(customer_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_created ON orders(created_at DESC);
CREATE INDEX idx_orders_number ON orders(order_number);

-- Order items indexes
CREATE INDEX idx_order_items_order ON order_items(order_id);
CREATE INDEX idx_order_items_sku ON order_items(product_sku);

-- Deals indexes
CREATE INDEX idx_deals_stage ON deals(stage);
CREATE INDEX idx_deals_assigned ON deals(assigned_to_id);
CREATE INDEX idx_deals_expected_close ON deals(expected_close_date);
CREATE INDEX idx_deals_updated ON deals(updated_at DESC);

-- Audit log indexes
CREATE INDEX idx_audit_admin ON admin_audit_log(admin_user_id);
CREATE INDEX idx_audit_action ON admin_audit_log(action);
CREATE INDEX idx_audit_target ON admin_audit_log(target_type, target_id);
CREATE INDEX idx_audit_created ON admin_audit_log(created_at DESC);

-- ============================================================================
-- READ-ONLY VIEWS FOR ADMIN PANEL
-- ============================================================================

-- Customer admin view (safe, no passwords)
CREATE VIEW customer_admin_view AS
SELECT
    c.id,
    c.email,
    c.name,
    c.created_at,
    c.subscription_status,
    COALESCE(SUM(o.total), 0)::DECIMAL(10,2) as total_spent,
    COUNT(DISTINCT t.id)::INT as ticket_count
FROM customers c
LEFT JOIN orders o ON o.customer_id = c.id
LEFT JOIN support_tickets t ON t.customer_id = c.id
GROUP BY c.id, c.email, c.name, c.created_at, c.subscription_status;

-- Support tickets view with customer info
CREATE VIEW support_tickets_view AS
SELECT
    t.id,
    t.customer_id,
    t.subject,
    t.status,
    t.priority,
    t.assigned_to_id,
    t.created_at,
    t.updated_at
FROM support_tickets t;

-- Orders view with customer info
CREATE VIEW orders_view AS
SELECT
    o.id,
    o.order_number,
    o.customer_id,
    o.total,
    o.status,
    o.tracking_number,
    o.created_at,
    o.shipped_at,
    o.delivered_at
FROM orders o;

-- Orders needing attention (delayed, stuck, etc.)
CREATE VIEW orders_needing_attention_view AS
SELECT
    o.id,
    o.order_number,
    o.customer_id,
    o.total,
    o.status,
    o.created_at,
    (NOW() - o.created_at) as age_hours
FROM orders o
WHERE
    (o.status = 'pending' AND o.created_at < NOW() - INTERVAL '24 hours')
    OR (o.status = 'processing' AND o.created_at < NOW() - INTERVAL '48 hours')
    OR (o.status = 'shipped' AND o.shipped_at < NOW() - INTERVAL '7 days' AND o.delivered_at IS NULL)
ORDER BY o.created_at;

-- Deals view
CREATE VIEW deals_view AS
SELECT
    d.id,
    d.company_name,
    d.contact_name,
    d.contact_email,
    d.stage,
    d.amount,
    d.probability,
    d.expected_close_date,
    d.assigned_to_id,
    d.notes,
    d.created_at,
    d.updated_at
FROM deals d;

-- ============================================================================
-- MATERIALIZED VIEWS FOR DASHBOARD METRICS (REFRESH EVERY 5 MIN)
-- ============================================================================

-- Operations metrics materialized view
CREATE MATERIALIZED VIEW operations_metrics_mv AS
SELECT
    COUNT(*) FILTER (WHERE status = 'pending')::INT as pending_orders,
    COUNT(*) FILTER (WHERE status = 'processing')::INT as processing_orders,
    COUNT(*) FILTER (WHERE DATE(shipped_at) = CURRENT_DATE)::INT as shipped_today,
    COALESCE(
        EXTRACT(EPOCH FROM AVG(shipped_at - created_at) FILTER (WHERE shipped_at IS NOT NULL)) / 3600,
        0
    )::FLOAT as average_fulfillment_time,
    0::INT as low_stock_items,  -- Would join inventory table in production
    0::INT as out_of_stock_items,  -- Would join inventory table in production
    COALESCE(SUM(total) FILTER (WHERE DATE(created_at) = CURRENT_DATE), 0)::DECIMAL(10,2) as today_revenue,
    COALESCE(SUM(total) FILTER (WHERE DATE_TRUNC('month', created_at) = DATE_TRUNC('month', CURRENT_DATE)), 0)::DECIMAL(10,2) as month_revenue,
    100.0::FLOAT as order_accuracy,  -- Would calculate from returns in production
    95.0::FLOAT as on_time_delivery_rate  -- Would calculate from delivery dates in production
FROM orders;

-- Sales metrics materialized view
CREATE MATERIALIZED VIEW sales_metrics_view AS
SELECT
    a.id as rep_id,
    a.name as rep_name,
    COALESCE(
        SUM(d.amount) FILTER (
            WHERE d.stage = 'closed_won'
            AND DATE_TRUNC('month', d.updated_at) = DATE_TRUNC('month', CURRENT_DATE)
        ),
        0
    )::DECIMAL(12,2) as current_month_revenue,
    0.0::FLOAT as quota_attainment,  -- Would calculate from quotas table
    COUNT(*) FILTER (WHERE d.stage NOT IN ('closed_won', 'closed_lost'))::INT as deals_in_pipeline,
    COUNT(*) FILTER (
        WHERE d.stage = 'closed_won'
        AND DATE_TRUNC('month', d.updated_at) = DATE_TRUNC('month', CURRENT_DATE)
    )::INT as deals_won_this_month,
    COALESCE(AVG(d.amount) FILTER (WHERE d.stage NOT IN ('closed_won', 'closed_lost')), 0)::DECIMAL(12,2) as average_deal_size
FROM admin_users a
LEFT JOIN deals d ON d.assigned_to_id = a.id
WHERE a.role = 'sales'
GROUP BY a.id, a.name;

-- Create indexes on materialized views
CREATE INDEX idx_operations_metrics_mv_refresh ON operations_metrics_mv ((1));
CREATE INDEX idx_sales_metrics_mv_rep ON sales_metrics_view(rep_id);

-- ============================================================================
-- FUNCTIONS FOR AUTO-REFRESH (CALL FROM CRON OR PG_CRON)
-- ============================================================================

CREATE OR REPLACE FUNCTION refresh_dashboard_metrics()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY operations_metrics_mv;
    REFRESH MATERIALIZED VIEW CONCURRENTLY sales_metrics_view;
END;
$$ LANGUAGE plpgsql;

-- Schedule refresh every 5 minutes (requires pg_cron extension)
-- SELECT cron.schedule('refresh-metrics', '*/5 * * * *', 'SELECT refresh_dashboard_metrics()');

-- ============================================================================
-- SAMPLE DATA (FOR TESTING)
-- ============================================================================

-- Insert sample admin users
INSERT INTO admin_users (email, name, password_hash, role) VALUES
('admin@example.com', 'Super Admin', '$2b$12$dummy_hash', 'admin'),
('support@example.com', 'Support Agent', '$2b$12$dummy_hash', 'customer_support'),
('ops@example.com', 'Operations Manager', '$2b$12$dummy_hash', 'operations'),
('sales@example.com', 'Sales Rep', '$2b$12$dummy_hash', 'sales');

-- Insert sample customers
INSERT INTO customers (id, email, name, password_hash, subscription_status) VALUES
('11111111-1111-1111-1111-111111111111', 'john@example.com', 'John Doe', '$2b$12$dummy_hash', 'active'),
('22222222-2222-2222-2222-222222222222', 'jane@example.com', 'Jane Smith', '$2b$12$dummy_hash', 'active'),
('33333333-3333-3333-3333-333333333333', 'bob@example.com', 'Bob Johnson', '$2b$12$dummy_hash', 'suspended');

-- Insert sample orders
INSERT INTO orders (id, order_number, customer_id, total, status) VALUES
('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', 'ORD-001', '11111111-1111-1111-1111-111111111111', 149.99, 'pending'),
('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', 'ORD-002', '22222222-2222-2222-2222-222222222222', 299.99, 'shipped');

-- Insert sample support tickets
INSERT INTO support_tickets (customer_id, subject, description, status, priority) VALUES
('11111111-1111-1111-1111-111111111111', 'Cannot login to account', 'Getting error when trying to login', 'open', 'high'),
('22222222-2222-2222-2222-222222222222', 'Question about billing', 'When will I be charged?', 'open', 'low');

-- Insert sample deals
INSERT INTO deals (company_name, contact_name, contact_email, stage, amount, expected_close_date, assigned_to_id)
SELECT 'Acme Corp', 'Alice Anderson', 'alice@acme.com', 'negotiation', 50000, CURRENT_DATE + 30, id
FROM admin_users WHERE role = 'sales' LIMIT 1;

-- Initial refresh of materialized views
REFRESH MATERIALIZED VIEW operations_metrics_mv;
REFRESH MATERIALIZED VIEW sales_metrics_view;
