-- Product sync functions (App Layer)
-- Explicit synchronization of projection tables

-- Sync tv_product table from v_product view
CREATE OR REPLACE FUNCTION app.sync_tv_product() RETURNS VOID AS $$
BEGIN
    -- Clear and repopulate tv_product
    DELETE FROM tv_product;
    INSERT INTO tv_product (id, data)
    SELECT id, data FROM v_product;
END;
$$ LANGUAGE plpgsql;

-- Sync single product in tv_product
CREATE OR REPLACE FUNCTION app.sync_tv_product_single(product_id UUID) RETURNS VOID AS $$
BEGIN
    DELETE FROM tv_product WHERE id = product_id;
    INSERT INTO tv_product (id, data)
    SELECT id, data FROM v_product WHERE id = product_id;
END;
$$ LANGUAGE plpgsql;
