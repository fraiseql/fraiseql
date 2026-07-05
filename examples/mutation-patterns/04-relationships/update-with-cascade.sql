-- ============================================================================
-- Pattern: Update with CASCADE Effects
-- ============================================================================
-- Use Case: Update a parent and report the affected related entities in the
--           typed cascade, so clients learn every side effect in one round trip.
--
-- This example shows:
-- - Updating the parent entity (Category)
-- - Cascading updates to children (Products)
-- - Assembling the cascade with the shipped `fraiseql.*` builders as an ARRAY of
--   UpdatedEntity entries (NOT an object keyed by type — that shape is rejected)
-- - Reading each cascade entity from its RLS-protected view (`v_*`), so cascade
--   row-visibility matches a query's
--
-- Requires `fraiseql setup` (installs `fraiseql.mutation_ok`, `fraiseql.build_cascade`,
-- `fraiseql.cascade_entity`, …) and the `v_category` / `v_product` read views.
-- ============================================================================

CREATE OR REPLACE FUNCTION graphql.update_category_and_products(input_payload jsonb)
RETURNS SETOF app.mutation_response AS $$
DECLARE
    v_category_id uuid := (input_payload->>'id')::uuid;
    v_new_name    text := input_payload->>'name';
    v_new_status  text := input_payload->>'status';
    v_old_status  text;
    v_product_ids uuid[];
    v_updated     jsonb;
BEGIN
    -- ------------------------------------------------------------------------
    -- Find and update the category
    -- ------------------------------------------------------------------------
    SELECT status INTO v_old_status FROM categories WHERE id = v_category_id;
    IF NOT FOUND THEN
        RETURN QUERY SELECT * FROM fraiseql.mutation_err(
            'not_found', 'Category not found', NULL, 404::smallint);
        RETURN;
    END IF;

    UPDATE categories
    SET name       = COALESCE(v_new_name, name),
        status     = COALESCE(v_new_status, status),
        updated_at = now()
    WHERE id = v_category_id;

    -- ------------------------------------------------------------------------
    -- CASCADE: disabling the category disables its products
    -- ------------------------------------------------------------------------
    IF v_new_status = 'disabled' AND v_old_status <> 'disabled' THEN
        WITH disabled AS (
            UPDATE products
            SET status = 'disabled', updated_at = now()
            WHERE category_id = v_category_id AND status <> 'disabled'
            RETURNING id
        )
        SELECT array_agg(id) INTO v_product_ids FROM disabled;
    END IF;

    -- ------------------------------------------------------------------------
    -- Assemble the cascade as an ARRAY of UpdatedEntity entries, each read from
    -- its RLS-protected view (never a base table). `build_cascade` drops any
    -- entry the caller cannot see.
    -- ------------------------------------------------------------------------
    v_updated := jsonb_build_array(
        fraiseql.cascade_entity('Category', v_category_id, 'UPDATED', 'v_category')
    );
    IF v_product_ids IS NOT NULL THEN
        v_updated := v_updated || (
            SELECT COALESCE(
                jsonb_agg(fraiseql.cascade_entity('Product', pid, 'UPDATED', 'v_product')),
                '[]'::jsonb)
            FROM unnest(v_product_ids) AS pid
        );
    END IF;

    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        p_entity         := (SELECT data FROM v_category WHERE id = v_category_id),
        p_entity_id      := v_category_id,
        p_entity_type    := 'Category',
        p_updated_fields := ARRAY['name', 'status'],
        p_cascade        := fraiseql.build_cascade(p_updated := v_updated)
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Usage
-- ============================================================================

-- Rename the category (no cascade side effects → cascade.updated has just the Category)
SELECT * FROM graphql.update_category_and_products('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "name": "Electronics & Gadgets"
}'::jsonb);

-- Disable the category (cascades to its products → cascade.updated lists each Product)
SELECT * FROM graphql.update_category_and_products('{
    "id": "660e8400-e29b-41d4-a716-446655440000",
    "status": "disabled"
}'::jsonb);
