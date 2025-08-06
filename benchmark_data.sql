-- Create tables for benchmark
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS posts (
    id SERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    content TEXT,
    author_id INTEGER NOT NULL REFERENCES users(id),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS comments (
    id SERIAL PRIMARY KEY,
    content TEXT NOT NULL,
    post_id INTEGER NOT NULL REFERENCES posts(id),
    author_id INTEGER NOT NULL REFERENCES users(id),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for performance
CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_author_id ON comments(author_id);

-- Insert test data
INSERT INTO users (name, email) VALUES
    ('John Doe', 'john@example.com'),
    ('Jane Smith', 'jane@example.com'),
    ('Bob Johnson', 'bob@example.com'),
    ('Alice Williams', 'alice@example.com'),
    ('Charlie Brown', 'charlie@example.com');

-- Generate posts (10 per user)
INSERT INTO posts (title, content, author_id)
SELECT
    'Post ' || (u.id * 10 + s.n) || ' by ' || u.name,
    'This is the content of post ' || (u.id * 10 + s.n) || '. Lorem ipsum dolor sit amet, consectetur adipiscing elit.',
    u.id
FROM users u
CROSS JOIN generate_series(1, 10) s(n);

-- Generate comments (5 per post)
INSERT INTO comments (content, post_id, author_id)
SELECT
    'Comment ' || s.n || ' on post ' || p.id,
    p.id,
    (RANDOM() * 4 + 1)::INTEGER
FROM posts p
CROSS JOIN generate_series(1, 5) s(n);

-- Create views for FraiseQL (similar to the architecture)
CREATE OR REPLACE VIEW user_with_posts AS
SELECT
    u.id,
    u.name,
    u.email,
    u.created_at,
    COALESCE(
        jsonb_agg(
            jsonb_build_object(
                'id', p.id,
                'title', p.title,
                'content', p.content,
                'created_at', p.created_at
            ) ORDER BY p.created_at DESC
        ) FILTER (WHERE p.id IS NOT NULL),
        '[]'::jsonb
    ) as posts
FROM users u
LEFT JOIN posts p ON u.id = p.author_id
GROUP BY u.id;

CREATE OR REPLACE VIEW post_with_comments AS
SELECT
    p.id,
    p.title,
    p.content,
    p.created_at,
    jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email
    ) as author,
    COALESCE(
        jsonb_agg(
            jsonb_build_object(
                'id', c.id,
                'content', c.content,
                'created_at', c.created_at,
                'author', jsonb_build_object(
                    'id', cu.id,
                    'name', cu.name
                )
            ) ORDER BY c.created_at
        ) FILTER (WHERE c.id IS NOT NULL),
        '[]'::jsonb
    ) as comments
FROM posts p
JOIN users u ON p.author_id = u.id
LEFT JOIN comments c ON p.id = c.post_id
LEFT JOIN users cu ON c.author_id = cu.id
GROUP BY p.id, p.title, p.content, p.created_at, u.id, u.name, u.email;

-- Analyze tables for query optimization
ANALYZE users;
ANALYZE posts;
ANALYZE comments;
