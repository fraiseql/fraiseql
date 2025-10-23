-- Create product functions
-- App and core layers for product creation

-- App function: Create product (ultra-direct mutation)
CREATE OR REPLACE FUNCTION app.create_product(
    input_payload JSONB
) RETURNS JSONB AS $$
DECLARE
    v_product_id UUID;
BEGIN
    -- Delegate to core business logic
    v_product_id := core.create_product(
        input_payload->>'sku',
        input_payload->>'name',
        input_payload->>'description',
        input_payload->>'category_id',
        input_payload->>'brand',
        input_payload->>'tags'
    );

    -- Return ultra-direct response (Rust transformer handles formatting)
    RETURN app.build_mutation_response(
        true,
        'SUCCESS',
        'Product created successfully',
        jsonb_build_object(
            'product', jsonb_build_object(
                'id', v_product_id,
                'sku', input_payload->>'sku',
                'name', input_payload->>'name',
                'description', input_payload->>'description'
            )
        )
    );
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Core function: Create product
CREATE OR REPLACE FUNCTION core.create_product(
    product_sku VARCHAR(50),
    product_name VARCHAR(200),
    product_description TEXT DEFAULT NULL,
    product_category_id UUID DEFAULT NULL,
    product_brand VARCHAR(100) DEFAULT NULL,
    product_tags TEXT[] DEFAULT NULL
) RETURNS UUID AS $$
DECLARE
    new_product_id UUID;
    product_slug VARCHAR(200);
BEGIN
    -- Business logic validation
    IF product_sku IS NULL OR product_name IS NULL THEN
        RAISE EXCEPTION 'SKU and name are required';
    END IF;

    -- Check duplicate SKU (business rule)
    IF EXISTS (SELECT 1 FROM products WHERE sku = product_sku) THEN
        RAISE EXCEPTION 'Product with SKU % already exists', product_sku;
    END IF;

    -- Generate slug from name
    product_slug := lower(regexp_replace(product_name, '[^a-zA-Z0-9]+', '-', 'g'));
    product_slug := trim(both '-' from product_slug);

    -- Ensure unique slug
    IF EXISTS (SELECT 1 FROM products WHERE slug = product_slug) THEN
        product_slug := product_slug || '-' || extract(epoch from now())::text;
    END IF;

    -- Generate UUID and create product
    new_product_id := gen_random_uuid();

    INSERT INTO products (id, sku, name, slug, description, category_id, brand, tags)
    VALUES (new_product_id, product_sku, product_name, product_slug, product_description, product_category_id, product_brand, product_tags);

    -- Sync projection tables (explicit sync)
    PERFORM app.sync_tv_product();

    RETURN new_product_id;
END;
$$ LANGUAGE plpgsql;
