# Audit Field Patterns

**Enterprise-grade change tracking and compliance patterns for FraiseQL applications**

## Overview

Audit field patterns are essential for enterprise applications requiring comprehensive change tracking, compliance monitoring, and data governance. FraiseQL's audit field patterns provide standardized approaches for tracking entity lifecycle events, user actions, and maintaining complete audit trails for regulatory compliance.

### Why Audit Fields Matter

Modern enterprise applications require detailed audit trails for:

- **Regulatory Compliance**: GDPR, SOX, HIPAA, and other regulations requiring data access tracking
- **Security Monitoring**: Detecting unauthorized changes and access patterns
- **Data Governance**: Understanding data lineage and transformation history
- **User Accountability**: Tracking who made what changes and when
- **System Debugging**: Investigating data issues and understanding change patterns
- **Business Intelligence**: Analyzing user behavior and system usage patterns

### Key Benefits

✅ **Comprehensive Tracking** - Complete lifecycle audit for all entities
✅ **Regulatory Ready** - Built-in compliance patterns for major regulations
✅ **Performance Optimized** - Efficient audit queries with proper indexing
✅ **GraphQL Integration** - First-class audit data exposure in APIs
✅ **Soft Delete Support** - Audit-aware deletion with recovery capabilities
✅ **Optimistic Locking** - Version-based concurrency control with audit trails

## Standard Audit Field Schema

All entities in FraiseQL applications should include these standardized audit fields:

### Core Audit Fields

```sql
-- Standard audit fields for all tb_* tables
CREATE TABLE tenant.tb_entity (
    -- Primary key fields
    id SERIAL,
    pk_entity UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Multi-tenant field
    fk_customer_org UUID NOT NULL,

    -- Entity data
    data JSONB NOT NULL,

    -- STANDARD AUDIT FIELDS
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID,

    -- Soft delete support
    deleted_at TIMESTAMPTZ,
    deleted_by UUID,

    -- Version control
    version INTEGER NOT NULL DEFAULT 1,

    -- Change tracking metadata
    change_reason TEXT,  -- Why this change was made
    change_source TEXT   -- How this change was made (api, import, system)
);
```

### Audit Field Semantics

| Field | Purpose | When Set | Required | Notes |
|-------|---------|----------|----------|-------|
| `created_at` | Entity creation timestamp | INSERT only | ✅ Yes | Immutable after creation |
| `created_by` | User who created entity | INSERT only | ✅ Yes | References user UUID |
| `updated_at` | Last modification timestamp | INSERT and UPDATE | ✅ Yes | Auto-updated on changes |
| `updated_by` | User who last modified | UPDATE only | ⚠️ Optional | NULL for system updates |
| `deleted_at` | Soft delete timestamp | Soft delete only | ❌ No | NULL for active entities |
| `deleted_by` | User who deleted entity | Soft delete only | ❌ No | Only set when deleted |
| `version` | Optimistic locking version | INSERT and UPDATE | ✅ Yes | Incremented on updates |
| `change_reason` | Human-readable change reason | Any change | ❌ No | For audit documentation |
| `change_source` | Change initiation method | Any change | ❌ No | api, import, system, etc. |

### Audit Field Best Practices

#### Required Fields Strategy

**Always Required:**

- `created_at` - Essential for all audit trails
- `created_by` - Required for accountability
- `updated_at` - Tracks last modification
- `version` - Enables optimistic locking

**Contextually Required:**

- `updated_by` - Required for user-initiated changes, NULL for system changes
- `deleted_at` / `deleted_by` - Only set during soft delete operations

#### Field Naming Conventions

- Use `_at` suffix for timestamps (TIMESTAMPTZ)
- Use `_by` suffix for user references (UUID)
- Use descriptive prefixes: `created_`, `updated_`, `deleted_`
- Version field without suffix: `version` (INTEGER)

#### Data Types and Constraints

```sql
-- Proper data types and constraints
ALTER TABLE tenant.tb_entity
    ADD CONSTRAINT check_audit_timestamps CHECK (
        created_at <= updated_at
        AND (deleted_at IS NULL OR deleted_at >= updated_at)
    ),
    ADD CONSTRAINT check_audit_users CHECK (
        created_by IS NOT NULL
        AND (updated_by IS NOT NULL OR updated_at = created_at)
        AND (deleted_by IS NULL OR deleted_at IS NOT NULL)
    ),
    ADD CONSTRAINT check_version_positive CHECK (version > 0);
```

## Compliance Requirements

### GDPR (General Data Protection Regulation)

**Article 30 - Records of Processing Activities**

FraiseQL audit fields support GDPR compliance through:

```sql
-- GDPR-compliant audit logging
CREATE TABLE audit.tb_data_processing_log (
    id BIGSERIAL PRIMARY KEY,
    processing_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    data_subject_id UUID NOT NULL,  -- Person whose data was processed
    processor_id UUID NOT NULL,     -- User who processed the data
    lawful_basis TEXT NOT NULL,     -- GDPR Article 6 basis
    processing_purpose TEXT NOT NULL,
    data_categories TEXT[],         -- Categories of personal data
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    operation_type TEXT NOT NULL,   -- CREATE, READ, UPDATE, DELETE
    retention_period INTERVAL,      -- How long data should be kept

    -- Request correlation
    request_id UUID,
    ip_address INET,
    user_agent TEXT
);
```

**Required Audit Capabilities:**

- ✅ Track all data processing activities
- ✅ Record lawful basis for processing
- ✅ Maintain data retention schedules
- ✅ Support data subject access requests
- ✅ Enable data portability exports
- ✅ Track consent withdrawal

### SOX (Sarbanes-Oxley Act)

**Section 404 - Internal Controls**

```sql
-- SOX-compliant financial data audit trail
CREATE TABLE audit.tb_financial_audit_log (
    id BIGSERIAL PRIMARY KEY,
    audit_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    financial_period DATE NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    field_name TEXT NOT NULL,
    old_value NUMERIC,
    new_value NUMERIC,
    change_amount NUMERIC GENERATED ALWAYS AS (new_value - old_value) STORED,

    -- SOX-specific fields
    authorized_by UUID NOT NULL,    -- User who authorized change
    reviewed_by UUID,              -- User who reviewed change
    sox_control_id TEXT NOT NULL,  -- Internal control reference
    supporting_doc_id UUID,        -- Link to supporting documentation

    -- Attestation
    attested_at TIMESTAMPTZ,
    attested_by UUID
);
```

### HIPAA (Health Insurance Portability and Accountability Act)

**Administrative Safeguards**

```sql
-- HIPAA-compliant access logging for healthcare data
CREATE TABLE audit.tb_phi_access_log (
    id BIGSERIAL PRIMARY KEY,
    access_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    patient_id UUID NOT NULL,      -- Patient whose data was accessed
    accessor_id UUID NOT NULL,     -- Healthcare worker
    access_type TEXT NOT NULL,     -- read, create, update, delete
    entity_type TEXT NOT NULL,     -- medical_record, prescription, etc.
    entity_id UUID NOT NULL,

    -- HIPAA-specific tracking
    minimum_necessary BOOLEAN NOT NULL DEFAULT true,
    treatment_relationship BOOLEAN NOT NULL,
    authorization_required BOOLEAN NOT NULL,
    authorization_id UUID,         -- Link to patient authorization

    -- Breach detection
    unusual_access BOOLEAN DEFAULT false,
    access_location TEXT,
    device_identifier TEXT
);
```

## Performance Considerations

### Essential Indexes for Audit Fields

```sql
-- Core audit field indexes
CREATE INDEX CONCURRENTLY idx_entity_audit_created
ON tenant.tb_entity (created_at DESC);

CREATE INDEX CONCURRENTLY idx_entity_audit_updated
ON tenant.tb_entity (updated_at DESC);

CREATE INDEX CONCURRENTLY idx_entity_audit_created_by
ON tenant.tb_entity (created_by);

CREATE INDEX CONCURRENTLY idx_entity_audit_version
ON tenant.tb_entity (version);

-- Soft delete support
CREATE INDEX CONCURRENTLY idx_entity_not_deleted
ON tenant.tb_entity (pk_organization)
WHERE deleted_at IS NULL;

CREATE INDEX CONCURRENTLY idx_entity_deleted
ON tenant.tb_entity (deleted_at)
WHERE deleted_at IS NOT NULL;

-- Composite indexes for common query patterns
CREATE INDEX CONCURRENTLY idx_entity_org_updated
ON tenant.tb_entity (pk_organization, updated_at DESC);

CREATE INDEX CONCURRENTLY idx_entity_user_activity
ON tenant.tb_entity (created_by, created_at DESC)
WHERE deleted_at IS NULL;
```

### Audit Query Optimization

```sql
-- Optimized audit queries using proper indexes
-- Recent changes by user
SELECT entity_type, entity_id, updated_at, change_reason
FROM tenant.tb_entity
WHERE updated_by = $1
AND updated_at > NOW() - INTERVAL '24 hours'
ORDER BY updated_at DESC
LIMIT 50;

-- Version conflict detection query
SELECT version, updated_at, updated_by
FROM tenant.tb_entity
WHERE pk_entity = $1
FOR UPDATE;  -- Prevent race conditions
```

## Migration Strategy

### Adding Audit Fields to Existing Tables

```sql
-- Safe migration script for existing tables
DO $$
DECLARE
    t_name TEXT;
BEGIN
    -- Add audit fields to all existing tb_* tables
    FOR t_name IN
        SELECT tablename
        FROM pg_tables
        WHERE schemaname = 'tenant'
        AND tablename LIKE 'tb_%'
    LOOP
        EXECUTE format('
            ALTER TABLE tenant.%I
            ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ DEFAULT NOW(),
            ADD COLUMN IF NOT EXISTS created_by UUID,
            ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ DEFAULT NOW(),
            ADD COLUMN IF NOT EXISTS updated_by UUID,
            ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS deleted_by UUID,
            ADD COLUMN IF NOT EXISTS version INTEGER DEFAULT 1,
            ADD COLUMN IF NOT EXISTS change_reason TEXT,
            ADD COLUMN IF NOT EXISTS change_source TEXT DEFAULT ''migration''
        ', t_name);

        -- Backfill created_by for existing records
        EXECUTE format('
            UPDATE tenant.%I
            SET created_by = ''00000000-0000-0000-0000-000000000000''::UUID
            WHERE created_by IS NULL
        ', t_name);

        -- Make created fields NOT NULL after backfill
        EXECUTE format('
            ALTER TABLE tenant.%I
            ALTER COLUMN created_at SET NOT NULL,
            ALTER COLUMN created_by SET NOT NULL,
            ALTER COLUMN updated_at SET NOT NULL,
            ALTER COLUMN version SET NOT NULL
        ', t_name);
    END LOOP;
END $$;
```

### Backfilling Audit Data

```sql
-- Estimate audit data quality after migration
CREATE OR REPLACE FUNCTION audit.assess_audit_data_quality()
RETURNS TABLE (
    table_name TEXT,
    total_records BIGINT,
    missing_created_by BIGINT,
    missing_updated_at BIGINT,
    records_with_versions BIGINT,
    audit_coverage_percent NUMERIC
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        t.tablename::TEXT,
        pg_class.reltuples::BIGINT AS total_records,
        COUNT(CASE WHEN created_by IS NULL THEN 1 END) AS missing_created_by,
        COUNT(CASE WHEN updated_at IS NULL THEN 1 END) AS missing_updated_at,
        COUNT(CASE WHEN version > 1 THEN 1 END) AS records_with_versions,
        ROUND(
            (COUNT(*) - COUNT(CASE WHEN created_by IS NULL THEN 1 END)) * 100.0 /
            GREATEST(COUNT(*), 1),
            2
        ) AS audit_coverage_percent
    FROM information_schema.tables t
    JOIN pg_class ON pg_class.relname = t.table_name
    WHERE t.table_schema = 'tenant'
    AND t.table_name LIKE 'tb_%'
    GROUP BY t.tablename, pg_class.reltuples;
END;
$$ LANGUAGE plpgsql;
```

## Troubleshooting Common Issues

### Issue 1: Missing Audit Data After Migration

**Symptom**: Existing records have NULL audit fields

**Solution**:
```sql
-- Identify tables with missing audit data
SELECT schemaname, tablename,
       COUNT(*) as total_rows,
       COUNT(created_by) as rows_with_created_by,
       COUNT(updated_at) as rows_with_updated_at
FROM information_schema.tables t
JOIN pg_stat_user_tables s ON s.relname = t.table_name
WHERE t.table_schema = 'tenant'
AND t.table_name LIKE 'tb_%'
GROUP BY schemaname, tablename
HAVING COUNT(created_by) < COUNT(*);

-- Backfill missing audit data
UPDATE tenant.tb_entity
SET created_by = '00000000-0000-0000-0000-000000000000'::UUID,
    updated_by = created_by,
    version = COALESCE(version, 1),
    change_source = 'migration'
WHERE created_by IS NULL;
```

### Issue 2: Version Conflicts in High-Concurrency Updates

**Symptom**: Frequent optimistic locking failures

**Solution**:
```sql
-- Add retry logic with exponential backoff
CREATE OR REPLACE FUNCTION core.update_with_retry(
    p_entity_id UUID,
    p_update_data JSONB,
    p_max_attempts INTEGER DEFAULT 3
) RETURNS app.mutation_result AS $$
DECLARE
    v_attempt INTEGER := 1;
    v_result app.mutation_result;
    v_wait_time NUMERIC;
BEGIN
    LOOP
        -- Attempt the update
        SELECT core.update_entity(p_entity_id, p_update_data) INTO v_result;

        -- If successful or non-version error, return
        IF v_result.status != 'noop:version_conflict' OR v_attempt >= p_max_attempts THEN
            RETURN v_result;
        END IF;

        -- Calculate exponential backoff delay
        v_wait_time := POWER(2, v_attempt) * 0.1; -- 0.1, 0.2, 0.4 seconds
        PERFORM pg_sleep(v_wait_time);

        v_attempt := v_attempt + 1;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

### Issue 3: Performance Impact of Audit Queries

**Symptom**: Slow queries when filtering by audit fields

**Solution**:
```sql
-- Add covering indexes for common audit query patterns
CREATE INDEX CONCURRENTLY idx_entity_audit_covering
ON tenant.tb_entity (updated_by, updated_at DESC)
INCLUDE (pk_entity, data);

-- Partition large audit tables by time
CREATE TABLE audit.tb_data_access_log_2024 PARTITION OF audit.tb_data_access_log
FOR VALUES FROM ('2024-01-01') TO ('2025-01-01');

-- Create monthly partitions automatically
CREATE OR REPLACE FUNCTION audit.create_monthly_partitions()
RETURNS void AS $$
DECLARE
    start_date DATE := DATE_TRUNC('month', NOW());
    end_date DATE := start_date + INTERVAL '1 month';
    partition_name TEXT;
BEGIN
    partition_name := 'tb_data_access_log_' || TO_CHAR(start_date, 'YYYY_MM');

    EXECUTE format('
        CREATE TABLE IF NOT EXISTS audit.%I PARTITION OF audit.tb_data_access_log
        FOR VALUES FROM (%L) TO (%L)
    ', partition_name, start_date, end_date);
END;
$$ LANGUAGE plpgsql;
```

## CRUD Function Implementation Patterns

All FraiseQL mutation functions must implement proper audit field handling following the app/core function split pattern. This ensures consistent audit trails across all operations.

### Create Operation with Audit Fields

The create operation establishes the complete audit trail for new entities:

```sql
-- Layer 1: app.* wrapper function
CREATE OR REPLACE FUNCTION app.create_entity(
    input_pk_organization UUID,
    input_created_by UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_entity_input;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_entity_input, input_payload);

    -- Delegate to core function
    RETURN core.create_entity(
        input_pk_organization,
        input_created_by,
        v_input,
        input_payload
    );
END;
$$ LANGUAGE plpgsql;

-- Layer 2: core.* implementation function
CREATE OR REPLACE FUNCTION core.create_entity(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_entity_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_entity_id UUID;
    v_change_source TEXT;
    v_validation_result JSONB;
BEGIN
    -- Audit metadata extraction
    v_change_source := COALESCE(
        input_payload->>'_change_source',
        'api'  -- Default for GraphQL mutations
    );

    -- Business validation
    v_validation_result := core.validate_entity_create(
        input_pk_organization,
        input_data
    );

    IF v_validation_result->>'status' != 'valid' THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'entity',
            NULL,
            'NOOP',
            'noop:validation_failed',
            ARRAY[]::TEXT[],
            v_validation_result->>'message',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_create',
                'validation_errors', v_validation_result->'errors'
            )
        );
    END IF;

    -- Create entity with full audit trail
    INSERT INTO tenant.tb_entity (
        pk_organization,
        data,
        -- Audit fields - creation
        created_at,
        created_by,
        updated_at,
        updated_by,
        version,
        change_reason,
        change_source
    ) VALUES (
        input_pk_organization,
        jsonb_build_object(
            'name', input_data.name,
            'description', input_data.description,
            'status', COALESCE(input_data.status, 'active')
        ),
        -- Audit values
        NOW(),                                      -- created_at
        input_created_by,                          -- created_by
        NOW(),                                      -- updated_at
        input_created_by,                          -- updated_by (same as created_by)
        1,                                         -- version (initial)
        input_payload->>'_change_reason',          -- change_reason
        v_change_source                            -- change_source
    ) RETURNING pk_entity INTO v_entity_id;

    -- Return success with audit metadata
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_created_by,
        'entity',
        v_entity_id,
        'INSERT',
        'new',
        ARRAY['name', 'description', 'status'],
        'Entity created successfully',
        NULL,  -- No "before" state for creates
        (SELECT data FROM public.tv_entity WHERE id = v_entity_id),
        jsonb_build_object(
            'trigger', 'api_create',
            'change_source', v_change_source,
            'initial_version', 1,
            'audit_complete', true
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### Update Operation with Optimistic Locking

Updates require careful handling of version conflicts and field change detection:

```sql
-- Layer 1: app.* wrapper function
CREATE OR REPLACE FUNCTION app.update_entity(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_pk_entity UUID,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_input app.type_entity_input;
BEGIN
    -- Convert JSONB to typed input
    v_input := jsonb_populate_record(NULL::app.type_entity_input, input_payload);

    -- Delegate to core function
    RETURN core.update_entity(
        input_pk_organization,
        input_updated_by,
        input_pk_entity,
        v_input,
        input_payload
    );
END;
$$ LANGUAGE plpgsql;

-- Layer 2: core.* implementation function
CREATE OR REPLACE FUNCTION core.update_entity(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_pk_entity UUID,
    input_data app.type_entity_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_current_record RECORD;
    v_expected_version INTEGER;
    v_changed_fields TEXT[];
    v_change_source TEXT;
    v_new_data JSONB;
BEGIN
    -- Get current state for optimistic locking and change detection
    SELECT version, data, updated_at INTO v_current_record
    FROM tenant.tb_entity
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization
    AND deleted_at IS NULL  -- Cannot update deleted entities
    FOR UPDATE;  -- Prevent concurrent modifications

    -- Entity not found or deleted
    IF v_current_record IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_updated_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:entity_not_found',
            ARRAY[]::TEXT[],
            'Entity not found or has been deleted',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_update',
                'entity_exists', false
            )
        );
    END IF;

    -- Optimistic locking check
    v_expected_version := (input_payload->>'_expected_version')::INTEGER;
    IF v_expected_version IS NOT NULL AND v_expected_version != v_current_record.version THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_updated_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:version_conflict',
            ARRAY[]::TEXT[],
            format('Version conflict: expected %s, current %s',
                   v_expected_version, v_current_record.version),
            v_current_record.data,
            v_current_record.data,  -- No change due to conflict
            jsonb_build_object(
                'trigger', 'api_update',
                'conflict_type', 'optimistic_lock',
                'expected_version', v_expected_version,
                'current_version', v_current_record.version,
                'retry_recommended', true
            )
        );
    END IF;

    -- Build new data object
    v_new_data := v_current_record.data || jsonb_build_object(
        'name', COALESCE(input_data.name, v_current_record.data->>'name'),
        'description', COALESCE(input_data.description, v_current_record.data->>'description'),
        'status', COALESCE(input_data.status, v_current_record.data->>'status')
    );

    -- Calculate which fields actually changed
    v_changed_fields := core.calculate_changed_fields(
        v_current_record.data,
        v_new_data
    );

    -- Check for NOOP (no actual changes)
    IF array_length(v_changed_fields, 1) IS NULL OR array_length(v_changed_fields, 1) = 0 THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_updated_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:no_changes',
            ARRAY[]::TEXT[],
            'No changes detected in update request',
            v_current_record.data,
            v_current_record.data,
            jsonb_build_object(
                'trigger', 'api_update',
                'noop_reason', 'identical_data',
                'idempotent_safe', true
            )
        );
    END IF;

    -- Extract audit metadata
    v_change_source := COALESCE(
        input_payload->>'_change_source',
        'api'
    );

    -- Perform update with audit trail
    UPDATE tenant.tb_entity
    SET
        data = v_new_data,
        -- Update audit fields
        updated_at = NOW(),
        updated_by = input_updated_by,
        version = version + 1,  -- Increment version for optimistic locking
        change_reason = input_payload->>'_change_reason',
        change_source = v_change_source
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization;

    -- Return success with complete audit information
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_updated_by,
        'entity',
        input_pk_entity,
        'UPDATE',
        'updated',
        v_changed_fields,
        'Entity updated successfully',
        v_current_record.data,
        (SELECT data FROM public.tv_entity WHERE id = input_pk_entity),
        jsonb_build_object(
            'trigger', 'api_update',
            'version_increment', v_current_record.version || ' → ' || (v_current_record.version + 1),
            'optimistic_lock_used', v_expected_version IS NOT NULL,
            'fields_changed_count', array_length(v_changed_fields, 1),
            'change_source', v_change_source
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### Soft Delete Operation with Audit Trail

Soft deletes maintain full audit trails while marking entities as inactive:

```sql
-- Layer 1: app.* wrapper function
CREATE OR REPLACE FUNCTION app.delete_entity(
    input_pk_organization UUID,
    input_deleted_by UUID,
    input_pk_entity UUID,
    input_payload JSONB DEFAULT '{}'::JSONB
) RETURNS app.mutation_result AS $$
BEGIN
    -- Direct delegation to core function
    RETURN core.delete_entity(
        input_pk_organization,
        input_deleted_by,
        input_pk_entity,
        input_payload
    );
END;
$$ LANGUAGE plpgsql;

-- Layer 2: core.* implementation function
CREATE OR REPLACE FUNCTION core.delete_entity(
    input_pk_organization UUID,
    input_deleted_by UUID,
    input_pk_entity UUID,
    input_payload JSONB DEFAULT '{}'::JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_current_record RECORD;
    v_delete_reason TEXT;
    v_change_source TEXT;
    v_cascade_deletes INTEGER := 0;
BEGIN
    -- Get current state (check if already deleted)
    SELECT version, data, deleted_at INTO v_current_record
    FROM tenant.tb_entity
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization
    FOR UPDATE;  -- Prevent race conditions

    -- Entity not found
    IF v_current_record IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_deleted_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:entity_not_found',
            ARRAY[]::TEXT[],
            'Entity not found',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_delete',
                'entity_exists', false
            )
        );
    END IF;

    -- Already deleted (idempotent operation)
    IF v_current_record.deleted_at IS NOT NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_deleted_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:already_deleted',
            ARRAY[]::TEXT[],
            'Entity is already deleted',
            NULL,  -- No current state for deleted entities
            NULL,
            jsonb_build_object(
                'trigger', 'api_delete',
                'idempotent_safe', true,
                'deleted_at', v_current_record.deleted_at
            )
        );
    END IF;

    -- Extract audit metadata
    v_delete_reason := COALESCE(
        input_payload->>'_change_reason',
        'Deleted via API'
    );

    v_change_source := COALESCE(
        input_payload->>'_change_source',
        'api'
    );

    -- Check for cascade deletion requirements
    SELECT COUNT(*) INTO v_cascade_deletes
    FROM tenant.tb_dependent_entity
    WHERE fk_entity = input_pk_entity
    AND deleted_at IS NULL;

    -- Perform soft delete with complete audit trail
    UPDATE tenant.tb_entity
    SET
        -- Soft delete fields
        deleted_at = NOW(),
        deleted_by = input_deleted_by,
        -- Update audit fields
        updated_at = NOW(),
        updated_by = input_deleted_by,
        version = version + 1,  -- Increment version
        change_reason = v_delete_reason,
        change_source = v_change_source
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization;

    -- Handle cascade deletions if configured
    IF input_payload->>'_cascade_delete' = 'true' AND v_cascade_deletes > 0 THEN
        UPDATE tenant.tb_dependent_entity
        SET
            deleted_at = NOW(),
            deleted_by = input_deleted_by,
            updated_at = NOW(),
            updated_by = input_deleted_by,
            version = version + 1,
            change_reason = 'Cascade delete from parent entity',
            change_source = v_change_source
        WHERE fk_entity = input_pk_entity
        AND deleted_at IS NULL;
    END IF;

    -- Return success with deletion audit information
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_deleted_by,
        'entity',
        input_pk_entity,
        'DELETE',
        'deleted',
        ARRAY['deleted_at', 'deleted_by'],
        'Entity deleted successfully',
        v_current_record.data,
        NULL,  -- No "after" state for deletions
        jsonb_build_object(
            'trigger', 'api_delete',
            'soft_delete', true,
            'delete_reason', v_delete_reason,
            'change_source', v_change_source,
            'cascade_deletes', v_cascade_deletes,
            'version_increment', v_current_record.version || ' → ' || (v_current_record.version + 1)
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### Restore/Undelete Operation

Audit-aware restoration of soft-deleted entities:

```sql
CREATE OR REPLACE FUNCTION core.restore_entity(
    input_pk_organization UUID,
    input_restored_by UUID,
    input_pk_entity UUID,
    input_payload JSONB DEFAULT '{}'::JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_current_record RECORD;
    v_restore_reason TEXT;
BEGIN
    -- Get current state (must be soft-deleted)
    SELECT version, data, deleted_at, deleted_by INTO v_current_record
    FROM tenant.tb_entity
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization
    FOR UPDATE;

    -- Entity not found
    IF v_current_record IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_restored_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:entity_not_found',
            ARRAY[]::TEXT[],
            'Entity not found',
            NULL,
            NULL,
            jsonb_build_object('trigger', 'api_restore')
        );
    END IF;

    -- Not deleted (cannot restore active entity)
    IF v_current_record.deleted_at IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_restored_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:not_deleted',
            ARRAY[]::TEXT[],
            'Entity is not deleted, cannot restore',
            v_current_record.data,
            v_current_record.data,
            jsonb_build_object(
                'trigger', 'api_restore',
                'entity_active', true
            )
        );
    END IF;

    v_restore_reason := COALESCE(
        input_payload->>'_change_reason',
        'Restored via API'
    );

    -- Restore entity with audit trail
    UPDATE tenant.tb_entity
    SET
        -- Clear soft delete fields
        deleted_at = NULL,
        deleted_by = NULL,
        -- Update audit fields
        updated_at = NOW(),
        updated_by = input_restored_by,
        version = version + 1,
        change_reason = v_restore_reason,
        change_source = COALESCE(input_payload->>'_change_source', 'api')
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization;

    -- Return success
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_restored_by,
        'entity',
        input_pk_entity,
        'UPDATE',
        'restored',
        ARRAY['deleted_at', 'deleted_by'],
        'Entity restored successfully',
        NULL,  -- No "before" state shown for restorations
        (SELECT data FROM public.tv_entity WHERE id = input_pk_entity),
        jsonb_build_object(
            'trigger', 'api_restore',
            'restore_reason', v_restore_reason,
            'previously_deleted_by', v_current_record.deleted_by,
            'deleted_duration_hours', ROUND(
                EXTRACT(EPOCH FROM (NOW() - v_current_record.deleted_at)) / 3600, 2
            )
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### Bulk Operations with Audit Tracking

Efficient bulk operations while maintaining individual audit trails:

```sql
-- Bulk update with audit tracking
CREATE OR REPLACE FUNCTION core.bulk_update_entities(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_updates JSONB,  -- Array of {id, data, expected_version}
    input_payload JSONB DEFAULT '{}'::JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_update_record JSONB;
    v_successful_updates UUID[] := ARRAY[]::UUID[];
    v_failed_updates JSONB[] := ARRAY[]::JSONB[];
    v_total_updates INTEGER := 0;
    v_change_source TEXT;
BEGIN
    v_change_source := COALESCE(input_payload->>'_change_source', 'api_bulk');

    -- Process each update individually to maintain audit trails
    FOR v_update_record IN SELECT * FROM jsonb_array_elements(input_updates)
    LOOP
        DECLARE
            v_result app.mutation_result;
            v_entity_id UUID := (v_update_record->>'id')::UUID;
        BEGIN
            -- Call individual update function
            SELECT core.update_entity(
                input_pk_organization,
                input_updated_by,
                v_entity_id,
                jsonb_populate_record(
                    NULL::app.type_entity_input,
                    v_update_record->'data'
                ),
                v_update_record || jsonb_build_object('_change_source', v_change_source)
            ) INTO v_result;

            v_total_updates := v_total_updates + 1;

            -- Track success/failure
            IF v_result.status IN ('INSERT', 'UPDATE') THEN
                v_successful_updates := array_append(v_successful_updates, v_entity_id);
            ELSE
                v_failed_updates := array_append(
                    v_failed_updates,
                    jsonb_build_object(
                        'id', v_entity_id,
                        'error', v_result.status,
                        'message', v_result.message
                    )
                );
            END IF;
        EXCEPTION WHEN OTHERS THEN
            v_failed_updates := array_append(
                v_failed_updates,
                jsonb_build_object(
                    'id', v_entity_id,
                    'error', 'exception',
                    'message', SQLERRM
                )
            );
        END;
    END LOOP;

    -- Return bulk operation result
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_updated_by,
        'entity',
        NULL,  -- No single entity ID for bulk operations
        CASE
            WHEN array_length(v_failed_updates, 1) = 0 THEN 'UPDATE'
            WHEN array_length(v_successful_updates, 1) = 0 THEN 'NOOP'
            ELSE 'UPDATE'  -- Partial success
        END,
        CASE
            WHEN array_length(v_failed_updates, 1) = 0 THEN 'bulk_updated'
            WHEN array_length(v_successful_updates, 1) = 0 THEN 'noop:bulk_failed'
            ELSE 'bulk_partial'
        END,
        ARRAY[]::TEXT[],  -- No specific fields for bulk operations
        format('Bulk update: %s successful, %s failed of %s total',
               array_length(v_successful_updates, 1),
               array_length(v_failed_updates, 1),
               v_total_updates),
        NULL,
        NULL,
        jsonb_build_object(
            'trigger', 'api_bulk_update',
            'total_requested', v_total_updates,
            'successful_count', array_length(v_successful_updates, 1),
            'failed_count', array_length(v_failed_updates, 1),
            'successful_ids', v_successful_updates,
            'failed_updates', v_failed_updates,
            'change_source', v_change_source
        )
    );
END;
$$ LANGUAGE plpgsql;
```

## View Patterns with Audit Field Exposure

FraiseQL views must expose audit information in a consistent, queryable format that supports GraphQL requirements while maintaining performance.

### Standard Audit-Enabled Views

Views should expose audit fields with proper naming conventions and user-friendly formats:

```sql
-- Standard entity view with comprehensive audit information
CREATE OR REPLACE VIEW public.v_entity AS
SELECT
    e.pk_entity AS id,
    e.pk_organization AS tenant_id,

    -- Entity data fields
    e.data->>'name' AS name,
    e.data->>'description' AS description,
    e.data->>'status' AS status,

    -- Core audit fields (always present)
    e.created_at,
    e.created_by,
    cu.data->>'name' AS created_by_name,
    cu.data->>'email' AS created_by_email,

    e.updated_at,
    e.updated_by,
    uu.data->>'name' AS updated_by_name,
    uu.data->>'email' AS updated_by_email,

    -- Soft delete audit fields
    e.deleted_at,
    e.deleted_by,
    du.data->>'name' AS deleted_by_name,
    du.data->>'email' AS deleted_by_email,

    -- Version and change tracking
    e.version,
    e.change_reason,
    e.change_source,

    -- Computed audit fields
    e.deleted_at IS NOT NULL AS is_deleted,
    EXTRACT(EPOCH FROM (NOW() - e.created_at)) / 86400 AS age_days,
    EXTRACT(EPOCH FROM (NOW() - e.updated_at)) / 86400 AS days_since_update,
    CASE
        WHEN e.updated_at > e.created_at + INTERVAL '1 minute' THEN true
        ELSE false
    END AS has_been_modified,

    -- Complete audit trail as structured JSONB
    jsonb_build_object(
        'created', jsonb_build_object(
            'timestamp', e.created_at,
            'by_id', e.created_by,
            'by_name', cu.data->>'name',
            'by_email', cu.data->>'email'
        ),
        'last_updated', CASE WHEN e.updated_at > e.created_at THEN
            jsonb_build_object(
                'timestamp', e.updated_at,
                'by_id', e.updated_by,
                'by_name', uu.data->>'name',
                'by_email', uu.data->>'email',
                'reason', e.change_reason,
                'source', e.change_source,
                'version', e.version
            ) END,
        'deleted', CASE WHEN e.deleted_at IS NOT NULL THEN
            jsonb_build_object(
                'timestamp', e.deleted_at,
                'by_id', e.deleted_by,
                'by_name', du.data->>'name',
                'by_email', du.data->>'email'
            ) END,
        'lifecycle', jsonb_build_object(
            'current_version', e.version,
            'age_days', ROUND(EXTRACT(EPOCH FROM (NOW() - e.created_at)) / 86400, 2),
            'days_since_update', ROUND(EXTRACT(EPOCH FROM (NOW() - e.updated_at)) / 86400, 2),
            'is_active', e.deleted_at IS NULL,
            'modification_count', GREATEST(e.version - 1, 0)
        )
    ) AS audit_trail

FROM tenant.tb_entity e
-- Left joins for user information (handle deleted users gracefully)
LEFT JOIN tenant.tb_user cu ON cu.pk_user = e.created_by
LEFT JOIN tenant.tb_user uu ON uu.pk_user = e.updated_by
LEFT JOIN tenant.tb_user du ON du.pk_user = e.deleted_by
WHERE e.deleted_at IS NULL;  -- Standard view excludes soft-deleted records
```

### Comprehensive Audit View (Including Deleted Records)

For admin interfaces and audit reporting, include soft-deleted entities:

```sql
-- Administrative view including all entities (active and deleted)
CREATE OR REPLACE VIEW public.v_entity_audit AS
SELECT
    *,
    -- Additional admin audit fields
    CASE
        WHEN deleted_at IS NOT NULL THEN 'deleted'
        WHEN version > 1 THEN 'modified'
        ELSE 'original'
    END AS lifecycle_status,

    -- Deletion audit information
    CASE WHEN deleted_at IS NOT NULL THEN
        ROUND(EXTRACT(EPOCH FROM (deleted_at - created_at)) / 86400, 2)
    END AS lifespan_days,

    CASE WHEN deleted_at IS NOT NULL THEN
        ROUND(EXTRACT(EPOCH FROM (NOW() - deleted_at)) / 86400, 2)
    END AS days_since_deletion,

    -- Change frequency analysis
    CASE
        WHEN version = 1 THEN 'never_modified'
        WHEN version <= 3 THEN 'low_activity'
        WHEN version <= 10 THEN 'moderate_activity'
        ELSE 'high_activity'
    END AS modification_pattern

FROM public.v_entity
UNION ALL
SELECT
    e.pk_entity AS id,
    e.pk_organization AS tenant_id,
    e.data->>'name' AS name,
    e.data->>'description' AS description,
    e.data->>'status' AS status,
    e.created_at,
    e.created_by,
    cu.data->>'name' AS created_by_name,
    cu.data->>'email' AS created_by_email,
    e.updated_at,
    e.updated_by,
    uu.data->>'name' AS updated_by_name,
    uu.data->>'email' AS updated_by_email,
    e.deleted_at,
    e.deleted_by,
    du.data->>'name' AS deleted_by_name,
    du.data->>'email' AS deleted_by_email,
    e.version,
    e.change_reason,
    e.change_source,
    true AS is_deleted,  -- All records in this UNION are deleted
    EXTRACT(EPOCH FROM (NOW() - e.created_at)) / 86400 AS age_days,
    EXTRACT(EPOCH FROM (NOW() - e.updated_at)) / 86400 AS days_since_update,
    true AS has_been_modified,  -- Deletion counts as modification

    -- Audit trail for deleted entities
    jsonb_build_object(
        'created', jsonb_build_object(
            'timestamp', e.created_at,
            'by_id', e.created_by,
            'by_name', cu.data->>'name'
        ),
        'deleted', jsonb_build_object(
            'timestamp', e.deleted_at,
            'by_id', e.deleted_by,
            'by_name', du.data->>'name',
            'reason', e.change_reason
        ),
        'lifecycle', jsonb_build_object(
            'final_version', e.version,
            'lifespan_days', ROUND(EXTRACT(EPOCH FROM (e.deleted_at - e.created_at)) / 86400, 2),
            'days_since_deletion', ROUND(EXTRACT(EPOCH FROM (NOW() - e.deleted_at)) / 86400, 2)
        )
    ) AS audit_trail,

    'deleted' AS lifecycle_status,
    ROUND(EXTRACT(EPOCH FROM (e.deleted_at - e.created_at)) / 86400, 2) AS lifespan_days,
    ROUND(EXTRACT(EPOCH FROM (NOW() - e.deleted_at)) / 86400, 2) AS days_since_deletion,

    CASE
        WHEN e.version = 1 THEN 'never_modified'
        WHEN e.version <= 3 THEN 'low_activity'
        WHEN e.version <= 10 THEN 'moderate_activity'
        ELSE 'high_activity'
    END AS modification_pattern

FROM tenant.tb_entity e
LEFT JOIN tenant.tb_user cu ON cu.pk_user = e.created_by
LEFT JOIN tenant.tb_user uu ON uu.pk_user = e.updated_by
LEFT JOIN tenant.tb_user du ON du.pk_user = e.deleted_by
WHERE e.deleted_at IS NOT NULL;  -- Only soft-deleted records
```

### Performance-Optimized Cached Views

For frequently accessed audit data, use TurboRouter cached views:

```sql
-- Cached view for high-performance audit queries
CREATE OR REPLACE VIEW public.tv_entity_audit AS
SELECT
    id,
    tenant_id,
    name,
    status,

    -- Essential audit fields only (for performance)
    created_at,
    created_by_name,
    updated_at,
    updated_by_name,
    version,
    is_deleted,
    days_since_update,

    -- Simplified audit metadata
    jsonb_build_object(
        'version', version,
        'last_update', updated_at,
        'is_active', NOT is_deleted,
        'age_category', CASE
            WHEN days_since_update < 1 THEN 'recent'
            WHEN days_since_update < 7 THEN 'current'
            WHEN days_since_update < 30 THEN 'aging'
            ELSE 'stale'
        END
    ) AS audit_summary,

    -- TurboRouter cache timestamp
    NOW() AS cached_at

FROM public.v_entity;
```

## Change Tracking Utility Functions

Comprehensive utilities for detecting, analyzing, and reporting data changes:

### Field Change Detection

```sql
-- Enhanced change detection with field-level analysis
CREATE OR REPLACE FUNCTION core.calculate_changed_fields(
    p_before JSONB,
    p_after JSONB
) RETURNS TEXT[] AS $$
DECLARE
    v_changed_fields TEXT[] := ARRAY[]::TEXT[];
    v_key TEXT;
    v_before_value JSONB;
    v_after_value JSONB;
BEGIN
    -- Check each key in the new data
    FOR v_key IN SELECT jsonb_object_keys(p_after)
    LOOP
        -- Skip private audit fields (starting with _)
        CONTINUE WHEN v_key LIKE '\_%';

        -- Get values for comparison
        v_before_value := COALESCE(p_before->v_key, 'null'::jsonb);
        v_after_value := p_after->v_key;

        -- Check if value actually changed (handles JSON null vs SQL NULL)
        IF v_after_value IS DISTINCT FROM v_before_value THEN
            v_changed_fields := array_append(v_changed_fields, v_key);
        END IF;
    END LOOP;

    -- Check for removed keys (fields that existed before but not after)
    FOR v_key IN SELECT jsonb_object_keys(p_before)
    LOOP
        CONTINUE WHEN v_key LIKE '\_%';

        IF NOT (p_after ? v_key) THEN
            v_changed_fields := array_append(v_changed_fields, v_key || '_REMOVED');
        END IF;
    END LOOP;

    RETURN v_changed_fields;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Generate detailed change report with before/after values
CREATE OR REPLACE FUNCTION core.generate_change_report(
    p_before JSONB,
    p_after JSONB,
    p_changed_fields TEXT[]
) RETURNS JSONB AS $$
DECLARE
    v_changes JSONB := '[]'::jsonb;
    v_field TEXT;
    v_change_detail JSONB;
BEGIN
    FOREACH v_field IN ARRAY p_changed_fields
    LOOP
        -- Handle removed fields
        IF v_field LIKE '%_REMOVED' THEN
            v_field := replace(v_field, '_REMOVED', '');
            v_change_detail := jsonb_build_object(
                'field', v_field,
                'change_type', 'removed',
                'old_value', p_before->v_field,
                'new_value', null
            );
        ELSE
            v_change_detail := jsonb_build_object(
                'field', v_field,
                'change_type', CASE
                    WHEN p_before ? v_field THEN 'modified'
                    ELSE 'added'
                END,
                'old_value', p_before->v_field,
                'new_value', p_after->v_field
            );
        END IF;

        v_changes := v_changes || jsonb_build_array(v_change_detail);
    END LOOP;

    RETURN jsonb_build_object(
        'total_changes', array_length(p_changed_fields, 1),
        'changes', v_changes,
        'generated_at', NOW()
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;
```

### Audit Metadata Generation

```sql
-- Build comprehensive audit metadata for mutation results
CREATE OR REPLACE FUNCTION core.build_audit_metadata(
    p_entity_type TEXT,
    p_operation_type TEXT,
    p_entity_id UUID,
    p_version_before INTEGER DEFAULT NULL,
    p_version_after INTEGER DEFAULT NULL,
    p_changed_fields TEXT[] DEFAULT ARRAY[]::TEXT[],
    p_change_reason TEXT DEFAULT NULL,
    p_change_source TEXT DEFAULT 'api',
    p_before_data JSONB DEFAULT NULL,
    p_after_data JSONB DEFAULT NULL
) RETURNS JSONB AS $$
DECLARE
    v_metadata JSONB;
    v_change_report JSONB;
BEGIN
    -- Generate detailed change report if data provided
    IF p_before_data IS NOT NULL AND p_after_data IS NOT NULL THEN
        v_change_report := core.generate_change_report(
            p_before_data,
            p_after_data,
            p_changed_fields
        );
    END IF;

    -- Build comprehensive metadata
    v_metadata := jsonb_build_object(
        'audit', jsonb_build_object(
            'entity_type', p_entity_type,
            'entity_id', p_entity_id,
            'operation', p_operation_type,
            'timestamp', NOW(),
            'change_source', p_change_source,
            'change_reason', p_change_reason
        ),
        'versioning', CASE WHEN p_version_before IS NOT NULL OR p_version_after IS NOT NULL THEN
            jsonb_build_object(
                'version_before', p_version_before,
                'version_after', p_version_after,
                'version_increment', CASE
                    WHEN p_version_before IS NOT NULL AND p_version_after IS NOT NULL
                    THEN p_version_after - p_version_before
                    ELSE NULL
                END,
                'optimistic_locking', true
            ) END,
        'field_changes', CASE WHEN array_length(p_changed_fields, 1) > 0 THEN
            jsonb_build_object(
                'fields_modified', p_changed_fields,
                'field_count', array_length(p_changed_fields, 1),
                'detailed_report', v_change_report
            ) END
    );

    -- Remove null values for cleaner metadata
    RETURN core.sanitize_jsonb_nulls(v_metadata);
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Remove null values from JSONB (cleanup utility)
CREATE OR REPLACE FUNCTION core.sanitize_jsonb_nulls(p_input JSONB)
RETURNS JSONB AS $$
DECLARE
    v_key TEXT;
    v_result JSONB := '{}'::jsonb;
BEGIN
    FOR v_key IN SELECT jsonb_object_keys(p_input)
    LOOP
        IF p_input->v_key IS NOT NULL AND p_input->v_key != 'null'::jsonb THEN
            v_result := v_result || jsonb_build_object(v_key, p_input->v_key);
        END IF;
    END LOOP;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql IMMUTABLE;
```

### Audit Query Patterns

Common query patterns for audit trail analysis:

```sql
-- User activity analysis
CREATE OR REPLACE VIEW public.v_user_activity_audit AS
SELECT
    u.pk_user AS user_id,
    u.data->>'name' AS user_name,
    u.data->>'email' AS user_email,

    -- Creation activity
    COUNT(CASE WHEN e.created_by = u.pk_user THEN 1 END) AS entities_created,
    MIN(CASE WHEN e.created_by = u.pk_user THEN e.created_at END) AS first_creation,
    MAX(CASE WHEN e.created_by = u.pk_user THEN e.created_at END) AS last_creation,

    -- Update activity
    COUNT(CASE WHEN e.updated_by = u.pk_user AND e.version > 1 THEN 1 END) AS entities_updated,
    MAX(CASE WHEN e.updated_by = u.pk_user THEN e.updated_at END) AS last_update,

    -- Deletion activity
    COUNT(CASE WHEN e.deleted_by = u.pk_user THEN 1 END) AS entities_deleted,
    MAX(CASE WHEN e.deleted_by = u.pk_user THEN e.deleted_at END) AS last_deletion,

    -- Activity summary
    COUNT(CASE WHEN e.created_by = u.pk_user OR e.updated_by = u.pk_user OR e.deleted_by = u.pk_user THEN 1 END) AS total_activity,

    -- Recent activity (last 30 days)
    COUNT(CASE
        WHEN (e.created_by = u.pk_user AND e.created_at > NOW() - INTERVAL '30 days')
          OR (e.updated_by = u.pk_user AND e.updated_at > NOW() - INTERVAL '30 days')
          OR (e.deleted_by = u.pk_user AND e.deleted_at > NOW() - INTERVAL '30 days')
        THEN 1 END) AS recent_activity_30d

FROM tenant.tb_user u
LEFT JOIN tenant.tb_entity e ON (
    e.created_by = u.pk_user
    OR e.updated_by = u.pk_user
    OR e.deleted_by = u.pk_user
)
WHERE u.deleted_at IS NULL  -- Only active users
GROUP BY u.pk_user, u.data->>'name', u.data->>'email';

-- Entity lifecycle analysis
CREATE OR REPLACE VIEW public.v_entity_lifecycle_audit AS
SELECT
    e.pk_entity AS entity_id,
    e.data->>'name' AS entity_name,
    e.pk_organization AS tenant_id,

    -- Lifecycle timestamps
    e.created_at,
    e.updated_at,
    e.deleted_at,

    -- Lifecycle metrics
    e.version,
    EXTRACT(EPOCH FROM (e.updated_at - e.created_at)) / 86400 AS active_lifespan_days,
    CASE WHEN e.deleted_at IS NOT NULL THEN
        EXTRACT(EPOCH FROM (e.deleted_at - e.created_at)) / 86400
    END AS total_lifespan_days,

    -- Modification patterns
    CASE
        WHEN e.version = 1 THEN 'never_modified'
        WHEN e.version <= 3 THEN 'lightly_modified'
        WHEN e.version <= 10 THEN 'moderately_modified'
        WHEN e.version <= 50 THEN 'heavily_modified'
        ELSE 'extremely_modified'
    END AS modification_category,

    -- Status classification
    CASE
        WHEN e.deleted_at IS NOT NULL THEN 'deleted'
        WHEN e.updated_at < NOW() - INTERVAL '90 days' THEN 'stale'
        WHEN e.updated_at < NOW() - INTERVAL '30 days' THEN 'aging'
        WHEN e.updated_at < NOW() - INTERVAL '7 days' THEN 'recent'
        ELSE 'current'
    END AS activity_status,

    -- Change frequency (changes per day)
    CASE WHEN e.updated_at > e.created_at THEN
        ROUND((e.version - 1) / GREATEST(EXTRACT(EPOCH FROM (e.updated_at - e.created_at)) / 86400, 1), 3)
    ELSE 0
    END AS avg_changes_per_day

FROM tenant.tb_entity e;

-- Audit trail search function
CREATE OR REPLACE FUNCTION audit.search_entity_changes(
    p_entity_type TEXT DEFAULT NULL,
    p_user_id UUID DEFAULT NULL,
    p_date_from TIMESTAMPTZ DEFAULT NULL,
    p_date_to TIMESTAMPTZ DEFAULT NOW(),
    p_operation_types TEXT[] DEFAULT ARRAY['INSERT', 'UPDATE', 'DELETE'],
    p_limit INTEGER DEFAULT 100
) RETURNS TABLE (
    entity_id UUID,
    entity_type TEXT,
    operation_type TEXT,
    changed_by UUID,
    changed_by_name TEXT,
    changed_at TIMESTAMPTZ,
    version_change TEXT,
    fields_changed TEXT[],
    change_reason TEXT,
    change_source TEXT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        e.pk_entity,
        'entity'::TEXT,  -- Would be dynamic in real implementation
        CASE
            WHEN e.version = 1 AND e.deleted_at IS NULL THEN 'INSERT'
            WHEN e.deleted_at IS NOT NULL THEN 'DELETE'
            ELSE 'UPDATE'
        END,
        COALESCE(e.updated_by, e.created_by),
        COALESCE(uu.data->>'name', cu.data->>'name'),
        GREATEST(e.created_at, COALESCE(e.updated_at, e.created_at)),
        CASE
            WHEN e.version = 1 THEN 'initial'
            ELSE (e.version - 1)::TEXT || ' → ' || e.version::TEXT
        END,
        ARRAY[]::TEXT[],  -- Would need change log table for historical field changes
        e.change_reason,
        e.change_source
    FROM tenant.tb_entity e
    LEFT JOIN tenant.tb_user cu ON cu.pk_user = e.created_by
    LEFT JOIN tenant.tb_user uu ON uu.pk_user = e.updated_by
    WHERE
        (p_entity_type IS NULL OR 'entity' = p_entity_type)
        AND (p_user_id IS NULL OR e.created_by = p_user_id OR e.updated_by = p_user_id)
        AND (p_date_from IS NULL OR GREATEST(e.created_at, COALESCE(e.updated_at, e.created_at)) >= p_date_from)
        AND GREATEST(e.created_at, COALESCE(e.updated_at, e.created_at)) <= p_date_to
    ORDER BY GREATEST(e.created_at, COALESCE(e.updated_at, e.created_at)) DESC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;
```

---
