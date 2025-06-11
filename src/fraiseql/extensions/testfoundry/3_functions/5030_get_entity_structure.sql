/**
 * Function: testfoundry_get_entity_structure
 * -------------------------------
 * Returns structural information about a given entity:
 *  - Whether the base view, projection table, and base table exist.
 *  - Lists dependent entities and dependent technical objects.
 *  - Lists unique constraint column groups (excluding constraints on pk_ fields).
 *
 * Parameters:
 *   - p_entity TEXT: The entity name (without "tb_" prefix).
 *   - p_schema TEXT DEFAULT 'public': Schema name.
 *
 * Returns:
 *   - TABLE (
 *       base_view_exists BOOLEAN,
 *       proj_table_exists BOOLEAN,
 *       base_table_exists BOOLEAN,
 *       dependent_entities TEXT[],
 *       dependent_objects TEXT[],
 *       unique_constraints TEXT[]
 *     )
 */


CREATE OR REPLACE FUNCTION testfoundry_get_entity_structure(
    p_entity TEXT,
    p_schema TEXT DEFAULT 'public'
)
RETURNS TABLE (
    base_view_exists BOOLEAN,
    proj_table_exists BOOLEAN,
    base_table_exists BOOLEAN,
    dependent_entities TEXT[],
    dependent_objects TEXT[],
    unique_constraints TEXT[]
)
LANGUAGE plpgsql
AS $$
DECLARE
    v_base_view TEXT;
    v_proj_table TEXT;
    v_base_table TEXT;
BEGIN
    -- Compose object names
    v_base_view := format('%I.v_%s', p_schema, p_entity);
    v_proj_table := format('%I.tv_%s', p_schema, p_entity);
    v_base_table := format('%I.tb_%s', p_schema, p_entity);

    -- Check existence
    SELECT EXISTS (
        SELECT 1 FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relname = format('v_%s', p_entity)
          AND n.nspname = p_schema
    ) INTO base_view_exists;

    SELECT EXISTS (
        SELECT 1 FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relname = format('tv_%s', p_entity)
          AND n.nspname = p_schema
    ) INTO proj_table_exists;

    SELECT EXISTS (
        SELECT 1 FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relname = format('tb_%s', p_entity)
          AND n.nspname = p_schema
    ) INTO base_table_exists;

    -- Find dependent entities (views, materialized views)
    SELECT array_agg(distinct dependent_ns.nspname || '.' || dependent_cls.relname)
    INTO dependent_entities
    FROM pg_depend d
    JOIN pg_class base_cls ON base_cls.oid = format('%I.tb_%s', p_schema, p_entity)::regclass
    JOIN pg_class dependent_cls ON d.refobjid = dependent_cls.oid
    JOIN pg_namespace dependent_ns ON dependent_cls.relnamespace = dependent_ns.oid
    WHERE d.objid = base_cls.oid
      AND d.deptype = 'n';

    -- Find additional dependent technical objects
    SELECT array_agg(objid::regclass::text)
    INTO dependent_objects
    FROM pg_depend
    WHERE refobjid = format('%I.tb_%s', p_schema, p_entity)::regclass
      AND classid IN (
          'pg_trigger'::regclass,
          'pg_constraint'::regclass,
          'pg_index'::regclass
      );

    -- Find unique constraints, excluding any involving pk_ columns
    SELECT array_agg(cols.column_list)
    INTO unique_constraints
    FROM (
        SELECT array_to_string(array_agg(att.attname ORDER BY att.attnum), ',') AS column_list
        FROM pg_constraint con
        JOIN pg_class rel ON rel.oid = con.conrelid
        JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        CROSS JOIN LATERAL unnest(con.conkey) AS key(attnum)
        JOIN pg_attribute att ON att.attrelid = rel.oid AND att.attnum = key.attnum
        WHERE con.contype = 'u'
          AND nsp.nspname = p_schema
          AND rel.relname = format('tb_%s', p_entity)
        GROUP BY con.conname
        HAVING bool_and(att.attname NOT LIKE 'pk_%')
    ) cols;

    RETURN;
END;
$$;
