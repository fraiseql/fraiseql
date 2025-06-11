-- ============================================================================
-- Function: testfoundry_generate_field_value
-- Purpose:
--   Generates the appropriate random value for a field, using:
--     - Custom random functions
--     - FK mapping resolution
--     - Nested composite input generation
--     - Fallback to simple random value based on type
--   Supports automatic handling of {pk,value} pairs (e.g., postal_code + fk_postal_code).
-- ============================================================================

CREATE OR REPLACE FUNCTION testfoundry_generate_field_value(
    p_field RECORD,
    p_current_json JSONB
)
RETURNS JSONB
LANGUAGE plpgsql
AS $$
DECLARE
    v_value JSONB;
    v_fk_args TEXT[];
    v_dep_field TEXT;
BEGIN
    -- Random generator
    IF p_field.generator_type = 'random' THEN
        IF p_field.random_function IS NOT NULL THEN
            EXECUTE format('SELECT %I()', p_field.random_function) INTO v_value;
        ELSE
            -- Fallback to random text
            v_value := to_jsonb(substr(md5(random()::text), 1, 12));
        END IF;

    -- Foreign Key resolver
    ELSIF p_field.generator_type = 'resolve_fk' THEN
        v_fk_args := ARRAY[]::TEXT[];

        IF p_field.fk_dependency_fields IS NOT NULL THEN
            FOREACH v_dep_field IN ARRAY p_field.fk_dependency_fields
            LOOP
                -- 🌟 Prefer fk_ field if exists
                IF p_current_json ? ('fk_' || v_dep_field) THEN
                    v_fk_args := array_append(v_fk_args, p_current_json ->> ('fk_' || v_dep_field));
                ELSE
                    v_fk_args := array_append(v_fk_args, p_current_json ->> v_dep_field);
                END IF;
            END LOOP;
        END IF;

        -- Call FK resolver
        v_value := testfoundry_random_value_from_mapping(p_field.fk_mapping_key, v_fk_args);

    -- Nested types
    ELSIF p_field.generator_type = 'nested' THEN
        v_value := testfoundry_generate_random_input(p_field.nested_type);

    ELSE
        -- Fallback to simple random generation based on type
        v_value := to_jsonb(testfoundry_random_value(p_field.field_type));
    END IF;

    RETURN v_value;
END;
$$;
