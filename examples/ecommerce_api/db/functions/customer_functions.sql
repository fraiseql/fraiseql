-- Customer Management Functions for E-commerce API
-- CQRS pattern: Functions for mutations

-- Register new customer
CREATE OR REPLACE FUNCTION register_customer(
    p_email VARCHAR,
    p_password VARCHAR,
    p_first_name VARCHAR,
    p_last_name VARCHAR,
    p_phone VARCHAR DEFAULT NULL
) RETURNS JSON AS $$
DECLARE
    v_customer_id UUID;
    v_wishlist_id UUID;
BEGIN
    -- Check if email already exists
    IF EXISTS (SELECT 1 FROM customers WHERE email = LOWER(p_email)) THEN
        RAISE EXCEPTION 'Email already registered';
    END IF;

    -- Create customer (password should be hashed in application layer)
    INSERT INTO customers (
        email,
        password_hash,
        first_name,
        last_name,
        phone
    ) VALUES (
        LOWER(p_email),
        p_password, -- In production, this should be properly hashed
        p_first_name,
        p_last_name,
        p_phone
    ) RETURNING id INTO v_customer_id;

    -- Create default wishlist
    INSERT INTO wishlists (customer_id, name)
    VALUES (v_customer_id, 'My Wishlist')
    RETURNING id INTO v_wishlist_id;

    RETURN json_build_object(
        'success', true,
        'customer_id', v_customer_id,
        'message', 'Customer registered successfully',
        'customer', json_build_object(
            'id', v_customer_id,
            'email', LOWER(p_email),
            'first_name', p_first_name,
            'last_name', p_last_name
        )
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Update customer profile
CREATE OR REPLACE FUNCTION update_customer_profile(
    p_customer_id UUID,
    p_first_name VARCHAR DEFAULT NULL,
    p_last_name VARCHAR DEFAULT NULL,
    p_phone VARCHAR DEFAULT NULL,
    p_metadata JSONB DEFAULT NULL
) RETURNS JSON AS $$
BEGIN
    UPDATE customers
    SET first_name = COALESCE(p_first_name, first_name),
        last_name = COALESCE(p_last_name, last_name),
        phone = COALESCE(p_phone, phone),
        metadata = COALESCE(p_metadata, metadata),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_customer_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Customer not found';
    END IF;

    RETURN json_build_object(
        'success', true,
        'message', 'Profile updated successfully',
        'customer', (
            SELECT row_to_json(c.*)
            FROM customers c
            WHERE c.id = p_customer_id
        )
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Add customer address
CREATE OR REPLACE FUNCTION add_customer_address(
    p_customer_id UUID,
    p_type VARCHAR,
    p_first_name VARCHAR,
    p_last_name VARCHAR,
    p_company VARCHAR DEFAULT NULL,
    p_address_line1 VARCHAR,
    p_address_line2 VARCHAR DEFAULT NULL,
    p_city VARCHAR,
    p_state_province VARCHAR DEFAULT NULL,
    p_postal_code VARCHAR DEFAULT NULL,
    p_country_code VARCHAR,
    p_phone VARCHAR DEFAULT NULL,
    p_is_default BOOLEAN DEFAULT false
) RETURNS JSON AS $$
DECLARE
    v_address_id UUID;
BEGIN
    -- If setting as default, unset other defaults
    IF p_is_default THEN
        UPDATE addresses
        SET is_default = false
        WHERE customer_id = p_customer_id
        AND type = p_type;
    END IF;

    -- Create address
    INSERT INTO addresses (
        customer_id,
        type,
        first_name,
        last_name,
        company,
        address_line1,
        address_line2,
        city,
        state_province,
        postal_code,
        country_code,
        phone,
        is_default
    ) VALUES (
        p_customer_id,
        p_type,
        p_first_name,
        p_last_name,
        p_company,
        p_address_line1,
        p_address_line2,
        p_city,
        p_state_province,
        p_postal_code,
        p_country_code,
        p_phone,
        p_is_default
    ) RETURNING id INTO v_address_id;

    RETURN json_build_object(
        'success', true,
        'address_id', v_address_id,
        'message', 'Address added successfully'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Add to wishlist
CREATE OR REPLACE FUNCTION add_to_wishlist(
    p_customer_id UUID,
    p_product_id UUID,
    p_variant_id UUID DEFAULT NULL,
    p_wishlist_id UUID DEFAULT NULL,
    p_priority INTEGER DEFAULT 0,
    p_notes TEXT DEFAULT NULL
) RETURNS JSON AS $$
DECLARE
    v_wishlist_id UUID;
BEGIN
    -- Get wishlist ID
    IF p_wishlist_id IS NOT NULL THEN
        -- Verify ownership
        SELECT id INTO v_wishlist_id
        FROM wishlists
        WHERE id = p_wishlist_id AND customer_id = p_customer_id;

        IF v_wishlist_id IS NULL THEN
            RAISE EXCEPTION 'Wishlist not found or access denied';
        END IF;
    ELSE
        -- Get default wishlist
        SELECT id INTO v_wishlist_id
        FROM wishlists
        WHERE customer_id = p_customer_id
        ORDER BY created_at
        LIMIT 1;

        IF v_wishlist_id IS NULL THEN
            -- Create default wishlist
            INSERT INTO wishlists (customer_id)
            VALUES (p_customer_id)
            RETURNING id INTO v_wishlist_id;
        END IF;
    END IF;

    -- Check if already in wishlist
    IF EXISTS (
        SELECT 1 FROM wishlist_items
        WHERE wishlist_id = v_wishlist_id
        AND product_id = p_product_id
        AND (variant_id = p_variant_id OR (variant_id IS NULL AND p_variant_id IS NULL))
    ) THEN
        RAISE EXCEPTION 'Product already in wishlist';
    END IF;

    -- Add to wishlist
    INSERT INTO wishlist_items (
        wishlist_id,
        product_id,
        variant_id,
        priority,
        notes
    ) VALUES (
        v_wishlist_id,
        p_product_id,
        p_variant_id,
        p_priority,
        p_notes
    );

    RETURN json_build_object(
        'success', true,
        'message', 'Added to wishlist',
        'wishlist_id', v_wishlist_id
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Submit product review
CREATE OR REPLACE FUNCTION submit_review(
    p_customer_id UUID,
    p_product_id UUID,
    p_order_id UUID DEFAULT NULL,
    p_rating INTEGER,
    p_title VARCHAR DEFAULT NULL,
    p_comment TEXT DEFAULT NULL
) RETURNS JSON AS $$
DECLARE
    v_review_id UUID;
    v_is_verified_purchase BOOLEAN := false;
BEGIN
    -- Validate rating
    IF p_rating < 1 OR p_rating > 5 THEN
        RAISE EXCEPTION 'Rating must be between 1 and 5';
    END IF;

    -- Check if already reviewed
    IF EXISTS (
        SELECT 1 FROM reviews
        WHERE customer_id = p_customer_id
        AND product_id = p_product_id
        AND (order_id = p_order_id OR (order_id IS NULL AND p_order_id IS NULL))
    ) THEN
        RAISE EXCEPTION 'You have already reviewed this product';
    END IF;

    -- Verify purchase if order_id provided
    IF p_order_id IS NOT NULL THEN
        SELECT EXISTS(
            SELECT 1 FROM orders o
            JOIN order_items oi ON oi.order_id = o.id
            JOIN product_variants pv ON oi.variant_id = pv.id
            WHERE o.id = p_order_id
            AND o.customer_id = p_customer_id
            AND pv.product_id = p_product_id
            AND o.status IN ('completed', 'delivered')
        ) INTO v_is_verified_purchase;
    END IF;

    -- Create review
    INSERT INTO reviews (
        product_id,
        customer_id,
        order_id,
        rating,
        title,
        comment,
        is_verified_purchase,
        status
    ) VALUES (
        p_product_id,
        p_customer_id,
        p_order_id,
        p_rating,
        p_title,
        p_comment,
        v_is_verified_purchase,
        'pending' -- Reviews go through moderation
    ) RETURNING id INTO v_review_id;

    RETURN json_build_object(
        'success', true,
        'review_id', v_review_id,
        'message', 'Review submitted for moderation',
        'is_verified_purchase', v_is_verified_purchase
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;

-- Mark review as helpful
CREATE OR REPLACE FUNCTION mark_review_helpful(
    p_review_id UUID,
    p_is_helpful BOOLEAN,
    p_customer_id UUID DEFAULT NULL,
    p_session_id VARCHAR DEFAULT NULL
) RETURNS JSON AS $$
BEGIN
    -- In production, track who marked what to prevent multiple votes
    IF p_is_helpful THEN
        UPDATE reviews
        SET helpful_count = helpful_count + 1
        WHERE id = p_review_id AND status = 'approved';
    ELSE
        UPDATE reviews
        SET not_helpful_count = not_helpful_count + 1
        WHERE id = p_review_id AND status = 'approved';
    END IF;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Review not found or not approved';
    END IF;

    RETURN json_build_object(
        'success', true,
        'message', 'Thank you for your feedback'
    );
EXCEPTION
    WHEN OTHERS THEN
        RETURN json_build_object(
            'success', false,
            'error', SQLERRM
        );
END;
$$ LANGUAGE plpgsql;
