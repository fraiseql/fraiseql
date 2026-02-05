-- ta_orders: Table-backed Arrow view for high-performance Arrow Flight queries
--
-- This table pre-computes and physically stores Arrow-optimized columnar data
-- for the orders entity. It's refreshed via trigger to stay in sync with tb_order.
--
-- Unlike logical views (va_orders), ta_orders is:
-- - A physical PostgreSQL table (stored on disk)
-- - Maintained via AFTER trigger (near real-time updates)
-- - Indexed with BRIN for fast range queries
-- - 10-100x faster for Arrow Flight queries on large tables

-- Create table with Arrow-compatible columns
CREATE TABLE IF NOT EXISTS ta_orders (
    -- Primary key (matches tb_order.id)
    id                  TEXT NOT NULL PRIMARY KEY,

    -- Extracted scalar columns (Arrow-compatible types)
    total               NUMERIC(10,2) NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL,
    customer_name       TEXT,

    -- Metadata for staleness tracking and ETL
    source_updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Ensure referential integrity
    CONSTRAINT fk_ta_orders_source
        FOREIGN KEY (id) REFERENCES tb_order(id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
);

-- BRIN index for time-series queries (1/1000th size of B-tree)
-- BRIN (Block Range Index) is optimal for time-ordered data
CREATE INDEX IF NOT EXISTS idx_ta_orders_created_at_brin
    ON ta_orders USING BRIN (created_at);

-- Refresh trigger: Keep ta_orders in sync with tb_order
-- This function is called AFTER each INSERT/UPDATE/DELETE on tb_order
CREATE OR REPLACE FUNCTION refresh_ta_orders_trigger()
RETURNS TRIGGER AS $$
BEGIN
    -- Handle INSERT and UPDATE: Add or update row in ta_orders
    IF (TG_OP = 'INSERT' OR TG_OP = 'UPDATE') AND NEW.deleted_at IS NULL THEN
        INSERT INTO ta_orders (
            id,
            total,
            created_at,
            customer_name,
            source_updated_at
        )
        VALUES (
            NEW.id,
            NEW.total,
            NEW.created_at,
            NEW.data->>'customer_name',
            NOW()
        )
        ON CONFLICT (id) DO UPDATE
        SET total = EXCLUDED.total,
            created_at = EXCLUDED.created_at,
            customer_name = EXCLUDED.customer_name,
            source_updated_at = NOW();

    -- Handle soft/hard deletes: Remove from ta_orders
    ELSIF (TG_OP = 'UPDATE' AND NEW.deleted_at IS NOT NULL) OR TG_OP = 'DELETE' THEN
        DELETE FROM ta_orders WHERE id = COALESCE(NEW.id, OLD.id);
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger: Fired after INSERT/UPDATE/DELETE on tb_order
CREATE TRIGGER IF NOT EXISTS trg_refresh_ta_orders
    AFTER INSERT OR UPDATE OR DELETE ON tb_order
    FOR EACH ROW
    EXECUTE FUNCTION refresh_ta_orders_trigger();

-- Command-based refresh function (idempotent)
-- Can be called manually: SELECT * FROM refresh_ta_orders();
-- Returns statistics: (rows_inserted, rows_updated, rows_deleted)
CREATE OR REPLACE FUNCTION refresh_ta_orders()
RETURNS TABLE(rows_inserted BIGINT, rows_updated BIGINT, rows_deleted BIGINT) AS $$
DECLARE
    v_inserted BIGINT := 0;
    v_updated BIGINT := 0;
    v_deleted BIGINT := 0;
BEGIN
    -- Upsert all non-deleted data from source table
    WITH upsert AS (
        INSERT INTO ta_orders (id, total, created_at, customer_name, source_updated_at)
        SELECT
            tb_order.id,
            tb_order.total,
            tb_order.created_at,
            tb_order.data->>'customer_name',
            NOW()
        FROM tb_order
        WHERE tb_order.deleted_at IS NULL
        ON CONFLICT (id) DO UPDATE
        SET total = EXCLUDED.total,
            created_at = EXCLUDED.created_at,
            customer_name = EXCLUDED.customer_name,
            source_updated_at = NOW()
        RETURNING (xmax = 0) AS inserted
    )
    SELECT COUNT(*) FILTER (WHERE inserted) INTO v_inserted FROM upsert;

    GET DIAGNOSTICS v_updated = ROW_COUNT;
    v_updated := v_updated - v_inserted;

    -- Delete orphaned rows (exist in ta_orders but not in source)
    WITH deleted AS (
        DELETE FROM ta_orders
        WHERE id NOT IN (SELECT id FROM tb_order WHERE deleted_at IS NULL)
        RETURNING 1
    )
    SELECT COUNT(*) INTO v_deleted FROM deleted;

    RETURN QUERY SELECT v_inserted, v_updated, v_deleted;
END;
$$ LANGUAGE plpgsql;

-- Initial population (only if table is empty)
-- This is useful for testing and initialization
INSERT INTO ta_orders (id, total, created_at, customer_name)
SELECT
    id,
    total,
    created_at,
    data->>'customer_name'
FROM tb_order
WHERE deleted_at IS NULL
ON CONFLICT (id) DO NOTHING;

-- Verify initial data
-- SELECT COUNT(*) as total_orders FROM ta_orders;
