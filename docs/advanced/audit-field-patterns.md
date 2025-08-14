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

---

*This documentation covers the foundational concepts of audit field patterns. Continue reading the following sections for detailed implementation patterns, GraphQL integration, and advanced compliance features.*
