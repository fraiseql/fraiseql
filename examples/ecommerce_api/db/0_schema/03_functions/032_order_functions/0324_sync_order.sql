-- Order sync functions (App Layer)
-- Explicit synchronization of projection tables

-- Sync tv_order table from v_order view
CREATE OR REPLACE FUNCTION app.sync_tv_order() RETURNS VOID AS $$
BEGIN
    -- Clear and repopulate tv_order
    DELETE FROM tv_order;
    INSERT INTO tv_order (id, data)
    SELECT id, data FROM v_order;
END;
$$ LANGUAGE plpgsql;

-- Sync single order in tv_order
CREATE OR REPLACE FUNCTION app.sync_tv_order_single(order_id UUID) RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_order WHERE id = order_id;
    INSERT INTO tv_order (id, data)
    SELECT id, data FROM v_order WHERE id = order_id;
END;
$$ LANGUAGE plpgsql;
