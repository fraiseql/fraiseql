-- ==============================================================================
-- FUNCTIONS: Business logic and helpers
-- ==============================================================================
-- File: 00_schema/03_functions.sql
-- Layer: Functions (business logic)
-- Contains: Slug generation, helper functions
-- ==============================================================================

-- Generate slug from text
CREATE OR REPLACE FUNCTION generate_slug(input_text TEXT)
RETURNS TEXT AS $$
BEGIN
    RETURN lower(regexp_replace(
        regexp_replace(input_text, '[^a-zA-Z0-9\s]', '', 'g'),
        '\s+', '-', 'g'
    ));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION generate_slug IS 'Convert text to URL-friendly slug';
