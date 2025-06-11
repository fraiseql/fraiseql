CREATE OR REPLACE FUNCTION testfoundry_generate_duplicate_fieldset(
    p_entity TEXT,
    p_base_view TEXT,
    p_constraint TEXT,
    p_id UUID,
    p_schema TEXT DEFAULT 'public'
)
RETURNS UUID
LANGUAGE plpgsql
AS $$
DECLARE
    v_fields TEXT[];
    v_json JSONB := '{}';
    v_value TEXT;
    v_field TEXT;
BEGIN
    -- Build a JSONB object from the given constraint columns
    v_fields := string_to_array(p_constraint, ',');

    FOREACH v_field IN ARRAY v_fields LOOP
        EXECUTE format('SELECT %I FROM %s WHERE id = $1', v_field, p_base_view)
        INTO v_value
        USING p_id;

        v_json := v_json || jsonb_build_object(v_field, v_value);
    END LOOP;

    -- Now insert using insert_entity(p_overrides)
    RETURN insert_entity(
        p_entity := p_entity,
        p_timing := 'current',
        p_schema := p_schema,
        p_overrides := v_json
    );
END;
$$;
