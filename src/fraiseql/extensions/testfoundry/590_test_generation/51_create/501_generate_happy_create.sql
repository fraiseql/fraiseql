CREATE OR REPLACE FUNCTION _testfoundry_generate_happy_create(
    p_entity TEXT,
    p_schema TEXT DEFAULT 'public'
)
RETURNS TEXT
LANGUAGE plpgsql
AS $$
DECLARE
    v_result TEXT := '';
    v_base_view TEXT;
    v_proj_table TEXT;
    v_input_json JSONB;
    v_plan_tests INTEGER := 3;
    v_structure RECORD;
BEGIN
    -- View and table names
    v_base_view := format('%I.v_%s', p_schema, p_entity);
    v_proj_table := format('%I.tv_%s', p_schema, p_entity);

    -- Fetch entity structure
    SELECT * INTO v_structure FROM testfoundry_get_entity_structure(p_entity, p_schema);

    IF NOT v_structure.base_view_exists THEN
        RAISE EXCEPTION 'Base view %.v_% does not exist', p_schema, p_entity;
    END IF;

    -- Adjust plan test count
    v_plan_tests := v_plan_tests + CASE WHEN v_structure.proj_table_exists THEN 2 ELSE 1 END;

    -- Header
    v_result := format('-- Happy path CREATE test for entity %s\n', p_entity);
    v_result := v_result || format('SELECT plan(%s);\n\n', v_plan_tests);

    -- Declare variables using utility
    v_result := v_result || generate_authentication_vars();

    -- Generate random input using helper function
    SELECT testfoundry_generate_random_input(p_entity) INTO v_input_json;

    -- Call the mutation
    v_result := v_result || format('SELECT * INTO v_result FROM create_%s_with_log(\n', p_entity);
    v_result := v_result || '    :v_org,\n';
    v_result := v_result || '    :v_user,\n';
    v_result := v_result || format('    %s\n', quote_literal(v_input_json::text)::text);
    v_result := v_result || ');\n\n';

    -- Assertions
    v_result := v_result || 'SELECT is(v_result.status, ''new'', ''Status should be new'');\n';
    v_result := v_result || format('SELECT is(v_result.entity, ''%s'', ''Entity should be %s'');\n', p_entity, p_entity);
    v_result := v_result || 'SELECT v_result.pk AS v_id \\gset\n\n';

    -- Main view existence
    v_result := v_result || format('SELECT ok(EXISTS (SELECT 1 FROM %s WHERE id = :v_id), ''Entity exists in view %s'');\n', v_base_view, v_base_view);

    -- Projection table existence
    IF v_structure.proj_table_exists THEN
        v_result := v_result || format('SELECT ok(EXISTS (SELECT 1 FROM %s WHERE id = :v_id), ''Entity exists in projection table %s'');\n', v_proj_table, v_proj_table);

        -- JSONB direct equality check only if projection exists
        v_result := v_result || format(
            'SELECT ok(\n' ||
            '  (SELECT json_data = (SELECT json_data FROM %2$s WHERE id = :v_id) FROM %1$s WHERE id = :v_id),\n' ||
            '  ''jsonb content matches between %1$s and %2$s''\n' ||
            ');\n',
            v_proj_table, v_base_view
        );
    END IF;

    -- Finish
    v_result := v_result || '\nSELECT * FROM finish();\n';

    RETURN v_result;
END;
$$;
