CREATE OR REPLACE FUNCTION _generate_blocked_delete(
    p_entity TEXT
)
RETURNS TEXT
LANGUAGE plpgsql
AS $$
DECLARE
    v_result TEXT := '';
    v_table TEXT;
    v_view TEXT;
    v_delete_function TEXT;
    r_dep RECORD;
    v_child_table TEXT;
    v_child_fk TEXT;
BEGIN
    -- Naming convention
    v_table := format('tb_%s', p_entity);
    v_view := format('v_%s', p_entity);
    v_delete_function := format('delete_%s_with_log', p_entity);

    -- Header
    v_result := v_result || format('-- Blocked DELETE test for entity %s', p_entity) || chr(10);
    v_result := v_result || 'SELECT plan(5);' || chr(10) || chr(10);

    -- Setup vars
    v_result := v_result || 'SELECT' || chr(10);
    v_result := v_result || '    ''22222222-2222-2222-2222-222222222222''::uuid AS v_org,' || chr(10);
    v_result := v_result || '    ''11111111-1111-1111-1111-111111111111''::uuid AS v_user,' || chr(10);
    v_result := v_result || '    NULL::mutation_result AS v_result,' || chr(10);
    v_result := v_result || '    gen_random_uuid() AS v_id' || chr(10);
    v_result := v_result || '\gset' || chr(10) || chr(10);

    -- Insert parent
    v_result := v_result || format('INSERT INTO %I (pk_%s, fk_customer_org, created_by)', v_table, p_entity) || chr(10);
    v_result := v_result || 'VALUES (:v_id, :v_org, :v_user);' || chr(10) || chr(10);

    -- Insert active (non-archived) dependents
    FOR r_dep IN
        SELECT dependent_entity, link_field
        FROM public.test_entity_dependents
        WHERE parent_entity = p_entity
    LOOP
        v_child_table := format('tb_%s', r_dep.dependent_entity);
        v_child_fk := r_dep.link_field;

        v_result := v_result || format('-- Insert active dependent %s', r_dep.dependent_entity) || chr(10);
        v_result := v_result || format(
            'INSERT INTO %I (pk_%s, %s, fk_customer_org, created_by, start_date, end_date)',
            v_child_table,
            r_dep.dependent_entity,
            v_child_fk
        ) || chr(10);
        v_result := v_result || format(
            'VALUES (gen_random_uuid(), :v_id, :v_org, :v_user, CURRENT_DATE, CURRENT_DATE + INTERVAL ''1 year'');'
        ) || chr(10) || chr(10);
    END LOOP;

    -- Perform DELETE
    v_result := v_result || format('SELECT * INTO v_result FROM %s(:v_id);', v_delete_function) || chr(10) || chr(10);

    -- Assertions
    v_result := v_result || 'SELECT isnt(v_result.status, ''deleted'', ''Deletion should not happen due to dependencies'');' || chr(10);
    v_result := v_result || 'SELECT like(v_result.status, ''noop%'', ''Blocked delete should return NOOP or dependency error'');' || chr(10);
    v_result := v_result || format('SELECT ok(EXISTS (SELECT 1 FROM %I WHERE pk_%s = :v_id), ''Entity still exists in table'');', v_table, p_entity) || chr(10);
    v_result := v_result || format('SELECT ok(EXISTS (SELECT 1 FROM %I WHERE id = :v_id), ''Entity still exists in view'');', v_view) || chr(10);

    -- Finish
    v_result := v_result || chr(10) || 'SELECT * FROM finish();' || chr(10);

    RETURN v_result;
END;
$$;
