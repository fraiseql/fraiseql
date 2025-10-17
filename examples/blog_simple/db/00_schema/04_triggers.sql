-- ==============================================================================
-- TRIGGERS: Automated behaviors
-- ==============================================================================
-- File: 00_schema/04_triggers.sql
-- Layer: Triggers (automation)
-- Contains: updated_at, slug generation, published_at
-- ==============================================================================

-- Update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_tb_user_updated_at
    BEFORE UPDATE ON tb_user
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_post_updated_at
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tb_comment_updated_at
    BEFORE UPDATE ON tb_comment
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Auto-generate slug for posts
CREATE OR REPLACE FUNCTION auto_generate_post_slug()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.identifier IS NULL OR NEW.identifier = '' THEN
        NEW.identifier = generate_slug(NEW.title);

        -- Ensure uniqueness
        WHILE EXISTS (SELECT 1 FROM tb_post WHERE identifier = NEW.identifier AND pk_post != COALESCE(NEW.pk_post, -1)) LOOP
            NEW.identifier = NEW.identifier || '-' || substr(NEW.id::text, 1, 8);
        END LOOP;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tb_post_auto_generate_slug
    BEFORE INSERT OR UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION auto_generate_post_slug();

-- Set published_at when status changes to published
CREATE OR REPLACE FUNCTION set_published_at()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status = 'published' AND (OLD.status IS NULL OR OLD.status != 'published') THEN
        NEW.published_at = NOW();
    ELSIF NEW.status != 'published' THEN
        NEW.published_at = NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tb_post_set_published_at
    BEFORE INSERT OR UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION set_published_at();
