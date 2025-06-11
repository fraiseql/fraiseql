-- ============================================================================
-- Function: testfoundry_infer_dependency_field_types
-- Purpose:
--   Dynamically infer the types (text, uuid, integer, etc.) for each dependency field.
-- ============================================================================

CREATE OR REPLACE FUNCTION testfoundry_infer_dependency_field_types(
    p_from_expression TEXT,
    p_dependency_field_mapping JSONB
)
RETURNS TEXT[]
LANGUAGE plpgsql
AS $$
DECLARE
    v_sql TEXT;
    v_field RECORD;
    v_field_list TEXT := '';
    v_types TEXT[];
BEGIN
    -- 1. Prepare the list of real DB fields
    FOR v_field IN SELECT * FROM jsonb_each_text(p_dependency_field_mapping)
    LOOP
        IF v_field_list <> '' THEN
            v_field_list := v_field_list || ', ';
        END IF;
        v_field_list := v_field_list || v_field.value;
    END LOOP;

    IF v_field_list = '' THEN
        RETURN NULL;
    END IF;

    -- 2. Build SQL
    v_sql := 'SELECT ' ||
        array_to_string(
            ARRAY(
                SELECT format('pg_typeof(%s)', f)
                FROM string_to_array(v_field_list, ',') AS f
            ),
            ', '
        ) || ' FROM ' || p_from_expression || ' LIMIT 1';

    -- 3. Execute
    EXECUTE v_sql INTO v_types;

    RETURN v_types;
END;
$$;
