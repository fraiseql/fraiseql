-- ============================================================================
-- Function: testfoundry_build_conditions
-- Purpose:
--   Dynamically generates SQL conditions based on dependency fields.
--   Supports automatic quoting of table and column names if qualified (table.column).
--   Replaces manual WHERE clause templates, enabling dynamic SQL generation
--   for WHERE, JOIN ON, or SET clauses.
--
-- Parameters:
--   - p_dependency_fields TEXT[]:
--       Array of logical field names, usually matching input JSON field names.
--
--   - p_dependency_field_mapping JSONB DEFAULT NULL:
--       (Optional) Mapping from logical field names to actual database field names.
--       If a field is mapped to "table.column", the table and column are quoted separately.
--
--   - p_placeholder_prefix TEXT DEFAULT '$':
--       Prefix to use for placeholder parameters.
--       Typically '$' (for $1, $2...), but can be ':' for named placeholders if needed.
--
--   - p_separator TEXT DEFAULT ' AND ':
--       Separator between conditions.
--       Default is ' AND ' (for WHERE/ON clauses); can be ', ' for SET clauses.
--
-- Returns:
--   TEXT:
--       A string representing the SQL condition fragment.
--       Placeholders like $1, $2, etc. are automatically assigned in order.
--
-- Behavior:
--   - Splits mapped field names on the first '.' to correctly quote "table"."column" names.
--   - If no dot is present, quotes the simple field name.
--   - Automatically numbers the placeholders according to the field order.
--   - Supports flexible formatting for WHERE, JOIN, SET, and other dynamic queries.
--
-- Example usage:
--   SELECT testfoundry_build_conditions(
--     ARRAY['city_code', 'country'],
--     '{"city_code": "info.administrative_code", "country": "c.name_local"}'
--   );
--   -- Output:
--   -- "info"."administrative_code" = $1 AND "c"."name_local" = $2
--
-- Notes:
--   - This function is critical for building dynamic test data generation
--     without manually writing fragile SQL templates.
-- ============================================================================

CREATE OR REPLACE FUNCTION testfoundry_build_conditions(
    p_dependency_fields TEXT[],
    p_dependency_field_mapping JSONB DEFAULT NULL,
    p_placeholder_prefix TEXT DEFAULT '$',
    p_separator TEXT DEFAULT ' AND '
)
RETURNS TEXT
LANGUAGE plpgsql
AS $$
DECLARE
    v_idx INT := 1;
    v_conditions TEXT := '';
    v_field TEXT;
    v_db_field TEXT;
    v_parts TEXT[];
    v_cast_type TEXT;
BEGIN
    IF array_length(p_dependency_fields, 1) IS NULL THEN
        RETURN '';
    END IF;

    FOREACH v_field IN ARRAY p_dependency_fields
    LOOP
        -- Resolve DB field name if mapping exists
        v_db_field := COALESCE(p_dependency_field_mapping ->> v_field, v_field);

        -- 🛠 Determine if we need an explicit type cast
        IF v_db_field ILIKE '%pk_%' OR v_db_field ILIKE '%fk_%' THEN
            v_cast_type := '::uuid';
        ELSE
            v_cast_type := '';
        END IF;

        IF v_idx > 1 THEN
            v_conditions := v_conditions || p_separator;
        END IF;

        -- If the db_field contains a dot (.), split table and column
        v_parts := string_to_array(v_db_field, '.');

        IF array_length(v_parts, 1) = 2 THEN
            -- table.column -> quote separately
            v_conditions := v_conditions || format('%I.%I = %s%s%s', v_parts[1], v_parts[2], p_placeholder_prefix, v_idx, v_cast_type);
        ELSE
            -- no dot, simple field
            v_conditions := v_conditions || format('%I = %s%s%s', v_db_field, p_placeholder_prefix, v_idx, v_cast_type);
        END IF;

        v_idx := v_idx + 1;
    END LOOP;

    RETURN v_conditions;
END;
$$;
