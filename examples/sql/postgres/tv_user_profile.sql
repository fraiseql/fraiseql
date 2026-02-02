-- tv_user_profile: Table-backed JSON view for complex user profiles
-- Purpose: Pre-compose user data with nested posts, comments, and metadata
-- Refresh: Trigger-based (real-time) when user, posts, or comments change
-- Performance: 100-200ms query vs 2-5s for logical view

-- ============================================================================
-- STEP 1: Create intermediate composition views (reusable helpers)
-- ============================================================================

-- Aggregate comments per post
CREATE OR REPLACE VIEW v_comments_by_post AS
SELECT
    fk_post,
    jsonb_agg(
        jsonb_build_object(
            'id', id,
            'text', text,
            'createdAt', created_at
        )
        ORDER BY created_at DESC
    ) AS comments_data
FROM tb_comment
WHERE deleted_at IS NULL
GROUP BY fk_post;

-- Compose posts with nested comments
CREATE OR REPLACE VIEW v_post_with_comments AS
SELECT
    fk_user,
    jsonb_agg(
        p.data || jsonb_build_object(
            'comments', COALESCE(c.comments_data, '[]'::jsonb)
        )
        ORDER BY p.created_at DESC
    ) AS posts_data
FROM v_post p
LEFT JOIN v_comments_by_post c ON c.fk_post = p.pk_post
GROUP BY fk_user;

-- ============================================================================
-- STEP 2: Create table-backed view (physical materialization)
-- ============================================================================

CREATE TABLE tv_user_profile (
    id TEXT NOT NULL PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (id) REFERENCES tb_user(id) ON DELETE CASCADE
);

-- Index for faster lookups
CREATE INDEX idx_tv_user_profile_data_gin
    ON tv_user_profile USING GIN(data);

-- Index for monitoring staleness
CREATE INDEX idx_tv_user_profile_updated_at
    ON tv_user_profile (updated_at);

-- ============================================================================
-- STEP 3: Create refresh trigger functions
-- ============================================================================

-- Refresh function for single user (used by triggers)
CREATE OR REPLACE FUNCTION refresh_tv_user_profile_for_user(user_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_user_profile (id, data, updated_at)
    SELECT
        u.id,
        u.data || jsonb_build_object(
            'posts', COALESCE(p.posts_data, '[]'::jsonb)
        ) AS data,
        NOW()
    FROM v_user u
    LEFT JOIN v_post_with_comments p ON p.fk_user = u.pk_user
    WHERE u.id = user_id
    ON CONFLICT (id) DO UPDATE SET
        data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Refresh trigger on user changes
CREATE OR REPLACE FUNCTION trg_refresh_tv_user_profile_on_user()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM refresh_tv_user_profile_for_user(
        COALESCE(NEW.id, OLD.id)
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Refresh trigger on post changes (affects user profile)
CREATE OR REPLACE FUNCTION trg_refresh_tv_user_profile_on_post()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM refresh_tv_user_profile_for_user(
        (SELECT id FROM tb_user WHERE pk_user = COALESCE(NEW.fk_user, OLD.fk_user))
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Refresh trigger on comment changes (affects user profile via posts)
CREATE OR REPLACE FUNCTION trg_refresh_tv_user_profile_on_comment()
RETURNS TRIGGER AS $$
DECLARE
    v_user_id UUID;
BEGIN
    -- Find user ID through comment → post → user chain
    SELECT u.id INTO v_user_id
    FROM tb_user u
    JOIN tb_post p ON p.fk_user = u.pk_user
    WHERE p.pk_post = COALESCE(NEW.fk_post, OLD.fk_post)
    LIMIT 1;

    IF v_user_id IS NOT NULL THEN
        PERFORM refresh_tv_user_profile_for_user(v_user_id);
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- STEP 4: Attach triggers to source tables
-- ============================================================================

-- Trigger on user table
DROP TRIGGER IF EXISTS trg_refresh_tv_user_profile_on_user ON tb_user;
CREATE TRIGGER trg_refresh_tv_user_profile_on_user
    AFTER INSERT OR UPDATE OR DELETE ON tb_user
    FOR EACH ROW
    EXECUTE FUNCTION trg_refresh_tv_user_profile_on_user();

-- Trigger on post table
DROP TRIGGER IF EXISTS trg_refresh_tv_user_profile_on_post ON tb_post;
CREATE TRIGGER trg_refresh_tv_user_profile_on_post
    AFTER INSERT OR UPDATE OR DELETE ON tb_post
    FOR EACH ROW
    EXECUTE FUNCTION trg_refresh_tv_user_profile_on_post();

-- Trigger on comment table
DROP TRIGGER IF EXISTS trg_refresh_tv_user_profile_on_comment ON tb_comment;
CREATE TRIGGER trg_refresh_tv_user_profile_on_comment
    AFTER INSERT OR UPDATE OR DELETE ON tb_comment
    FOR EACH ROW
    EXECUTE FUNCTION trg_refresh_tv_user_profile_on_comment();

-- ============================================================================
-- STEP 5: Batch refresh function (for bulk operations)
-- ============================================================================

CREATE OR REPLACE FUNCTION refresh_tv_user_profile(
    user_id_filter UUID DEFAULT NULL
)
RETURNS TABLE(rows_inserted BIGINT, rows_updated BIGINT) AS $$
DECLARE
    v_inserted BIGINT := 0;
    v_updated BIGINT := 0;
BEGIN
    -- Upsert all user profiles (or filter by user_id)
    WITH upsert AS (
        INSERT INTO tv_user_profile (id, data, updated_at)
        SELECT
            u.id,
            u.data || jsonb_build_object(
                'posts', COALESCE(p.posts_data, '[]'::jsonb)
            ) AS data,
            NOW()
        FROM v_user u
        LEFT JOIN v_post_with_comments p ON p.fk_user = u.pk_user
        WHERE user_id_filter IS NULL OR u.id = user_id_filter
        ON CONFLICT (id) DO UPDATE SET
            data = EXCLUDED.data,
            updated_at = NOW()
        RETURNING (xmax = 0) AS inserted
    )
    SELECT COUNT(*) FILTER (WHERE inserted) INTO v_inserted FROM upsert;

    GET DIAGNOSTICS v_updated = ROW_COUNT;
    v_updated := v_updated - v_inserted;

    RETURN QUERY SELECT v_inserted, v_updated;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- STEP 6: Monitoring functions
-- ============================================================================

-- Check staleness of profiles
CREATE OR REPLACE FUNCTION monitor_tv_user_profile_staleness()
RETURNS TABLE(
    max_age_seconds NUMERIC,
    rows_total BIGINT,
    rows_fresh BIGINT,
    rows_stale BIGINT,
    percentage_stale NUMERIC
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        EXTRACT(EPOCH FROM (NOW() - MIN(updated_at)))::NUMERIC as max_age_seconds,
        COUNT(*)::BIGINT as rows_total,
        COUNT(*) FILTER (WHERE updated_at > NOW() - INTERVAL '1 minute')::BIGINT as rows_fresh,
        COUNT(*) FILTER (WHERE updated_at <= NOW() - INTERVAL '1 minute')::BIGINT as rows_stale,
        ROUND(
            100.0 * COUNT(*) FILTER (WHERE updated_at <= NOW() - INTERVAL '1 minute') / COUNT(*),
            2
        )::NUMERIC as percentage_stale
    FROM tv_user_profile;
END;
$$ LANGUAGE plpgsql;

-- Compare data consistency
CREATE OR REPLACE FUNCTION verify_tv_user_profile_consistency()
RETURNS TABLE(
    consistency_check TEXT,
    result TEXT,
    details TEXT
) AS $$
BEGIN
    -- Check 1: Row count matches
    RETURN QUERY
    SELECT
        'Row count match'::TEXT,
        CASE
            WHEN (SELECT COUNT(*) FROM tv_user_profile) = (SELECT COUNT(*) FROM v_user)
            THEN 'PASS'::TEXT
            ELSE 'FAIL'::TEXT
        END,
        'tv_user_profile: ' || (SELECT COUNT(*)::TEXT FROM tv_user_profile) ||
        ' | v_user: ' || (SELECT COUNT(*)::TEXT FROM v_user);

    -- Check 2: All users have profiles
    RETURN QUERY
    SELECT
        'All users have profiles'::TEXT,
        CASE
            WHEN (SELECT COUNT(*) FROM v_user WHERE id NOT IN (SELECT id FROM tv_user_profile)) = 0
            THEN 'PASS'::TEXT
            ELSE 'FAIL'::TEXT
        END,
        'Missing profiles: ' || (SELECT COUNT(*)::TEXT FROM v_user WHERE id NOT IN (SELECT id FROM tv_user_profile));

    -- Check 3: No orphaned profiles
    RETURN QUERY
    SELECT
        'No orphaned profiles'::TEXT,
        CASE
            WHEN (SELECT COUNT(*) FROM tv_user_profile WHERE id NOT IN (SELECT id FROM v_user)) = 0
            THEN 'PASS'::TEXT
            ELSE 'FAIL'::TEXT
        END,
        'Orphaned profiles: ' || (SELECT COUNT(*)::TEXT FROM tv_user_profile WHERE id NOT IN (SELECT id FROM v_user));

    -- Check 4: JSONB structure valid
    RETURN QUERY
    SELECT
        'JSONB structure valid'::TEXT,
        CASE
            WHEN (SELECT COUNT(*) FROM tv_user_profile WHERE data IS NULL OR data = 'null'::JSONB) = 0
            THEN 'PASS'::TEXT
            ELSE 'FAIL'::TEXT
        END,
        'Invalid JSONB: ' || (SELECT COUNT(*)::TEXT FROM tv_user_profile WHERE data IS NULL OR data = 'null'::JSONB);
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- STEP 7: Initial population (run once after table creation)
-- ============================================================================

-- Uncomment to populate on first run:
-- SELECT * FROM refresh_tv_user_profile();

-- ============================================================================
-- STEP 8: Usage examples
-- ============================================================================

-- Query single user profile (very fast)
-- SELECT data FROM tv_user_profile WHERE id = '550e8400-e29b-41d4-a716-446655440000';

-- Query multiple profiles
-- SELECT id, data->>'name' as name, jsonb_array_length(data->'posts') as post_count
-- FROM tv_user_profile
-- ORDER BY updated_at DESC
-- LIMIT 10;

-- Monitor staleness
-- SELECT * FROM monitor_tv_user_profile_staleness();

-- Verify consistency
-- SELECT * FROM verify_tv_user_profile_consistency();

-- Bulk refresh (e.g., after import)
-- SELECT * FROM refresh_tv_user_profile();

-- Refresh single user (e.g., after profile update)
-- SELECT refresh_tv_user_profile_for_user('550e8400-e29b-41d4-a716-446655440000'::UUID);
