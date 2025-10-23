-- Update product functions
-- App and core layers for product updates

-- App function: Update product (ultra-direct mutation)
CREATE OR REPLACE FUNCTION app.update_product(
    product_id UUID,
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_updated_data JSONB;
BEGIN
    -- Delegate to core business logic
    PERFORM core.update_product(
        product_id,
        input_payload->>'name',
        input_payload->>'description',
        input_payload->>'category_id',
        input_payload->>'brand',
        input_payload->>'tags',
        input_payload->>'is_active',
        input_payload->>'is_featured'
    );

    -- Get updated product data
    SELECT data INTO v_updated_data FROM tv_product WHERE id = product_id;

    -- Return ultra-direct response (Rust transformer handles formatting)
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        'Product updated successfully',
        jsonb_build_object('product', v_updated_data)
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Update product
CREATE OR REPLACE FUNCTION core.update_product(
    product_id UUID,
    new_name VARCHAR(200) DEFAULT NULL,
    new_description TEXT DEFAULT NULL,
    new_category_id UUID DEFAULT NULL,
    new_brand VARCHAR(100) DEFAULT NULL,
    new_tags TEXT[] DEFAULT NULL,
    new_is_active BOOLEAN DEFAULT NULL,
    new_is_featured BOOLEAN DEFAULT NULL
) RETURNS BOOLEAN AS $$
DECLARE
    product_slug VARCHAR(200);
BEGIN
    -- Generate new slug if name changed
    IF new_name IS NOT NULL THEN
        product_slug := lower(regexp_replace(new_name, '[^a-zA-Z0-9]+', '-', 'g'));
        product_slug := trim(both '-' from product_slug);

        -- Ensure unique slug (excluding current product)
        IF EXISTS (SELECT 1 FROM products WHERE slug = product_slug AND id != product_id) THEN
            product_slug := product_slug || '-' || extract(epoch from now())::text;
        END IF;
    END IF;

    UPDATE products
    SET
        name = COALESCE(new_name, name),
        slug = COALESCE(product_slug, slug),
        description = COALESCE(new_description, description),
        category_id = COALESCE(new_category_id, category_id),
        brand = COALESCE(new_brand, brand),
        tags = COALESCE(new_tags, tags),
        is_active = COALESCE(new_is_active, is_active),
        is_featured = COALESCE(new_is_featured, is_featured),
        updated_at = CURRENT_TIMESTAMP
    WHERE id = product_id;

    -- Sync projection tables
    PERFORM app.sync_tv_product();

    RETURN FOUND;
END;
$$ LANGUAGE plpgsql;
