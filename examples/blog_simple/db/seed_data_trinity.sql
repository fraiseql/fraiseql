-- FraiseQL Blog Simple - Seed Data with Trinity Identifiers
--
-- Sample data demonstrating Trinity pattern:
-- - pk_* columns are SERIAL (auto-generated)
-- - id columns are UUID (auto-generated)
-- - identifier columns are human-readable slugs

-- Clean existing data
TRUNCATE TABLE tb_post_tags, tb_comments, tb_posts, tb_tags, tb_users RESTART IDENTITY CASCADE;

-- ============================================================================
-- Sample Users (identifier = username)
-- ============================================================================
INSERT INTO tb_users (username, email, password_hash, role, profile_data) VALUES
    ('admin', 'admin@example.com', '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYzS.WZ0nXS', 'admin',
     '{"first_name": "Admin", "last_name": "User", "bio": "System administrator", "avatar_url": "https://ui-avatars.com/api/?name=Admin+User"}'::jsonb),
    ('johndoe', 'john@example.com', '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYzS.WZ0nXS', 'author',
     '{"first_name": "John", "last_name": "Doe", "bio": "Tech enthusiast and writer", "avatar_url": "https://ui-avatars.com/api/?name=John+Doe"}'::jsonb),
    ('janedoe', 'jane@example.com', '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYzS.WZ0nXS', 'author',
     '{"first_name": "Jane", "last_name": "Doe", "bio": "Developer and blogger", "avatar_url": "https://ui-avatars.com/api/?name=Jane+Doe"}'::jsonb),
    ('bobsmith', 'bob@example.com', '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYzS.WZ0nXS', 'user',
     '{"first_name": "Bob", "last_name": "Smith", "bio": "Regular reader", "avatar_url": "https://ui-avatars.com/api/?name=Bob+Smith"}'::jsonb);

-- Note: identifier column is auto-generated from username by trigger
-- e.g., @johndoe, @janedoe, @bobsmith

-- ============================================================================
-- Sample Tags (identifier = slug)
-- ============================================================================
INSERT INTO tb_tags (name, slug, color, description) VALUES
    ('Technology', 'technology', '#3b82f6', 'Posts about technology and innovation'),
    ('Python', 'python', '#3776ab', 'Python programming and tutorials'),
    ('Web Development', 'web-development', '#10b981', 'Web development tips and tricks'),
    ('Database', 'database', '#8b5cf6', 'Database design and optimization'),
    ('Tutorial', 'tutorial', '#f59e0b', 'Step-by-step tutorials'),
    ('News', 'news', '#ef4444', 'Latest tech news and updates');

-- Note: identifier column is auto-generated from slug by trigger

-- ============================================================================
-- Sample Posts (identifier = slug)
-- ============================================================================
-- Post 1 by johndoe
INSERT INTO tb_posts (title, slug, content, excerpt, fk_author, status, published_at)
VALUES (
    'Getting Started with FraiseQL',
    'getting-started-with-fraiseql',
    E'# Introduction to FraiseQL\n\nFraiseQL is a modern GraphQL framework built for PostgreSQL. In this post, we''ll explore the basics.\n\n## What is FraiseQL?\n\nFraiseQL combines the power of PostgreSQL with GraphQL''s flexibility...\n\n## Trinity Identifiers\n\nWith Trinity pattern, every entity has three identifiers:\n- pk_* (SERIAL) for fast joins\n- id (UUID) for public API\n- identifier (TEXT) for human URLs',
    'Learn the basics of FraiseQL and how to get started with GraphQL and PostgreSQL.',
    (SELECT pk_user FROM tb_users WHERE username = 'johndoe'),
    'published',
    NOW() - INTERVAL '2 days'
);

-- Post 2 by janedoe
INSERT INTO tb_posts (title, slug, content, excerpt, fk_author, status, published_at)
VALUES (
    'Understanding Trinity Identifiers',
    'understanding-trinity-identifiers',
    E'# Trinity Identifiers Explained\n\nTrinity identifiers provide three ways to reference entities:\n\n## The Three Tiers\n\n1. **pk_* (SERIAL)**: Internal primary key for database joins\n2. **id (UUID)**: Public API identifier for security\n3. **identifier (TEXT)**: Human-readable URL slug\n\n## Why Trinity?\n\nSERIAL joins are faster due to smaller index size (4 bytes vs 16 bytes for UUID).',
    'Deep dive into the Trinity identifier pattern and its benefits for performance.',
    (SELECT pk_user FROM tb_users WHERE username = 'janedoe'),
    'published',
    NOW() - INTERVAL '1 day'
);

-- Post 3 by johndoe (draft)
INSERT INTO tb_posts (title, slug, content, excerpt, fk_author, status)
VALUES (
    'Advanced PostgreSQL Techniques',
    'advanced-postgresql-techniques',
    E'# PostgreSQL Performance Tips\n\n## Indexing Strategies\n\nProper indexing is crucial for query performance...\n\n## Query Optimization\n\nUse EXPLAIN ANALYZE to understand query plans.',
    'Advanced techniques for optimizing PostgreSQL queries.',
    (SELECT pk_user FROM tb_users WHERE username = 'johndoe'),
    'draft'
);

-- Post 4 by janedoe
INSERT INTO tb_posts (title, slug, content, excerpt, fk_author, status, published_at)
VALUES (
    'Building RESTful APIs with Python',
    'building-restful-apis-with-python',
    E'# REST API Development\n\n## FastAPI Framework\n\nFastAPI makes it easy to build high-performance APIs...\n\n## Best Practices\n\n- Use proper HTTP methods\n- Implement pagination\n- Add rate limiting',
    'Learn how to build production-ready REST APIs using Python and FastAPI.',
    (SELECT pk_user FROM tb_users WHERE username = 'janedoe'),
    'published',
    NOW() - INTERVAL '3 days'
);

-- Note: identifier column is auto-generated from slug by trigger

-- ============================================================================
-- Post-Tag associations (using SERIAL foreign keys)
-- ============================================================================
-- Post 1 tags
INSERT INTO tb_post_tags (fk_post, fk_tag)
SELECT
    (SELECT pk_post FROM tb_posts WHERE slug = 'getting-started-with-fraiseql'),
    pk_tag
FROM tb_tags WHERE slug IN ('technology', 'tutorial', 'database');

-- Post 2 tags
INSERT INTO tb_post_tags (fk_post, fk_tag)
SELECT
    (SELECT pk_post FROM tb_posts WHERE slug = 'understanding-trinity-identifiers'),
    pk_tag
FROM tb_tags WHERE slug IN ('database', 'tutorial');

-- Post 3 tags
INSERT INTO tb_post_tags (fk_post, fk_tag)
SELECT
    (SELECT pk_post FROM tb_posts WHERE slug = 'advanced-postgresql-techniques'),
    pk_tag
FROM tb_tags WHERE slug IN ('database', 'tutorial');

-- Post 4 tags
INSERT INTO tb_post_tags (fk_post, fk_tag)
SELECT
    (SELECT pk_post FROM tb_posts WHERE slug = 'building-restful-apis-with-python'),
    pk_tag
FROM tb_tags WHERE slug IN ('python', 'web-development', 'tutorial');

-- ============================================================================
-- Sample Comments (using SERIAL foreign keys)
-- ============================================================================
-- Comment 1 on Post 1
INSERT INTO tb_comments (fk_post, fk_author, content, status)
VALUES (
    (SELECT pk_post FROM tb_posts WHERE slug = 'getting-started-with-fraiseql'),
    (SELECT pk_user FROM tb_users WHERE username = 'bobsmith'),
    'Great introduction! Very helpful for beginners.',
    'approved'
);

-- Comment 2 on Post 1 (reply to Comment 1)
INSERT INTO tb_comments (fk_post, fk_author, fk_parent, content, status)
VALUES (
    (SELECT pk_post FROM tb_posts WHERE slug = 'getting-started-with-fraiseql'),
    (SELECT pk_user FROM tb_users WHERE username = 'johndoe'),
    (SELECT pk_comment FROM tb_comments WHERE content = 'Great introduction! Very helpful for beginners.'),
    'Thanks Bob! Glad you found it useful.',
    'approved'
);

-- Comment 3 on Post 2
INSERT INTO tb_comments (fk_post, fk_author, content, status)
VALUES (
    (SELECT pk_post FROM tb_posts WHERE slug = 'understanding-trinity-identifiers'),
    (SELECT pk_user FROM tb_users WHERE username = 'johndoe'),
    'Excellent explanation of the Trinity pattern!',
    'approved'
);

-- Comment 4 on Post 4
INSERT INTO tb_comments (fk_post, fk_author, content, status)
VALUES (
    (SELECT pk_post FROM tb_posts WHERE slug = 'building-restful-apis-with-python'),
    (SELECT pk_user FROM tb_users WHERE username = 'bobsmith'),
    'Do you have a tutorial on authentication?',
    'approved'
);

-- Comment 5 on Post 4 (reply to Comment 4)
INSERT INTO tb_comments (fk_post, fk_author, fk_parent, content, status)
VALUES (
    (SELECT pk_post FROM tb_posts WHERE slug = 'building-restful-apis-with-python'),
    (SELECT pk_user FROM tb_users WHERE username = 'janedoe'),
    (SELECT pk_comment FROM tb_comments WHERE content = 'Do you have a tutorial on authentication?'),
    'Great question! I''ll write a follow-up post on that.',
    'approved'
);

-- ============================================================================
-- Verification queries
-- ============================================================================
-- Show all users with Trinity IDs
SELECT pk_user, id, identifier, username FROM tb_users ORDER BY pk_user;

-- Show all posts with Trinity IDs and SERIAL FK
SELECT pk_post, id, identifier, title, fk_author FROM tb_posts ORDER BY pk_post;

-- Show all comments with SERIAL FKs
SELECT pk_comment, id, fk_post, fk_author, substr(content, 1, 30) as content_preview
FROM tb_comments ORDER BY pk_comment;

-- Show post-tag associations (SERIAL FKs)
SELECT
    p.identifier as post_slug,
    t.identifier as tag_slug
FROM tb_post_tags pt
JOIN tb_posts p ON p.pk_post = pt.fk_post
JOIN tb_tags t ON t.pk_tag = pt.fk_tag
ORDER BY p.pk_post, t.pk_tag;

-- Trinity identifier examples:
-- Users: @johndoe, @janedoe, @bobsmith
-- Posts: /posts/getting-started-with-fraiseql
-- Tags: /tags/python, /tags/tutorial
