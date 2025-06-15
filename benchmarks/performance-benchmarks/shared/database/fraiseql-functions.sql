-- FraiseQL Layered Function Architecture for Benchmark
-- Following the patterns: api.* -> core.* -> sync.*

SET search_path TO benchmark, public;

-- Create schemas for function organization (using prefix approach for simplicity)
-- Since this is a benchmark, we'll use the simpler prefix-based approach

-- Create the standard mutation result type
CREATE TYPE mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);

-- ============================================================================
-- SYNC FUNCTIONS (Projection Refresh)
-- ============================================================================

-- Refresh user projection in table view
CREATE OR REPLACE FUNCTION sync_refresh_user_projection(p_user_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_users (id, data)
    SELECT
        u.id,
        jsonb_build_object(
            'id', u.id,
            'email', u.email,
            'username', u.username,
            'fullName', u.full_name,
            'createdAt', u.created_at,
            'isActive', u.is_active,
            'orderCount', COALESCE((
                SELECT COUNT(*)::int
                FROM tb_orders o
                WHERE o.user_id = u.id
            ), 0),
            'totalSpent', COALESCE((
                SELECT SUM(o.total_amount)::float
                FROM tb_orders o
                WHERE o.user_id = u.id
            ), 0.0),
            'reviewCount', COALESCE((
                SELECT COUNT(*)::int
                FROM tb_product_reviews pr
                WHERE pr.user_id = u.id
            ), 0),
            'averageRating', (
                SELECT AVG(pr.rating)::float
                FROM tb_product_reviews pr
                WHERE pr.user_id = u.id
            )
        ) as data
    FROM tb_users u
    WHERE u.id = p_user_id
    ON CONFLICT (id) DO UPDATE
    SET data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Incremental update for user order count
CREATE OR REPLACE FUNCTION sync_increment_user_order_count(p_user_id UUID)
RETURNS VOID AS $$
BEGIN
    UPDATE tv_users
    SET data = jsonb_set(
        jsonb_set(
            data,
            '{orderCount}',
            to_jsonb(COALESCE((data->>'orderCount')::INTEGER, 0) + 1)
        ),
        '{totalSpent}',
        to_jsonb((
            SELECT COALESCE(SUM(total_amount), 0)::float
            FROM tb_orders
            WHERE user_id = p_user_id
        ))
    ),
    updated_at = NOW()
    WHERE id = p_user_id;
END;
$$ LANGUAGE plpgsql;

-- Refresh product projection
CREATE OR REPLACE FUNCTION sync_refresh_product_projection(p_product_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_products (id, data)
    SELECT
        p.id,
        jsonb_build_object(
            'id', p.id,
            'name', p.name,
            'slug', p.slug,
            'description', p.description,
            'price', p.price,
            'stockQuantity', p.stock_quantity,
            'tags', COALESCE(p.tags, '[]'::jsonb),
            'createdAt', p.created_at,
            'updatedAt', p.updated_at,
            'reviewCount', COALESCE((
                SELECT COUNT(*)::int
                FROM tb_product_reviews pr
                WHERE pr.product_id = p.id
            ), 0),
            'averageRating', (
                SELECT AVG(pr.rating)::float
                FROM tb_product_reviews pr
                WHERE pr.product_id = p.id
            ),
            'categories', COALESCE((
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', c.id,
                        'name', c.name,
                        'slug', c.slug,
                        'description', c.description,
                        'parentId', c.parent_id
                    )
                )
                FROM tb_categories c
                JOIN tb_product_categories pc ON pc.category_id = c.id
                WHERE pc.product_id = p.id
            ), '[]'::jsonb),
            'reviews', COALESCE((
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', pr.id,
                        'rating', pr.rating,
                        'title', pr.title,
                        'comment', pr.comment,
                        'createdAt', pr.created_at,
                        'user', jsonb_build_object(
                            'id', u.id,
                            'username', u.username,
                            'fullName', u.full_name
                        )
                    )
                    ORDER BY pr.created_at DESC
                )
                FROM tb_product_reviews pr
                JOIN tb_users u ON u.id = pr.user_id
                WHERE pr.product_id = p.id
            ), '[]'::jsonb)
        ) as data
    FROM tb_products p
    WHERE p.id = p_product_id
    ON CONFLICT (id) DO UPDATE
    SET data = EXCLUDED.data,
        updated_at = NOW();

    -- Also update popular products view
    PERFORM sync_refresh_popular_products();
END;
$$ LANGUAGE plpgsql;

-- Refresh order projection
CREATE OR REPLACE FUNCTION sync_refresh_order_projection(p_order_id UUID)
RETURNS VOID AS $$
BEGIN
    INSERT INTO tv_orders (id, data)
    SELECT
        o.id,
        jsonb_build_object(
            'id', o.id,
            'userId', o.user_id,
            'status', o.status,
            'totalAmount', o.total_amount,
            'createdAt', o.created_at,
            'updatedAt', o.updated_at,
            'items', COALESCE((
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', oi.id,
                        'productId', oi.product_id,
                        'productName', p.name,
                        'quantity', oi.quantity,
                        'unitPrice', oi.unit_price,
                        'totalPrice', oi.total_price
                    )
                    ORDER BY oi.id
                )
                FROM tb_order_items oi
                JOIN tb_products p ON p.id = oi.product_id
                WHERE oi.order_id = o.id
            ), '[]'::jsonb)
        ) as data
    FROM tb_orders o
    WHERE o.id = p_order_id
    ON CONFLICT (id) DO UPDATE
    SET data = EXCLUDED.data,
        updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Refresh popular products (called periodically or after significant changes)
CREATE OR REPLACE FUNCTION sync_refresh_popular_products()
RETURNS VOID AS $$
BEGIN
    -- Clear and rebuild popular products
    TRUNCATE tv_popular_products;

    INSERT INTO tv_popular_products (id, data)
    SELECT
        p.id,
        jsonb_build_object(
            'id', p.id,
            'name', p.name,
            'slug', p.slug,
            'price', p.price,
            'reviewCount', COALESCE(review_stats.review_count, 0),
            'averageRating', COALESCE(review_stats.avg_rating, 0.0),
            'totalRevenue', COALESCE(revenue_stats.total_revenue, 0.0)
        ) as data
    FROM tb_products p
    LEFT JOIN (
        SELECT
            product_id,
            COUNT(*)::int as review_count,
            AVG(rating)::float as avg_rating
        FROM tb_product_reviews
        GROUP BY product_id
    ) review_stats ON review_stats.product_id = p.id
    LEFT JOIN (
        SELECT
            oi.product_id,
            SUM(oi.total_price)::float as total_revenue
        FROM tb_order_items oi
        JOIN tb_orders o ON o.id = oi.order_id
        WHERE o.status = 'completed'
        GROUP BY oi.product_id
    ) revenue_stats ON revenue_stats.product_id = p.id
    WHERE p.is_active = true
    ORDER BY COALESCE(revenue_stats.total_revenue, 0) DESC, COALESCE(review_stats.avg_rating, 0) DESC;
END;
$$ LANGUAGE plpgsql;

-- Refresh user statistics
CREATE OR REPLACE FUNCTION sync_refresh_user_stats()
RETURNS VOID AS $$
BEGIN
    -- Clear and rebuild user stats
    TRUNCATE tv_user_stats;

    INSERT INTO tv_user_stats (id, data)
    SELECT
        u.id,
        jsonb_build_object(
            'userId', u.id,
            'username', u.username,
            'orderCount', COALESCE(order_stats.order_count, 0),
            'totalSpent', COALESCE(order_stats.total_spent, 0.0),
            'reviewCount', COALESCE(review_stats.review_count, 0),
            'averageRating', COALESCE(review_stats.avg_rating, 0.0)
        ) as data
    FROM tb_users u
    LEFT JOIN (
        SELECT
            user_id,
            COUNT(*)::int as order_count,
            SUM(total_amount)::float as total_spent
        FROM tb_orders
        WHERE status = 'completed'
        GROUP BY user_id
    ) order_stats ON order_stats.user_id = u.id
    LEFT JOIN (
        SELECT
            user_id,
            COUNT(*)::int as review_count,
            AVG(rating)::float as avg_rating
        FROM tb_product_reviews
        GROUP BY user_id
    ) review_stats ON review_stats.user_id = u.id
    WHERE u.is_active = true;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- CORE FUNCTIONS (Business Logic)
-- ============================================================================

-- Core function to create a user
CREATE OR REPLACE FUNCTION core_create_user(
    p_email TEXT,
    p_username TEXT,
    p_full_name TEXT,
    p_password_hash TEXT
) RETURNS JSONB AS $$
DECLARE
    v_user_id UUID;
    v_user_data JSONB;
BEGIN
    -- Check for existing user
    IF EXISTS (SELECT 1 FROM tb_users WHERE email = p_email) THEN
        RETURN jsonb_build_object(
            'error', 'Email already exists',
            'existing_username', (SELECT username FROM tb_users WHERE email = p_email)
        );
    END IF;

    IF EXISTS (SELECT 1 FROM tb_users WHERE username = p_username) THEN
        RETURN jsonb_build_object(
            'error', 'Username already taken',
            'suggested_username', p_username || '_' || substr(md5(random()::text), 1, 4)
        );
    END IF;

    -- Create user
    INSERT INTO tb_users (email, username, full_name, password_hash, is_active)
    VALUES (p_email, p_username, p_full_name, p_password_hash, true)
    RETURNING id INTO v_user_id;

    -- Get full user data
    SELECT row_to_json(u)::JSONB INTO v_user_data
    FROM tb_users u
    WHERE u.id = v_user_id;

    -- Update projections
    PERFORM sync_refresh_user_projection(v_user_id);
    PERFORM sync_refresh_user_stats();

    RETURN v_user_data;
EXCEPTION
    WHEN OTHERS THEN
        RETURN jsonb_build_object('error', SQLERRM);
END;
$$ LANGUAGE plpgsql;

-- Core function to create an order
CREATE OR REPLACE FUNCTION core_create_order(
    p_user_id UUID,
    p_items JSONB  -- Array of {product_id, quantity}
) RETURNS JSONB AS $$
DECLARE
    v_order_id UUID;
    v_total_amount DECIMAL(10,2) := 0;
    v_item JSONB;
    v_product RECORD;
    v_order_data JSONB;
BEGIN
    -- Validate user exists
    IF NOT EXISTS (SELECT 1 FROM tb_users WHERE id = p_user_id) THEN
        RETURN jsonb_build_object('error', 'User not found');
    END IF;

    -- Create order
    INSERT INTO tb_orders (user_id, status, total_amount)
    VALUES (p_user_id, 'pending', 0)
    RETURNING id INTO v_order_id;

    -- Process items
    FOR v_item IN SELECT * FROM jsonb_array_elements(p_items)
    LOOP
        -- Get product info
        SELECT id, price, stock_quantity, name
        INTO v_product
        FROM tb_products
        WHERE id = (v_item->>'product_id')::UUID;

        IF NOT FOUND THEN
            -- Rollback by deleting the order
            DELETE FROM tb_orders WHERE id = v_order_id;
            RETURN jsonb_build_object(
                'error', 'Product not found',
                'product_id', v_item->>'product_id'
            );
        END IF;

        -- Check stock
        IF v_product.stock_quantity < (v_item->>'quantity')::INTEGER THEN
            DELETE FROM tb_orders WHERE id = v_order_id;
            RETURN jsonb_build_object(
                'error', 'Insufficient stock',
                'product_name', v_product.name,
                'available', v_product.stock_quantity,
                'requested', (v_item->>'quantity')::INTEGER
            );
        END IF;

        -- Add order item
        INSERT INTO tb_order_items (
            order_id,
            product_id,
            quantity,
            unit_price,
            total_price
        ) VALUES (
            v_order_id,
            v_product.id,
            (v_item->>'quantity')::INTEGER,
            v_product.price,
            v_product.price * (v_item->>'quantity')::INTEGER
        );

        -- Update total
        v_total_amount := v_total_amount + (v_product.price * (v_item->>'quantity')::INTEGER);

        -- Update product stock
        UPDATE tb_products
        SET stock_quantity = stock_quantity - (v_item->>'quantity')::INTEGER
        WHERE id = v_product.id;
    END LOOP;

    -- Update order total
    UPDATE tb_orders
    SET total_amount = v_total_amount,
        status = 'confirmed'
    WHERE id = v_order_id;

    -- Get complete order data
    SELECT row_to_json(o)::JSONB INTO v_order_data
    FROM tb_orders o
    WHERE o.id = v_order_id;

    -- Update projections
    PERFORM sync_refresh_order_projection(v_order_id);
    PERFORM sync_increment_user_order_count(p_user_id);

    RETURN v_order_data;
EXCEPTION
    WHEN OTHERS THEN
        -- Clean up on error
        DELETE FROM tb_orders WHERE id = v_order_id;
        RETURN jsonb_build_object('error', SQLERRM);
END;
$$ LANGUAGE plpgsql;

-- Core function to add a product review
CREATE OR REPLACE FUNCTION core_add_product_review(
    p_user_id UUID,
    p_product_id UUID,
    p_rating INTEGER,
    p_title TEXT,
    p_comment TEXT
) RETURNS JSONB AS $$
DECLARE
    v_review_id UUID;
    v_review_data JSONB;
BEGIN
    -- Validate rating
    IF p_rating < 1 OR p_rating > 5 THEN
        RETURN jsonb_build_object('error', 'Rating must be between 1 and 5');
    END IF;

    -- Check if user already reviewed this product
    IF EXISTS (
        SELECT 1 FROM tb_product_reviews
        WHERE user_id = p_user_id AND product_id = p_product_id
    ) THEN
        RETURN jsonb_build_object(
            'error', 'You have already reviewed this product',
            'existing_review_id', (
                SELECT id FROM tb_product_reviews
                WHERE user_id = p_user_id AND product_id = p_product_id
            )
        );
    END IF;

    -- Create review
    INSERT INTO tb_product_reviews (
        user_id, product_id, rating, title, comment
    ) VALUES (
        p_user_id, p_product_id, p_rating, p_title, p_comment
    ) RETURNING id INTO v_review_id;

    -- Get review data
    SELECT row_to_json(r)::JSONB INTO v_review_data
    FROM tb_product_reviews r
    WHERE r.id = v_review_id;

    -- Update projections
    PERFORM sync_refresh_product_projection(p_product_id);
    PERFORM sync_refresh_user_projection(p_user_id);

    RETURN v_review_data;
EXCEPTION
    WHEN OTHERS THEN
        RETURN jsonb_build_object('error', SQLERRM);
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- API FUNCTIONS (Public Interface)
-- ============================================================================

-- Public API function to create a user
CREATE OR REPLACE FUNCTION api_create_user(p_input JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_user_data JSONB;
    v_start_time TIMESTAMP;
    v_execution_time_ms INTEGER;
BEGIN
    v_start_time := clock_timestamp();

    -- Input validation
    IF NOT (p_input ? 'email' AND p_input ? 'username' AND p_input ? 'fullName') THEN
        v_result.status := 'validation_error';
        v_result.message := 'Email, username, and fullName are required';
        RETURN v_result;
    END IF;

    -- Call core function
    v_user_data := core_create_user(
        p_email => p_input->>'email',
        p_username => p_input->>'username',
        p_full_name => p_input->>'fullName',
        p_password_hash => md5(COALESCE(p_input->>'password', 'default'))  -- Just for demo
    );

    -- Handle result
    IF v_user_data->>'error' IS NOT NULL THEN
        v_result.status := 'error';
        v_result.message := v_user_data->>'error';
        v_result.extra_metadata := v_user_data - 'error';
    ELSE
        v_result.id := (v_user_data->>'id')::UUID;
        v_result.status := 'success';
        v_result.message := 'User created successfully';
        v_result.object_data := v_user_data;
        v_result.updated_fields := ARRAY['email', 'username', 'full_name'];
    END IF;

    -- Calculate execution time
    v_execution_time_ms := EXTRACT(MILLISECONDS FROM clock_timestamp() - v_start_time)::INTEGER;

    -- Add metadata
    v_result.extra_metadata := COALESCE(v_result.extra_metadata, '{}'::jsonb) ||
        jsonb_build_object('execution_time_ms', v_execution_time_ms);

    RETURN v_result;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Public API function to create an order
CREATE OR REPLACE FUNCTION api_create_order(p_input JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_order_data JSONB;
    v_start_time TIMESTAMP;
BEGIN
    v_start_time := clock_timestamp();

    -- Validation
    IF NOT (p_input ? 'userId' AND p_input ? 'items') THEN
        v_result.status := 'validation_error';
        v_result.message := 'User ID and items are required';
        RETURN v_result;
    END IF;

    -- Call core function
    v_order_data := core_create_order(
        p_user_id => (p_input->>'userId')::UUID,
        p_items => p_input->'items'
    );

    -- Handle result
    IF v_order_data->>'error' IS NOT NULL THEN
        v_result.status := 'error';
        v_result.message := v_order_data->>'error';
        v_result.extra_metadata := v_order_data - 'error';
    ELSE
        v_result.id := (v_order_data->>'id')::UUID;
        v_result.status := 'success';
        v_result.message := 'Order created successfully';
        v_result.object_data := v_order_data;
    END IF;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Public API function to add a product review
CREATE OR REPLACE FUNCTION api_add_product_review(p_input JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_result mutation_result;
    v_review_data JSONB;
BEGIN
    -- Validation
    IF NOT (p_input ? 'userId' AND p_input ? 'productId' AND p_input ? 'rating') THEN
        v_result.status := 'validation_error';
        v_result.message := 'User ID, product ID, and rating are required';
        RETURN v_result;
    END IF;

    -- Call core function
    v_review_data := core_add_product_review(
        p_user_id => (p_input->>'userId')::UUID,
        p_product_id => (p_input->>'productId')::UUID,
        p_rating => (p_input->>'rating')::INTEGER,
        p_title => p_input->>'title',
        p_comment => p_input->>'comment'
    );

    -- Handle result
    IF v_review_data->>'error' IS NOT NULL THEN
        v_result.status := 'error';
        v_result.message := v_review_data->>'error';
        v_result.extra_metadata := v_review_data - 'error';
    ELSE
        v_result.id := (v_review_data->>'id')::UUID;
        v_result.status := 'success';
        v_result.message := 'Review added successfully';
        v_result.object_data := v_review_data;
    END IF;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- ============================================================================
-- INITIAL DATA POPULATION
-- ============================================================================

-- Function to populate all table views from base tables
CREATE OR REPLACE FUNCTION sync_populate_all_projections()
RETURNS VOID AS $$
DECLARE
    v_user_id UUID;
    v_product_id UUID;
    v_order_id UUID;
    v_category_id UUID;
BEGIN
    -- Populate user projections
    FOR v_user_id IN SELECT id FROM tb_users
    LOOP
        PERFORM sync_refresh_user_projection(v_user_id);
    END LOOP;

    -- Populate product projections
    FOR v_product_id IN SELECT id FROM tb_products
    LOOP
        PERFORM sync_refresh_product_projection(v_product_id);
    END LOOP;

    -- Populate order projections
    FOR v_order_id IN SELECT id FROM tb_orders
    LOOP
        PERFORM sync_refresh_order_projection(v_order_id);
    END LOOP;

    -- Populate category projections
    INSERT INTO tv_categories (id, data)
    SELECT
        c.id,
        jsonb_build_object(
            'id', c.id,
            'name', c.name,
            'slug', c.slug,
            'description', c.description,
            'parentId', c.parent_id
        )
    FROM tb_categories c
    ON CONFLICT (id) DO UPDATE
    SET data = EXCLUDED.data,
        updated_at = NOW();

    -- Populate aggregate views
    PERFORM sync_refresh_popular_products();
    PERFORM sync_refresh_user_stats();

    -- Populate products by category
    INSERT INTO tv_products_by_category (data)
    SELECT jsonb_build_object(
        'category', c.name,
        'products', jsonb_agg(
            jsonb_build_object(
                'id', p.id,
                'name', p.name,
                'slug', p.slug,
                'price', p.price,
                'stockQuantity', p.stock_quantity
            )
            ORDER BY p.name
        )
    )
    FROM tb_categories c
    JOIN tb_product_categories pc ON pc.category_id = c.id
    JOIN tb_products p ON p.id = pc.product_id
    WHERE p.is_active = true
    GROUP BY c.id, c.name;

    RAISE NOTICE 'All projections populated successfully';
END;
$$ LANGUAGE plpgsql;

-- Call the population function at the end
SELECT sync_populate_all_projections();
