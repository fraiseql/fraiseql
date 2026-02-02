-- Users database initialization (Trinity Pattern)
-- Used by users-service (owns User entity)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (federation ID), v_* (view)

DROP TABLE IF EXISTS tb_users CASCADE;

CREATE TABLE tb_users (
    pk_user SERIAL PRIMARY KEY,
    id VARCHAR(50) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Index key fields for federation and performance
CREATE INDEX idx_tb_users_id ON tb_users(id);
CREATE INDEX idx_tb_users_email ON tb_users(email);

-- Create view (Trinity Pattern v_* naming)
CREATE VIEW v_users AS
SELECT pk_user, id, name, email, created_at
FROM tb_users;

-- Insert test data
INSERT INTO tb_users (id, name, email) VALUES
    ('user1', 'Alice Johnson', 'alice@example.com'),
    ('user2', 'Bob Smith', 'bob@example.com'),
    ('user3', 'Charlie Brown', 'charlie@example.com'),
    ('user4', 'Diana Prince', 'diana@example.com'),
    ('user5', 'Eve Adams', 'eve@example.com');
