-- ============================================================================
-- Function: testfoundry_list_input_fields
-- Purpose:
--   Lists all fields of a given input composite type,
--   enriched with dynamic generation metadata.
-- ============================================================================
CREATE OR REPLACE FUNCTION testfoundry_list_input_fields(p_entity TEXT)
RETURNS TABLE (
    field_name TEXT,
    field_type TEXT,
    generator_type TEXT,
    fk_mapping_key TEXT,
    fk_dependency_fields TEXT[],
    nested_type TEXT,
    random_function TEXT,
    required BOOLEAN,
    generator_group TEXT,
    group_leader BOOLEAN,
    group_dependency_fields TEXT[],
    field_description TEXT,
    random_pk_field TEXT,
    random_value_field TEXT,
    dependency_fields TEXT[],
    dependency_field_types TEXT[],
    dependency_field_mapping JSONB
)
LANGUAGE sql
AS $$
    SELECT
        a.attname AS field_name,
        t.typname AS field_type,
        f.generator_type,
        f.fk_mapping_key,
        f.fk_dependency_fields,
        f.nested_type,
        f.random_function,
        COALESCE(f.required, TRUE) AS required,
        f.generator_group,
        COALESCE(f.group_leader, FALSE) AS group_leader,
        f.group_dependency_fields,
        f.field_description,
        fk.random_pk_field,
        fk.random_value_field,
        fk.dependency_fields,
        fk.dependency_field_types,
        fk.dependency_field_mapping
    FROM pg_type typ
    JOIN pg_attribute a ON a.attrelid = typ.typrelid
    JOIN pg_type t ON a.atttypid = t.oid
    LEFT JOIN testfoundry_tb_input_field_mapping f
      ON f.input_type = p_entity
     AND f.field_name = a.attname
    LEFT JOIN testfoundry_tb_fk_mapping fk
      ON f.fk_mapping_key = fk.input_type
    WHERE typ.typname = 'type_' || p_entity || '_input'
      AND a.attnum > 0
      AND NOT a.attisdropped
    ORDER BY a.attnum
$$;
