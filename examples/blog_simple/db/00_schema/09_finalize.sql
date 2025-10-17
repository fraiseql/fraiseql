-- ==============================================================================
-- FINALIZATION: Permissions and grants
-- ==============================================================================
-- File: 00_schema/09_finalize.sql
-- Layer: Finalization
-- Contains: Permissions, grants, analyze
-- ==============================================================================

-- Grant permissions (adjust for your security model)
GRANT USAGE ON SCHEMA public TO PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO PUBLIC;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO PUBLIC;

-- Analyze tables for query optimization
ANALYZE tb_user;
ANALYZE tb_post;
ANALYZE tb_comment;
