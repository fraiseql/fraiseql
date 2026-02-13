-- ta_users: Table-backed Arrow view for high-performance Arrow Flight queries
--
-- This table pre-computes and physically stores Arrow-optimized columnar data
-- for the users entity. It's refreshed via trigger to stay in sync with tb_user.
--
-- Unlike logical views (va_users), ta_users is:
-- - A physical PostgreSQL table (stored on disk)
-- - Maintained via AFTER trigger (near real-time updates)
-- - Indexed with BRIN for fast range queries
-- - 10-100x faster for Arrow Flight queries on large tables

-- Create table with Arrow-compatible columns
CREATE TABLE IF NOT EXISTS ta_users (
    -- Primary key (matches tb_user.id)
    id                  TEXT NOT NULL PRIMARY KEY,

    -- Extracted scalar columns (Arrow-compatible types)
    email               TEXT NOT NULL,
    name                TEXT,
    created_at          TIMESTAMPTZ NOT NULL,

    -- Metadata for staleness tracking and ETL
    source_updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Ensure referential integrity
    CONSTRAINT fk_ta_users_source
        FOREIGN KEY (id) REFERENCES tb_user(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
);

-- BRIN index for time-series queries (1/1000th size of B-tree)
-- BRIN (Block Range Index) is optimal for time-ordered data
CREATE INDEX IF NOT EXISTS idx_ta_users_created_at_brin
    ON ta_users USING BRIN (created_at);

-- Refresh trigger: Keep ta_users in sync with tb_user
-- This function is called AFTER each INSERT/UPDATE/DELETE on tb_user
CREATE OR REPLACE FUNCTION refresh_ta_users_trigger()
RETURNS TRIGGER AS $$
BEGIN
    -- Handle INSERT and UPDATE: Add or update row in ta_users
    IF (TG_OP = 'INSERT' OR TG_OP = 'UPDATE') AND NEW.deleted_at IS NULL THEN
        INSERT INTO ta_users (
            id,
            email,
            name,
            created_at,
            source_updated_at
        )
        VALUES (
            NEW.id,
            NEW.email,
            NEW.data->>'name',
            NEW.created_at,
            NOW()
        )
        ON CONFLICT (id) DO UPDATE
        SET email = EXCLUDED.email,
            name = EXCLUDED.name,
            created_at = EXCLUDED.created_at,
            source_updated_at = NOW();

    -- Handle soft/hard deletes: Remove from ta_users
    ELSIF (TG_OP = 'UPDATE' AND NEW.deleted_at IS NOT NULL) OR TG_OP = 'DELETE' THEN
        DELETE FROM ta_users WHERE id = COALESCE(NEW.id, OLD.id);
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger: Fired after INSERT/UPDATE/DELETE on tb_user
CREATE TRIGGER IF NOT EXISTS trg_refresh_ta_users
    AFTER INSERT OR UPDATE OR DELETE ON tb_user
    FOR EACH ROW
    EXECUTE FUNCTION refresh_ta_users_trigger();

-- Command-based refresh function (idempotent)
-- Can be called manually: SELECT * FROM refresh_ta_users();
-- Returns statistics: (rows_inserted, rows_updated, rows_deleted)
CREATE OR REPLACE FUNCTION refresh_ta_users()
RETURNS TABLE(rows_inserted BIGINT, rows_updated BIGINT, rows_deleted BIGINT) AS $$
DECLARE
    v_inserted BIGINT := 0;
    v_updated BIGINT := 0;
    v_deleted BIGINT := 0;
BEGIN
    -- Upsert all non-deleted data from source table
    WITH upsert AS (
        INSERT INTO ta_users (id, email, name, created_at, source_updated_at)
        SELECT
            tb_user.id,
            tb_user.email,
            tb_user.data->>'name',
            tb_user.created_at,
            NOW()
        FROM tb_user
        WHERE tb_user.deleted_at IS NULL
        ON CONFLICT (id) DO UPDATE
        SET email = EXCLUDED.email,
            name = EXCLUDED.name,
            created_at = EXCLUDED.created_at,
            source_updated_at = NOW()
        RETURNING (xmax = 0) AS inserted
    )
    SELECT COUNT(*) FILTER (WHERE inserted) INTO v_inserted FROM upsert;

    GET DIAGNOSTICS v_updated = ROW_COUNT;
    v_updated := v_updated - v_inserted;

    -- Delete orphaned rows (exist in ta_users but not in source)
    WITH deleted AS (
        DELETE FROM ta_users
        WHERE id NOT IN (SELECT id FROM tb_user WHERE deleted_at IS NULL)
        RETURNING 1
    )
    SELECT COUNT(*) INTO v_deleted FROM deleted;

    RETURN QUERY SELECT v_inserted, v_updated, v_deleted;
END;
$$ LANGUAGE plpgsql;

-- Initial population (only if table is empty)
-- This is useful for testing and initialization
INSERT INTO ta_users (id, email, name, created_at)
SELECT
    id,
    email,
    data->>'name',
    created_at
FROM tb_user
WHERE deleted_at IS NULL
ON CONFLICT (id) DO NOTHING;

-- Verify initial data
-- SELECT COUNT(*) as total_users FROM ta_users;
