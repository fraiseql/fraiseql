-- ============================================================================
-- Pattern: Update with CASCADE Effects
-- ============================================================================
-- Use Case: Update parent and report changes to related entities
-- Benefits: Transparency of side effects, audit trail, client awareness
--
-- This example shows:
-- - Updating parent entity
-- - Cascading updates to children
-- - Reporting all affected entities
-- - Using CASCADE for transparency
-- ============================================================================

CREATE OR REPLACE FUNCTION update_category_and_products(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    category_record record;
    category_id uuid := (input_payload->>'id')::uuid;
    new_name text := input_payload->>'name';
    new_status text := input_payload->>'status';
    affected_products jsonb;
    updated_count int;
BEGIN
    -- ========================================================================
    -- Find and Update Category
    -- ========================================================================

    SELECT * INTO category_record FROM categories WHERE id = category_id;
    IF NOT FOUND THEN
        result.status := 'not_found:category';
        result.message := 'Category not found';
        RETURN result;
    END IF;

    UPDATE categories
    SET
        name = COALESCE(new_name, name),
        status = COALESCE(new_status, status),
        updated_at = now()
    WHERE id = category_id
    RETURNING * INTO category_record;

    -- ========================================================================
    -- CASCADE: Update Related Products
    -- ========================================================================

    -- If category was disabled, disable all products
    IF new_status = 'disabled' AND category_record.status != 'disabled' THEN
        WITH updated AS (
            UPDATE products
            SET status = 'disabled', updated_at = now()
            WHERE category_id = category_id AND status != 'disabled'
            RETURNING *
        )
        SELECT jsonb_agg(row_to_json(updated)) INTO affected_products FROM updated;

        GET DIAGNOSTICS updated_count = ROW_COUNT;
    ELSE
        affected_products := '[]'::jsonb;
        updated_count := 0;
    END IF;

    -- ========================================================================
    -- Success Response
    -- ========================================================================

    result.status := 'updated';
    result.message := format('Category updated. %s product(s) affected', updated_count);
    result.entity := row_to_json(category_record);
    result.entity_id := category_record.id::text;
    result.entity_type := 'Category';
    result.updated_fields := ARRAY['name', 'status'];

    -- Report CASCADE effects
    IF updated_count > 0 THEN
        result.cascade := jsonb_build_object(
            'updated', jsonb_build_object(
                'products', affected_products,
                'count', updated_count,
                'reason', 'Category disabled'
            )
        );
    END IF;

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

-- Update category name
SELECT * FROM update_category_and_products('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "name": "Electronics & Gadgets"
}'::jsonb);
-- Returns: status='updated', cascade.updated.products=[] (no side effects)

-- Disable category (cascades to products)
SELECT * FROM update_category_and_products('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "status": "disabled"
}'::jsonb);
-- Returns: status='updated', cascade.updated.products=[...], count=15
