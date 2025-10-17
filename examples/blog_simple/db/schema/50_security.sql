-- ==============================================================================
-- SECURITY: Row Level Security (RLS) examples
-- ==============================================================================

ALTER TABLE tb_user ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_post ENABLE ROW LEVEL SECURITY;
ALTER TABLE tb_comment ENABLE ROW LEVEL SECURITY;

-- Policy: Users can only see their own data
CREATE POLICY user_own_data ON tb_user
    FOR ALL
    USING (id = current_setting('app.current_user_id', true)::uuid);

-- Policy: Published posts are visible to all, drafts only to author
CREATE POLICY posts_visibility ON tb_post
    FOR SELECT
    USING (
        status = 'published'
        OR EXISTS (
            SELECT 1 FROM tb_user
            WHERE tb_user.pk_user = tb_post.fk_author
            AND tb_user.id = current_setting('app.current_user_id', true)::uuid
        )
    );

-- Policy: Users can insert their own posts
CREATE POLICY posts_insert ON tb_post
    FOR INSERT
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM tb_user
            WHERE tb_user.pk_user = fk_author
            AND tb_user.id = current_setting('app.current_user_id', true)::uuid
        )
    );

-- Policy: Users can update their own posts
CREATE POLICY posts_update ON tb_post
    FOR UPDATE
    USING (
        EXISTS (
            SELECT 1 FROM tb_user
            WHERE tb_user.pk_user = tb_post.fk_author
            AND tb_user.id = current_setting('app.current_user_id', true)::uuid
        )
    );

-- Policy: Approved comments are visible to all
CREATE POLICY comments_visibility ON tb_comment
    FOR SELECT
    USING (status = 'approved');
