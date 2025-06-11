/**
 * Function: _testfoundry_generate_contrainst_violation_create_test
 * ---------------------------------------------------
 * Generates a CREATE test for constraint violations on a given entity.
 * It tries to create invalid data that violates simple CHECK constraints,
 * such as "value > 0" or "field IN (...)" conditions.
 * Complex expressions (e.g., arithmetic operations, subqueries) are automatically skipped.
 *
 * Parameters:
 *   - p_entity TEXT: The logical entity name (suffix used without "tb_" prefix).
 *   - p_schema TEXT DEFAULT 'public': The schema where the table and views are located.
 *
 * Returns:
 *   - TEXT block containing a full psql test script.
 *
 * Behavior:
 *   - Declares authentication variables.
 *   - Tries to create an entity with deliberately invalid input.
 *   - Expects and asserts that an error occurs.
 *
 * Usage Example:
 *   SELECT _testfoundry_generate_contrainst_violation_create_test('invoice');
 */

CREATE OR REPLACE FUNCTION _testfoundry_generate_contrainst_violation_create_test(
    p_entity TEXT,
    p_schema TEXT DEFAULT 'public'
)
RETURNS TEXT
LANGUAGE plpgsql
AS $$
DECLARE
    v_result TEXT := '';
    v_table TEXT;
    v_base_view TEXT;
    v_invalid_fields TEXT := '';
    v_input_field TEXT;
    v_structure RECORD;
    r_constraint RECORD;
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
    v_result := format('-- CONSTRAINT violation CREATE test for entity %s\n', p_entity);
    v_result := v_result || format('SELECT plan(%s);\n\n', v_n_assertions);

    -- Declare variables using utility
    v_result := v_result || generate_authentication_vars();

    -- Build invalid input by violating simple constraints
    FOR r_constraint IN
        SELECT conname, pg_get_expr(conbin, conrelid) AS expression
        FROM pg_constraint
        WHERE contype = 'c'
          AND conrelid = format('%I.tb_%s', p_schema, p_entity)::regclass
    LOOP
        -- Skip complex expressions
        IF r_constraint.expression ~* 'SELECT|CASE|COALESCE|\*|/|\+|-' THEN
            CONTINUE;
        END IF;

        -- Try to parse simple constraint patterns
        IF r_constraint.expression ILIKE '% > 0' THEN
            -- Find the related field (naive parsing)
            SELECT attname INTO r_col
            FROM pg_attribute
            WHERE attrelid = format('%I.tb_%s', p_schema, p_entity)::regclass
              AND position(attname in r_constraint.expression) > 0
              AND NOT attisdropped
            LIMIT 1;

            IF FOUND THEN
                v_invalid_fields := v_invalid_fields || format('"%s": 0,', r_col.attname);
            END IF;

        ELSIF r_constraint.expression ILIKE '%IN (' THEN
            SELECT attname INTO r_col
            FROM pg_attribute
            WHERE attrelid = format('%I.tb_%s', p_schema, p_entity)::regclass
              AND position(attname in r_constraint.expression) > 0
              AND NOT attisdropped
            LIMIT 1;

            IF FOUND THEN
                v_invalid_fields := v_invalid_fields || format('"%s": ''invalid_value'',', r_col.attname);
            END IF;
        END IF;
    END LOOP;

    -- Remove trailing comma
    v_invalid_fields := regexp_replace(v_invalid_fields, ',$', '');

    -- Begin constraint violation attempt
    v_result := v_result || format('SELECT * INTO v_result FROM create_%s_with_log(\n', p_entity);
    v_result := v_result || '    :v_org,\n';
    v_result := v_result || '    :v_user,\n';
    v_result := v_result || format('    ''{%s}''::jsonb\n', v_invalid_fields);
    v_result := v_result || ');\n\n';

    -- Assertions
    v_result := v_result || 'SELECT like(v_result.status, ''error%'', ''Constraint violation should trigger an error'');\n';

    -- Finish
    v_result := v_result || '\nSELECT * FROM finish();\n';

    RETURN v_result;
END;
$$;
