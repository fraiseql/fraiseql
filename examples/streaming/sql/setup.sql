-- FraiseQL Streaming Example - Database Setup
-- PostgreSQL with real-time event streaming

-- Drop existing objects if present
DROP TABLE IF EXISTS live_metrics CASCADE;
DROP TABLE IF EXISTS user_activity CASCADE;
DROP TABLE IF EXISTS messages CASCADE;
DROP TABLE IF EXISTS events CASCADE;

-- Create events table for streaming events
CREATE TABLE events (
    id SERIAL PRIMARY KEY,
    type VARCHAR(50) NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    data TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create messages table for real-time messaging
CREATE TABLE messages (
    id SERIAL PRIMARY KEY,
    userId INTEGER NOT NULL,
    content TEXT NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    reactions INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create user_activity table for presence tracking
CREATE TABLE user_activity (
    userId SERIAL PRIMARY KEY,
    username VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'offline',
    lastSeen TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    activeNow BOOLEAN DEFAULT FALSE,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create live_metrics table for system metrics
CREATE TABLE live_metrics (
    id SERIAL PRIMARY KEY,
    metric VARCHAR(100) NOT NULL,
    value NUMERIC(10, 2) NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    source VARCHAR(100) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create indexes for performance
CREATE INDEX idx_events_type ON events(type);
CREATE INDEX idx_events_timestamp ON events(timestamp DESC);
CREATE INDEX idx_messages_userId ON messages(userId);
CREATE INDEX idx_messages_timestamp ON messages(timestamp DESC);
CREATE INDEX idx_user_activity_status ON user_activity(status);
CREATE INDEX idx_live_metrics_metric ON live_metrics(metric);
CREATE INDEX idx_live_metrics_timestamp ON live_metrics(timestamp DESC);

-- Insert sample events
INSERT INTO events (type, data) VALUES
    ('user_login', '{"userId": 1, "username": "alice", "device": "web"}'),
    ('user_action', '{"userId": 2, "action": "purchase", "amount": 99.99}'),
    ('system_alert', '{"severity": "info", "message": "System backup completed"}'),
    ('user_login', '{"userId": 3, "username": "charlie", "device": "mobile"}'),
    ('user_action', '{"userId": 1, "action": "view_product", "productId": 42}');

-- Insert sample messages
INSERT INTO messages (userId, content) VALUES
    (1, 'Hey everyone, just started using FraiseQL!'),
    (2, 'The streaming features are incredible'),
    (1, 'How do subscriptions work in FraiseQL?'),
    (3, 'Check out the documentation for examples'),
    (2, 'Loving the real-time updates!');

-- Insert sample user activity
INSERT INTO user_activity (username, status, activeNow) VALUES
    ('alice', 'online', TRUE),
    ('bob', 'idle', FALSE),
    ('charlie', 'online', TRUE),
    ('diana', 'offline', FALSE),
    ('eve', 'away', FALSE);

-- Insert sample metrics
INSERT INTO live_metrics (metric, value, source) VALUES
    ('cpu_usage', 45.2, 'server-1'),
    ('memory_usage', 62.8, 'server-1'),
    ('query_latency_ms', 12.5, 'database'),
    ('requests_per_second', 1250.0, 'api-gateway'),
    ('cache_hit_ratio', 0.87, 'redis'),
    ('database_connections', 23.0, 'pool'),
    ('error_rate', 0.02, 'api-gateway');

-- Verify data
SELECT 'Events:' AS info;
SELECT COUNT(*) as event_count FROM events;

SELECT 'Messages:' AS info;
SELECT COUNT(*) as message_count FROM messages;

SELECT 'User Activity:' AS info;
SELECT COUNT(*) as user_count FROM user_activity;

SELECT 'Live Metrics:' AS info;
SELECT COUNT(*) as metric_count FROM live_metrics;

-- Sample queries to verify schema
SELECT 'Recent events:' AS info;
SELECT type, COUNT(*) as count FROM events GROUP BY type;

SELECT 'Active users:' AS info;
SELECT COUNT(*) as active_users FROM user_activity WHERE activeNow = TRUE;

SELECT 'System health snapshot:' AS info;
SELECT metric, value FROM live_metrics ORDER BY timestamp DESC LIMIT 5;
