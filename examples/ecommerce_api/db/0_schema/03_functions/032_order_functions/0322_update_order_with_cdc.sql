-- Update order with CDC logging
-- Ultra-direct mutation response + Debezium-compatible event logging

-- App function: Update order (ultra-direct + CDC)
CREATE OR REPLACE FUNCTION app.update_order(
    order_id UUID,
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_before_data JSONB;
    v_after_data JSONB;
    v_mutation_response JSONB;
BEGIN
    -- Get order data BEFORE update (for CDC event)
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

    -- Delegate to core business logic (actual update)
    PERFORM core.update_order(
        order_id,
        input_payload->>'status',
        input_payload->>'payment_status',
        input_payload->>'fulfillment_status',
        input_payload->>'notes'
    );

    -- Get updated order data AFTER update
    SELECT data INTO v_after_data FROM tv_order WHERE id = order_id;

    -- Build ultra-direct response for client (snake_case, Rust transforms)
    v_mutation_response := app.build_mutation_response(
        true,
        'SUCCESS',
        'Order updated successfully',
        jsonb_build_object('order', v_after_data)
    );

    -- Log CDC event ASYNCHRONOUSLY (doesn't block response)
    -- This is for Debezium/Kafka/event streaming
    PERFORM app.log_cdc_event(
        'ORDER_UPDATED',                 -- event_type
        'order',                          -- entity_type
        order_id,                         -- entity_id
        'UPDATE',                         -- operation
        v_before_data,                    -- before (original state)
        v_after_data,                     -- after (updated state)
        jsonb_build_object(               -- metadata
            'updated_at', NOW(),
            'updated_by', current_user,
            'source', 'graphql_api',
            'fields_updated', input_payload,
            'status_changed', (v_before_data->>'status') != (v_after_data->>'status')
        )
    );

    -- Return response immediately (client doesn't wait for CDC logging)
    RETURN v_mutation_response;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

COMMENT ON FUNCTION app.update_order IS
    'Update order with ultra-direct response + CDC event logging.
    Client receives response immediately (PostgreSQL → Rust → Client).
    CDC event logged asynchronously for Debezium/Kafka streaming.';


-- Core function: Update order (business logic only)
CREATE OR REPLACE FUNCTION core.update_order(
    order_id UUID,
    new_status order_status DEFAULT NULL,
    new_payment_status payment_status DEFAULT NULL,
    new_fulfillment_status VARCHAR(50) DEFAULT NULL,
    new_notes TEXT DEFAULT NULL
) RETURNS BOOLEAN AS $$
DECLARE
    current_status order_status;
BEGIN
    -- Get current status for business rule validation
    SELECT status INTO current_status FROM orders WHERE id = order_id;

    -- Business rules for status transitions
    IF new_status IS NOT NULL AND current_status = 'delivered' AND new_status != 'refunded' THEN
        RAISE EXCEPTION 'Cannot change status of delivered orders (except refunds)';
    END IF;

    IF new_status IS NOT NULL AND current_status = 'cancelled' THEN
        RAISE EXCEPTION 'Cannot change status of cancelled orders';
    END IF;

    -- Business logic: Update only provided fields
    UPDATE orders
    SET
        status = COALESCE(new_status, status),
        payment_status = COALESCE(new_payment_status, payment_status),
        fulfillment_status = COALESCE(new_fulfillment_status, fulfillment_status),
        notes = COALESCE(new_notes, notes),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = order_id;

    -- Sync projection tables (for read queries)
    PERFORM app.sync_tv_order();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION core.update_order IS
    'Core business logic for order updates. Called by app layer.';
