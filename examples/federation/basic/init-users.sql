-- Users database initialization (Trinity Pattern)
-- Used by users-service (owns User entity)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (federation ID), v_* (view)

DROP TABLE IF EXISTS tb_user CASCADE;

CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,
    id VARCHAR(50) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Index key fields for federation and performance
CREATE INDEX idx_tb_user_id ON tb_user(id);
CREATE INDEX idx_tb_user_email ON tb_user(email);

-- Create view (Trinity Pattern v_* naming)
-- Returns pk_* (for internal joins) and data (JSONB for GraphQL)
CREATE VIEW v_user AS
SELECT
    pk_user,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM tb_user;

-- Insert test data
INSERT INTO tb_user (id, name, email) VALUES
    ('user1', 'Alice Johnson', 'alice@example.com'),
    ('user2', 'Bob Smith', 'bob@example.com'),
    ('user3', 'Charlie Brown', 'charlie@example.com'),
    ('user4', 'Diana Prince', 'diana@example.com'),
    ('user5', 'Eve Adams', 'eve@example.com');
