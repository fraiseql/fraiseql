-- Delete product functions
-- App and core layers for product deletion

-- App function: Delete product
CREATE OR REPLACE FUNCTION app.delete_product(
    product_id UUID
) RETURNS mutation_response AS $$
DECLARE
    v_deleted_data JSONB;
BEGIN
    -- Get product data before deletion
    SELECT data INTO v_deleted_data FROM tv_product WHERE id = product_id;

    -- Delegate to core business logic
    PERFORM core.delete_product(product_id);

    RETURN app.build_mutation_response(
        'deleted',
        'Product deleted successfully',
        v_deleted_data,
        'Product',
        product_id::text
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Delete product
CREATE OR REPLACE FUNCTION core.delete_product(product_id UUID) RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM products WHERE id = product_id;

    -- Sync projection tables
    PERFORM app.sync_tv_product();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
