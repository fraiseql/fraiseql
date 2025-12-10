-- ============================================================================
-- Pattern: Calculated Fields
-- ============================================================================
-- Use Case: Auto-compute derived fields based on input
-- Benefits: Consistency, reduced client complexity, business logic centralization
--
-- This example shows:
-- - Calculating totals and discounts
-- - Auto-generating slugs
-- - Computing expiration dates
-- - Setting default values based on logic
-- ============================================================================

CREATE OR REPLACE FUNCTION create_order(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    order_record record;

    -- Input fields
    customer_id uuid := (input_payload->>'customer_id')::uuid;
    items jsonb := input_payload->'items';
    discount_code text := input_payload->>'discount_code';
    shipping_type text := COALESCE(input_payload->>'shipping_type', 'standard');

    -- Calculated fields
    subtotal numeric := 0;
    discount_percent numeric := 0;
    discount_amount numeric := 0;
    shipping_cost numeric := 0;
    tax_rate numeric := 0.10;  -- 10% tax
    tax_amount numeric := 0;
    total_amount numeric := 0;
    estimated_delivery_date date;
    priority_level text;
BEGIN
    -- ========================================================================
    -- Validate Input
    -- ========================================================================

    IF customer_id IS NULL THEN
        result.status := 'failed:validation';
        result.message := 'Customer ID is required';
        RETURN result;
    END IF;

    IF items IS NULL OR jsonb_array_length(items) = 0 THEN
        result.status := 'failed:validation';
        result.message := 'At least one item is required';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Calculate: Subtotal
    -- ========================================================================

    SELECT SUM((item->>'quantity')::numeric * (item->>'price')::numeric)
    INTO subtotal
    FROM jsonb_array_elements(items) AS item;

    -- ========================================================================
    -- Calculate: Discount
    -- ========================================================================

    CASE discount_code
        WHEN 'SAVE10' THEN discount_percent := 0.10;
        WHEN 'SAVE20' THEN discount_percent := 0.20;
        WHEN 'VIP50' THEN discount_percent := 0.50;
        ELSE discount_percent := 0;
    END CASE;

    discount_amount := subtotal * discount_percent;

    -- ========================================================================
    -- Calculate: Shipping Cost
    -- ========================================================================

    shipping_cost := CASE shipping_type
        WHEN 'standard' THEN 5.00
        WHEN 'express' THEN 15.00
        WHEN 'overnight' THEN 30.00
        ELSE 5.00
    END;

    -- Free shipping for orders over $100 after discount
    IF (subtotal - discount_amount) >= 100 THEN
        shipping_cost := 0;
    END IF;

    -- ========================================================================
    -- Calculate: Tax (on subtotal - discount + shipping)
    -- ========================================================================

    tax_amount := (subtotal - discount_amount + shipping_cost) * tax_rate;

    -- ========================================================================
    -- Calculate: Total
    -- ========================================================================

    total_amount := subtotal - discount_amount + shipping_cost + tax_amount;

    -- ========================================================================
    -- Calculate: Estimated Delivery Date
    -- ========================================================================

    estimated_delivery_date := CURRENT_DATE + CASE shipping_type
        WHEN 'standard' THEN INTERVAL '5 days'
        WHEN 'express' THEN INTERVAL '2 days'
        WHEN 'overnight' THEN INTERVAL '1 day'
        ELSE INTERVAL '5 days'
    END;

    -- ========================================================================
    -- Calculate: Priority Level
    -- ========================================================================

    priority_level := CASE
        WHEN total_amount > 500 THEN 'high'
        WHEN total_amount > 200 THEN 'medium'
        ELSE 'normal'
    END;

    -- ========================================================================
    -- Create Order with Calculated Fields
    -- ========================================================================

    INSERT INTO orders (
        customer_id,
        items,
        subtotal,
        discount_code,
        discount_percent,
        discount_amount,
        shipping_type,
        shipping_cost,
        tax_amount,
        total_amount,
        estimated_delivery_date,
        priority_level,
        status
    ) VALUES (
        customer_id,
        items,
        subtotal,
        discount_code,
        discount_percent,
        discount_amount,
        shipping_type,
        shipping_cost,
        tax_amount,
        total_amount,
        estimated_delivery_date,
        priority_level,
        'pending'
    )
    RETURNING * INTO order_record;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'created';
    result.message := format('Order created. Total: $%.2f', total_amount);
    result.entity := row_to_json(order_record);
    result.entity_id := order_record.id::text;
    result.entity_type := 'Order';
    result.metadata := jsonb_build_object(
        'calculations', jsonb_build_object(
            'subtotal', subtotal,
            'discount_applied', discount_amount,
            'shipping_cost', shipping_cost,
            'tax_amount', tax_amount,
            'total_amount', total_amount,
            'free_shipping', shipping_cost = 0
        )
    );

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Usage Examples
-- ============================================================================

-- Standard order with discount
SELECT * FROM create_order('{
    "customer_id": "550e8400-e29b-41d4-a716-446655440000",
    "items": [
        {"product": "Widget", "quantity": 2, "price": 25.00},
        {"product": "Gadget", "quantity": 1, "price": 50.00}
    ],
    "discount_code": "SAVE10",
    "shipping_type": "standard"
}'::jsonb);
-- Returns: status='created'
-- Calculations: subtotal=100, discount=10, shipping=5 (free if >$100), tax, total

-- Large order with free shipping
SELECT * FROM create_order('{
    "customer_id": "550e8400-e29b-41d4-a716-446655440000",
    "items": [
        {"product": "Laptop", "quantity": 1, "price": 1200.00}
    ],
    "discount_code": "SAVE20",
    "shipping_type": "express"
}'::jsonb);
-- Returns: status='created', priority='high', free_shipping=true
-- Calculations: subtotal=1200, discount=240, shipping=0 (free), total=1056

-- Overnight shipping
SELECT * FROM create_order('{
    "customer_id": "550e8400-e29b-41d4-a716-446655440000",
    "items": [
        {"product": "Book", "quantity": 3, "price": 15.00}
    ],
    "shipping_type": "overnight"
}'::jsonb);
-- Returns: estimated_delivery_date=tomorrow
-- Calculations: subtotal=45, shipping=30, total with tax

-- ============================================================================
-- Key Takeaways
-- ============================================================================

/*
Calculated Fields Pattern Benefits:
1. Client doesn't need to know calculation logic
2. Calculations are consistent across all clients
3. Business rules centralized in database
4. Calculations are atomic with transaction
5. Audit trail of exactly what was calculated

Common Calculated Fields:
- Totals, subtotals, taxes
- Discounts and promotions
- Slugs from titles
- Expiration dates from durations
- Status/priority from amounts
- Search vectors from text fields
*/
