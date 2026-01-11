-- MySQL Test Database Initialization
--
-- This script creates test views with JSON data for FraiseQL integration tests.
-- Note: MySQL uses JSON type (not JSONB like PostgreSQL)

-- ============================================================================
-- Test View: v_user
-- ============================================================================

CREATE TABLE IF NOT EXISTS users_test (
    id CHAR(36) PRIMARY KEY,
    email VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    age INT,
    active BOOLEAN DEFAULT TRUE,
    role VARCHAR(50) DEFAULT 'user',
    tags JSON,
    metadata JSON,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Insert test data
INSERT INTO users_test (id, email, name, age, active, role, tags, metadata) VALUES
    ('00000000-0000-0000-0000-000000000001', 'alice@example.com', 'Alice Smith', 30, true, 'admin', JSON_ARRAY('admin', 'developer'), JSON_OBJECT('city', 'Paris', 'country', 'France')),
    ('00000000-0000-0000-0000-000000000002', 'bob@example.com', 'Bob Jones', 25, true, 'user', JSON_ARRAY('user'), JSON_OBJECT('city', 'London', 'country', 'UK')),
    ('00000000-0000-0000-0000-000000000003', 'charlie@test.com', 'Charlie Brown', 35, false, 'moderator', JSON_ARRAY('moderator'), JSON_OBJECT('city', 'New York', 'country', 'USA')),
    ('00000000-0000-0000-0000-000000000004', 'diana@example.com', 'Diana Prince', 28, true, 'user', JSON_ARRAY('user', 'premium'), JSON_OBJECT('city', 'Berlin', 'country', 'Germany')),
    ('00000000-0000-0000-0000-000000000005', 'eve@test.com', 'Eve Wilson', 22, true, 'user', JSON_ARRAY('user'), JSON_OBJECT('city', 'Tokyo', 'country', 'Japan'))
ON DUPLICATE KEY UPDATE email=email;

-- Create JSON view (MySQL's JSON functions)
CREATE OR REPLACE VIEW v_user AS
SELECT
    JSON_OBJECT(
        'id', id,
        'email', email,
        'name', name,
        'age', age,
        'active', active,
        'role', role,
        'tags', tags,
        'metadata', metadata,
        'created_at', created_at,
        'deleted_at', deleted_at
    ) AS data
FROM users_test;

-- ============================================================================
-- Test View: v_post
-- ============================================================================

CREATE TABLE IF NOT EXISTS posts_test (
    id CHAR(36) PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    content TEXT,
    author_id CHAR(36),
    published BOOLEAN DEFAULT FALSE,
    views INT DEFAULT 0,
    tags JSON,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (author_id) REFERENCES users_test(id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Insert test data
INSERT INTO posts_test (id, title, content, author_id, published, views, tags) VALUES
    ('00000000-0000-0000-0000-000000000101', 'Introduction to GraphQL', 'GraphQL is a query language...', '00000000-0000-0000-0000-000000000001', true, 150, JSON_ARRAY('graphql', 'tutorial')),
    ('00000000-0000-0000-0000-000000000102', 'Rust Performance', 'Rust offers zero-cost abstractions...', '00000000-0000-0000-0000-000000000001', true, 200, JSON_ARRAY('rust', 'performance')),
    ('00000000-0000-0000-0000-000000000103', 'Draft Post', 'This is a draft...', '00000000-0000-0000-0000-000000000002', false, 0, JSON_ARRAY('draft')),
    ('00000000-0000-0000-0000-000000000104', 'PostgreSQL Tips', 'JSONB is powerful...', '00000000-0000-0000-0000-000000000003', true, 75, JSON_ARRAY('postgresql', 'database'))
ON DUPLICATE KEY UPDATE title=title;

-- Create JSON view
CREATE OR REPLACE VIEW v_post AS
SELECT
    JSON_OBJECT(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', (
            SELECT JSON_OBJECT(
                'id', u.id,
                'name', u.name,
                'email', u.email
            )
            FROM users_test u
            WHERE u.id = p.author_id
        ),
        'published', p.published,
        'views', p.views,
        'tags', p.tags,
        'created_at', p.created_at
    ) AS data
FROM posts_test p;

-- ============================================================================
-- Test View: v_product
-- ============================================================================

CREATE TABLE IF NOT EXISTS products_test (
    id CHAR(36) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    stock INT DEFAULT 0,
    category VARCHAR(100),
    attributes JSON
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Insert test data
INSERT INTO products_test (id, name, price, stock, category, attributes) VALUES
    ('00000000-0000-0000-0000-000000000201', 'Laptop', 999.99, 10, 'electronics', JSON_OBJECT('brand', 'Dell', 'screen', 15)),
    ('00000000-0000-0000-0000-000000000202', 'Mouse', 29.99, 50, 'electronics', JSON_OBJECT('brand', 'Logitech', 'wireless', true)),
    ('00000000-0000-0000-0000-000000000203', 'Desk', 299.99, 5, 'furniture', JSON_OBJECT('material', 'wood', 'adjustable', false)),
    ('00000000-0000-0000-0000-000000000204', 'Chair', 199.99, 15, 'furniture', JSON_OBJECT('material', 'leather', 'adjustable', true))
ON DUPLICATE KEY UPDATE name=name;

-- Create JSON view
CREATE OR REPLACE VIEW v_product AS
SELECT
    JSON_OBJECT(
        'id', id,
        'name', name,
        'price', price,
        'stock', stock,
        'category', category,
        'attributes', attributes
    ) AS data
FROM products_test;

-- ============================================================================
-- Verification
-- ============================================================================

SELECT 'v_user' AS view_name, COUNT(*) AS row_count FROM v_user
UNION ALL
SELECT 'v_post', COUNT(*) FROM v_post
UNION ALL
SELECT 'v_product', COUNT(*) FROM v_product;
