-- Admin Panel Database Schema
-- Complete schema for customer support, operations, and sales dashboards

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";  -- For full-text search

-- ============================================================================
-- CORE TABLES
-- ============================================================================

-- Customers table
CREATE TABLE tb_customer (
    pk_customer INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    subscription_status VARCHAR(50) NOT NULL DEFAULT 'active',
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Support tickets table
CREATE TABLE tb_support_ticket (
    pk_support_ticket INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_customer INT NOT NULL REFERENCES tb_customer(pk_customer),
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
CREATE TABLE tb_order (
    pk_order INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    order_number VARCHAR(50) UNIQUE NOT NULL,
    fk_customer INT NOT NULL REFERENCES tb_customer(pk_customer),
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
CREATE TABLE tb_order_item (
    pk_order_item INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_order INT NOT NULL REFERENCES tb_order(pk_order) ON DELETE CASCADE,
    product_name VARCHAR(255) NOT NULL,
    product_sku VARCHAR(100) NOT NULL,
    quantity INT NOT NULL,
    unit_price DECIMAL(10, 2) NOT NULL,
    total_price DECIMAL(10, 2) NOT NULL
);

-- Deals/opportunities table
CREATE TABLE tb_deal (
    pk_deal INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
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
CREATE TABLE tb_admin_user (
    pk_admin_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(50) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Audit log table (critical for compliance)
CREATE TABLE tb_admin_audit_log (
    pk_admin_audit_log INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_admin_user INT NOT NULL REFERENCES tb_admin_user(pk_admin_user),
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
CREATE INDEX idx_tb_customer_email ON tb_customer USING gin(email gin_trgm_ops);
CREATE INDEX idx_tb_customer_name ON tb_customer USING gin(name gin_trgm_ops);
CREATE INDEX idx_tb_customer_status ON tb_customer(subscription_status);
CREATE INDEX idx_tb_customer_created ON tb_customer(created_at DESC);

-- Support tickets indexes
CREATE INDEX idx_tb_support_ticket_fk_customer ON tb_support_ticket(fk_customer);
CREATE INDEX idx_tb_order_fk_customer ON tb_order(fk_customer);
CREATE INDEX idx_tb_order_item_fk_order ON tb_order_item(fk_order);
CREATE INDEX idx_tb_admin_audit_log_fk_admin_user ON tb_admin_audit_log(fk_admin_user);
CREATE INDEX idx_tb_admin_audit_log_action ON tb_admin_audit_log(action);
CREATE INDEX idx_tb_admin_audit_log_target ON tb_admin_audit_log(target_type, target_id);
CREATE INDEX idx_tb_admin_audit_log_created ON tb_admin_audit_log(created_at DESC);

-- ============================================================================
-- READ-ONLY VIEWS FOR ADMIN PANEL
-- ============================================================================

-- Customer admin view (safe, no passwords)
CREATE VIEW v_customer_admin AS
SELECT
    c.pk_customer,
    c.id,
    jsonb_build_object(
        'id', c.id,
        'email', c.email,
        'name', c.name,
        'created_at', c.created_at,
        'subscription_status', c.subscription_status,
        'total_spent', COALESCE(SUM(o.total), 0)::DECIMAL(10,2),
        'ticket_count', COUNT(DISTINCT t.id)::INT
    ) as data
FROM tb_customer c
LEFT JOIN tb_order o ON o.fk_customer = c.pk_customer
LEFT JOIN tb_support_ticket t ON t.fk_customer = c.pk_customer
GROUP BY c.pk_customer, c.id, c.email, c.name, c.created_at, c.subscription_status;

-- Support tickets view with customer info
CREATE VIEW v_support_ticket AS
SELECT
    t.pk_support_ticket,
    t.id,
    jsonb_build_object(
        'id', t.id,
        'fk_customer', t.fk_customer,
        'subject', t.subject,
        'status', t.status,
        'priority', t.priority,
        'assigned_to_id', t.assigned_to_id,
        'created_at', t.created_at,
        'updated_at', t.updated_at
    ) as data
FROM tb_support_ticket t;

-- Orders view with customer info
CREATE VIEW v_order AS
SELECT
    o.pk_order,
    o.id,
    jsonb_build_object(
        'id', o.id,
        'order_number', o.order_number,
        'fk_customer', o.fk_customer,
        'total', o.total,
        'status', o.status,
        'tracking_number', o.tracking_number,
        'created_at', o.created_at,
        'shipped_at', o.shipped_at,
        'delivered_at', o.delivered_at
    ) as data
FROM tb_order o;

-- Orders needing attention (delayed, stuck, etc.)
CREATE VIEW v_order_attention AS
SELECT
    o.pk_order,
    o.id,
    jsonb_build_object(
        'id', o.id,
        'order_number', o.order_number,
        'fk_customer', o.fk_customer,
        'total', o.total,
        'status', o.status,
        'created_at', o.created_at,
        'age_hours', (NOW() - o.created_at)
    ) as data
FROM tb_order o
WHERE
    (o.status = 'pending' AND o.created_at < NOW() - INTERVAL '24 hours')
    OR (o.status = 'processing' AND o.created_at < NOW() - INTERVAL '48 hours')
    OR (o.status = 'shipped' AND o.shipped_at < NOW() - INTERVAL '7 days' AND o.delivered_at IS NULL)
ORDER BY o.created_at;

-- Deals view
CREATE VIEW v_deal AS
SELECT
    d.pk_deal,
    d.id,
    jsonb_build_object(
        'id', d.id,
        'company_name', d.company_name,
        'contact_name', d.contact_name,
        'contact_email', d.contact_email,
        'stage', d.stage,
        'amount', d.amount,
        'probability', d.probability,
        'expected_close_date', d.expected_close_date,
        'assigned_to_id', d.assigned_to_id,
        'notes', d.notes,
        'created_at', d.created_at,
        'updated_at', d.updated_at
    ) as data
FROM tb_deal d;

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
FROM tb_order;

-- Sales metrics materialized view
CREATE MATERIALIZED VIEW sales_metrics_view AS
SELECT
    a.pk_admin_user as rep_id,
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
FROM tb_admin_user a
LEFT JOIN tb_deal d ON d.assigned_to_id = a.id
WHERE a.role = 'sales'
GROUP BY a.id, a.name;

-- Create indexes on materialized views
CREATE INDEX idx_operations_metrics_mv_refresh ON operations_metrics_mv ((1));
CREATE INDEX idx_sales_metrics_mv_rep ON sales_metrics_view(pk_admin_user);

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
INSERT INTO tb_admin_user (email, name, password_hash, role) VALUES
('admin@example.com', 'Super Admin', '$2b$12$dummy_hash', 'admin'),
('support@example.com', 'Support Agent', '$2b$12$dummy_hash', 'customer_support'),
('ops@example.com', 'Operations Manager', '$2b$12$dummy_hash', 'operations'),
('sales@example.com', 'Sales Rep', '$2b$12$dummy_hash', 'sales');

-- Insert sample customers
INSERT INTO tb_customer (id, email, name, password_hash, subscription_status) VALUES
('11111111-1111-1111-1111-111111111111', 'john@example.com', 'John Doe', '$2b$12$dummy_hash', 'active'),
('22222222-2222-2222-2222-222222222222', 'jane@example.com', 'Jane Smith', '$2b$12$dummy_hash', 'active'),
('33333333-3333-3333-3333-333333333333', 'bob@example.com', 'Bob Johnson', '$2b$12$dummy_hash', 'suspended');

-- Insert sample orders
INSERT INTO tb_order (id, order_number, fk_customer, total, status) VALUES
('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', 'ORD-001', 1, 149.99, 'pending'),
('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', 'ORD-002', 2, 299.99, 'shipped');

-- Insert sample support tickets
INSERT INTO tb_support_ticket (fk_customer, subject, description, status, priority) VALUES
(1, 'Cannot login to account', 'Getting error when trying to login', 'open', 'high'),
(2, 'Question about billing', 'When will I be charged?', 'open', 'low');

-- Insert sample deals
INSERT INTO tb_deal (company_name, contact_name, contact_email, stage, amount, expected_close_date, assigned_to_id) VALUES
('Acme Corp', 'Alice Anderson', 'alice@acme.com', 'negotiation', 50000, CURRENT_DATE + 30, '11111111-1111-1111-1111-111111111111');

-- Initial refresh of materialized views
REFRESH MATERIALIZED VIEW operations_metrics_mv;
REFRESH MATERIALIZED VIEW sales_metrics_view;
