-- Table to manually define custom field mappings between table columns and view expressions
-- for automatic pgTAP test generation. Used when the default JSONB field inference is insufficient.

-- Main table
CREATE TABLE testfoundry_tb_field_mapping (
    id INTEGER GENERATED ALWAYS AS IDENTITY,
    pk_field_mapping UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    entity TEXT NOT NULL CHECK (entity = lower(entity)),
    input_field TEXT NOT NULL CHECK (input_field = lower(input_field)),
    view_expression TEXT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT uq_entity_input_field UNIQUE (entity, input_field)
);

-- Comments
COMMENT ON TABLE testfoundry_tb_field_mapping IS
'Mapping table for connecting entity fields to view expressions for automated pgTAP tests.
Handles complex JSONB mappings and supports auto-managed timestamps.';

COMMENT ON COLUMN testfoundry_tb_field_mapping.pk_field_mapping IS
'Primary key UUID for this field mapping.';

COMMENT ON COLUMN testfoundry_tb_field_mapping.entity IS
'Entity name (lowercase, e.g., "user", "invoice") this mapping applies to.';

COMMENT ON COLUMN testfoundry_tb_field_mapping.input_field IS
'Input field name (lowercase column from the table) requiring view expression mapping.';

COMMENT ON COLUMN testfoundry_tb_field_mapping.view_expression IS
'SQL fragment to extract the field value from the corresponding view.';

COMMENT ON COLUMN testfoundry_tb_field_mapping.created_at IS
'Timestamp automatically set on insert.';

COMMENT ON COLUMN testfoundry_tb_field_mapping.updated_at IS
'Timestamp automatically refreshed on each row update.';
