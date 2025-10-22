-- Update order functions
-- App and core layers for order updates

-- App function: Update order (ultra-direct mutation)
CREATE OR REPLACE FUNCTION app.update_order(
    order_id UUID,
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_updated_data JSONB;
BEGIN
    -- Delegate to core business logic
    PERFORM core.update_order(
        order_id,
        input_payload->>'status',
        input_payload->>'payment_status',
        input_payload->>'fulfillment_status',
        input_payload->>'notes'
    );

    -- Get updated order data
    SELECT data INTO v_updated_data FROM tv_order WHERE id = order_id;

    -- Return ultra-direct response (Rust transformer handles formatting)
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        'Order updated successfully',
        jsonb_build_object('order', v_updated_data)
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Update order
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

    UPDATE orders
    SET
        status = COALESCE(new_status, status),
        payment_status = COALESCE(new_payment_status, payment_status),
        fulfillment_status = COALESCE(new_fulfillment_status, fulfillment_status),
        notes = COALESCE(new_notes, notes),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = order_id;

    -- Sync projection tables
    PERFORM app.sync_tv_order();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
