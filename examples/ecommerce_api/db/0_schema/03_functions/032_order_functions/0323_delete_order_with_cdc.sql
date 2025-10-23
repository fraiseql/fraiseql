-- Delete order with CDC logging
-- Ultra-direct mutation response + Debezium-compatible event logging

-- App function: Delete order (ultra-direct + CDC)
CREATE OR REPLACE FUNCTION app.delete_order(
    order_id UUID
) RETURNS JSONB AS $$
DECLARE
    v_before_data JSONB;
    v_mutation_response JSONB;
BEGIN
    -- Get order data BEFORE deletion (for CDC event)
    SELECT data INTO v_before_data FROM tv_order WHERE id = order_id;

    IF v_before_data IS NULL THEN
        -- Order not found - return error immediately
        RETURN app.build_mutation_response(
            false,
            'NOT_FOUND',
            'Order not found',
            jsonb_build_object('order_id', order_id)
        );
    END IF;

    -- Delegate to core business logic (actual deletion)
    -- This will raise exception if order is not in 'pending' status
    PERFORM core.delete_order(order_id);

    -- Build ultra-direct response for client (snake_case, Rust transforms)
    v_mutation_response := app.build_mutation_response(
        true,
        'SUCCESS',
        'Order deleted successfully',
        jsonb_build_object(
            'order', v_before_data,
            'deleted_order_id', order_id
        )
    );

    -- Log CDC event ASYNCHRONOUSLY (doesn't block response)
    -- This is for Debezium/Kafka/event streaming
    PERFORM app.log_cdc_event(
        'ORDER_DELETED',                 -- event_type
        'order',                          -- entity_type
        order_id,                         -- entity_id
        'DELETE',                         -- operation
        v_before_data,                    -- before (full order with items)
        NULL,                             -- after (deleted, so NULL)
        jsonb_build_object(               -- metadata
            'deleted_at', NOW(),
            'deleted_by', current_user,
            'source', 'graphql_api',
            'order_status', v_before_data->>'status'
        )
    );

    -- Return response immediately (client doesn't wait for CDC logging)
    RETURN v_mutation_response;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

COMMENT ON FUNCTION app.delete_order IS
    'Delete order with ultra-direct response + CDC event logging.
    Client receives response immediately (PostgreSQL → Rust → Client).
    CDC event logged asynchronously for Debezium/Kafka streaming.
    Business rule: Only pending orders can be deleted.';


-- Core function: Delete order (business logic only)
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

    -- Delete order (cascade will handle order_items)
    DELETE FROM orders WHERE id = order_id;

    -- Sync projection tables (for read queries)
    PERFORM app.sync_tv_order();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION core.delete_order IS
    'Core business logic for order deletion. Called by app layer.';
