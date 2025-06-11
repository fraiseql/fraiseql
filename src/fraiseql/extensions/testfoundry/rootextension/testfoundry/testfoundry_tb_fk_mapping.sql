-- ============================================================================
-- Table: testfoundry_tb_fk_mapping
-- Purpose:
--   Stores dynamic foreign key resolution mappings and random value generation rules,
--   supporting dynamic WHERE clause generation based on dependency fields
--   and dynamic JSON object building for random value selection.
--
-- Columns:
--   input_type                TEXT PRIMARY KEY
--       Logical name of the input field (e.g., 'country', 'postal_code', 'city_code').

--   from_expression           TEXT NOT NULL
--       Full FROM or JOIN expression for the SQL query.

--   select_field              TEXT NOT NULL
--       Field to SELECT when resolving a FK (typically UUID PK).

--   random_select_expression  TEXT
--       (Optional) Full SQL expression to use for random value selection.
--       Example: 'name_local' or 'CONCAT(name_local, '' ('', iso_code, '')'')'.
--       If NULL, defaults to select_field unless random_pk_field + random_value_field are provided.

--   random_pk_field           TEXT
--       (Optional) Field name used as PK when dynamically building a {pk, value} JSON structure.

--   random_value_field        TEXT
--       (Optional) Field name used as visible value when dynamically building a {pk, value} JSON structure.

--   random_select_where       TEXT
--       (Optional) WHERE condition applied when randomly selecting a record.
--       Example: 'deleted_at IS NULL'.

--   dependency_fields         TEXT[]
--       (Optional) List of input fields used to dynamically build WHERE clauses.
--       Example: ARRAY['city_code', 'country'].

--   dependency_field_mapping  JSONB
--       (Optional) Mapping from input fields to actual database fields.
--       Example: '{"city_code": "info.administrative_code", "country": "c.name_local"}'.
--
-- Notes:
--   - WHERE clauses are dynamically generated from dependency_fields and dependency_field_mapping.
--   - If random_select_expression is NULL but random_pk_field and random_value_field are set,
--     then a dynamic jsonb_build_object('pk', ..., 'value', ...) is generated at runtime.
--   - Enables much more scalable, flexible, and DRY random input generation.
-- ============================================================================

CREATE TABLE IF NOT EXISTS testfoundry_tb_fk_mapping (
    input_type TEXT PRIMARY KEY,
    from_expression TEXT NOT NULL,
    select_field TEXT NOT NULL,
    random_select_expression TEXT,
    random_pk_field TEXT,
    random_value_field TEXT,
    random_select_where TEXT,
    dependency_fields TEXT[],              -- Logical input fields (ex: ['postal_code', 'country'])
    dependency_field_mapping JSONB,         -- Map logical field -> real DB field
    dependency_field_types TEXT[]           -- NEW!! Logical input field types (ex: ['uuid', 'text'])
);
