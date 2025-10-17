-- Update customer with CDC logging
-- Ultra-direct mutation response + Debezium-compatible event logging

-- App function: Update customer (ultra-direct + CDC)
CREATE OR REPLACE FUNCTION app.update_customer(
    customer_id UUID,
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_before_data JSONB;
    v_after_data JSONB;
    v_mutation_response JSONB;
BEGIN
    -- Get customer data BEFORE update (for CDC event)
    SELECT data INTO v_before_data FROM tv_customer WHERE id = customer_id;

    IF v_before_data IS NULL THEN
        -- Customer not found - return error immediately
        RETURN app.build_mutation_response(
            false,
            'NOT_FOUND',
            'Customer not found',
            jsonb_build_object('customer_id', customer_id)
        );
    END IF;

    -- Delegate to core business logic (actual update)
    PERFORM core.update_customer(
        customer_id,
        input_payload->>'first_name',
        input_payload->>'last_name',
        input_payload->>'phone'
    );

    -- Get updated customer data AFTER update
    SELECT data INTO v_after_data FROM tv_customer WHERE id = customer_id;

    -- Build ultra-direct response for client (snake_case, Rust transforms)
    v_mutation_response := app.build_mutation_response(
        true,
        'SUCCESS',
        'Customer updated successfully',
        jsonb_build_object('customer', v_after_data)
    );

    -- Log CDC event ASYNCHRONOUSLY (doesn't block response)
    -- This is for Debezium/Kafka/event streaming
    PERFORM app.log_cdc_event(
        'CUSTOMER_UPDATED',              -- event_type
        'customer',                       -- entity_type
        customer_id,                      -- entity_id
        'UPDATE',                         -- operation
        v_before_data,                    -- before (original state)
        v_after_data,                     -- after (updated state)
        jsonb_build_object(               -- metadata
            'updated_at', NOW(),
            'updated_by', current_user,
            'source', 'graphql_api',
            'fields_updated', input_payload
        )
    );

    -- Return response immediately (client doesn't wait for CDC logging)
    RETURN v_mutation_response;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

COMMENT ON FUNCTION app.update_customer IS
    'Update customer with ultra-direct response + CDC event logging.
    Client receives response immediately (PostgreSQL → Rust → Client).
    CDC event logged asynchronously for Debezium/Kafka streaming.';


-- Core function: Update customer (business logic only)
CREATE OR REPLACE FUNCTION core.update_customer(
    customer_id UUID,
    new_first_name VARCHAR(100) DEFAULT NULL,
    new_last_name VARCHAR(100) DEFAULT NULL,
    new_phone VARCHAR(50) DEFAULT NULL
) RETURNS BOOLEAN AS $$
BEGIN
    -- Business logic: Update only provided fields
    UPDATE customers
    SET
        first_name = COALESCE(new_first_name, first_name),
        last_name = COALESCE(new_last_name, last_name),
        phone = COALESCE(new_phone, phone),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = customer_id;

    -- Sync projection tables (for read queries)
    PERFORM app.sync_tv_customer();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION core.update_customer IS
    'Core business logic for customer updates. Called by app layer.';
