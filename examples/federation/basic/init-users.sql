-- Users database initialization
-- Used by users-service (owns User entity)

CREATE TABLE users (
    id VARCHAR(50) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Index key fields for federation
CREATE INDEX idx_users_id ON users(id);

-- Insert test data
INSERT INTO users (id, name, email) VALUES
    ('user1', 'Alice Johnson', 'alice@example.com'),
    ('user2', 'Bob Smith', 'bob@example.com'),
    ('user3', 'Charlie Brown', 'charlie@example.com'),
    ('user4', 'Diana Prince', 'diana@example.com'),
    ('user5', 'Eve Adams', 'eve@example.com');
