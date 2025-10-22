-- Update customer functions
-- App and core layers for customer updates

-- App function: Update customer (ultra-direct mutation)
CREATE OR REPLACE FUNCTION app.update_customer(
    customer_id UUID,
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_updated_data JSONB;
BEGIN
    -- Delegate to core business logic
    PERFORM core.update_customer(
        customer_id,
        input_payload->>'first_name',
        input_payload->>'last_name',
        input_payload->>'phone'
    );

    -- Get updated customer data
    SELECT data INTO v_updated_data FROM tv_customer WHERE id = customer_id;

    -- Return ultra-direct response (Rust transformer handles formatting)
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        'Customer updated successfully',
        jsonb_build_object('customer', v_updated_data)
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Update customer
CREATE OR REPLACE FUNCTION core.update_customer(
    customer_id UUID,
    new_first_name VARCHAR(100) DEFAULT NULL,
    new_last_name VARCHAR(100) DEFAULT NULL,
    new_phone VARCHAR(50) DEFAULT NULL
) RETURNS BOOLEAN AS $$
BEGIN
    UPDATE customers
    SET
        first_name = COALESCE(new_first_name, first_name),
        last_name = COALESCE(new_last_name, last_name),
        phone = COALESCE(new_phone, phone),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = customer_id;

    -- Sync projection tables
    PERFORM app.sync_tv_customer();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
