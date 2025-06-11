/**
 * Function: _testfoundry_generate_happy_hard_delete_test
 * ------------------------------------------
 * Generates a test script to verify hard delete behavior for an entity.
 * It inserts a dummy record, performs a hard delete, and asserts that
 * the record is removed from both the view and the base table.
 *
 * Parameters:
 *   - p_entity TEXT: The logical entity name (suffix used without "tb_" prefix).
 *
 * Returns:
 *   - TEXT block containing a full psql test script.
 *
 * Behavior:
 *   - Declares authentication variables.
 *   - Inserts a dummy record.
 *   - Executes the hard delete function.
 *   - Asserts that the record is no longer present.
 *
 * Usage Example:
 *   SELECT _testfoundry_generate_happy_hard_delete_test('invoice');
 */

CREATE OR REPLACE FUNCTION _testfoundry_generate_happy_hard_delete_test(
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
BEGIN
    -- Naming convention
    v_table := format('%I.tb_%s', p_schema, p_entity);
    v_view := format('%I.v_%s', p_schema, p_entity);
    v_delete_function := format('delete_%s_with_log', p_entity);

    -- Header
    v_result := v_result || format('-- Happy Hard DELETE test for entity %s\n', p_entity);
    v_result := v_result || 'SELECT plan(4);\n\n';

    -- Declare variables using utility
    v_result := v_result || generate_authentication_vars();
    v_result := v_result || 'SELECT gen_random_uuid() AS v_id \gset\n\n';

    -- Insert dummy record
    v_result := v_result || format('INSERT INTO %s (pk_%s, fk_customer_org, created_by)\n', v_table, p_entity);
    v_result := v_result || 'VALUES (:v_id, :v_org, :v_user);\n\n';

    -- Perform DELETE
    v_result := v_result || format('SELECT * INTO v_result FROM %s(:v_id);\n\n', v_delete_function);

    -- Assertions
    v_result := v_result || 'SELECT is(v_result.status, ''deleted'', ''Status should be deleted'');\n';
    v_result := v_result || format('SELECT is(v_result.entity, ''%s'', ''Entity type matches'');\n', p_entity);

    -- Check it's no longer in view
    v_result := v_result || format('SELECT not ok(EXISTS (SELECT 1 FROM %s WHERE id = :v_id), ''Entity no longer exists in %s view'');\n', v_view, p_entity);

    -- Check it's physically gone from table
    v_result := v_result || format('SELECT not ok(EXISTS (SELECT 1 FROM %s WHERE pk_%s = :v_id), ''Entity no longer exists in %s table'');\n', v_table, p_entity, p_entity);

    -- Finish
    v_result := v_result || '\nSELECT * FROM finish();\n';

    RETURN v_result;
END;
$$;
