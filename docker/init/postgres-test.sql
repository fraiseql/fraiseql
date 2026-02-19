-- FraiseQL integration test data for PostgreSQL
-- Loaded automatically by docker-compose.test.yml

-- ============================================================================
-- Users table + v_user view
-- ============================================================================
CREATE TABLE users (
    id    SERIAL PRIMARY KEY,
    data  JSONB NOT NULL
);

INSERT INTO users (data) VALUES
('{"id": 1, "name": "Alice",   "email": "alice@example.com",   "role": "admin",     "age": 28, "active": true,  "metadata": {"city": "Paris",    "country": "FR"}}'),
('{"id": 2, "name": "Bob",     "email": "bob@example.com",     "role": "user",      "age": 25, "active": true,  "metadata": {"city": "London",   "country": "GB"}}'),
('{"id": 3, "name": "Charlie", "email": "charlie@example.com", "role": "moderator", "age": 35, "active": false, "metadata": {"city": "Berlin",   "country": "DE"}}'),
('{"id": 4, "name": "Diana",   "email": "diana@example.com",   "role": "user",      "age": 30, "active": true,  "metadata": {"city": "Paris",    "country": "FR"}}'),
('{"id": 5, "name": "Eve",     "email": "eve@example.com",     "role": "admin",     "age": 22, "active": true,  "metadata": {"city": "New York", "country": "US"}}');

CREATE VIEW v_user AS SELECT data FROM users;

-- ============================================================================
-- Posts table + v_post view
-- ============================================================================
CREATE TABLE posts (
    id    SERIAL PRIMARY KEY,
    data  JSONB NOT NULL
);

INSERT INTO posts (data) VALUES
('{"title": "Hello World",     "author": {"name": "Alice", "email": "alice@example.com"},   "published": true,  "views": 100}'),
('{"title": "GraphQL Basics",  "author": {"name": "Bob",   "email": "bob@example.com"},     "published": true,  "views": 250}'),
('{"title": "Advanced Queries","author": {"name": "Alice", "email": "alice@example.com"},   "published": true,  "views": 75}'),
('{"title": "Draft Post",      "author": {"name": "Charlie","email": "charlie@example.com"},"published": false, "views": 0}');

CREATE VIEW v_post AS SELECT data FROM posts;

-- ============================================================================
-- Fact table for introspector tests (tf_sales)
-- ============================================================================
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE tf_sales (
    id          SERIAL PRIMARY KEY,
    revenue     NUMERIC(12,2) NOT NULL,
    quantity    INTEGER NOT NULL,
    cost        NUMERIC(12,2) NOT NULL,
    discount    NUMERIC(5,2) DEFAULT 0,
    data        JSONB,
    customer_id UUID NOT NULL,
    product_id  UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tf_sales_customer_id  ON tf_sales (customer_id);
CREATE INDEX idx_tf_sales_product_id   ON tf_sales (product_id);
CREATE INDEX idx_tf_sales_occurred_at  ON tf_sales (occurred_at);
CREATE INDEX idx_tf_sales_data_gin     ON tf_sales USING GIN (data);

INSERT INTO tf_sales (revenue, quantity, cost, discount, data, customer_id, product_id, occurred_at) VALUES
(99.99,  1, 50.00, 0,    '{"region": "EU", "channel": "web"}',    uuid_generate_v4(), uuid_generate_v4(), '2024-01-15'),
(249.50, 5, 200.00, 10.0, '{"region": "US", "channel": "mobile"}', uuid_generate_v4(), uuid_generate_v4(), '2024-02-20'),
(15.00,  3, 10.00, 0,    '{"region": "EU", "channel": "api"}',    uuid_generate_v4(), uuid_generate_v4(), '2024-03-10');

-- ============================================================================
-- Fact table for event introspector tests (tf_events)
-- ============================================================================
CREATE TABLE tf_events (
    id             BIGSERIAL PRIMARY KEY,
    duration_ms    BIGINT NOT NULL,
    error_count    INTEGER NOT NULL DEFAULT 0,
    request_size   BIGINT NOT NULL DEFAULT 0,
    response_size  BIGINT NOT NULL DEFAULT 0,
    status_code    INTEGER NOT NULL,
    data           JSONB,
    endpoint       VARCHAR(255) NOT NULL,
    occurred_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tf_events_endpoint    ON tf_events (endpoint);
CREATE INDEX idx_tf_events_status_code ON tf_events (status_code);
CREATE INDEX idx_tf_events_occurred_at ON tf_events (occurred_at);
CREATE INDEX idx_tf_events_data_gin    ON tf_events USING GIN (data);

INSERT INTO tf_events (duration_ms, error_count, request_size, response_size, status_code, data, endpoint, occurred_at) VALUES
(120, 0, 512,  2048, 200, '{"method": "GET",  "path": "/api/users"}',  '/api/users',  '2024-01-15'),
(45,  0, 256,  1024, 200, '{"method": "GET",  "path": "/api/posts"}',  '/api/posts',  '2024-02-20'),
(500, 1, 1024, 128,  500, '{"method": "POST", "path": "/api/submit"}', '/api/submit', '2024-03-10');
