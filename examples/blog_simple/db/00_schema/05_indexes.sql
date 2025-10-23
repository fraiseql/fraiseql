-- ==============================================================================
-- INDEXES: Performance optimization
-- ==============================================================================
-- File: 00_schema/05_indexes.sql
-- Layer: Indexes (performance)
-- Contains: All indexes for query optimization
-- ==============================================================================

-- User indexes
CREATE INDEX idx_tb_user_id ON tb_user(id);
CREATE INDEX idx_tb_user_identifier ON tb_user(identifier);
CREATE INDEX idx_tb_user_email ON tb_user(email);

-- Post indexes
CREATE INDEX idx_tb_post_id ON tb_post(id);
CREATE INDEX idx_tb_post_identifier ON tb_post(identifier);
CREATE INDEX idx_tb_post_fk_author ON tb_post(fk_author);
CREATE INDEX idx_tb_post_status ON tb_post(status);
CREATE INDEX idx_tb_post_published_at ON tb_post(published_at) WHERE published_at IS NOT NULL;
CREATE INDEX idx_tb_post_title_search ON tb_post USING GIN (to_tsvector('english', title));
CREATE INDEX idx_tb_post_content_search ON tb_post USING GIN (to_tsvector('english', content));

-- Comment indexes
CREATE INDEX idx_tb_comment_id ON tb_comment(id);
CREATE INDEX idx_tb_comment_fk_post ON tb_comment(fk_post);
CREATE INDEX idx_tb_comment_fk_author ON tb_comment(fk_author);
CREATE INDEX idx_tb_comment_fk_parent ON tb_comment(fk_parent) WHERE fk_parent IS NOT NULL;
CREATE INDEX idx_tb_comment_status ON tb_comment(status);
