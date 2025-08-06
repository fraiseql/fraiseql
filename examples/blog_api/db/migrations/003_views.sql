-- Blog API CQRS Schema - Read Side Views
-- All read-side views are prefixed with v_ and include JSONB data column

-- Users view with JSONB data
CREATE OR REPLACE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'User',
        'id', id,
        'email', email,
        'name', name,
        'bio', bio,
        'avatarUrl', avatar_url,
        'isActive', is_active,
        'roles', roles,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) AS data
FROM tb_users;

-- Posts view with JSONB data
CREATE OR REPLACE VIEW v_posts AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'authorId', p.author_id,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'isPublished', p.is_published,
        'publishedAt', p.published_at,
        'viewCount', p.view_count,
        'createdAt', p.created_at,
        'updatedAt', p.updated_at
    ) AS data
FROM tb_posts p;

-- Comments view with JSONB data
CREATE OR REPLACE VIEW v_comments AS
SELECT
    c.id,
    jsonb_build_object(
        '__typename', 'Comment',
        'id', c.id,
        'postId', c.post_id,
        'authorId', c.author_id,
        'parentId', c.parent_id,
        'content', c.content,
        'isEdited', c.is_edited,
        'createdAt', c.created_at,
        'updatedAt', c.updated_at
    ) AS data
FROM tb_comments c;

-- Materialized view for post statistics (optional optimization)
CREATE MATERIALIZED VIEW IF NOT EXISTS v_post_stats AS
SELECT
    p.id AS post_id,
    jsonb_build_object(
        '__typename', 'PostStats',
        'postId', p.id,
        'commentCount', COUNT(DISTINCT c.id),
        'replyCount', COUNT(DISTINCT r.id),
        'lastCommentAt', MAX(c.created_at)
    ) AS data
FROM tb_posts p
LEFT JOIN tb_comments c ON c.post_id = p.id AND c.parent_id IS NULL
LEFT JOIN tb_comments r ON r.parent_id = c.id
GROUP BY p.id;

-- Create indexes on materialized view
CREATE INDEX idx_v_post_stats_post_id ON v_post_stats(post_id);

-- Function to refresh post statistics
CREATE OR REPLACE FUNCTION refresh_post_stats()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY v_post_stats;
END;
$$ LANGUAGE plpgsql;

-- Optional: Create indexes on views for better performance
-- Note: These would actually be on the underlying tables
CREATE INDEX IF NOT EXISTS idx_tb_users_data_email ON tb_users((email));
CREATE INDEX IF NOT EXISTS idx_tb_posts_data_slug ON tb_posts((slug));
CREATE INDEX IF NOT EXISTS idx_tb_comments_data_post ON tb_comments((post_id));
