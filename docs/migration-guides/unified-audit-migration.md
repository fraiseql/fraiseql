# Migrating to Unified Audit Table

## Overview
If you're using the old dual-table audit system, migrate to the unified approach.

## Old System (Before)
```sql
-- Separate tables
tenant.tb_audit_log      -- CDC data
audit_events             -- Crypto chain
bridge_audit_to_chain()  -- Bridge trigger
```

## New System (After)
```sql
-- Single unified table
audit_events  -- CDC + Crypto in one table
```

## Migration Steps

### 1. Backup Existing Data
```sql
-- Export old audit logs
COPY tenant.tb_audit_log TO '/tmp/old_audit_log.csv' CSV HEADER;
COPY audit_events TO '/tmp/old_audit_events.csv' CSV HEADER;
```

### 2. Apply New Migration
```sql
\i src/fraiseql/enterprise/migrations/002_unified_audit.sql
```

### 3. Migrate Data (if needed)
```sql
-- Insert old tb_audit_log data into unified audit_events
INSERT INTO audit_events (
    tenant_id, user_id, entity_type, entity_id,
    operation_type, operation_subtype, changed_fields,
    old_data, new_data, metadata, timestamp
)
SELECT
    pk_organization, user_id, entity_type, entity_id,
    operation_type, operation_subtype, changed_fields,
    old_data, new_data, metadata, created_at
FROM tenant.tb_audit_log
ORDER BY created_at ASC;
-- Note: Crypto fields will be auto-populated by trigger
```

### 4. Update Function Calls
```sql
-- Change all log_and_return_mutation() calls to use new signature
-- See examples in examples/blog_api/db/functions/core_functions.sql
```

### 5. Drop Old Tables (after verification)
```sql
DROP TABLE IF EXISTS tenant.tb_audit_log CASCADE;
-- Keep only unified audit_events
```

## Breaking Changes
- Function signature slightly different (returns TABLE instead of composite type)
- Crypto fields now auto-populated (don't pass them manually)
- Single table queries instead of JOINs

## Benefits
- ✅ Simpler schema
- ✅ Better performance
- ✅ Single source of truth
- ✅ Easier to query</content>
</xai:function_call">docs/migration-guides/unified-audit-migration.md
