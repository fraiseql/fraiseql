-- ==============================================================================
-- TRIGGERS: Automated updates
-- ==============================================================================

-- Trigger for updated_at
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

-- Trigger to set published_at when status changes to published
CREATE OR REPLACE FUNCTION set_published_at()
RETURNS TRIGGER AS $$
BEGIN
    -- Set published_at when status changes to published
    IF NEW.status = 'published' AND (OLD.status != 'published' OR NEW.published_at IS NULL) THEN
        NEW.published_at = NOW();
    END IF

    -- Clear published_at when status changes away from published
    IF NEW.status != 'published' AND OLD.status = 'published' THEN
        NEW.published_at = NULL;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tb_post_set_published_at
    BEFORE UPDATE ON tb_post
    FOR EACH ROW EXECUTE FUNCTION set_published_at();

-- Trigger to auto-generate slug from title for posts
CREATE OR REPLACE FUNCTION auto_generate_post_slug()
RETURNS TRIGGER AS $$
BEGIN
    -- Generate identifier (slug) from title if not provided
    IF NEW.identifier IS NULL OR NEW.identifier = '' THEN
        NEW.identifier = generate_slug(NEW.title);

        -- Ensure uniqueness by appending part of UUID
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

-- Trigger to auto-generate slug from name for tags
CREATE OR REPLACE FUNCTION auto_generate_tag_slug()
RETURNS TRIGGER AS $$
BEGIN
    -- Generate identifier (slug) from name if not provided
    IF NEW.identifier IS NULL OR NEW.identifier = '' THEN
        NEW.identifier = generate_slug(NEW.name);

        -- Ensure uniqueness by appending part of UUID
        WHILE EXISTS (SELECT 1 FROM tb_tag WHERE identifier = NEW.identifier AND pk_tag != COALESCE(NEW.pk_tag, -1)) LOOP
            NEW.identifier = NEW.identifier || '-' || substr(NEW.id::text, 1, 8);
        END LOOP;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tb_tag_auto_generate_slug
    BEFORE INSERT OR UPDATE ON tb_tag
    FOR EACH ROW EXECUTE FUNCTION auto_generate_tag_slug();
