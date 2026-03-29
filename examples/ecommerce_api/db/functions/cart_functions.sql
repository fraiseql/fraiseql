-- Shopping Cart Functions for E-commerce API
-- CQRS pattern: Functions for mutations

-- Add item to cart
CREATE OR REPLACE FUNCTION add_to_cart(
    p_cart_id UUID,
    p_variant_id UUID,
    p_quantity INTEGER,
    p_customer_id UUID DEFAULT NULL,
    p_session_id VARCHAR DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    v_cart_id UUID;
    v_cart_item_id UUID;
    v_current_price DECIMAL(10, 2);
    v_available_quantity INTEGER;
    v_existing_quantity INTEGER;
BEGIN
    -- Get current price
    SELECT price INTO v_current_price
    FROM product_variants
    WHERE id = p_variant_id AND is_active = true;

    IF v_current_price IS NULL THEN
        RETURN ROW('failed:not_found', 'Product variant not found or inactive', p_variant_id::text, 'CartItem', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Check inventory
    SELECT quantity - reserved_quantity INTO v_available_quantity
    FROM inventory
    WHERE variant_id = p_variant_id;

    IF v_available_quantity IS NULL OR v_available_quantity < p_quantity THEN
        RETURN ROW('failed:validation', 'Insufficient inventory', p_variant_id::text, 'CartItem', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Get or create cart
    IF p_cart_id IS NOT NULL THEN
        v_cart_id := p_cart_id;
        -- Verify cart ownership
        IF NOT EXISTS (
            SELECT 1 FROM carts
            WHERE id = p_cart_id
            AND status = 'active'
            AND expires_at > CURRENT_TIMESTAMP
            AND (
                (customer_id = p_customer_id AND p_customer_id IS NOT NULL) OR
                (session_id = p_session_id AND p_session_id IS NOT NULL)
            )
        ) THEN
            RETURN ROW('failed:not_found', 'Cart not found or access denied', p_cart_id::text, 'Cart', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
        END IF;
    ELSE
        -- Create new cart
        INSERT INTO carts (customer_id, session_id)
        VALUES (p_customer_id, p_session_id)
        RETURNING id INTO v_cart_id;
    END IF;

    -- Check if item already in cart
    SELECT quantity INTO v_existing_quantity
    FROM cart_items
    WHERE cart_id = v_cart_id AND variant_id = p_variant_id;

    IF v_existing_quantity IS NOT NULL THEN
        -- Update quantity
        UPDATE cart_items
        SET quantity = v_existing_quantity + p_quantity,
            price_at_time = v_current_price,
            updated_at = CURRENT_TIMESTAMP
        WHERE cart_id = v_cart_id AND variant_id = p_variant_id
        RETURNING id INTO v_cart_item_id;
    ELSE
        -- Insert new item
        INSERT INTO cart_items (cart_id, variant_id, quantity, price_at_time)
        VALUES (v_cart_id, p_variant_id, p_quantity, v_current_price)
        RETURNING id INTO v_cart_item_id;
    END IF;

    -- Update cart timestamp
    UPDATE carts SET updated_at = CURRENT_TIMESTAMP WHERE id = v_cart_id;

    -- Return cart summary
    RETURN ROW(
        'new',
        'Item added to cart',
        v_cart_item_id::text,
        'CartItem',
        (SELECT to_jsonb(shopping_cart.*) FROM shopping_cart WHERE id = v_cart_id),
        NULL::text[],
        NULL::jsonb,
        jsonb_build_object('cart_id', v_cart_id)
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Update cart item quantity
CREATE OR REPLACE FUNCTION update_cart_item(
    p_cart_item_id UUID,
    p_quantity INTEGER,
    p_customer_id UUID DEFAULT NULL,
    p_session_id VARCHAR DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    v_cart_id UUID;
    v_variant_id UUID;
    v_available_quantity INTEGER;
BEGIN
    -- Get cart and variant info
    SELECT ci.cart_id, ci.variant_id INTO v_cart_id, v_variant_id
    FROM cart_items ci
    JOIN carts c ON ci.cart_id = c.id
    WHERE ci.id = p_cart_item_id
    AND c.status = 'active'
    AND c.expires_at > CURRENT_TIMESTAMP
    AND (
        (c.customer_id = p_customer_id AND p_customer_id IS NOT NULL) OR
        (c.session_id = p_session_id AND p_session_id IS NOT NULL)
    );

    IF v_cart_id IS NULL THEN
        RETURN ROW('failed:not_found', 'Cart item not found or access denied', p_cart_item_id::text, 'CartItem', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    IF p_quantity <= 0 THEN
        -- Remove item
        DELETE FROM cart_items WHERE id = p_cart_item_id;

        RETURN ROW(
            'deleted',
            'Item removed from cart',
            p_cart_item_id::text,
            'CartItem',
            NULL::jsonb,
            NULL::text[],
            NULL::jsonb,
            jsonb_build_object('cart_id', v_cart_id)
        )::mutation_response;
    ELSE
        -- Check inventory
        SELECT quantity - reserved_quantity INTO v_available_quantity
        FROM inventory
        WHERE variant_id = v_variant_id;

        IF v_available_quantity < p_quantity THEN
            RETURN ROW('failed:validation', 'Insufficient inventory', p_cart_item_id::text, 'CartItem', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
        END IF;

        -- Update quantity
        UPDATE cart_items
        SET quantity = p_quantity,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = p_cart_item_id;

        -- Update cart timestamp
        UPDATE carts SET updated_at = CURRENT_TIMESTAMP WHERE id = v_cart_id;

        RETURN ROW(
            'updated',
            'Cart updated',
            p_cart_item_id::text,
            'CartItem',
            (SELECT to_jsonb(shopping_cart.*) FROM shopping_cart WHERE id = v_cart_id),
            ARRAY['quantity'],
            NULL::jsonb,
            jsonb_build_object('cart_id', v_cart_id)
        )::mutation_response;
    END IF;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Clear cart
CREATE OR REPLACE FUNCTION clear_cart(
    p_cart_id UUID,
    p_customer_id UUID DEFAULT NULL,
    p_session_id VARCHAR DEFAULT NULL
) RETURNS mutation_response AS $$
BEGIN
    -- Verify cart ownership
    IF NOT EXISTS (
        SELECT 1 FROM carts
        WHERE id = p_cart_id
        AND status = 'active'
        AND (
            (customer_id = p_customer_id AND p_customer_id IS NOT NULL) OR
            (session_id = p_session_id AND p_session_id IS NOT NULL)
        )
    ) THEN
        RETURN ROW('failed:not_found', 'Cart not found or access denied', p_cart_id::text, 'Cart', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Delete all items
    DELETE FROM cart_items WHERE cart_id = p_cart_id;

    -- Update cart
    UPDATE carts
    SET updated_at = CURRENT_TIMESTAMP
    WHERE id = p_cart_id;

    RETURN ROW(
        'deleted',
        'Cart cleared',
        p_cart_id::text,
        'Cart',
        NULL::jsonb,
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Apply coupon to cart
CREATE OR REPLACE FUNCTION apply_coupon_to_cart(
    p_cart_id UUID,
    p_coupon_code VARCHAR,
    p_customer_id UUID DEFAULT NULL,
    p_session_id VARCHAR DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    v_coupon RECORD;
    v_cart_subtotal DECIMAL(10, 2);
    v_discount_amount DECIMAL(10, 2);
BEGIN
    -- Verify cart ownership
    IF NOT EXISTS (
        SELECT 1 FROM carts
        WHERE id = p_cart_id
        AND status = 'active'
        AND (
            (customer_id = p_customer_id AND p_customer_id IS NOT NULL) OR
            (session_id = p_session_id AND p_session_id IS NOT NULL)
        )
    ) THEN
        RETURN ROW('failed:not_found', 'Cart not found or access denied', p_cart_id::text, 'Cart', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Get coupon details
    SELECT * INTO v_coupon
    FROM coupons
    WHERE code = UPPER(p_coupon_code)
    AND is_active = true
    AND valid_from <= CURRENT_TIMESTAMP
    AND (valid_until IS NULL OR valid_until > CURRENT_TIMESTAMP)
    AND (usage_limit IS NULL OR usage_count < usage_limit);

    IF v_coupon IS NULL THEN
        RETURN ROW('failed:validation', 'Invalid or expired coupon', NULL, 'Coupon', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Get cart subtotal
    SELECT SUM(quantity * price_at_time) INTO v_cart_subtotal
    FROM cart_items
    WHERE cart_id = p_cart_id;

    -- Check minimum purchase amount
    IF v_coupon.minimum_purchase_amount IS NOT NULL AND
       v_cart_subtotal < v_coupon.minimum_purchase_amount THEN
        RETURN ROW('failed:validation', 'Cart total does not meet minimum purchase requirement', p_cart_id::text, 'Cart', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Calculate discount
    IF v_coupon.discount_type = 'percentage' THEN
        v_discount_amount := v_cart_subtotal * (v_coupon.discount_value / 100);
    ELSE
        v_discount_amount := LEAST(v_coupon.discount_value, v_cart_subtotal);
    END IF;

    -- Store coupon in cart metadata
    UPDATE carts
    SET metadata = jsonb_set(
        COALESCE(metadata, '{}'::jsonb),
        '{coupon}',
        jsonb_build_object(
            'code', v_coupon.code,
            'discount_amount', v_discount_amount,
            'discount_type', v_coupon.discount_type,
            'discount_value', v_coupon.discount_value
        )
    ),
    updated_at = CURRENT_TIMESTAMP
    WHERE id = p_cart_id;

    RETURN ROW(
        'updated',
        'Coupon applied',
        p_cart_id::text,
        'Cart',
        (SELECT to_jsonb(shopping_cart.*) FROM shopping_cart WHERE id = p_cart_id),
        ARRAY['metadata'],
        NULL::jsonb,
        jsonb_build_object('discount_amount', v_discount_amount, 'coupon_code', v_coupon.code)
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;
