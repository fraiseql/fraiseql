--
-- ============================================================================
-- Function: testfoundry_random_value_from_mapping
-- Purpose:
--   Randomly select a value based on FK mapping definitions.
--   Supports parameterized random selection with safe dynamic SQL.
-- ============================================================================

CREATE OR REPLACE FUNCTION testfoundry_random_value_from_mapping(
    p_input_type TEXT,
    p_args TEXT[] DEFAULT ARRAY[]::TEXT[]
)
RETURNS JSONB
LANGUAGE plpgsql
AS $$
DECLARE
    v_mapping RECORD;
    v_sql TEXT;
    v_where TEXT;
    v_result JSONB;
    v_casted_values TEXT[];
    v_idx INT;
BEGIN
    -- Step 1: Fetch mapping
    SELECT
        from_expression,
        COALESCE(random_select_expression,
                 CASE WHEN random_pk_field IS NOT NULL AND random_value_field IS NOT NULL
                      THEN testfoundry_select_pk_value(random_pk_field, random_value_field)
                      ELSE select_field
                 END) AS select_expr,
        select_field,
        random_select_where,
        dependency_fields,
        dependency_field_mapping,
        dependency_field_types
    INTO v_mapping
    FROM testfoundry_tb_fk_mapping
    WHERE input_type = p_input_type;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'No FK mapping found for input_type: %', p_input_type;
    END IF;

    -- Step 2: Build WHERE clause from dependency mappings
    v_where := testfoundry_build_conditions(
        v_mapping.dependency_fields,
        v_mapping.dependency_field_mapping
    );

    -- Step 3: Build SQL statement
    v_sql := 'SELECT to_jsonb(' || v_mapping.select_expr || ') FROM ' || v_mapping.from_expression;

    IF v_mapping.random_select_where IS NOT NULL THEN
        v_sql := v_sql || ' WHERE (' || v_mapping.random_select_where || ')';
    END IF;

    IF v_where IS NOT NULL AND length(trim(v_where)) > 0 THEN
        IF v_mapping.random_select_where IS NOT NULL THEN
            v_sql := v_sql || ' AND (' || v_where || ')';
        ELSE
            v_sql := v_sql || ' WHERE ' || v_where;
        END IF;
    END IF;

    v_sql := v_sql || ' ORDER BY random() LIMIT 1';

    -- Step 4: Cast arguments according to dependency_field_types
    v_casted_values := ARRAY[]::TEXT[];

    IF array_length(p_args, 1) IS NOT NULL THEN
        FOR v_idx IN 1..array_length(p_args, 1)
        LOOP
            CASE v_mapping.dependency_field_types[v_idx]
                WHEN 'uuid' THEN
                    v_casted_values := v_casted_values || (p_args[v_idx])::UUID;
                WHEN 'integer' THEN
                    v_casted_values := v_casted_values || (p_args[v_idx])::INTEGER;
                WHEN 'text' THEN
                    v_casted_values := v_casted_values || (p_args[v_idx])::TEXT;
                ELSE
                    v_casted_values := v_casted_values || p_args[v_idx]; -- fallback as text
            END CASE;
        END LOOP;

        -- Step 5: Execute safely with casted parameters
        CASE array_length(v_casted_values, 1)
            WHEN 1 THEN EXECUTE v_sql INTO v_result USING v_casted_values[1];
            WHEN 2 THEN EXECUTE v_sql INTO v_result USING v_casted_values[1], v_casted_values[2];
            WHEN 3 THEN EXECUTE v_sql INTO v_result USING v_casted_values[1], v_casted_values[2], v_casted_values[3];
            WHEN 4 THEN EXECUTE v_sql INTO v_result USING v_casted_values[1], v_casted_values[2], v_casted_values[3], v_casted_values[4];
            ELSE
                RAISE EXCEPTION 'Too many parameters passed for testfoundry_random_value_from_mapping (limit 4)';
        END CASE;
    ELSE
        EXECUTE v_sql INTO v_result;
    END IF;

    RETURN v_result;
END;
$$;
