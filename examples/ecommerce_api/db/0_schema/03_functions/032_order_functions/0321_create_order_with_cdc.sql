-- Create order with CDC logging
-- Ultra-direct mutation response + Debezium-compatible event logging

-- App function: Create order (ultra-direct + CDC)
CREATE OR REPLACE FUNCTION app.create_order(
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_order_id UUID;
    v_after_data JSONB;
    v_mutation_response JSONB;
BEGIN
    -- Delegate to core business logic (actual creation)
    v_order_id := core.create_order(
        (input_payload->>'customer_id')::UUID,
        (input_payload->>'items')::JSONB,
        (input_payload->>'shipping_address_id')::UUID,
        (input_payload->>'billing_address_id')::UUID,
        input_payload->>'notes'
    );

    -- Get complete order data from projection table (for response + CDC)
    SELECT data INTO v_after_data FROM tv_order WHERE id = v_order_id;

    -- Build ultra-direct response for client (snake_case, Rust transforms)
    v_mutation_response := app.build_mutation_response(
        true,
        'SUCCESS',
        'Order created successfully',
        jsonb_build_object('order', v_after_data)
    );

    -- Log CDC event ASYNCHRONOUSLY (doesn't block response)
    -- This is for Debezium/Kafka/event streaming
    PERFORM app.log_cdc_event(
        'ORDER_CREATED',                 -- event_type
        'order',                          -- entity_type
        v_order_id,                       -- entity_id
        'CREATE',                         -- operation
        NULL,                             -- before (new entity, so NULL)
        v_after_data,                     -- after (full order with items)
        jsonb_build_object(               -- metadata
            'created_at', NOW(),
            'created_by', current_user,
            'source', 'graphql_api',
            'customer_id', input_payload->>'customer_id',
            'item_count', jsonb_array_length(input_payload->'items')
        )
    );

    -- Return response immediately (client doesn't wait for CDC logging)
    RETURN v_mutation_response;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

COMMENT ON FUNCTION app.create_order IS
    'Create order with ultra-direct response + CDC event logging.
    Client receives response immediately (PostgreSQL → Rust → Client).
    CDC event logged asynchronously for Debezium/Kafka streaming.';


-- Core function: Create order (business logic only)
CREATE OR REPLACE FUNCTION core.create_order(
    order_customer_id UUID,
    order_items JSONB,
    order_shipping_address_id UUID DEFAULT NULL,
    order_billing_address_id UUID DEFAULT NULL,
    order_notes TEXT DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    new_order_id UUID;
    order_number VARCHAR(50);
    subtotal DECIMAL(10, 2) := 0;
    item_record JSONB;
    product_price DECIMAL(10, 2);
BEGIN
    -- Business logic validation
    IF order_customer_id IS NULL THEN
        RAISE EXCEPTION 'Customer ID is required';
    END IF;

    IF order_items IS NULL OR jsonb_array_length(order_items) = 0 THEN
        RAISE EXCEPTION 'Order must contain at least one item';
    END IF;

    -- Generate order number
    order_number := 'ORD-' || to_char(CURRENT_TIMESTAMP, 'YYYYMMDD') || '-' || lpad(nextval('order_seq')::text, 6, '0');

    -- Calculate subtotal from items
    FOR item_record IN SELECT * FROM jsonb_array_elements(order_items)
    LOOP
        -- In a real system, you'd look up product prices
        -- For now, assume price is provided in the item
        subtotal := subtotal + (item_record->>'quantity')::integer * (item_record->>'price')::decimal;
    END LOOP;

    -- Generate UUID and create order
    new_order_id := gen_random_uuid();

    INSERT INTO orders (id, order_number, customer_id, subtotal, total_amount, shipping_address_id, billing_address_id, notes)
    VALUES (new_order_id, order_number, order_customer_id, subtotal, subtotal, order_shipping_address_id, order_billing_address_id, order_notes);

    -- Create order items
    FOR item_record IN SELECT * FROM jsonb_array_elements(order_items)
    LOOP
        INSERT INTO order_items (order_id, variant_id, quantity, unit_price, total_price)
        VALUES (
            new_order_id,
            (item_record->>'variant_id')::UUID,
            (item_record->>'quantity')::integer,
            (item_record->>'price')::decimal,
            (item_record->>'quantity')::integer * (item_record->>'price')::decimal
        );
    END LOOP;

    -- Sync projection tables (for read queries)
    PERFORM app.sync_tv_order();

    RETURN new_order_id;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION core.create_order IS
    'Core business logic for order creation. Called by app layer.';
