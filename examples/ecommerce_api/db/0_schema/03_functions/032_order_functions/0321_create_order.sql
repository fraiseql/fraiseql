-- Create order functions
-- App and core layers for order creation

-- App function: Create order (ultra-direct mutation)
CREATE OR REPLACE FUNCTION app.create_order(
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_order_id UUID;
BEGIN
    -- Delegate to core business logic
    v_order_id := core.create_order(
        (input_payload->>'customer_id')::UUID,
        (input_payload->>'items')::JSONB,
        (input_payload->>'shipping_address_id')::UUID,
        (input_payload->>'billing_address_id')::UUID,
        input_payload->>'notes'
    );

    -- Return ultra-direct response (Rust transformer handles formatting)
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        'Order created successfully',
        jsonb_build_object(
            'order', jsonb_build_object(
                'id', v_order_id,
                'customer_id', input_payload->>'customer_id',
                'status', 'pending'
            )
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Create order
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

    -- Sync projection tables (explicit sync)
    PERFORM app.sync_tv_order();

    RETURN new_order_id;
END;
$$ LANGUAGE plpgsql;
