-- Customer Management Functions for E-commerce API
-- CQRS pattern: Functions for mutations

-- Register new customer
CREATE OR REPLACE FUNCTION register_customer(
    p_email VARCHAR,
    p_password VARCHAR,
    p_first_name VARCHAR,
    p_last_name VARCHAR,
    p_phone VARCHAR DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    v_customer_id UUID;
    v_wishlist_id UUID;
BEGIN
    -- Check if email already exists
    IF EXISTS (SELECT 1 FROM customers WHERE email = LOWER(p_email)) THEN
        RETURN ROW('conflict:duplicate', 'Email already registered', NULL, 'Customer', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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

    RETURN ROW(
        'new',
        'Customer registered successfully',
        v_customer_id::text,
        'Customer',
        jsonb_build_object(
            'id', v_customer_id,
            'email', LOWER(p_email),
            'first_name', p_first_name,
            'last_name', p_last_name
        ),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Update customer profile
CREATE OR REPLACE FUNCTION update_customer_profile(
    p_customer_id UUID,
    p_first_name VARCHAR DEFAULT NULL,
    p_last_name VARCHAR DEFAULT NULL,
    p_phone VARCHAR DEFAULT NULL,
    p_metadata JSONB DEFAULT NULL
) RETURNS mutation_response AS $$
DECLARE
    v_updated_fields text[] := '{}';
BEGIN
    -- Track which fields are being updated
    IF p_first_name IS NOT NULL THEN v_updated_fields := v_updated_fields || 'first_name'; END IF;
    IF p_last_name IS NOT NULL THEN v_updated_fields := v_updated_fields || 'last_name'; END IF;
    IF p_phone IS NOT NULL THEN v_updated_fields := v_updated_fields || 'phone'; END IF;
    IF p_metadata IS NOT NULL THEN v_updated_fields := v_updated_fields || 'metadata'; END IF;

    UPDATE customers
    SET first_name = COALESCE(p_first_name, first_name),
        last_name = COALESCE(p_last_name, last_name),
        phone = COALESCE(p_phone, phone),
        metadata = COALESCE(p_metadata, metadata),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = p_customer_id;

    IF NOT FOUND THEN
        RETURN ROW('failed:not_found', 'Customer not found', p_customer_id::text, 'Customer', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    RETURN ROW(
        'updated',
        'Profile updated successfully',
        p_customer_id::text,
        'Customer',
        (SELECT to_jsonb(c.*) FROM customers c WHERE c.id = p_customer_id),
        v_updated_fields,
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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
) RETURNS mutation_response AS $$
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

    RETURN ROW(
        'new',
        'Address added successfully',
        v_address_id::text,
        'Address',
        jsonb_build_object(
            'id', v_address_id,
            'type', p_type,
            'city', p_city,
            'country_code', p_country_code,
            'is_default', p_is_default
        ),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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
) RETURNS mutation_response AS $$
DECLARE
    v_wishlist_id UUID;
    v_wishlist_item_id UUID;
BEGIN
    -- Get wishlist ID
    IF p_wishlist_id IS NOT NULL THEN
        -- Verify ownership
        SELECT id INTO v_wishlist_id
        FROM wishlists
        WHERE id = p_wishlist_id AND customer_id = p_customer_id;

        IF v_wishlist_id IS NULL THEN
            RETURN ROW('failed:not_found', 'Wishlist not found or access denied', p_wishlist_id::text, 'Wishlist', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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
        RETURN ROW('conflict:duplicate', 'Product already in wishlist', v_wishlist_id::text, 'Wishlist', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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
    ) RETURNING id INTO v_wishlist_item_id;

    RETURN ROW(
        'new',
        'Added to wishlist',
        v_wishlist_item_id::text,
        'WishlistItem',
        jsonb_build_object(
            'wishlist_id', v_wishlist_id,
            'product_id', p_product_id,
            'variant_id', p_variant_id
        ),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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
) RETURNS mutation_response AS $$
DECLARE
    v_review_id UUID;
    v_is_verified_purchase BOOLEAN := false;
BEGIN
    -- Validate rating
    IF p_rating < 1 OR p_rating > 5 THEN
        RETURN ROW('failed:validation', 'Rating must be between 1 and 5', NULL, 'Review', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    -- Check if already reviewed
    IF EXISTS (
        SELECT 1 FROM reviews
        WHERE customer_id = p_customer_id
        AND product_id = p_product_id
        AND (order_id = p_order_id OR (order_id IS NULL AND p_order_id IS NULL))
    ) THEN
        RETURN ROW('conflict:duplicate', 'You have already reviewed this product', NULL, 'Review', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
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

    RETURN ROW(
        'new',
        'Review submitted for moderation',
        v_review_id::text,
        'Review',
        jsonb_build_object(
            'id', v_review_id,
            'rating', p_rating,
            'is_verified_purchase', v_is_verified_purchase,
            'status', 'pending'
        ),
        NULL::text[],
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;

-- Mark review as helpful
CREATE OR REPLACE FUNCTION mark_review_helpful(
    p_review_id UUID,
    p_is_helpful BOOLEAN,
    p_customer_id UUID DEFAULT NULL,
    p_session_id VARCHAR DEFAULT NULL
) RETURNS mutation_response AS $$
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
        RETURN ROW('failed:not_found', 'Review not found or not approved', p_review_id::text, 'Review', NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
    END IF;

    RETURN ROW(
        'updated',
        'Thank you for your feedback',
        p_review_id::text,
        'Review',
        NULL::jsonb,
        CASE WHEN p_is_helpful THEN ARRAY['helpful_count'] ELSE ARRAY['not_helpful_count'] END,
        NULL::jsonb,
        NULL::jsonb
    )::mutation_response;
EXCEPTION
    WHEN OTHERS THEN
        RETURN ROW('failed:error', SQLERRM, NULL, NULL, NULL, NULL::text[], NULL::jsonb, NULL::jsonb)::mutation_response;
END;
$$ LANGUAGE plpgsql;
