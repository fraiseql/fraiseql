-- Customer mutation functions (business logic)
-- Functions for customer management operations

-- Function to create a new customer
CREATE OR REPLACE FUNCTION create_customer(
    customer_email VARCHAR(255),
    customer_password_hash VARCHAR(255),
    customer_first_name VARCHAR(100),
    customer_last_name VARCHAR(100)
) RETURNS UUID AS $$
DECLARE
    new_customer_id UUID;
BEGIN
    -- Validate required fields
    IF customer_email IS NULL OR customer_password_hash IS NULL THEN
        RAISE EXCEPTION 'Email and password are required';
    END IF;

    -- Check if email already exists
    IF EXISTS (SELECT 1 FROM customers WHERE email = customer_email) THEN
        RAISE EXCEPTION 'Customer with email % already exists', customer_email;
    END IF;

    -- Create customer
    INSERT INTO customers (email, password_hash, first_name, last_name)
    VALUES (customer_email, customer_password_hash, customer_first_name, customer_last_name)
    RETURNING id INTO new_customer_id;

    RETURN new_customer_id;
END;
$$ LANGUAGE plpgsql;

-- Function to update customer information
CREATE OR REPLACE FUNCTION update_customer(
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

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
