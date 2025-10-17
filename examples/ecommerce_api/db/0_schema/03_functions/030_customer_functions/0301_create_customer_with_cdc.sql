-- Create customer with CDC logging
-- Ultra-direct mutation response + Debezium-compatible event logging

-- App function: Create customer (ultra-direct + CDC)
CREATE OR REPLACE FUNCTION app.create_customer(
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_customer_id UUID;
    v_after_data JSONB;
    v_mutation_response JSONB;
BEGIN
    -- Delegate to core business logic (actual creation)
    v_customer_id := core.create_customer(
        input_payload->>'email',
        input_payload->>'password_hash',
        input_payload->>'first_name',
        input_payload->>'last_name'
    );

    -- Build customer data for response
    v_after_data := jsonb_build_object(
        'id', v_customer_id,
        'email', input_payload->>'email',
        'first_name', input_payload->>'first_name',
        'last_name', input_payload->>'last_name',
        'created_at', NOW()
    );

    -- Build ultra-direct response for client (snake_case, Rust transforms)
    v_mutation_response := app.build_mutation_response(
        true,
        'SUCCESS',
        'Customer created successfully',
        jsonb_build_object('customer', v_after_data)
    );

    -- Log CDC event ASYNCHRONOUSLY (doesn't block response)
    -- This is for Debezium/Kafka/event streaming
    PERFORM app.log_cdc_event(
        'CUSTOMER_CREATED',              -- event_type
        'customer',                       -- entity_type
        v_customer_id,                    -- entity_id
        'CREATE',                         -- operation
        NULL,                             -- before (new entity, so NULL)
        v_after_data,                     -- after (full entity)
        jsonb_build_object(               -- metadata
            'created_at', NOW(),
            'created_by', current_user,
            'source', 'graphql_api'
        )
    );

    -- Return response immediately (client doesn't wait for CDC logging)
    RETURN v_mutation_response;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

COMMENT ON FUNCTION app.create_customer IS
    'Create customer with ultra-direct response + CDC event logging.
    Client receives response immediately (PostgreSQL → Rust → Client).
    CDC event logged asynchronously for Debezium/Kafka streaming.';


-- Core function: Create customer (business logic only)
CREATE OR REPLACE FUNCTION core.create_customer(
    customer_email VARCHAR(255),
    customer_password_hash VARCHAR(255),
    customer_first_name VARCHAR(100) DEFAULT NULL,
    customer_last_name VARCHAR(100) DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    new_customer_id UUID;
BEGIN
    -- Business logic validation
    IF customer_email IS NULL OR customer_password_hash IS NULL THEN
        RAISE EXCEPTION 'Email and password are required';
    END IF;

    -- Check duplicate email (business rule)
    IF EXISTS (SELECT 1 FROM customers WHERE email = customer_email) THEN
        RAISE EXCEPTION 'Customer with email % already exists', customer_email;
    END IF;

    -- Generate UUID and create customer
    new_customer_id := gen_random_uuid();

    INSERT INTO customers (id, email, password_hash, first_name, last_name)
    VALUES (
        new_customer_id,
        customer_email,
        customer_password_hash,
        customer_first_name,
        customer_last_name
    );

    -- Sync projection tables (for read queries)
    PERFORM app.sync_tv_customer();

    RETURN new_customer_id;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION core.create_customer IS
    'Core business logic for customer creation. Called by app layer.';
