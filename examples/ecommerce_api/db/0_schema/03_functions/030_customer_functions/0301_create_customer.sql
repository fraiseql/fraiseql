-- Create customer functions
-- App and core layers for customer creation

-- App function: Create customer (ultra-direct mutation)
CREATE OR REPLACE FUNCTION app.create_customer(
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_customer_id UUID;
BEGIN
    -- Delegate to core business logic
    v_customer_id := core.create_customer(
        input_payload->>'email',
        input_payload->>'password_hash',
        input_payload->>'first_name',
        input_payload->>'last_name'
    );

    -- Return ultra-direct response (Rust transformer handles formatting)
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        'Customer created successfully',
        jsonb_build_object(
            'customer', jsonb_build_object(
                'id', v_customer_id,
                'email', input_payload->>'email',
                'first_name', input_payload->>'first_name',
                'last_name', input_payload->>'last_name'
            )
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Create customer
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
    VALUES (new_customer_id, customer_email, customer_password_hash, customer_first_name, customer_last_name);

    -- Sync projection tables (explicit sync)
    PERFORM app.sync_tv_customer();

    RETURN new_customer_id;
END;
$$ LANGUAGE plpgsql;
