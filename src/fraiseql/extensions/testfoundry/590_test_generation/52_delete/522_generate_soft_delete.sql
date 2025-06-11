/**
 * Function: _generate_happy_soft_delete_test
 * ------------------------------------------
 * Generates a test script to verify soft delete behavior for an entity.
 * It inserts a dummy record, performs a soft delete, and asserts that
 * the record is marked deleted in the base table and removed from the view.
 * Also verifies that dependent archived records remain linked.
 *
 * Parameters:
 *   - p_entity TEXT: The logical entity name (suffix used without "tb_" prefix).
 *
 * Returns:
 *   - TEXT block containing a full psql test script.
 *
 * Behavior:
 *   - Declares authentication variables.
 *   - Inserts a dummy record and dependents.
 *   - Executes the soft delete function.
 *   - Asserts soft deletion and dependent integrity.
 *
 * Usage Example:
 *   SELECT _generate_happy_soft_delete_test('invoice');
 */

CREATE OR REPLACE FUNCTION _generate_happy_soft_delete_test(
    p_entity TEXT,
    p_schema TEXT DEFAULT 'public'
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
    v_archived_condition TEXT;
BEGIN
    -- Naming convention
    v_table := format('%I.tb_%s', p_schema, p_entity);
    v_view := format('%I.v_%s', p_schema, p_entity);
    v_delete_function := format('delete_%s_with_log', p_entity);

    -- Header
    v_result := v_result || format('-- Happy Soft DELETE test for entity %s\n', p_entity);
    v_result := v_result || 'SELECT plan(5);\n\n';

    -- Declare variables using utility
    v_result := v_result || generate_authentication_vars();
    v_result := v_result || 'SELECT gen_random_uuid() AS v_id \gset\n\n';

    -- Insert parent
    v_result := v_result || format('INSERT INTO %s (pk_%s, fk_customer_org, created_by)\n', v_table, p_entity);
    v_result := v_result || 'VALUES (:v_id, :v_org, :v_user);\n\n';

    -- Insert archived dependents if defined
    FOR r_dep IN
        SELECT dependent_entity, link_field, is_archived_condition
        FROM public.testfoundry_tb_entity_dependents
        WHERE parent_entity = p_entity
    LOOP
        v_child_table := format('%I.tb_%s', p_schema, r_dep.dependent_entity);
        v_child_fk := r_dep.link_field;
        v_archived_condition := coalesce(r_dep.is_archived_condition, 'TRUE');

        v_result := v_result || format('-- Insert archived dependent %s\n', r_dep.dependent_entity);
        v_result := v_result || format(
            'INSERT INTO %s (pk_%s, %s, fk_customer_org, created_by, start_date, end_date)\n',
            v_child_table,
            r_dep.dependent_entity,
            v_child_fk
        );
        v_result := v_result || format(
            'VALUES (gen_random_uuid(), :v_id, :v_org, :v_user, ''2020-01-01'', ''2020-12-31'');\n\n'
        );
    END LOOP;

    -- Perform DELETE
    v_result := v_result || format('SELECT * INTO v_result FROM %s(:v_id);\n\n', v_delete_function);

    -- Assertions
    v_result := v_result || 'SELECT is(v_result.status, ''deleted'', ''Entity soft-deleted'');\n';
    v_result := v_result || format('SELECT ok(EXISTS (SELECT 1 FROM %s WHERE pk_%s = :v_id AND deleted_at IS NOT NULL), ''Soft-deleted in table'');\n', v_table, p_entity);
    v_result := v_result || format('SELECT not ok(EXISTS (SELECT 1 FROM %s WHERE id = :v_id), ''Entity missing from view'');\n', v_view);

    -- Assert dependents still present
    FOR r_dep IN
        SELECT dependent_entity, link_field
        FROM public.testfoundry_tb_entity_dependents
        WHERE parent_entity = p_entity
    LOOP
        v_child_table := format('%I.tb_%s', p_schema, r_dep.dependent_entity);

        v_result := v_result || format(
            'SELECT ok(EXISTS (SELECT 1 FROM %s WHERE %s = :v_id), ''Archived dependent %s still linked'');\n',
            v_child_table,
            r_dep.link_field,
            r_dep.dependent_entity
        );
    END LOOP;

    -- Finish
    v_result := v_result || '\nSELECT * FROM finish();\n';

    RETURN v_result;
END;
$$;
