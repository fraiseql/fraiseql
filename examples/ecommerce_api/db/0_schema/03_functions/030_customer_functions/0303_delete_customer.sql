-- Delete customer functions
-- App and core layers for customer deletion

-- App function: Delete customer (ultra-direct mutation)
CREATE OR REPLACE FUNCTION app.delete_customer(
    customer_id UUID
) RETURNS JSONB AS $$
DECLARE
    v_deleted_data JSONB;
BEGIN
    -- Get customer data before deletion
    SELECT data INTO v_deleted_data FROM tv_customer WHERE id = customer_id;

    -- Delegate to core business logic
    PERFORM core.delete_customer(customer_id);

    -- Return ultra-direct response (Rust transformer handles formatting)
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        'Customer deleted successfully',
        jsonb_build_object('customer', v_deleted_data)
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Delete customer
CREATE OR REPLACE FUNCTION core.delete_customer(customer_id UUID) RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM customers WHERE id = customer_id;

    -- Sync projection tables
    PERFORM app.sync_tv_customer();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
