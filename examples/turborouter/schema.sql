-- TurboRouter Example Database Schema

-- Users table
CREATE TABLE tb_user (
    pk_user INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Posts table
CREATE TABLE tb_post (
    pk_post INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID UNIQUE NOT NULL DEFAULT uuid_generate_v4(),
    fk_user INT NOT NULL REFERENCES tb_user(pk_user),
    title VARCHAR(500) NOT NULL,
    content TEXT NOT NULL,
    published BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_tb_post_fk_user ON tb_post(fk_user);
CREATE INDEX idx_tb_post_published ON tb_post(published) WHERE published = true;
CREATE INDEX idx_tb_post_created ON tb_post(created_at DESC);
CREATE INDEX idx_tb_user_id ON tb_user(id);
CREATE INDEX idx_tb_post_id ON tb_post(id);

-- Views for GraphQL queries
CREATE VIEW v_user AS
SELECT
    pk_user,
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) as data
FROM tb_user;

CREATE VIEW v_post AS
SELECT
    pk_post,
    id,
    jsonb_build_object(
        'id', id,
        'fk_user', fk_user,
        'title', title,
        'content', content,
        'published', published,
        'created_at', created_at
    ) as data
FROM tb_post;

-- Sample data
INSERT INTO tb_user (name, email) VALUES
('Alice Johnson', 'alice@example.com'),
('Bob Smith', 'bob@example.com'),
('Carol Williams', 'carol@example.com');

INSERT INTO tb_post (fk_user, title, content, published) VALUES
(1, 'Getting Started with FraiseQL', 'FraiseQL makes GraphQL development fast and type-safe...', true),
(1, 'TurboRouter Performance', 'TurboRouter provides 2-4x performance improvements...', true),
(2, 'Database-First GraphQL', 'Let PostgreSQL do the heavy lifting...', true),
(2, 'Draft Post', 'This is not published yet', false),
(3, 'CQRS with PostgreSQL', 'Views for queries, functions for mutations...', true);
