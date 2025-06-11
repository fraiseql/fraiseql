CREATE OR REPLACE FUNCTION _testfoundry_generate_duplicate_create_test(
    p_entity TEXT,
    p_schema TEXT DEFAULT 'public'
)
RETURNS TEXT
LANGUAGE plpgsql
AS $$
DECLARE
    v_result TEXT := '';
    v_structure RECORD;
    v_first_id UUID;
    v_base_view TEXT;
    v_constraint TEXT;
    v_idx INT;
    v_entity_func TEXT;
    v_func_exists BOOLEAN;
    v_input_type TEXT;
BEGIN
    -- Fetch entity structure
    SELECT * INTO v_structure FROM testfoundry_get_entity_structure(p_entity, p_schema);

    IF NOT v_structure.base_view_exists THEN
        RAISE EXCEPTION 'Base view %.v_% does not exist', p_schema, p_entity;
    END IF;

    IF v_structure.unique_constraints IS NULL OR cardinality(v_structure.unique_constraints) = 0 THEN
        RAISE EXCEPTION 'No unique constraints defined for %', p_entity;
    END IF;

    v_base_view := format('%I.v_%s', p_schema, p_entity);

    -- Check if create_{entity}_with_log function exists
    SELECT EXISTS (
        SELECT 1
        FROM pg_proc
        JOIN pg_namespace nsp ON nsp.oid = pg_proc.pronamespace
        WHERE proname = format('create_%s_with_log', p_entity)
          AND nsp.nspname = p_schema
    ) INTO v_func_exists;

    IF NOT v_func_exists THEN
        RAISE EXCEPTION 'Required function create_%_with_log does not exist in schema %', p_entity, p_schema;
    END IF;

    -- Prepare names
    v_entity_func := format('create_%s_with_log', p_entity);
    v_input_type := format('type_%s_%s_input', p_schema, p_entity);

    -- Header
    v_result := format('-- DUPLICATE CREATE tests for entity %s\n', p_entity);
    v_result := v_result || format('SELECT plan(%s);\n\n', cardinality(v_structure.unique_constraints) * 2);

    -- Authentication vars
    v_result := v_result || generate_authentication_vars();

    -- Insert a valid entity
    v_result := v_result || format('-- Insert first valid %s\n', p_entity);
    v_result := v_result || format('SELECT insert_entity(''%s'', ''current'') AS v_id \gset\n\n', p_entity);

    -- Attempt duplicate inserts based on each unique constraint
    FOR v_idx IN 1..cardinality(v_structure.unique_constraints) LOOP
        v_constraint := v_structure.unique_constraints[v_idx];

        v_result := v_result || format('-- Prepare duplicate data for unique constraint %s\n', v_constraint);
        v_result := v_result || format('SELECT insert_duplicate_record_set(''%s'', ''%s'', ''%s'', :v_id) AS v_fields \gset\n\n',
                                       p_entity, v_base_view, v_constraint);

        -- Build composite input instance using json_populate_record
         v_result := v_result || 'DO $BODY$' || chr(10);
         v_result := v_result || 'DECLARE' || chr(10);
         v_result := v_result || format('    v_input %s;', v_input_type) || chr(10);
         v_result := v_result || 'BEGIN' || chr(10);
         v_result := v_result || '    SELECT *' || chr(10);
         v_result := v_result || format('    FROM jsonb_populate_record(NULL::%s, :v_fields)', v_input_type) || chr(10);
         v_result := v_result || '    INTO v_input;' || chr(10);
         v_result := v_result || format('    SELECT %s(v_input) INTO v_result;', v_entity_func) || chr(10);
         v_result := v_result || 'END' || chr(10);
         v_result := v_result || '$BODY$;' || chr(10) || chr(10);

        -- Assertions
        v_result := v_result || 'SELECT isnt(v_result, NULL, ''Duplicate insert should not succeed'');\n';
        v_result := v_result || 'SELECT like(v_result::text, ''%error%'', ''Duplicate create should return error'');\n\n';
    END LOOP;

    -- Finish
    v_result := v_result || '\nSELECT * FROM finish();\n';

    RETURN v_result;
END;
$$;
