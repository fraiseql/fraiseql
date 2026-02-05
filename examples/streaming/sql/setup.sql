-- FraiseQL Streaming Example - Database Setup (Trinity Pattern)
-- PostgreSQL with real-time event streaming
-- Pattern: tb_* (table), pk_* (INTEGER primary key), fk_* (INTEGER foreign key), id (UUID), v_* (view)

-- Drop existing objects if present
DROP TABLE IF EXISTS tb_metric CASCADE;
DROP TABLE IF EXISTS tb_user_activity CASCADE;
DROP TABLE IF EXISTS tb_message CASCADE;
DROP TABLE IF EXISTS tb_event CASCADE;

-- Create event table for streaming events (Trinity Pattern)
CREATE TABLE tb_event (
    pk_event SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    type VARCHAR(50) NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    data TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create message table for real-time messaging (Trinity Pattern)
CREATE TABLE tb_message (
    pk_message SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_user_activity INTEGER NOT NULL REFERENCES tb_user_activity(pk_user_activity),
    content TEXT NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    reactions INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create user_activity table for presence tracking (Trinity Pattern)
CREATE TABLE tb_user_activity (
    pk_user_activity SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    username VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'offline',
    last_seen TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    active_now BOOLEAN DEFAULT FALSE,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create live_metrics table for system metrics (Trinity Pattern)
CREATE TABLE tb_metric (
    pk_metric SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    metric VARCHAR(100) NOT NULL,
    value NUMERIC(10, 2) NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    source VARCHAR(100) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes for performance
CREATE INDEX idx_tb_event_type ON tb_event(type);
CREATE INDEX idx_tb_event_timestamp ON tb_event(timestamp DESC);
CREATE INDEX idx_tb_event_id ON tb_event(id);
CREATE INDEX idx_tb_message_fk_user_activity ON tb_message(fk_user_activity);
CREATE INDEX idx_tb_message_timestamp ON tb_message(timestamp DESC);
CREATE INDEX idx_tb_message_id ON tb_message(id);
CREATE INDEX idx_tb_user_activity_status ON tb_user_activity(status);
CREATE INDEX idx_tb_user_activity_id ON tb_user_activity(id);
CREATE INDEX idx_tb_metric_metric ON tb_metric(metric);
CREATE INDEX idx_tb_metric_timestamp ON tb_metric(timestamp DESC);
CREATE INDEX idx_tb_metric_id ON tb_metric(id);

-- Create views (Trinity Pattern v_* naming)
-- Each view returns pk_* (for internal joins) and data (JSONB for GraphQL)
CREATE VIEW v_event AS
SELECT
    pk_event,
    jsonb_build_object(
        'id', id,
        'type', type,
        'timestamp', timestamp,
        'data', data,
        'created_at', created_at
    ) AS data
FROM tb_event;

CREATE VIEW v_message AS
SELECT
    m.pk_message,
    jsonb_build_object(
        'id', m.id,
        'user_id', u.id,
        'username', u.username,
        'content', m.content,
        'timestamp', m.timestamp,
        'reactions', m.reactions,
        'created_at', m.created_at
    ) AS data
FROM tb_message m
JOIN tb_user_activity u ON m.fk_user_activity = u.pk_user_activity;

CREATE VIEW v_user_activity AS
SELECT
    pk_user_activity,
    jsonb_build_object(
        'id', id,
        'username', username,
        'status', status,
        'last_seen', last_seen,
        'active_now', active_now,
        'updated_at', updated_at
    ) AS data
FROM tb_user_activity;

CREATE VIEW v_metric AS
SELECT
    pk_metric,
    jsonb_build_object(
        'id', id,
        'metric', metric,
        'value', value,
        'timestamp', timestamp,
        'source', source,
        'created_at', created_at
    ) AS data
FROM tb_metric;

-- Insert sample user activity first (required for messages foreign key)
INSERT INTO tb_user_activity (username, status, active_now) VALUES
    ('alice', 'online', TRUE),
    ('bob', 'idle', FALSE),
    ('charlie', 'online', TRUE),
    ('diana', 'offline', FALSE),
    ('eve', 'away', FALSE);

-- Insert sample events
INSERT INTO tb_event (type, data) VALUES
    ('user_login', '{"userId": 1, "username": "alice", "device": "web"}'),
    ('user_action', '{"userId": 2, "action": "purchase", "amount": 99.99}'),
    ('system_alert', '{"severity": "info", "message": "System backup completed"}'),
    ('user_login', '{"userId": 3, "username": "charlie", "device": "mobile"}'),
    ('user_action', '{"userId": 1, "action": "view_product", "productId": 42}');

-- Insert sample messages (using surrogate key references to user_activity)
INSERT INTO tb_message (fk_user_activity, content) VALUES
    (1, 'Hey everyone, just started using FraiseQL!'),
    (2, 'The streaming features are incredible'),
    (1, 'How do subscriptions work in FraiseQL?'),
    (3, 'Check out the documentation for examples'),
    (2, 'Loving the real-time updates!');

-- Insert sample metrics
INSERT INTO tb_metric (metric, value, source) VALUES
    ('cpu_usage', 45.2, 'server-1'),
    ('memory_usage', 62.8, 'server-1'),
    ('query_latency_ms', 12.5, 'database'),
    ('requests_per_second', 1250.0, 'api-gateway'),
    ('cache_hit_ratio', 0.87, 'redis'),
    ('database_connections', 23.0, 'pool'),
    ('error_rate', 0.02, 'api-gateway');

-- Verify data
SELECT 'Events:' AS info;
SELECT COUNT(*) as event_count FROM tb_event;

SELECT 'Messages:' AS info;
SELECT COUNT(*) as message_count FROM tb_message;

SELECT 'User Activity:' AS info;
SELECT COUNT(*) as user_count FROM tb_user_activity;

SELECT 'Live Metrics:' AS info;
SELECT COUNT(*) as metric_count FROM tb_metric;

-- Sample queries to verify schema
SELECT 'Recent events:' AS info;
SELECT type, COUNT(*) as count FROM tb_event GROUP BY type;

SELECT 'Active users:' AS info;
SELECT COUNT(*) as active_users FROM tb_user_activity WHERE active_now = TRUE;

SELECT 'System health snapshot:' AS info;
SELECT metric, value FROM tb_metric ORDER BY timestamp DESC LIMIT 5;
