-- Order Management Functions for E-commerce API
-- CQRS pattern: Functions for mutations

-- Create order from cart
CREATE OR REPLACE FUNCTION create_order_from_cart(
    p_cart_id UUID,
    p_customer_id UUID,
    p_shipping_address_id UUID,
    p_billing_address_id UUID DEFAULT NULL,
    p_payment_method JSONB DEFAULT NULL,
    p_notes TEXT DEFAULT NULL
) RETURNS JSON AS $$
DECLARE
    v_order_id UUID;
    v_order_number VARCHAR(50);
    v_subtotal DECIMAL(10, 2) := 0;
    v_tax_amount DECIMAL(10, 2) := 0;
    v_shipping_amount DECIMAL(10, 2) := 0;
    v_discount_amount DECIMAL(10, 2) := 0;
    v_total_amount DECIMAL(10, 2);
    v_cart_metadata JSONB;
    v_item RECORD;
BEGIN
    -- Verify cart ownership and has items
    SELECT metadata INTO v_cart_metadata
    FROM carts
    WHERE id = p_cart_id
    AND customer_id = p_customer_id
    AND status = 'active'
    AND expires_at > CURRENT_TIMESTAMP
    AND EXISTS (SELECT 1 FROM cart_items WHERE cart_id = p_cart_id);

    IF v_cart_metadata IS NULL THEN
        RAISE EXCEPTION 'Cart not found, empty, or access denied';
    END IF;

    -- Generate order number
    v_order_number := 'ORD-' || TO_CHAR(CURRENT_TIMESTAMP, 'YYYYMMDD') || '-' ||
                      LPAD(nextval('pg_catalog.pg_sequence'::regclass)::TEXT, 6, '0');

    -- Calculate subtotal
    SELECT SUM(quantity * price_at_time) INTO v_subtotal
    FROM cart_items
    WHERE cart_id = p_cart_id;

    -- Get discount from coupon if applied
    IF v_cart_metadata ? 'coupon' THEN
        v_discount_amount := (v_cart_metadata->'coupon'->>'discount_amount')::DECIMAL(10, 2);
    END IF;

    -- Calculate tax (simplified - 10% for demo)
    v_tax_amount := (v_subtotal - v_discount_amount) * 0.10;

    -- Calculate shipping (simplified - flat rate for demo)
    v_shipping_amount := CASE
        WHEN v_subtotal >= 100 THEN 0  -- Free shipping over $100
        ELSE 10.00
    END;

    -- Calculate total
    v_total_amount := v_subtotal - v_discount_amount + v_tax_amount + v_shipping_amount;

    -- Create order
    INSERT INTO orders (
        order_number,
        customer_id,
        status,
        subtotal,
        tax_amount,
        shipping_amount,
        discount_amount,
        total_amount,
        shipping_address_id,
        billing_address_id,
        notes,
        metadata
    ) VALUES (
        v_order_number,
        p_customer_id,
        'pending',
        v_subtotal,
        v_tax_amount,
        v_shipping_amount,
        v_discount_amount,
        v_total_amount,
        p_shipping_address_id,
        COALESCE(p_billing_address_id, p_shipping_address_id),
        p_notes,
        json_build_object(
            'payment_method', p_payment_method,
            'coupon', v_cart_metadata->'coupon',
            'cart_id', p_cart_id
        )::jsonb
    ) RETURNING id INTO v_order_id;

    -- Create order items and reserve inventory
    FOR v_item IN
        SELECT ci.*, pv.price as current_price
        FROM cart_items ci
        JOIN product_variants pv ON ci.variant_id = pv.id
        WHERE ci.cart_id = p_cart_id
    LOOP
        -- Insert order item
        INSERT INTO order_items (
            order_id,
            variant_id,
            quantity,
            unit_price,
            total_price
        ) VALUES (
            v_order_id,
            v_item.variant_id,
            v_item.quantity,
            v_item.price_at_time,
            v_item.quantity * v_item.price_at_time
        );

        -- Reserve inventory
        UPDATE inventory
        SET reserved_quantity = reserved_quantity + v_item.quantity
        WHERE variant_id = v_item.variant_id
        AND quantity - reserved_quantity >= v_item.quantity;

        IF NOT FOUND THEN
            RAISE EXCEPTION 'Insufficient inventory for variant %', v_item.variant_id;
        END IF;
    END LOOP;

    -- Update coupon usage if applied
    IF v_cart_metadata ? 'coupon' THEN
        UPDATE coupons
        SET usage_count = usage_count + 1
        WHERE code = v_cart_metadata->'coupon'->>'code';
    END IF;

    -- Mark cart as converted
    UPDATE carts
    SET status = 'converted',
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_cart_id;

    -- Return order details
    RETURN json_build_object(
        'success', true,
        'order_id', v_order_id,
        'order_number', v_order_number,
        'total_amount', v_total_amount,
        'message', 'Order created successfully',
        'order', (
            SELECT row_to_json(order_detail.*)
            FROM order_detail
            WHERE id = v_order_id
        )
    );
EXCEPTION
    WHEN OTHERS THEN
        -- Rollback will happen automatically
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Update order status
CREATE OR REPLACE FUNCTION update_order_status(
    p_order_id UUID,
    p_status VARCHAR,
    p_notes TEXT DEFAULT NULL
) RETURNS JSON AS $$
DECLARE
    v_old_status VARCHAR;
    v_customer_id UUID;
BEGIN
    -- Get current status
    SELECT status, customer_id INTO v_old_status, v_customer_id
    FROM orders
    WHERE id = p_order_id;

    IF v_old_status IS NULL THEN
        RAISE EXCEPTION 'Order not found';
    END IF;

    -- Validate status transition
    IF v_old_status = 'cancelled' OR v_old_status = 'completed' THEN
        RAISE EXCEPTION 'Cannot update status of % order', v_old_status;
    END IF;

    -- Update order status
    UPDATE orders
    SET status = p_status,
        updated_at = CURRENT_TIMESTAMP,
        metadata = jsonb_set(
            COALESCE(metadata, '{}'::jsonb),
            '{status_history}',
            COALESCE(metadata->'status_history', '[]'::jsonb) ||
            json_build_object(
                'from', v_old_status,
                'to', p_status,
                'timestamp', CURRENT_TIMESTAMP,
                'notes', p_notes
            )::jsonb
        )
    WHERE id = p_order_id;

    -- Handle inventory based on status
    IF p_status = 'cancelled' THEN
        -- Release reserved inventory
        UPDATE inventory i
        SET reserved_quantity = i.reserved_quantity - oi.quantity
        FROM order_items oi
        WHERE oi.order_id = p_order_id
        AND i.variant_id = oi.variant_id;
    ELSIF p_status = 'shipped' THEN
        -- Convert reserved to sold
        UPDATE inventory i
        SET quantity = i.quantity - oi.quantity,
            reserved_quantity = i.reserved_quantity - oi.quantity
        FROM order_items oi
        WHERE oi.order_id = p_order_id
        AND i.variant_id = oi.variant_id;
    END IF;

    RETURN json_build_object(
        'success', true,
        'message', 'Order status updated',
        'order_id', p_order_id,
        'old_status', v_old_status,
        'new_status', p_status
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Process payment
CREATE OR REPLACE FUNCTION process_order_payment(
    p_order_id UUID,
    p_payment_details JSONB
) RETURNS JSON AS $$
DECLARE
    v_order RECORD;
    v_payment_id VARCHAR;
BEGIN
    -- Get order details
    SELECT * INTO v_order
    FROM orders
    WHERE id = p_order_id
    AND payment_status != 'paid';

    IF v_order IS NULL THEN
        RAISE EXCEPTION 'Order not found or already paid';
    END IF;

    -- Simulate payment processing
    -- In real implementation, this would integrate with payment gateway
    v_payment_id := 'PAY-' || uuid_generate_v4()::TEXT;

    -- Update order payment status
    UPDATE orders
    SET payment_status = 'paid',
        status = CASE
            WHEN status = 'pending' THEN 'processing'
            ELSE status
        END,
        metadata = jsonb_set(
            COALESCE(metadata, '{}'::jsonb),
            '{payment}',
            json_build_object(
                'payment_id', v_payment_id,
                'method', p_payment_details->>'method',
                'amount', v_order.total_amount,
                'currency', v_order.currency_code,
                'processed_at', CURRENT_TIMESTAMP,
                'details', p_payment_details
            )::jsonb
        ),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_order_id;

    RETURN json_build_object(
        'success', true,
        'message', 'Payment processed successfully',
        'payment_id', v_payment_id,
        'order_id', p_order_id,
        'amount', v_order.total_amount
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Cancel order
CREATE OR REPLACE FUNCTION cancel_order(
    p_order_id UUID,
    p_customer_id UUID,
    p_reason TEXT
) RETURNS JSON AS $$
DECLARE
    v_order RECORD;
BEGIN
    -- Get order details
    SELECT * INTO v_order
    FROM orders
    WHERE id = p_order_id
    AND customer_id = p_customer_id
    AND status NOT IN ('shipped', 'delivered', 'completed', 'cancelled');

    IF v_order IS NULL THEN
        RAISE EXCEPTION 'Order not found or cannot be cancelled';
    END IF;

    -- Update order status
    UPDATE orders
    SET status = 'cancelled',
        metadata = jsonb_set(
            COALESCE(metadata, '{}'::jsonb),
            '{cancellation}',
            json_build_object(
                'reason', p_reason,
                'cancelled_at', CURRENT_TIMESTAMP,
                'cancelled_by', 'customer'
            )::jsonb
        ),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_order_id;

    -- Release reserved inventory
    UPDATE inventory i
    SET reserved_quantity = i.reserved_quantity - oi.quantity
    FROM order_items oi
    WHERE oi.order_id = p_order_id
    AND i.variant_id = oi.variant_id;

    -- Process refund if payment was made
    IF v_order.payment_status = 'paid' THEN
        UPDATE orders
        SET payment_status = 'refund_pending'
        WHERE id = p_order_id;
    END IF;

    RETURN json_build_object(
        'success', true,
        'message', 'Order cancelled successfully',
        'order_id', p_order_id,
        'refund_pending', v_order.payment_status = 'paid'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;
