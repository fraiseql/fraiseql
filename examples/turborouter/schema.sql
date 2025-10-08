-- TurboRouter Example Database Schema

-- Users table
CREATE TABLE tb_users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Posts table
CREATE TABLE tb_posts (
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES tb_users(id),
    title VARCHAR(500) NOT NULL,
    content TEXT NOT NULL,
    published BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_posts_user_id ON tb_posts(user_id);
CREATE INDEX idx_posts_published ON tb_posts(published) WHERE published = true;
CREATE INDEX idx_posts_created ON tb_posts(created_at DESC);

-- Views for GraphQL queries
CREATE VIEW v_users AS
SELECT
    id,
    name,
    email,
    created_at
FROM tb_users;

CREATE VIEW v_posts AS
SELECT
    id,
    user_id,
    title,
    content,
    published,
    created_at
FROM tb_posts;

-- Sample data
INSERT INTO tb_users (name, email) VALUES
('Alice Johnson', 'alice@example.com'),
('Bob Smith', 'bob@example.com'),
('Carol Williams', 'carol@example.com');

INSERT INTO tb_posts (user_id, title, content, published) VALUES
(1, 'Getting Started with FraiseQL', 'FraiseQL makes GraphQL development fast and type-safe...', true),
(1, 'TurboRouter Performance', 'TurboRouter provides 2-4x performance improvements...', true),
(2, 'Database-First GraphQL', 'Let PostgreSQL do the heavy lifting...', true),
(2, 'Draft Post', 'This is not published yet', false),
(3, 'CQRS with PostgreSQL', 'Views for queries, functions for mutations...', true);
