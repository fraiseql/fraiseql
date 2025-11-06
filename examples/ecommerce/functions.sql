-- PostgreSQL Functions for E-commerce Mutations
-- These functions implement the business logic for FraiseQL mutations

SET search_path TO ecommerce, public;

-- Helper function to hash passwords
CREATE OR REPLACE FUNCTION hash_password(password TEXT)
RETURNS TEXT AS $$
BEGIN
    RETURN crypt(password, gen_salt('bf', 8));
END;
$$ LANGUAGE plpgsql;

-- Helper function to verify passwords
CREATE OR REPLACE FUNCTION verify_password(password TEXT, hash TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN hash = crypt(password, hash);
END;
$$ LANGUAGE plpgsql;

-- Helper function to generate JWT token (simplified)
CREATE OR REPLACE FUNCTION generate_token(user_id UUID)
RETURNS TEXT AS $$
BEGIN
    -- In production, use proper JWT library
    RETURN encode(jsonb_build_object(
        'user_id', user_id,
        'exp', extract(epoch from now() + interval '7 days')
    )::text::bytea, 'base64');
END;
$$ LANGUAGE plpgsql;

-- Register mutation
CREATE OR REPLACE FUNCTION graphql.register(input jsonb)
RETURNS jsonb AS $$
DECLARE
    new_user_id UUID;
    user_data jsonb;
BEGIN
    -- Validate input
    IF input->>'email' IS NULL OR input->>'password' IS NULL OR input->>'name' IS NULL THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Email, password, and name are required',
            'code', 'VALIDATION_ERROR'
        );
    END IF;

    -- Check if email already exists
    IF EXISTS (SELECT 1 FROM tb_user WHERE email = input->>'email') THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Email already registered',
            'code', 'EMAIL_EXISTS'
        );
    END IF;

    -- Create user
    INSERT INTO tb_user (email, password_hash, name, phone)
    VALUES (
        input->>'email',
        hash_password(input->>'password'),
        input->>'name',
        input->>'phone'
    )
    RETURNING id INTO new_user_id;

    -- Get user data
    SELECT data INTO user_data FROM v_users WHERE data->>'id' = new_user_id::text;

    -- Return success
    RETURN jsonb_build_object(
        'type', 'success',
        'user', user_data,
        'token', generate_token(new_user_id),
        'message', 'Registration successful'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Registration failed: ' || SQLERRM,
            'code', 'INTERNAL_ERROR'
        );
END;
$$ LANGUAGE plpgsql;

-- Login mutation
CREATE OR REPLACE FUNCTION graphql.login(input jsonb)
RETURNS jsonb AS $$
DECLARE
    user_record RECORD;
    user_data jsonb;
BEGIN
    -- Validate input
    IF input->>'email' IS NULL OR input->>'password' IS NULL THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Email and password are required',
            'code', 'VALIDATION_ERROR'
        );
    END IF;

    -- Find user
    SELECT id, password_hash, is_active
    INTO user_record
    FROM tb_user
    WHERE email = input->>'email';

    IF NOT FOUND THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Invalid email or password',
            'code', 'INVALID_CREDENTIALS'
        );
    END IF;

    -- Check if account is active
    IF NOT user_record.is_active THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Account is disabled',
            'code', 'ACCOUNT_DISABLED'
        );
    END IF;

    -- Verify password
    IF NOT verify_password(input->>'password', user_record.password_hash) THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Invalid email or password',
            'code', 'INVALID_CREDENTIALS'
        );
    END IF;

    -- Get user data
    SELECT data INTO user_data FROM v_users WHERE data->>'id' = user_record.id::text;

    -- Return success
    RETURN jsonb_build_object(
        'type', 'success',
        'user', user_data,
        'token', generate_token(user_record.id),
        'message', 'Login successful'
    );
END;
$$ LANGUAGE plpgsql;

-- Add to cart mutation
CREATE OR REPLACE FUNCTION graphql.add_to_cart(input jsonb, context jsonb DEFAULT '{}')
RETURNS jsonb AS $$
DECLARE
    cart_id UUID;
    product_record RECORD;
    cart_data jsonb;
    user_id UUID;
    quantity INT;
BEGIN
    -- Get user from context
    user_id := (context->>'user_id')::UUID;
    quantity := COALESCE((input->>'quantity')::INT, 1);

    -- Validate quantity
    IF quantity <= 0 THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Quantity must be positive',
            'code', 'INVALID_QUANTITY'
        );
    END IF;

    -- Get product info
    SELECT id, price, inventory_count, is_active
    INTO product_record
    FROM products
    WHERE id = (input->>'product_id')::UUID;

    IF NOT FOUND THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Product not found',
            'code', 'PRODUCT_NOT_FOUND'
        );
    END IF;

    -- Check if product is active
    IF NOT product_record.is_active THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Product is not available',
            'code', 'PRODUCT_UNAVAILABLE'
        );
    END IF;

    -- Check inventory
    IF product_record.inventory_count < quantity THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Insufficient inventory',
            'code', 'INSUFFICIENT_INVENTORY'
        );
    END IF;

    -- Find or create cart
    IF user_id IS NOT NULL THEN
        SELECT id INTO cart_id FROM carts
        WHERE user_id = user_id
        AND expires_at > CURRENT_TIMESTAMP
        ORDER BY created_at DESC
        LIMIT 1;
    ELSE
        -- For anonymous users, use session_id from context
        SELECT id INTO cart_id FROM carts
        WHERE session_id = context->>'session_id'
        AND expires_at > CURRENT_TIMESTAMP
        ORDER BY created_at DESC
        LIMIT 1;
    END IF;

    -- Create cart if not exists
    IF cart_id IS NULL THEN
        INSERT INTO carts (user_id, session_id)
        VALUES (user_id, context->>'session_id')
        RETURNING id INTO cart_id;
    END IF;

    -- Add or update cart item
    INSERT INTO cart_items (cart_id, product_id, quantity, price)
    VALUES (cart_id, product_record.id, quantity, product_record.price)
    ON CONFLICT (cart_id, product_id) DO UPDATE
    SET quantity = cart_items.quantity + EXCLUDED.quantity,
        price = EXCLUDED.price,
        updated_at = CURRENT_TIMESTAMP;

    -- Update cart totals
    UPDATE carts
    SET items_count = (SELECT SUM(quantity) FROM cart_items WHERE cart_id = carts.id),
        subtotal = (SELECT SUM(quantity * price) FROM cart_items WHERE cart_id = carts.id),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = cart_id;

    -- Get updated cart data
    SELECT data INTO cart_data FROM v_carts WHERE data->>'id' = cart_id::text;

    RETURN jsonb_build_object(
        'type', 'success',
        'cart', cart_data,
        'message', 'Item added to cart'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Failed to add item to cart: ' || SQLERRM,
            'code', 'INTERNAL_ERROR'
        );
END;
$$ LANGUAGE plpgsql;

-- Update cart item mutation
CREATE OR REPLACE FUNCTION graphql.update_cart_item(input jsonb, context jsonb DEFAULT '{}')
RETURNS jsonb AS $$
DECLARE
    cart_id UUID;
    cart_data jsonb;
    new_quantity INT;
    product_inventory INT;
BEGIN
    new_quantity := (input->>'quantity')::INT;

    -- Validate quantity
    IF new_quantity < 0 THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Quantity cannot be negative',
            'code', 'INVALID_QUANTITY'
        );
    END IF;

    -- Get cart ID from cart item
    SELECT ci.cart_id, p.inventory_count
    INTO cart_id, product_inventory
    FROM cart_items ci
    JOIN products p ON ci.product_id = p.id
    WHERE ci.id = (input->>'cart_item_id')::UUID;

    IF NOT FOUND THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Cart item not found',
            'code', 'ITEM_NOT_FOUND'
        );
    END IF;

    -- Check inventory
    IF new_quantity > product_inventory THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Insufficient inventory',
            'code', 'INSUFFICIENT_INVENTORY'
        );
    END IF;

    -- Update or delete item
    IF new_quantity = 0 THEN
        DELETE FROM cart_items WHERE id = (input->>'cart_item_id')::UUID;
    ELSE
        UPDATE cart_items
        SET quantity = new_quantity,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = (input->>'cart_item_id')::UUID;
    END IF;

    -- Update cart totals
    UPDATE carts
    SET items_count = (SELECT COALESCE(SUM(quantity), 0) FROM cart_items WHERE cart_id = carts.id),
        subtotal = (SELECT COALESCE(SUM(quantity * price), 0) FROM cart_items WHERE cart_id = carts.id),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = cart_id;

    -- Get updated cart data
    SELECT data INTO cart_data FROM v_carts WHERE data->>'id' = cart_id::text;

    RETURN jsonb_build_object(
        'type', 'success',
        'cart', cart_data,
        'message', 'Cart updated'
    );
END;
$$ LANGUAGE plpgsql;

-- Checkout mutation
CREATE OR REPLACE FUNCTION graphql.checkout(input jsonb, context jsonb DEFAULT '{}')
RETURNS jsonb AS $$
DECLARE
    user_id UUID;
    cart_record RECORD;
    order_id UUID;
    order_number VARCHAR(50);
    order_data jsonb;
    tax_rate DECIMAL := 0.08; -- 8% tax rate
    shipping_cost DECIMAL := 10.00; -- Flat shipping rate
    discount DECIMAL := 0;
    coupon_record RECORD;
BEGIN
    -- Get user from context
    user_id := (context->>'user_id')::UUID;

    IF user_id IS NULL THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Authentication required',
            'code', 'UNAUTHORIZED'
        );
    END IF;

    -- Get active cart
    SELECT c.*,
           (SELECT COUNT(*) FROM cart_items WHERE cart_id = c.id) as item_count
    INTO cart_record
    FROM carts c
    WHERE c.user_id = user_id
    AND c.expires_at > CURRENT_TIMESTAMP
    ORDER BY c.created_at DESC
    LIMIT 1;

    IF NOT FOUND OR cart_record.item_count = 0 THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Cart is empty',
            'code', 'EMPTY_CART'
        );
    END IF;

    -- Validate addresses exist
    IF NOT EXISTS (SELECT 1 FROM addresses WHERE id = (input->>'shipping_address_id')::UUID AND user_id = user_id) THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Invalid shipping address',
            'code', 'INVALID_ADDRESS'
        );
    END IF;

    -- Apply coupon if provided
    IF input->>'coupon_code' IS NOT NULL THEN
        SELECT * INTO coupon_record
        FROM coupons
        WHERE code = input->>'coupon_code'
        AND is_active = true
        AND valid_from <= CURRENT_TIMESTAMP
        AND (valid_until IS NULL OR valid_until > CURRENT_TIMESTAMP)
        AND (usage_limit IS NULL OR usage_count < usage_limit);

        IF FOUND THEN
            IF coupon_record.minimum_amount IS NULL OR cart_record.subtotal >= coupon_record.minimum_amount THEN
                IF coupon_record.discount_type = 'percentage' THEN
                    discount := cart_record.subtotal * (coupon_record.discount_value / 100);
                ELSE
                    discount := coupon_record.discount_value;
                END IF;

                -- Update coupon usage
                UPDATE coupons
                SET usage_count = usage_count + 1
                WHERE id = coupon_record.id;
            END IF;
        END IF;
    END IF;

    -- Generate order number
    order_number := 'ORD-' || to_char(CURRENT_DATE, 'YYYYMMDD') || '-' ||
                    lpad(nextval('order_number_seq')::text, 6, '0');

    -- Create order
    INSERT INTO orders (
        order_number,
        user_id,
        shipping_address_id,
        billing_address_id,
        subtotal,
        tax_amount,
        shipping_amount,
        discount_amount,
        total,
        notes
    ) VALUES (
        order_number,
        user_id,
        (input->>'shipping_address_id')::UUID,
        COALESCE((input->>'billing_address_id')::UUID, (input->>'shipping_address_id')::UUID),
        cart_record.subtotal,
        cart_record.subtotal * tax_rate,
        shipping_cost,
        discount,
        cart_record.subtotal + (cart_record.subtotal * tax_rate) + shipping_cost - discount,
        input->>'notes'
    ) RETURNING id INTO order_id;

    -- Copy cart items to order items
    INSERT INTO order_items (order_id, product_id, quantity, price, total)
    SELECT order_id, product_id, quantity, price, quantity * price
    FROM cart_items
    WHERE cart_id = cart_record.id;

    -- Update product inventory
    UPDATE products p
    SET inventory_count = inventory_count - ci.quantity
    FROM cart_items ci
    WHERE p.id = ci.product_id
    AND ci.cart_id = cart_record.id;

    -- Clear cart
    DELETE FROM cart_items WHERE cart_id = cart_record.id;
    DELETE FROM carts WHERE id = cart_record.id;

    -- Get order data
    SELECT data INTO order_data FROM v_orders WHERE data->>'id' = order_id::text;

    RETURN jsonb_build_object(
        'type', 'success',
        'order', order_data,
        'message', 'Order placed successfully'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Checkout failed: ' || SQLERRM,
            'code', 'CHECKOUT_ERROR'
        );
END;
$$ LANGUAGE plpgsql;

-- Create address mutation
CREATE OR REPLACE FUNCTION graphql.create_address(input jsonb, context jsonb DEFAULT '{}')
RETURNS jsonb AS $$
DECLARE
    user_id UUID;
    address_id UUID;
    address_data jsonb;
BEGIN
    -- Get user from context
    user_id := (context->>'user_id')::UUID;

    IF user_id IS NULL THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Authentication required',
            'code', 'UNAUTHORIZED'
        );
    END IF;

    -- If setting as default, unset other defaults
    IF COALESCE((input->>'is_default')::BOOLEAN, false) THEN
        UPDATE addresses
        SET is_default = false
        WHERE user_id = user_id;
    END IF;

    -- Create address
    INSERT INTO addresses (
        user_id,
        label,
        street1,
        street2,
        city,
        state,
        postal_code,
        country,
        is_default
    ) VALUES (
        user_id,
        input->>'label',
        input->>'street1',
        input->>'street2',
        input->>'city',
        input->>'state',
        input->>'postal_code',
        COALESCE(input->>'country', 'US'),
        COALESCE((input->>'is_default')::BOOLEAN, false)
    ) RETURNING id INTO address_id;

    -- Get address data
    SELECT data INTO address_data FROM v_addresses WHERE data->>'id' = address_id::text;

    RETURN jsonb_build_object(
        'type', 'success',
        'address', address_data,
        'message', 'Address created successfully'
    );
END;
$$ LANGUAGE plpgsql;

-- Create review mutation
CREATE OR REPLACE FUNCTION graphql.create_review(input jsonb, context jsonb DEFAULT '{}')
RETURNS jsonb AS $$
DECLARE
    user_id UUID;
    review_id UUID;
    review_data jsonb;
    has_purchased BOOLEAN;
BEGIN
    -- Get user from context
    user_id := (context->>'user_id')::UUID;

    IF user_id IS NULL THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Authentication required',
            'code', 'UNAUTHORIZED'
        );
    END IF;

    -- Validate rating
    IF (input->>'rating')::INT NOT BETWEEN 1 AND 5 THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'Rating must be between 1 and 5',
            'code', 'INVALID_RATING'
        );
    END IF;

    -- Check if user has purchased this product
    SELECT EXISTS (
        SELECT 1
        FROM orders o
        JOIN order_items oi ON o.id = oi.order_id
        WHERE o.user_id = user_id
        AND oi.product_id = (input->>'product_id')::UUID
        AND o.status = 'delivered'
    ) INTO has_purchased;

    -- Check for existing review
    IF EXISTS (
        SELECT 1 FROM reviews
        WHERE user_id = user_id
        AND product_id = (input->>'product_id')::UUID
    ) THEN
        RETURN jsonb_build_object(
            'type', 'error',
            'message', 'You have already reviewed this product',
            'code', 'DUPLICATE_REVIEW'
        );
    END IF;

    -- Create review
    INSERT INTO reviews (
        product_id,
        user_id,
        rating,
        title,
        comment,
        is_verified
    ) VALUES (
        (input->>'product_id')::UUID,
        user_id,
        (input->>'rating')::INT,
        input->>'title',
        input->>'comment',
        has_purchased
    ) RETURNING id INTO review_id;

    -- Get review data
    SELECT data INTO review_data FROM v_reviews WHERE data->>'id' = review_id::text;

    RETURN jsonb_build_object(
        'type', 'success',
        'review', review_data,
        'message', 'Review submitted successfully'
    );
END;
$$ LANGUAGE plpgsql;

-- Create sequence for order numbers
CREATE SEQUENCE IF NOT EXISTS order_number_seq START 1;
