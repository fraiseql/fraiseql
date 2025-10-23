-- Customer sync functions (App Layer)
-- Explicit synchronization of projection tables

-- Sync tv_customer table from v_customer view
CREATE OR REPLACE FUNCTION app.sync_tv_customer() RETURNS VOID AS $$
BEGIN
    -- Clear and repopulate tv_customer
    DELETE FROM tv_customer;
    INSERT INTO tv_customer (id, data)
    SELECT id, data FROM v_customer;
END;
$$ LANGUAGE plpgsql;

-- Sync single customer in tv_customer
CREATE OR REPLACE FUNCTION app.sync_tv_customer_single(customer_id UUID) RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_customer WHERE id = customer_id;
    INSERT INTO tv_customer (id, data)
    SELECT id, data FROM v_customer WHERE id = customer_id;
END;
$$ LANGUAGE plpgsql;
