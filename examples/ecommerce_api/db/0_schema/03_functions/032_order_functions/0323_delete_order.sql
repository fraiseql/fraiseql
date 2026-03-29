-- Delete order functions
-- App and core layers for order deletion

-- App function: Delete order
CREATE OR REPLACE FUNCTION app.delete_order(
    order_id UUID
) RETURNS mutation_response AS $$
DECLARE
    v_deleted_data JSONB;
BEGIN
    -- Get order data before deletion
    SELECT data INTO v_deleted_data FROM tv_order WHERE id = order_id;

    -- Delegate to core business logic
    PERFORM core.delete_order(order_id);

    RETURN app.build_mutation_response(
        'deleted',
        'Order deleted successfully',
        v_deleted_data,
        'Order',
        order_id::text
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Delete order
CREATE OR REPLACE FUNCTION core.delete_order(order_id UUID) RETURNS BOOLEAN AS $$
DECLARE
    current_status order_status;
BEGIN
    -- Get current status for business rule validation
    SELECT status INTO current_status FROM orders WHERE id = order_id;

    -- Business rule: only allow deletion of pending orders
    IF current_status != 'pending' THEN
        RAISE EXCEPTION 'Can only delete orders with pending status';
    END IF;

    DELETE FROM orders WHERE id = order_id;

    -- Sync projection tables
    PERFORM app.sync_tv_order();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
