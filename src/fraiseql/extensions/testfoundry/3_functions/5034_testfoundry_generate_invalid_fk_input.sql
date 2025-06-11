/**
 * Function: generate_invalid_fk_input
 * ------------------------------------
 * Generates invalid foreign key input data for testing.
 *
 * Parameters:
 *   - p_entity TEXT: Name of the entity (table suffix without "tb_").
 *   - p_schema TEXT DEFAULT 'public': Schema where the entity is located.
 *   - p_field TEXT DEFAULT NULL: (Optional) Specific field name to generate invalid input for. If NULL, generates for all FK fields.
 *
 * Returns:
 *   - JSONB object with invalid UUID values ('00000000-0000-0000-0000-000000000000') for each FK field.
 *
 * Usage Examples:
 *   1. All FK fields invalid:
 *      SELECT generate_invalid_fk_input('my_entity');
 *
 *      -- Output:
 *      {"customer_id": "00000000-0000-0000-0000-000000000000", "address_id": "00000000-0000-0000-0000-000000000000"}
 *
 *   2. Specific FK field invalid:
 *      SELECT generate_invalid_fk_input('my_entity', 'public', 'customer_id');
 *
 *      -- Output:
 *      {"customer_id": "00000000-0000-0000-0000-000000000000"}
 */


CREATE OR REPLACE FUNCTION generate_invalid_fk_input(
    p_entity TEXT,
    p_schema TEXT DEFAULT 'public',
    p_field TEXT DEFAULT NULL
)
RETURNS JSONB
LANGUAGE plpgsql
AS $$
DECLARE
    v_table TEXT;
    v_json_fields TEXT := '';
    v_input_field TEXT;
    r_fk RECORD;
    r_col RECORD;
BEGIN
    v_table := format('%I.tb_%s', p_schema, p_entity);

    FOR r_fk IN
        SELECT unnest(conkey) AS attnum
        FROM pg_constraint
        WHERE contype = 'f'
          AND conrelid = format('%I.tb_%s', p_schema, p_entity)::regclass
    LOOP
        -- Find the table column name
        SELECT attname INTO r_col
        FROM pg_attribute
        WHERE attrelid = format('%I.tb_%s', p_schema, p_entity)::regclass
          AND attnum = r_fk.attnum;

        -- Determine input field name
        IF EXISTS (
            SELECT 1 FROM test_field_mapping
            WHERE entity = p_entity AND input_field = r_col.attname
        ) THEN
            SELECT input_field INTO v_input_field
            FROM test_field_mapping
            WHERE entity = p_entity AND input_field = r_col.attname;

        ELSIF r_col.attname LIKE 'fk_%' THEN
            v_input_field := lower(substring(r_col.attname from 4)) || '_id';
        ELSE
            v_input_field := r_col.attname;
        END IF;

        -- Only add if matching specific field or no field filter
        IF p_field IS NULL OR v_input_field = p_field THEN
            v_json_fields := v_json_fields || format(
                '"%s": "00000000-0000-0000-0000-000000000000",',
                v_input_field
            );
        END IF;
    END LOOP;

    -- Remove trailing comma if any
    v_json_fields := regexp_replace(v_json_fields, ',$', '');

    -- Return as JSONB
    RETURN ('{' || v_json_fields || '}')::jsonb;
END;
$$;
