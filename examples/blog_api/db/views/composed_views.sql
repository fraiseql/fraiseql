-- Example of composed views that eliminate N+1 queries in FraiseQL
-- These views pre-aggregate related data into JSONB structures

-- Basic author view
CREATE OR REPLACE VIEW v_authors AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Author',
        'id', id,
        'email', email,
        'name', name,
        'bio', bio,
        'avatarUrl', avatar_url
    ) AS data
FROM tb_user;

-- Composed view: Posts with author data included
-- This eliminates N+1 when fetching posts with their authors
CREATE OR REPLACE VIEW v_posts_with_author AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'isPublished', p.is_published,
        'publishedAt', p.published_at,
        'viewCount', p.view_count,
        'createdAt', p.created_at,
        'updatedAt', p.updated_at,
        -- Author data embedded directly
        'author', (
            SELECT jsonb_build_object(
                '__typename', 'User',
                'id', u.id,
                'email', u.email,
                'name', u.name,
                'bio', u.bio,
                'avatarUrl', u.avatar_url
            )
            FROM tb_users u
            WHERE u.id = p.author_id
        )
    ) AS data
FROM tb_posts p;

-- Composed view: Posts with author AND all comments with their authors
-- This eliminates N+1 for posts -> comments -> comment authors
CREATE OR REPLACE VIEW v_posts_full AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'isPublished', p.is_published,
        'publishedAt', p.published_at,
        'viewCount', p.view_count,
        'createdAt', p.created_at,
        'updatedAt', p.updated_at,
        -- Author embedded
        'author', (
            SELECT jsonb_build_object(
                '__typename', 'User',
                'id', u.id,
                'email', u.email,
                'name', u.name,
                'bio', u.bio,
                'avatarUrl', u.avatar_url
            )
            FROM tb_users u
            WHERE u.id = p.author_id
        ),
        -- Comments with their authors embedded
        'comments', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    '__typename', 'Comment',
                    'id', c.id,
                    'content', c.content,
                    'isEdited', c.is_edited,
                    'createdAt', c.created_at,
                    'updatedAt', c.updated_at,
                    -- Comment author embedded
                    'author', (
                        SELECT jsonb_build_object(
                            '__typename', 'User',
                            'id', cu.id,
                            'email', cu.email,
                            'name', cu.name,
                            'avatarUrl', cu.avatar_url
                        )
                        FROM tb_users cu
                        WHERE cu.id = c.author_id
                    ),
                    -- Nested replies with their authors
                    'replies', COALESCE(
                        (SELECT jsonb_agg(
                            jsonb_build_object(
                                '__typename', 'Comment',
                                'id', r.id,
                                'content', r.content,
                                'isEdited', r.is_edited,
                                'createdAt', r.created_at,
                                'author', (
                                    SELECT jsonb_build_object(
                                        '__typename', 'User',
                                        'id', ru.id,
                                        'name', ru.name,
                                        'avatarUrl', ru.avatar_url
                                    )
                                    FROM tb_users ru
                                    WHERE ru.id = r.author_id
                                )
                            )
                            ORDER BY r.created_at
                        )
                        FROM tb_comments r
                        WHERE r.parent_id = c.id),
                        '[]'::jsonb
                    )
                )
                ORDER BY c.created_at
            )
            FROM tb_comments c
            WHERE c.post_id = p.id AND c.parent_id IS NULL),
            '[]'::jsonb
        ),
        -- Comment statistics
        'commentStats', jsonb_build_object(
            'totalComments', (
                SELECT COUNT(*)
                FROM tb_comments
                WHERE post_id = p.id
            ),
            'uniqueCommenters', (
                SELECT COUNT(DISTINCT author_id)
                FROM tb_comments
                WHERE post_id = p.id
            )
        )
    ) AS data
FROM tb_posts p;

-- Composed view: User with all their posts and post statistics
-- This eliminates N+1 when fetching users with their posts
CREATE OR REPLACE VIEW v_users_with_posts AS
SELECT
    u.id,
    jsonb_build_object(
        '__typename', 'User',
        'id', u.id,
        'email', u.email,
        'name', u.name,
        'bio', u.bio,
        'avatarUrl', u.avatar_url,
        'isActive', u.is_active,
        'roles', u.roles,
        'createdAt', u.created_at,
        'updatedAt', u.updated_at,
        -- All user's posts embedded
        'posts', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    '__typename', 'Post',
                    'id', p.id,
                    'title', p.title,
                    'slug', p.slug,
                    'excerpt', p.excerpt,
                    'tags', p.tags,
                    'isPublished', p.is_published,
                    'publishedAt', p.published_at,
                    'viewCount', p.view_count,
                    'commentCount', (
                        SELECT COUNT(*)
                        FROM tb_comments
                        WHERE post_id = p.id
                    )
                )
                ORDER BY p.created_at DESC
            )
            FROM tb_posts p
            WHERE p.author_id = u.id),
            '[]'::jsonb
        ),
        -- User statistics
        'stats', jsonb_build_object(
            'postCount', (
                SELECT COUNT(*)
                FROM tb_posts
                WHERE author_id = u.id
            ),
            'publishedPostCount', (
                SELECT COUNT(*)
                FROM tb_posts
                WHERE author_id = u.id AND is_published = true
            ),
            'totalViews', (
                SELECT COALESCE(SUM(view_count), 0)
                FROM tb_posts
                WHERE author_id = u.id
            ),
            'totalComments', (
                SELECT COUNT(*)
                FROM tb_comments c
                JOIN tb_posts p ON p.id = c.post_id
                WHERE p.author_id = u.id
            )
        )
    ) AS data
FROM tb_users u;

-- Composed view: Blog feed with posts, authors, and comment counts
-- Optimized for homepage/feed queries
CREATE OR REPLACE VIEW v_blog_feed AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'PostSummary',
        'id', p.id,
        'title', p.title,
        'slug', p.slug,
        'excerpt', COALESCE(p.excerpt, LEFT(p.content, 200) || '...'),
        'tags', p.tags,
        'publishedAt', p.published_at,
        'viewCount', p.view_count,
        -- Minimal author info for feed
        'author', jsonb_build_object(
            '__typename', 'UserSummary',
            'id', u.id,
            'name', u.name,
            'avatarUrl', u.avatar_url
        ),
        -- Pre-computed stats
        'stats', jsonb_build_object(
            'commentCount', (
                SELECT COUNT(*)
                FROM tb_comments
                WHERE post_id = p.id
            ),
            'readTime', CEIL(LENGTH(p.content) / 1000.0), -- Rough estimate: 200 words/minute
            'hasComments', EXISTS(
                SELECT 1
                FROM tb_comments
                WHERE post_id = p.id
            )
        ),
        -- Latest comments preview
        'latestComments', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    'id', c.id,
                    'excerpt', LEFT(c.content, 100),
                    'authorName', cu.name,
                    'createdAt', c.created_at
                )
            )
            FROM (
                SELECT c.*, cu.name
                FROM tb_comments c
                JOIN tb_users cu ON cu.id = c.author_id
                WHERE c.post_id = p.id
                ORDER BY c.created_at DESC
                LIMIT 3
            ) c),
            '[]'::jsonb
        )
    ) AS data
FROM tb_posts p
JOIN tb_users u ON u.id = p.author_id
WHERE p.is_published = true;

-- Performance indexes for composed views
CREATE INDEX IF NOT EXISTS idx_posts_author_published
    ON tb_posts(author_id, is_published, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_comments_post_parent
    ON tb_comments(post_id, parent_id)
    WHERE parent_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_comments_parent
    ON tb_comments(parent_id)
    WHERE parent_id IS NOT NULL;

-- Example materialized view for expensive aggregations
-- Refreshed periodically to avoid recalculation
CREATE MATERIALIZED VIEW IF NOT EXISTS v_popular_posts AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'PopularPost',
        'id', p.id,
        'title', p.title,
        'slug', p.slug,
        'author', jsonb_build_object(
            'id', u.id,
            'name', u.name
        ),
        'metrics', jsonb_build_object(
            'viewCount', p.view_count,
            'commentCount', COUNT(DISTINCT c.id),
            'uniqueCommenters', COUNT(DISTINCT c.author_id),
            'engagementScore', (
                p.view_count +
                (COUNT(DISTINCT c.id) * 10) +
                (COUNT(DISTINCT c.author_id) * 5)
            )
        ),
        'topCommenters', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'userId', author_id,
                    'name', name,
                    'commentCount', comment_count
                )
                ORDER BY comment_count DESC
            )
            FROM (
                SELECT
                    c.author_id,
                    u.name,
                    COUNT(*) as comment_count
                FROM tb_comments c
                JOIN tb_users u ON u.id = c.author_id
                WHERE c.post_id = p.id
                GROUP BY c.author_id, u.name
                ORDER BY COUNT(*) DESC
                LIMIT 5
            ) top_commenters
        )
    ) AS data
FROM tb_posts p
JOIN tb_users u ON u.id = p.author_id
LEFT JOIN tb_comments c ON c.post_id = p.id
WHERE p.is_published = true
GROUP BY p.id, p.title, p.slug, p.view_count, u.id, u.name
HAVING p.view_count > 100 OR COUNT(DISTINCT c.id) > 5;

CREATE UNIQUE INDEX idx_popular_posts_id ON v_popular_posts(id);

-- Function to refresh materialized views
CREATE OR REPLACE FUNCTION refresh_blog_statistics()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY v_post_stats;
    REFRESH MATERIALIZED VIEW CONCURRENTLY v_popular_posts;
END;
$$ LANGUAGE plpgsql;

-- Example of how FraiseQL would query these views
-- No N+1 queries needed!

-- 1. Get all posts with authors and comment counts
-- SELECT * FROM v_blog_feed WHERE data->>'publishedAt' > '2024-01-01' ORDER BY data->>'publishedAt' DESC;

-- 2. Get a single post with all comments and nested replies
-- SELECT * FROM v_posts_full WHERE id = 'some-uuid';

-- 3. Get a user with all their posts
-- SELECT * FROM v_users_with_posts WHERE id = 'user-uuid';

-- 4. Get popular posts with engagement metrics
-- SELECT * FROM v_popular_posts ORDER BY data->'metrics'->>'engagementScore' DESC;
