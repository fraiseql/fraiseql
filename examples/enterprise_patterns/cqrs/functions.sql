-- CQRS Enterprise Pattern - Write Model Functions
-- PostgreSQL functions encapsulating business logic (Commands)

-- ============================================================================
-- HELPER FUNCTION: Log to Audit Trail
-- ============================================================================

CREATE OR REPLACE FUNCTION log_audit(
    p_tenant_id UUID,
    p_operation VARCHAR(20),
    p_entity_type VARCHAR(50),
    p_entity_id UUID,
    p_user_id UUID,
    p_old_values JSONB DEFAULT NULL,
    p_new_values JSONB DEFAULT NULL,
    p_changed_fields TEXT[] DEFAULT NULL,
    p_metadata JSONB DEFAULT NULL,
    p_ip_address INET DEFAULT NULL
)
RETURNS VOID AS $$
DECLARE
    v_operation_subtype TEXT;
BEGIN
    -- Determine operation subtype
    v_operation_subtype := CASE
        WHEN p_operation = 'INSERT' THEN 'new'
        WHEN p_operation = 'UPDATE' THEN 'updated'
        WHEN p_operation = 'DELETE' THEN 'deleted'
        ELSE 'unknown'
    END;

    -- Insert into unified audit_events table
    -- Crypto fields auto-populated by populate_crypto_trigger
    INSERT INTO audit_events (
        tenant_id, user_id, entity_type, entity_id,
        operation_type, operation_subtype, changed_fields,
        old_data, new_data, metadata, ip_address
    ) VALUES (
        p_tenant_id, p_user_id, p_entity_type, p_entity_id,
        p_operation, v_operation_subtype, p_changed_fields,
        p_old_values, p_new_values, p_metadata, p_ip_address
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- FUNCTION: Create Order
-- ============================================================================

CREATE OR REPLACE FUNCTION fn_create_order(
    p_customer_id INT,
    p_items JSONB,  -- Array of {product_id, quantity}
    p_notes TEXT DEFAULT NULL,
    p_changed_by VARCHAR(255) DEFAULT 'system'
)
RETURNS TABLE(
    id INT,
    order_number VARCHAR(50),
    customer_id INT,
    status VARCHAR(50),
    subtotal DECIMAL(10, 2),
    tax DECIMAL(10, 2),
    shipping DECIMAL(10, 2),
    total DECIMAL(10, 2),
    created_at TIMESTAMP,
    version INT
) AS $$
DECLARE
    v_order_id INT;
    v_order_number VARCHAR(50);
    v_subtotal DECIMAL(10, 2) := 0;
    v_tax DECIMAL(10, 2);
    v_shipping DECIMAL(10, 2) := 10.00;  -- Flat rate for simplicity
    v_total DECIMAL(10, 2);
    v_item JSONB;
    v_product_price DECIMAL(10, 2);
    v_product_available INT;
    v_item_subtotal DECIMAL(10, 2);
BEGIN
    -- Validate customer exists
    IF NOT EXISTS (SELECT 1 FROM tb_customers WHERE tb_customers.id = p_customer_id) THEN
        RAISE EXCEPTION 'Customer % does not exist', p_customer_id;
    END IF;

    -- Validate items array is not empty
    IF jsonb_array_length(p_items) = 0 THEN
        RAISE EXCEPTION 'Order must contain at least one item';
    END IF;

    -- Generate order number
    v_order_number := 'ORD-' || TO_CHAR(NOW(), 'YYYY') || '-' ||
                      LPAD((SELECT COALESCE(MAX(id), 0) + 1 FROM tb_orders)::TEXT, 5, '0');

    -- Validate all products and calculate subtotal
    FOR v_item IN SELECT * FROM jsonb_array_elements(p_items)
    LOOP
        -- Get product price and availability
        SELECT p.price, p.quantity_available - p.quantity_reserved
        INTO v_product_price, v_product_available
        FROM tb_products p
        WHERE p.id = (v_item->>'product_id')::INT
          AND p.is_active = true;

        IF NOT FOUND THEN
            RAISE EXCEPTION 'Product % does not exist or is not active',
                v_item->>'product_id';
        END IF;

        -- Check inventory
        IF v_product_available < (v_item->>'quantity')::INT THEN
            RAISE EXCEPTION 'Insufficient inventory for product %. Available: %, Requested: %',
                v_item->>'product_id', v_product_available, v_item->>'quantity';
        END IF;

        -- Calculate item subtotal
        v_item_subtotal := v_product_price * (v_item->>'quantity')::INT;
        v_subtotal := v_subtotal + v_item_subtotal;
    END LOOP;

    -- Calculate tax (10% for simplicity)
    v_tax := ROUND(v_subtotal * 0.10, 2);

    -- Calculate total
    v_total := v_subtotal + v_tax + v_shipping;

    -- Create order
    INSERT INTO tb_orders (
        order_number,
        customer_id,
        status,
        subtotal,
        tax,
        shipping,
        total,
        notes
    ) VALUES (
        v_order_number,
        p_customer_id,
        'pending',
        v_subtotal,
        v_tax,
        v_shipping,
        v_total,
        p_notes
    ) RETURNING tb_orders.id INTO v_order_id;

    -- Create order items and reserve inventory
    FOR v_item IN SELECT * FROM jsonb_array_elements(p_items)
    LOOP
        SELECT price INTO v_product_price
        FROM tb_products
        WHERE id = (v_item->>'product_id')::INT;

        v_item_subtotal := v_product_price * (v_item->>'quantity')::INT;

        INSERT INTO tb_order_items (
            order_id,
            product_id,
            quantity,
            unit_price,
            subtotal
        ) VALUES (
            v_order_id,
            (v_item->>'product_id')::INT,
            (v_item->>'quantity')::INT,
            v_product_price,
            v_item_subtotal
        );

        -- Reserve inventory
        UPDATE tb_products
        SET quantity_reserved = quantity_reserved + (v_item->>'quantity')::INT
        WHERE id = (v_item->>'product_id')::INT;
    END LOOP;

    -- Log to audit trail
    PERFORM log_audit(
        NULL, -- tenant_id (would need to be passed in)
        'INSERT',
        'order',
        v_order_id::TEXT::UUID, -- Convert to UUID
        NULL, -- user_id (would need to be passed in)
        NULL,
        jsonb_build_object(
            'order_number', v_order_number,
            'customer_id', p_customer_id,
            'status', 'pending',
            'total', v_total
        ),
        ARRAY['order_number', 'customer_id', 'status', 'subtotal', 'tax', 'shipping', 'total'],
        jsonb_build_object('business_action', 'order_created')
    );

    -- Return created order
    RETURN QUERY
    SELECT
        o.id,
        o.order_number,
        o.customer_id,
        o.status,
        o.subtotal,
        o.tax,
        o.shipping,
        o.total,
        o.created_at,
        o.version
    FROM tb_orders o
    WHERE o.id = v_order_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- FUNCTION: Process Payment (with Optimistic Locking)
-- ============================================================================

CREATE OR REPLACE FUNCTION fn_process_payment(
    p_order_id INT,
    p_amount DECIMAL(10, 2),
    p_payment_method VARCHAR(50),
    p_transaction_id VARCHAR(255) DEFAULT NULL,
    p_version INT DEFAULT NULL,  -- For optimistic locking
    p_changed_by VARCHAR(255) DEFAULT 'system'
)
RETURNS TABLE(
    id INT,
    order_number VARCHAR(50),
    status VARCHAR(50),
    total DECIMAL(10, 2),
    paid_at TIMESTAMP,
    version INT
) AS $$
DECLARE
    v_current_version INT;
    v_current_status VARCHAR(50);
    v_order_total DECIMAL(10, 2);
    v_payment_id INT;
BEGIN
    -- Get current order state
    SELECT o.version, o.status, o.total
    INTO v_current_version, v_current_status, v_order_total
    FROM tb_orders o
    WHERE o.id = p_order_id
    FOR UPDATE;  -- Lock the row

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Order % does not exist', p_order_id;
    END IF;

    -- Optimistic locking check
    IF p_version IS NOT NULL AND v_current_version != p_version THEN
        RAISE EXCEPTION 'Order % was modified by another user. Expected version %, but current version is %',
            p_order_id, p_version, v_current_version;
    END IF;

    -- Validate current status
    IF v_current_status != 'pending' THEN
        RAISE EXCEPTION 'Order % cannot be paid. Current status: %',
            p_order_id, v_current_status;
    END IF;

    -- Validate payment amount
    IF p_amount != v_order_total THEN
        RAISE EXCEPTION 'Payment amount % does not match order total %',
            p_amount, v_order_total;
    END IF;

    -- Create payment record
    INSERT INTO tb_payments (
        order_id,
        amount,
        payment_method,
        transaction_id,
        status,
        processed_at
    ) VALUES (
        p_order_id,
        p_amount,
        p_payment_method,
        p_transaction_id,
        'completed',
        NOW()
    ) RETURNING tb_payments.id INTO v_payment_id;

    -- Update order status
    UPDATE tb_orders
    SET
        status = 'paid',
        paid_at = NOW()
    WHERE tb_orders.id = p_order_id;

    -- Log to audit trail
    PERFORM log_audit(
        'UPDATE',
        'order',
        p_order_id,
        p_changed_by,
        jsonb_build_object('status', v_current_status),
        jsonb_build_object('status', 'paid', 'paid_at', NOW())
    );

    -- Return updated order
    RETURN QUERY
    SELECT
        o.id,
        o.order_number,
        o.status,
        o.total,
        o.paid_at,
        o.version
    FROM tb_orders o
    WHERE o.id = p_order_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- FUNCTION: Cancel Order
-- ============================================================================

CREATE OR REPLACE FUNCTION fn_cancel_order(
    p_order_id INT,
    p_reason TEXT,
    p_changed_by VARCHAR(255) DEFAULT 'system'
)
RETURNS TABLE(
    id INT,
    order_number VARCHAR(50),
    status VARCHAR(50),
    cancelled_at TIMESTAMP,
    cancellation_reason TEXT,
    refund_amount DECIMAL(10, 2)
) AS $$
DECLARE
    v_current_status VARCHAR(50);
    v_refund_amount DECIMAL(10, 2) := 0;
    v_payment_id INT;
BEGIN
    -- Get current order status
    SELECT o.status INTO v_current_status
    FROM tb_orders o
    WHERE o.id = p_order_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Order % does not exist', p_order_id;
    END IF;

    -- Validate can cancel
    IF v_current_status IN ('shipped', 'delivered') THEN
        RAISE EXCEPTION 'Order % cannot be cancelled. Current status: %',
            p_order_id, v_current_status;
    END IF;

    IF v_current_status = 'cancelled' THEN
        RAISE EXCEPTION 'Order % is already cancelled', p_order_id;
    END IF;

    -- Calculate refund if order was paid
    IF v_current_status IN ('paid', 'processing') THEN
        SELECT amount INTO v_refund_amount
        FROM tb_payments
        WHERE order_id = p_order_id
          AND status = 'completed'
        ORDER BY created_at DESC
        LIMIT 1;

        -- Process refund
        IF v_refund_amount IS NOT NULL AND v_refund_amount > 0 THEN
            SELECT id INTO v_payment_id
            FROM tb_payments
            WHERE order_id = p_order_id
              AND status = 'completed'
            ORDER BY created_at DESC
            LIMIT 1;

            UPDATE tb_payments
            SET
                status = 'refunded',
                refunded_at = NOW(),
                refund_amount = v_refund_amount
            WHERE id = v_payment_id;
        END IF;
    END IF;

    -- Release reserved inventory
    UPDATE tb_products p
    SET quantity_reserved = quantity_reserved - oi.quantity
    FROM tb_order_items oi
    WHERE oi.order_id = p_order_id
      AND oi.product_id = p.id;

    -- Update order status
    UPDATE tb_orders
    SET
        status = 'cancelled',
        cancelled_at = NOW(),
        cancellation_reason = p_reason
    WHERE tb_orders.id = p_order_id;

    -- Log to audit trail
    PERFORM log_audit(
        'UPDATE',
        'order',
        p_order_id,
        p_changed_by,
        jsonb_build_object('status', v_current_status),
        jsonb_build_object(
            'status', 'cancelled',
            'cancellation_reason', p_reason,
            'refund_amount', v_refund_amount
        )
    );

    -- Return updated order
    RETURN QUERY
    SELECT
        o.id,
        o.order_number,
        o.status,
        o.cancelled_at,
        o.cancellation_reason,
        v_refund_amount
    FROM tb_orders o
    WHERE o.id = p_order_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- FUNCTION: Update Order Status
-- ============================================================================

CREATE OR REPLACE FUNCTION fn_update_order_status(
    p_order_id INT,
    p_new_status VARCHAR(50),
    p_changed_by VARCHAR(255) DEFAULT 'system'
)
RETURNS TABLE(
    id INT,
    order_number VARCHAR(50),
    status VARCHAR(50),
    shipped_at TIMESTAMP,
    delivered_at TIMESTAMP,
    version INT
) AS $$
DECLARE
    v_current_status VARCHAR(50);
    v_old_values JSONB;
    v_new_values JSONB;
BEGIN
    -- Get current status
    SELECT o.status INTO v_current_status
    FROM tb_orders o
    WHERE o.id = p_order_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Order % does not exist', p_order_id;
    END IF;

    -- Validate status transition
    IF v_current_status = 'cancelled' THEN
        RAISE EXCEPTION 'Cannot change status of cancelled order';
    END IF;

    -- Build old/new values for audit
    v_old_values := jsonb_build_object('status', v_current_status);
    v_new_values := jsonb_build_object('status', p_new_status);

    -- Update order with appropriate timestamps
    UPDATE tb_orders
    SET
        status = p_new_status,
        shipped_at = CASE
            WHEN p_new_status = 'shipped' AND shipped_at IS NULL THEN NOW()
            ELSE shipped_at
        END,
        delivered_at = CASE
            WHEN p_new_status = 'delivered' AND delivered_at IS NULL THEN NOW()
            ELSE delivered_at
        END
    WHERE tb_orders.id = p_order_id;

    -- If shipped or delivered, release reserved inventory and deduct from available
    IF p_new_status IN ('shipped', 'delivered') AND v_current_status NOT IN ('shipped', 'delivered') THEN
        UPDATE tb_products p
        SET
            quantity_reserved = quantity_reserved - oi.quantity,
            quantity_available = quantity_available - oi.quantity
        FROM tb_order_items oi
        WHERE oi.order_id = p_order_id
          AND oi.product_id = p.id;
    END IF;

    -- Log to audit trail
    PERFORM log_audit(
        'UPDATE',
        'order',
        p_order_id,
        p_changed_by,
        v_old_values,
        v_new_values
    );

    -- Return updated order
    RETURN QUERY
    SELECT
        o.id,
        o.order_number,
        o.status,
        o.shipped_at,
        o.delivered_at,
        o.version
    FROM tb_orders o
    WHERE o.id = p_order_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- FUNCTION: Add Product
-- ============================================================================

CREATE OR REPLACE FUNCTION fn_add_product(
    p_sku VARCHAR(100),
    p_name VARCHAR(255),
    p_description TEXT,
    p_price DECIMAL(10, 2),
    p_cost DECIMAL(10, 2),
    p_quantity_available INT DEFAULT 0,
    p_changed_by VARCHAR(255) DEFAULT 'system'
)
RETURNS TABLE(
    id INT,
    sku VARCHAR(100),
    name VARCHAR(255),
    price DECIMAL(10, 2),
    quantity_available INT,
    created_at TIMESTAMP
) AS $$
DECLARE
    v_product_id INT;
BEGIN
    -- Validate SKU is unique
    IF EXISTS (SELECT 1 FROM tb_products WHERE tb_products.sku = p_sku) THEN
        RAISE EXCEPTION 'Product with SKU % already exists', p_sku;
    END IF;

    -- Create product
    INSERT INTO tb_products (
        sku,
        name,
        description,
        price,
        cost,
        quantity_available
    ) VALUES (
        p_sku,
        p_name,
        p_description,
        p_price,
        p_cost,
        p_quantity_available
    ) RETURNING tb_products.id INTO v_product_id;

    -- Log to audit trail
    PERFORM log_audit(
        'INSERT',
        'product',
        v_product_id,
        p_changed_by,
        NULL,
        jsonb_build_object(
            'sku', p_sku,
            'name', p_name,
            'price', p_price,
            'quantity_available', p_quantity_available
        )
    );

    -- Return created product
    RETURN QUERY
    SELECT
        p.id,
        p.sku,
        p.name,
        p.price,
        p.quantity_available,
        p.created_at
    FROM tb_products p
    WHERE p.id = v_product_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- FUNCTION: Update Product Inventory
-- ============================================================================

CREATE OR REPLACE FUNCTION fn_update_product_inventory(
    p_product_id INT,
    p_quantity_change INT,  -- Positive to add, negative to remove
    p_changed_by VARCHAR(255) DEFAULT 'system'
)
RETURNS TABLE(
    id INT,
    sku VARCHAR(100),
    name VARCHAR(255),
    quantity_available INT,
    quantity_reserved INT,
    quantity_in_stock INT
) AS $$
DECLARE
    v_old_quantity INT;
    v_new_quantity INT;
BEGIN
    -- Get current quantity
    SELECT quantity_available INTO v_old_quantity
    FROM tb_products
    WHERE id = p_product_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Product % does not exist', p_product_id;
    END IF;

    v_new_quantity := v_old_quantity + p_quantity_change;

    -- Validate new quantity
    IF v_new_quantity < 0 THEN
        RAISE EXCEPTION 'Cannot reduce inventory below zero. Current: %, Change: %',
            v_old_quantity, p_quantity_change;
    END IF;

    -- Update inventory
    UPDATE tb_products
    SET quantity_available = v_new_quantity
    WHERE id = p_product_id;

    -- Log to audit trail
    PERFORM log_audit(
        'UPDATE',
        'product',
        p_product_id,
        p_changed_by,
        jsonb_build_object('quantity_available', v_old_quantity),
        jsonb_build_object('quantity_available', v_new_quantity)
    );

    -- Return updated product
    RETURN QUERY
    SELECT
        p.id,
        p.sku,
        p.name,
        p.quantity_available,
        p.quantity_reserved,
        (p.quantity_available - p.quantity_reserved) as quantity_in_stock
    FROM tb_products p
    WHERE p.id = p_product_id;
END;
$$ LANGUAGE plpgsql;
