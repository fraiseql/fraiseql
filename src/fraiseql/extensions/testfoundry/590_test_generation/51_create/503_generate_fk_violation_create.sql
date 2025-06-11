CREATE OR REPLACE FUNCTION _testfoundry_generate_fk_violation_create_test(
    p_entity TEXT,
    p_schema TEXT DEFAULT 'public'
)
RETURNS TEXT
LANGUAGE plpgsql
AS $$
DECLARE
    v_result TEXT := '';
    v_base_view TEXT;
    v_table TEXT;
    v_json_fields TEXT := '';
    v_input_field TEXT;
    v_structure RECORD;
    r_fk RECORD;
    r_col RECORD;
    v_n_assertions INT := 2;
BEGIN
    -- Setup names
    v_table := format('%I.tb_%s', p_schema, p_entity);
    v_base_view := format('%I.v_%s', p_schema, p_entity);

    -- Fetch entity structure
    SELECT * INTO v_structure FROM testfoundry_get_entity_structure(p_entity, p_schema);

    IF NOT v_structure.base_view_exists THEN
        RAISE EXCEPTION 'Base view %.v_% does not exist', p_schema, p_entity;
    END IF;

    -- Header
    v_result := format('-- FK violation CREATE test for entity %s\n', p_entity);
    v_result := v_result || format('SELECT plan(%s);\n\n', v_n_assertions);

    -- Declare variables using utility
    v_result := v_result || generate_authentication_vars();

    -- Begin FK violation attempt
    v_result := v_result || format('SELECT * INTO v_result FROM create_%s_with_log(\n', p_entity);
    v_result := v_result || '    :v_org,\n';
    v_result := v_result || '    :v_user,\n';
    v_result := v_result || format('    %s\n', quote_literal(generate_invalid_fk_input(p_entity, p_schema)::text));
    v_result := v_result || ');\n\n';

    -- Assertions
    v_result := v_result || 'SELECT like(v_result.status, ''error%'', ''FK violation should trigger an error'');\n';

    -- Finish
    v_result := v_result || '\nSELECT * FROM finish();\n';

    RETURN v_result;
END;
$$;
