-- Delete customer with CDC logging
-- Ultra-direct mutation response + Debezium-compatible event logging

-- App function: Delete customer (ultra-direct + CDC)
CREATE OR REPLACE FUNCTION app.delete_customer(
    customer_id UUID
) RETURNS JSONB AS $$
DECLARE
    v_before_data JSONB;
    v_mutation_response JSONB;
BEGIN
    -- Get customer data BEFORE deletion (for CDC event)
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

    -- Delegate to core business logic (actual deletion)
    PERFORM core.delete_customer(customer_id);

    -- Build ultra-direct response for client (snake_case, Rust transforms)
    v_mutation_response := app.build_mutation_response(
        true,
        'SUCCESS',
        'Customer deleted successfully',
        jsonb_build_object(
            'customer', v_before_data,
            'deleted_customer_id', customer_id
        )
    );

    -- Log CDC event ASYNCHRONOUSLY (doesn't block response)
    -- This is for Debezium/Kafka/event streaming
    PERFORM app.log_cdc_event(
        'CUSTOMER_DELETED',              -- event_type
        'customer',                       -- entity_type
        customer_id,                      -- entity_id
        'DELETE',                         -- operation
        v_before_data,                    -- before (full entity)
        NULL,                             -- after (deleted, so NULL)
        jsonb_build_object(               -- metadata
            'deleted_at', NOW(),
            'deleted_by', current_user
        )
    );

    -- Return response immediately (client doesn't wait for CDC logging)
    RETURN v_mutation_response;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

COMMENT ON FUNCTION app.delete_customer IS
    'Delete customer with ultra-direct response + CDC event logging.
    Client receives response immediately (PostgreSQL → Rust → Client).
    CDC event logged asynchronously for Debezium/Kafka streaming.';


-- Core function: Delete customer (business logic only)
CREATE OR REPLACE FUNCTION core.delete_customer(customer_id UUID)
RETURNS BOOLEAN AS $$
BEGIN
    -- Delete from source table
    DELETE FROM customers WHERE id = customer_id;

    -- Sync projection tables (for read queries)
    PERFORM app.sync_tv_customer();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION core.delete_customer IS
    'Core business logic for customer deletion. Called by app layer.';
