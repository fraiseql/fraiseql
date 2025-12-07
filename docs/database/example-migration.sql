-- example-migration.sql
-- Migrates simple blog schema from users/posts to tb_user/tb_post
-- Run this script to migrate your database to Trinity Pattern

BEGIN;

-- Step 1: Rename base tables
ALTER TABLE users RENAME TO tb_user;
ALTER TABLE posts RENAME TO tb_post;
ALTER TABLE comments RENAME TO tb_comment;

-- Step 2: Create views
CREATE VIEW v_user AS
SELECT id, name, email, created_at
FROM tb_user
WHERE deleted_at IS NULL;

CREATE VIEW v_post AS
SELECT id, user_id, title, content, created_at
FROM tb_post
WHERE deleted_at IS NULL;

CREATE VIEW v_comment AS
SELECT id, post_id, user_id, content, created_at
FROM tb_comment
WHERE deleted_at IS NULL;

-- Step 3: Create computed views
CREATE VIEW tv_user_with_stats AS
SELECT
    u.id,
    u.name,
    u.email,
    COUNT(DISTINCT p.id) as post_count,
    COUNT(DISTINCT c.id) as comment_count,
    MAX(p.created_at) as last_post_at
FROM tb_user u
LEFT JOIN tb_post p ON p.user_id = u.id
LEFT JOIN tb_comment c ON c.user_id = u.id
GROUP BY u.id, u.name, u.email;

CREATE VIEW tv_post_with_stats AS
SELECT
    p.id,
    p.title,
    p.user_id,
    u.name as author_name,
    COUNT(c.id) as comment_count,
    MAX(c.created_at) as last_comment_at
FROM tb_post p
JOIN tb_user u ON u.id = p.user_id
LEFT JOIN tb_comment c ON c.post_id = p.id
GROUP BY p.id, p.title, p.user_id, u.name;

COMMIT;

-- Verification
SELECT 'tb_user rows' as check, COUNT(*) as count FROM tb_user
UNION ALL
SELECT 'v_user rows', COUNT(*) FROM v_user
UNION ALL
SELECT 'tv_user_with_stats rows', COUNT(*) FROM tv_user_with_stats;