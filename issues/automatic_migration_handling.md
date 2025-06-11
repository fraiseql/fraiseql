# Automatic Migration Handling System

## Overview

This document describes a schema evolution tracking system that enables automatic migration and data copying between different schema versions. By maintaining a detailed history of all schema changes, the system can automatically generate migration scripts and handle data transformations when copying between environments with different schema versions.

## Core Concept

Instead of maintaining static copy scripts for each version, we track every schema change in a central evolution table. This allows the system to:
- Generate migration scripts between any two versions
- Automatically adapt data copy operations to handle schema differences
- Maintain a complete audit trail of schema changes
- Enable bidirectional migrations (upgrade and downgrade)

## Implementation

### 1. Schema Evolution Tracking Table

```sql
CREATE TABLE core.tb_schema_evolution (
    id SERIAL PRIMARY KEY,
    version TEXT NOT NULL,
    evolution_type TEXT CHECK (evolution_type IN (
        'add_table', 'drop_table', 'rename_table',
        'add_column', 'drop_column', 'rename_column',
        'change_type', 'add_constraint', 'drop_constraint'
    )),
    schema_name TEXT NOT NULL,
    table_name TEXT NOT NULL,
    object_name TEXT, -- column name, constraint name, etc.
    old_value TEXT,   -- old name for renames, old type for changes
    new_value TEXT,   -- new name for renames, new type for changes
    additional_info JSONB, -- for complex changes
    applied_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    applied_by TEXT DEFAULT CURRENT_USER
);

-- Indexes for efficient lookups
CREATE INDEX idx_schema_evolution_version ON core.tb_schema_evolution(version);
CREATE INDEX idx_schema_evolution_table ON core.tb_schema_evolution(schema_name, table_name);
```

### 2. Version Tracking Table

```sql
CREATE TABLE core.tb_schema_version (
    version TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    description TEXT
);
```

### 3. Example Evolution Entries

```sql
-- Version 1.1.0 changes
INSERT INTO core.tb_schema_evolution (version, evolution_type, schema_name, table_name, object_name, old_value, new_value) VALUES
('1.1.0', 'add_column', 'core', 'tb_organization', 'external_id', NULL, 'UUID'),
('1.1.0', 'rename_column', 'core', 'tb_machine', 'serial_no', 'serial_no', 'serial_number'),
('1.1.0', 'drop_column', 'core', 'tb_machine', 'deprecated_field', 'TEXT', NULL);

-- Version 1.2.0 changes
INSERT INTO core.tb_schema_evolution (version, evolution_type, schema_name, table_name, object_name, old_value, new_value, additional_info) VALUES
('1.2.0', 'add_table', 'core', 'tb_audit_log', NULL, NULL, NULL, '{"columns": ["id", "action", "timestamp"]}'),
('1.2.0', 'change_type', 'core', 'tb_meter', 'reading_value', 'INTEGER', 'NUMERIC(15,3)', NULL);
```

### 4. Core Functions

#### 4.1 Migration Script Generator

```sql
CREATE OR REPLACE FUNCTION core.generate_migration_script(
    p_from_version TEXT,
    p_to_version TEXT
) RETURNS TEXT AS $$
DECLARE
    v_migration_script TEXT := '';
    r RECORD;
BEGIN
    -- Generate migration script based on evolution history
    FOR r IN
        SELECT * FROM core.tb_schema_evolution
        WHERE version > p_from_version AND version <= p_to_version
        ORDER BY version, id
    LOOP
        v_migration_script := v_migration_script || E'\n' ||
        CASE r.evolution_type
            WHEN 'add_column' THEN
                format('ALTER TABLE %I.%I ADD COLUMN %I %s;',
                       r.schema_name, r.table_name, r.object_name, r.new_value)

            WHEN 'rename_column' THEN
                format('ALTER TABLE %I.%I RENAME COLUMN %I TO %I;',
                       r.schema_name, r.table_name, r.old_value, r.new_value)

            WHEN 'drop_column' THEN
                format('ALTER TABLE %I.%I DROP COLUMN %I;',
                       r.schema_name, r.table_name, r.object_name)

            WHEN 'change_type' THEN
                format('ALTER TABLE %I.%I ALTER COLUMN %I TYPE %s;',
                       r.schema_name, r.table_name, r.object_name, r.new_value)

            WHEN 'add_table' THEN
                format('-- Create table %I.%I (see full definition)',
                       r.schema_name, r.table_name)

            WHEN 'drop_table' THEN
                format('DROP TABLE IF EXISTS %I.%I CASCADE;',
                       r.schema_name, r.table_name)

            WHEN 'rename_table' THEN
                format('ALTER TABLE %I.%I RENAME TO %I;',
                       r.schema_name, r.old_value, r.new_value)

            WHEN 'add_constraint' THEN
                format('ALTER TABLE %I.%I ADD CONSTRAINT %I %s;',
                       r.schema_name, r.table_name, r.object_name, r.new_value)

            WHEN 'drop_constraint' THEN
                format('ALTER TABLE %I.%I DROP CONSTRAINT %I;',
                       r.schema_name, r.table_name, r.object_name)
        END;
    END LOOP;

    RETURN v_migration_script;
END;
$$ LANGUAGE plpgsql;
```

#### 4.2 Adaptive Copy Script Generator

```sql
CREATE OR REPLACE FUNCTION core.generate_adaptive_copy_script(
    p_schema_name TEXT,
    p_table_name TEXT,
    p_source_version TEXT,
    p_target_version TEXT
) RETURNS TEXT AS $$
DECLARE
    v_column_mappings JSONB := '{}'::JSONB;
    v_excluded_columns TEXT[] := ARRAY[]::TEXT[];
    v_added_columns JSONB := '{}'::JSONB;
    r RECORD;
    v_select_list TEXT;
    v_insert_list TEXT;
BEGIN
    -- Build column mappings from evolution history
    FOR r IN
        SELECT * FROM core.tb_schema_evolution
        WHERE schema_name = p_schema_name
        AND table_name = p_table_name
        AND version > p_source_version
        AND version <= p_target_version
    LOOP
        CASE r.evolution_type
            WHEN 'rename_column' THEN
                v_column_mappings := v_column_mappings ||
                    jsonb_build_object(r.old_value, r.new_value);

            WHEN 'drop_column' THEN
                v_excluded_columns := array_append(v_excluded_columns, r.object_name);

            WHEN 'add_column' THEN
                v_added_columns := v_added_columns ||
                    jsonb_build_object(r.object_name, r.additional_info->>'default_value');
        END CASE;
    END LOOP;

    -- Generate column lists for copy
    SELECT
        string_agg(
            CASE
                WHEN v_column_mappings ? column_name THEN
                    format('%I AS %I', column_name, v_column_mappings->>column_name)
                ELSE quote_ident(column_name)
            END, ', '
        ) FILTER (WHERE NOT column_name = ANY(v_excluded_columns)),
        string_agg(
            CASE
                WHEN v_column_mappings ? column_name THEN
                    quote_ident(v_column_mappings->>column_name)
                ELSE quote_ident(column_name)
            END, ', '
        ) FILTER (WHERE NOT column_name = ANY(v_excluded_columns))
    INTO v_select_list, v_insert_list
    FROM information_schema.columns
    WHERE table_schema = p_schema_name
    AND table_name = p_table_name;

    -- Return the copy script
    RETURN format(
        'INSERT INTO %I.%I (%s) SELECT %s FROM production_source.%I.%I',
        p_schema_name, p_table_name, v_insert_list, v_select_list,
        p_schema_name, p_table_name
    );
END;
$$ LANGUAGE plpgsql;
```

#### 4.3 Automated Copy Process

```sql
CREATE OR REPLACE FUNCTION core.copy_with_evolution_handling()
RETURNS VOID AS $$
DECLARE
    v_source_version TEXT;
    v_target_version TEXT;
    r RECORD;
    v_copy_sql TEXT;
BEGIN
    -- Get versions
    SELECT version INTO v_source_version
    FROM production_source.core.tb_schema_version
    ORDER BY applied_at DESC LIMIT 1;

    SELECT version INTO v_target_version
    FROM core.tb_schema_version
    ORDER BY applied_at DESC LIMIT 1;

    RAISE NOTICE 'Copying data from version % to %', v_source_version, v_target_version;

    -- For each table, generate and execute appropriate copy script
    FOR r IN
        SELECT DISTINCT schema_name, table_name
        FROM information_schema.tables
        WHERE table_schema IN ('core', 'i18n', 'crm', 'catalog', 'dim', 'scd', 'fact')
        AND table_type = 'BASE TABLE'
        ORDER BY
            CASE schema_name
                WHEN 'i18n' THEN 1
                WHEN 'core' THEN 2
                WHEN 'crm' THEN 3
                WHEN 'catalog' THEN 4
                WHEN 'dim' THEN 5
                WHEN 'scd' THEN 6
                WHEN 'fact' THEN 7
            END,
            table_name
    LOOP
        BEGIN
            v_copy_sql := core.generate_adaptive_copy_script(
                r.schema_name,
                r.table_name,
                v_source_version,
                v_target_version
            );

            EXECUTE v_copy_sql;
            RAISE NOTICE 'Copied data for %.%', r.schema_name, r.table_name;
        EXCEPTION
            WHEN OTHERS THEN
                RAISE WARNING 'Failed to copy %.%: %', r.schema_name, r.table_name, SQLERRM;
        END;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

### 5. Workflow Implementation

#### 5.1 Recording Schema Changes

When making schema changes, always record them:

```sql
-- Example: Adding a new column
BEGIN;
    -- 1. Make the actual schema change
    ALTER TABLE core.tb_organization ADD COLUMN external_id UUID;

    -- 2. Record it in evolution table
    INSERT INTO core.tb_schema_evolution
    (version, evolution_type, schema_name, table_name, object_name, new_value, additional_info)
    VALUES ('1.3.0', 'add_column', 'core', 'tb_organization', 'external_id', 'UUID',
            '{"default_value": "gen_random_uuid()"}');

    -- 3. Update version
    INSERT INTO core.tb_schema_version (version, description)
    VALUES ('1.3.0', 'Added external_id to organization table');
COMMIT;
```

#### 5.2 Copying Data Between Versions

```sql
-- Set up connection to production backup
-- (This would typically use dblink or foreign data wrapper)

-- Run the automated copy
SELECT core.copy_with_evolution_handling();
```

#### 5.3 Generating Migration Scripts

```sql
-- Generate migration script from version 1.0.0 to 1.3.0
SELECT core.generate_migration_script('1.0.0', '1.3.0');
```

### 6. Benefits

1. **Automatic Schema Reconciliation**: No need to manually track which columns exist in which version
2. **Version Independence**: Can copy data between any two versions
3. **Self-Documenting**: Evolution table serves as complete change history
4. **Reversible**: Can generate both upgrade and downgrade scripts
5. **Testable**: Can preview migrations without executing them
6. **Maintainable**: Changes are recorded once and used everywhere

### 7. Best Practices

1. **Always Record Changes**: Every schema modification must be recorded in the evolution table
2. **Use Semantic Versioning**: Follow semantic versioning (major.minor.patch) for clarity
3. **Include Defaults**: When adding non-nullable columns, always specify default values
4. **Test Migrations**: Test migrations on a copy before applying to production
5. **Document Complex Changes**: Use the additional_info field for complex transformations

### 8. Advanced Features

#### 8.1 Complex Transformations

For complex data transformations, store transformation functions:

```sql
-- Store transformation function in evolution table
INSERT INTO core.tb_schema_evolution
(version, evolution_type, schema_name, table_name, object_name, old_value, new_value, additional_info)
VALUES (
    '1.4.0',
    'change_type',
    'core',
    'tb_address',
    'address_data',
    'TEXT',
    'JSONB',
    '{"transformation": "core.transform_address_to_jsonb(address_data)"}'
);
```

#### 8.2 Conditional Logic

Handle conditional migrations:

```sql
INSERT INTO core.tb_schema_evolution
(version, evolution_type, schema_name, table_name, object_name, additional_info)
VALUES (
    '1.5.0',
    'add_column',
    'core',
    'tb_machine',
    'region',
    '{"condition": "WHERE country_code IS NOT NULL", "default_value": "extract_region(country_code)"}'
);
```

### 9. Migration Execution Order

The system respects dependency order when copying data:
1. Foundation tables (no foreign keys)
2. Tables with single dependencies
3. Tables with multiple dependencies
4. Self-referential tables (special handling)

### 10. Error Handling

The system includes comprehensive error handling:
- Logs all copy operations
- Continues on non-critical errors
- Reports summary of successful/failed operations
- Supports retry mechanisms for failed tables

## Conclusion

This automatic migration handling system provides a robust, maintainable solution for managing schema evolution across environments. By tracking all changes in a central location and generating migrations dynamically, it eliminates the manual effort and potential errors associated with maintaining separate copy scripts for each version.
