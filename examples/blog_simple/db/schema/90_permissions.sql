-- ==============================================================================
-- PERMISSIONS: Grant basic permissions
-- ==============================================================================

-- Note: In production, create dedicated roles with minimal permissions
GRANT USAGE ON SCHEMA public TO PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO PUBLIC;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO PUBLIC;
