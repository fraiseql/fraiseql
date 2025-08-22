# Prompt: Implement Audit Field Patterns Documentation

## Objective

Create comprehensive documentation for FraiseQL's audit field patterns, which are **essential** for enterprise applications requiring change tracking, compliance, and data governance. This pattern standardizes how entities track creation, modification, and deletion metadata.

## Current State

**Status: MINIMAL DOCUMENTATION (10% coverage)**
- Basic `created_at` fields mentioned in examples
- No standardized audit field patterns
- Missing change tracking documentation
- No compliance or governance guidance

## Target Documentation

Create new documentation file: `docs/advanced/audit-field-patterns.md`

## Implementation Requirements

### 1. Document Standard Audit Field Pattern

**Core audit fields for all entities:**
```sql
-- Standard audit fields in tb_* tables
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

    -- Change tracking
    change_reason TEXT,  -- Why this change was made
    change_source TEXT   -- How this change was made (api, import, system)
);
```

### 2. Document Audit Field Semantics

**Field meanings and usage:**

| Field | Purpose | When Set | Required |
|-------|---------|----------|----------|
| `created_at` | Entity creation timestamp | INSERT only | Yes |
| `created_by` | User who created entity | INSERT only | Yes |
| `updated_at` | Last modification timestamp | INSERT and UPDATE | Yes |
| `updated_by` | User who last modified | UPDATE only | No* |
| `deleted_at` | Soft delete timestamp | Soft delete only | No |
| `deleted_by` | User who deleted entity | Soft delete only | No |
| `version` | Optimistic locking version | INSERT and UPDATE | Yes |
| `change_reason` | Human-readable change reason | Any change | No |
| `change_source` | How change was initiated | Any change | No |

*`updated_by` can be NULL for system-initiated updates

### 3. Document Function Implementation Patterns

**Create operation with audit fields:**
```sql
CREATE OR REPLACE FUNCTION core.create_entity(
    input_pk_organization UUID,
    input_created_by UUID,
    input_data app.type_entity_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_entity_id UUID;
    v_change_source TEXT;
BEGIN
    -- Determine change source
    v_change_source := COALESCE(
        input_payload->>'_change_source',
        'api'  -- Default for GraphQL mutations
    );

    -- Create entity with full audit trail
    INSERT INTO tenant.tb_entity (
        pk_organization,
        data,
        -- Audit fields
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
            'description', input_data.description
        ),
        -- Audit values
        NOW(),
        input_created_by,
        NOW(),
        input_created_by,
        1,  -- Initial version
        input_payload->>'_change_reason',
        v_change_source
    ) RETURNING pk_entity INTO v_entity_id;

    -- Return via mutation result pattern
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_created_by,
        'entity',
        v_entity_id,
        'INSERT',
        'new',
        ARRAY['name', 'description'],
        'Entity created successfully',
        NULL,
        (SELECT data FROM public.tv_entity WHERE id = v_entity_id),
        jsonb_build_object(
            'trigger', 'api_create',
            'change_source', v_change_source,
            'initial_version', 1
        )
    );
END;
$$ LANGUAGE plpgsql;
```

**Update operation with audit fields:**
```sql
CREATE OR REPLACE FUNCTION core.update_entity(
    input_pk_organization UUID,
    input_updated_by UUID,
    input_pk_entity UUID,
    input_data app.type_entity_input,
    input_payload JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_current_version INTEGER;
    v_expected_version INTEGER;
    v_current_data JSONB;
    v_changed_fields TEXT[];
BEGIN
    -- Get current state for optimistic locking
    SELECT version, data INTO v_current_version, v_current_data
    FROM tenant.tb_entity
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization;

    -- Check for concurrent modifications
    v_expected_version := (input_payload->>'_expected_version')::INTEGER;
    IF v_expected_version IS NOT NULL AND v_expected_version != v_current_version THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_updated_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:version_conflict',
            ARRAY[]::TEXT[],
            format('Version conflict: expected %s, current %s',
                   v_expected_version, v_current_version),
            v_current_data,
            v_current_data,
            jsonb_build_object(
                'trigger', 'api_update',
                'conflict_type', 'optimistic_lock',
                'expected_version', v_expected_version,
                'current_version', v_current_version
            )
        );
    END IF;

    -- Calculate changed fields
    v_changed_fields := core.calculate_changed_fields(
        v_current_data,
        input_payload
    );

    -- Perform update with audit trail
    UPDATE tenant.tb_entity
    SET
        data = data || jsonb_build_object(
            'name', input_data.name,
            'description', input_data.description
        ),
        -- Update audit fields
        updated_at = NOW(),
        updated_by = input_updated_by,
        version = version + 1,  -- Increment version
        change_reason = input_payload->>'_change_reason',
        change_source = COALESCE(input_payload->>'_change_source', 'api')
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization;

    -- Return success
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_updated_by,
        'entity',
        input_pk_entity,
        'UPDATE',
        'updated',
        v_changed_fields,
        'Entity updated successfully',
        v_current_data,
        (SELECT data FROM public.tv_entity WHERE id = input_pk_entity),
        jsonb_build_object(
            'trigger', 'api_update',
            'version_increment', v_current_version || ' → ' || (v_current_version + 1),
            'optimistic_lock_used', v_expected_version IS NOT NULL
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### 4. Document Soft Delete Pattern

**Soft delete with audit trail:**
```sql
CREATE OR REPLACE FUNCTION core.delete_entity(
    input_pk_organization UUID,
    input_deleted_by UUID,
    input_pk_entity UUID,
    input_payload JSONB DEFAULT '{}'::JSONB
) RETURNS app.mutation_result AS $$
DECLARE
    v_current_data JSONB;
    v_delete_reason TEXT;
BEGIN
    -- Get current state
    SELECT data INTO v_current_data
    FROM tenant.tb_entity
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization
    AND deleted_at IS NULL;  -- Not already deleted

    -- Check if already deleted (NOOP)
    IF v_current_data IS NULL THEN
        RETURN core.log_and_return_mutation(
            input_pk_organization,
            input_deleted_by,
            'entity',
            input_pk_entity,
            'NOOP',
            'noop:already_deleted',
            ARRAY[]::TEXT[],
            'Entity is already deleted',
            NULL,
            NULL,
            jsonb_build_object(
                'trigger', 'api_delete',
                'idempotent_safe', true
            )
        );
    END IF;

    v_delete_reason := COALESCE(
        input_payload->>'_change_reason',
        'Deleted via API'
    );

    -- Perform soft delete with audit
    UPDATE tenant.tb_entity
    SET
        -- Soft delete fields
        deleted_at = NOW(),
        deleted_by = input_deleted_by,
        -- Update audit fields
        updated_at = NOW(),
        updated_by = input_deleted_by,
        version = version + 1,
        change_reason = v_delete_reason,
        change_source = COALESCE(input_payload->>'_change_source', 'api')
    WHERE pk_entity = input_pk_entity
    AND pk_organization = input_pk_organization;

    -- Return success
    RETURN core.log_and_return_mutation(
        input_pk_organization,
        input_deleted_by,
        'entity',
        input_pk_entity,
        'DELETE',
        'deleted',
        ARRAY['deleted_at', 'deleted_by'],
        'Entity deleted successfully',
        v_current_data,
        NULL,  -- No "after" state for deletions
        jsonb_build_object(
            'trigger', 'api_delete',
            'soft_delete', true,
            'delete_reason', v_delete_reason
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### 5. Document View Pattern with Audit Fields

**Query views exposing audit information:**
```sql
-- Standard view with audit fields
CREATE OR REPLACE VIEW public.v_entity AS
SELECT
    e.pk_entity AS id,
    e.pk_organization AS tenant_id,

    -- Entity data
    e.data->>'name' AS name,
    e.data->>'description' AS description,

    -- Audit fields exposed
    e.created_at,
    cu.data->>'name' AS created_by_name,
    e.updated_at,
    uu.data->>'name' AS updated_by_name,
    e.deleted_at,
    du.data->>'name' AS deleted_by_name,
    e.version,
    e.change_reason,
    e.change_source,

    -- Computed fields
    e.deleted_at IS NOT NULL AS is_deleted,
    EXTRACT(EPOCH FROM (NOW() - e.updated_at)) / 86400 AS days_since_update,

    -- Complete audit trail in JSONB for APIs
    jsonb_build_object(
        'created', jsonb_build_object(
            'at', e.created_at,
            'by', e.created_by,
            'by_name', cu.data->>'name'
        ),
        'updated', CASE WHEN e.updated_at > e.created_at THEN
            jsonb_build_object(
                'at', e.updated_at,
                'by', e.updated_by,
                'by_name', uu.data->>'name',
                'reason', e.change_reason,
                'source', e.change_source
            ) END,
        'deleted', CASE WHEN e.deleted_at IS NOT NULL THEN
            jsonb_build_object(
                'at', e.deleted_at,
                'by', e.deleted_by,
                'by_name', du.data->>'name'
            ) END,
        'version', e.version
    ) AS audit_trail

FROM tenant.tb_entity e
LEFT JOIN tenant.tb_user cu ON cu.pk_user = e.created_by
LEFT JOIN tenant.tb_user uu ON uu.pk_user = e.updated_by
LEFT JOIN tenant.tb_user du ON du.pk_user = e.deleted_by
WHERE e.deleted_at IS NULL;  -- Exclude soft-deleted by default

-- Separate view including soft-deleted records
CREATE OR REPLACE VIEW public.v_entity_with_deleted AS
SELECT * FROM public.v_entity
UNION ALL
SELECT * FROM public.v_entity WHERE deleted_at IS NOT NULL;
```

### 6. Document Change Tracking Utilities

**Helper functions for audit fields:**
```sql
-- Calculate which fields changed between two JSONB objects
CREATE OR REPLACE FUNCTION core.calculate_changed_fields(
    p_before JSONB,
    p_after JSONB
) RETURNS TEXT[] AS $$
DECLARE
    v_changed_fields TEXT[] := ARRAY[]::TEXT[];
    v_key TEXT;
BEGIN
    -- Check each key in the new data
    FOR v_key IN SELECT jsonb_object_keys(p_after)
    LOOP
        -- Skip private fields (starting with _)
        CONTINUE WHEN v_key LIKE '\_%';

        -- Check if value changed
        IF p_after->v_key IS DISTINCT FROM COALESCE(p_before->v_key, 'null'::jsonb) THEN
            v_changed_fields := array_append(v_changed_fields, v_key);
        END IF;
    END LOOP;

    RETURN v_changed_fields;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Generate audit summary for mutation result metadata
CREATE OR REPLACE FUNCTION core.build_audit_metadata(
    p_entity_type TEXT,
    p_operation_type TEXT,
    p_version_before INTEGER,
    p_version_after INTEGER,
    p_changed_fields TEXT[],
    p_change_reason TEXT DEFAULT NULL,
    p_change_source TEXT DEFAULT 'api'
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        'audit', jsonb_build_object(
            'entity_type', p_entity_type,
            'operation', p_operation_type,
            'version_change', CASE
                WHEN p_version_before IS NOT NULL AND p_version_after IS NOT NULL
                THEN p_version_before || ' → ' || p_version_after
                WHEN p_version_after IS NOT NULL
                THEN 'initial → ' || p_version_after
                ELSE NULL
            END,
            'fields_changed', p_changed_fields,
            'change_reason', p_change_reason,
            'change_source', p_change_source,
            'timestamp', NOW()
        )
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;
```

### 7. Document GraphQL Integration

**GraphQL types with audit fields:**
```python
from datetime import datetime
from typing import Optional

@fraiseql.type
class AuditTrail:
    """Audit trail information for an entity."""
    created_at: datetime
    created_by: Optional[str] = None
    created_by_name: Optional[str] = None
    updated_at: Optional[datetime] = None
    updated_by: Optional[str] = None
    updated_by_name: Optional[str] = None
    deleted_at: Optional[datetime] = None
    deleted_by: Optional[str] = None
    deleted_by_name: Optional[str] = None
    version: int
    change_reason: Optional[str] = None
    change_source: Optional[str] = None

@fraiseql.type
class Entity:
    """Entity with full audit trail."""
    id: UUID
    name: str
    description: Optional[str] = None

    # Audit information
    audit_trail: AuditTrail
    is_deleted: bool = False
    days_since_update: Optional[float] = None

@fraiseql.input
class EntityUpdateInput:
    """Update input with audit metadata."""
    name: Optional[str] = None
    description: Optional[str] = None

    # Audit metadata (private fields)
    _expected_version: Optional[int] = None
    _change_reason: Optional[str] = None
    _change_source: str = "api"
```

### 8. Document Compliance Patterns

**GDPR/Data Protection compliance:**
```sql
-- Track data access for compliance
CREATE TABLE audit.tb_data_access_log (
    id BIGSERIAL PRIMARY KEY,
    accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accessed_by UUID NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    access_type TEXT NOT NULL,  -- read, create, update, delete
    ip_address INET,
    user_agent TEXT,
    request_id UUID  -- For correlating with application logs
);

-- Function to log data access
CREATE OR REPLACE FUNCTION audit.log_data_access(
    p_entity_type TEXT,
    p_entity_id UUID,
    p_access_type TEXT,
    p_accessed_by UUID,
    p_ip_address INET DEFAULT NULL,
    p_user_agent TEXT DEFAULT NULL,
    p_request_id UUID DEFAULT NULL
) RETURNS void AS $$
BEGIN
    INSERT INTO audit.tb_data_access_log (
        accessed_by, entity_type, entity_id, access_type,
        ip_address, user_agent, request_id
    ) VALUES (
        p_accessed_by, p_entity_type, p_entity_id, p_access_type,
        p_ip_address, p_user_agent, p_request_id
    );
END;
$$ LANGUAGE plpgsql;
```

**Data retention policies:**
```sql
-- Automated cleanup of old audit data
CREATE OR REPLACE FUNCTION audit.cleanup_old_audit_data()
RETURNS void AS $$
BEGIN
    -- Delete access logs older than 7 years (GDPR requirement)
    DELETE FROM audit.tb_data_access_log
    WHERE accessed_at < NOW() - INTERVAL '7 years';

    -- Archive old change logs (keep forever but compress)
    INSERT INTO audit.tb_entity_change_log_archive
    SELECT * FROM core.tb_entity_change_log
    WHERE created_at < NOW() - INTERVAL '2 years';

    DELETE FROM core.tb_entity_change_log
    WHERE created_at < NOW() - INTERVAL '2 years';
END;
$$ LANGUAGE plpgsql;
```

### 9. Documentation Structure

Create comprehensive sections:
1. **Overview** - Why audit fields matter
2. **Standard Fields** - Required audit fields for all entities
3. **Function Patterns** - How to implement in mutations
4. **Soft Delete Pattern** - Audit-aware deletion
5. **View Patterns** - Exposing audit info in queries
6. **Change Tracking** - Field-level change detection
7. **GraphQL Integration** - Types and inputs
8. **Optimistic Locking** - Version-based concurrency control
9. **Compliance Patterns** - GDPR, SOX, HIPAA considerations
10. **Performance Considerations** - Indexing and cleanup
11. **Best Practices** - Do's and don'ts
12. **Troubleshooting** - Common audit issues

## Success Criteria

After implementation:
- [ ] Complete audit field documentation created
- [ ] Standard patterns documented for all CRUD operations
- [ ] Compliance guidance provided
- [ ] GraphQL integration patterns shown
- [ ] Performance optimization covered
- [ ] Migration guidance included
- [ ] Follows FraiseQL documentation style

## File Location

Create: `docs/advanced/audit-field-patterns.md`

Update: `docs/advanced/index.md` to include link

## Implementation Methodology

### Development Workflow

**Critical: Structured Audit Documentation Approach**

Break this comprehensive audit pattern into focused commits:

1. **Foundation Structure Commit** (15-20 minutes)
   ```bash
   # Create document structure with core audit concepts
   git add docs/advanced/audit-field-patterns.md
   git commit -m "docs: initialize audit field patterns guide

   - Add document structure and overview
   - Document standard audit field definitions
   - Define audit field semantics table
   - List compliance requirements
   - References #[issue-number]"
   ```

2. **CRUD Function Patterns Commit** (35-45 minutes)
   ```bash
   # Complete function implementation patterns
   git add docs/advanced/audit-field-patterns.md
   git commit -m "docs: add audit-aware CRUD function patterns

   - Document create operations with audit trails
   - Show update patterns with version control
   - Include soft delete with audit tracking
   - Add optimistic locking examples"
   ```

3. **View and Query Patterns Commit** (25-35 minutes)
   ```bash
   # Complete view patterns and change tracking
   git add docs/advanced/audit-field-patterns.md
   git commit -m "docs: add audit field view and tracking patterns

   - Document audit-enabled view patterns
   - Include change tracking utility functions
   - Show audit trail exposure in GraphQL
   - Add field change calculation examples"
   ```

4. **GraphQL Integration Commit** (20-30 minutes)
   ```bash
   # Complete GraphQL type definitions and resolvers
   git add docs/advanced/audit-field-patterns.md
   git commit -m "docs: add GraphQL audit field integration

   - Define AuditTrail GraphQL types
   - Document audit-aware input types
   - Show resolver patterns for audit data
   - Include private audit metadata handling"
   ```

5. **Compliance and Performance Commit** (30-40 minutes)
   ```bash
   # Complete compliance patterns and optimization
   git add docs/advanced/audit-field-patterns.md
   git commit -m "docs: add audit compliance and performance patterns

   - Document GDPR/SOX compliance patterns
   - Include data access logging functions
   - Add data retention and cleanup strategies
   - Show performance optimization techniques"
   ```

6. **Integration and Polish Commit** (15-20 minutes)
   ```bash
   # Finalize with best practices and cross-references
   git add docs/advanced/audit-field-patterns.md docs/advanced/index.md
   git commit -m "docs: complete audit field patterns guide

   - Add troubleshooting and best practices
   - Include migration guidance for existing systems
   - Update advanced index with audit patterns
   - Add cross-references to mutation patterns
   - Ready for review"
   ```

### Quality Validation

After each commit:
- [ ] Build documentation (`mkdocs serve`)
- [ ] Validate all SQL syntax in examples
- [ ] Test GraphQL type definitions
- [ ] Check compliance pattern accuracy
- [ ] Verify cross-references work
- [ ] Ensure audit examples follow PrintOptim patterns

### Risk Management

**For compliance content:**
```bash
# Research compliance requirements carefully
# Validate GDPR/SOX examples with legal guidance
# Include disclaimers about legal compliance
```

**For complex SQL examples:**
```bash
# Test audit function examples in separate database
# Verify performance implications of audit queries
# Include index recommendations
```

**Recovery strategy:**
```bash
# For large changes, use incremental commits
git add -p  # Stage changes selectively
git commit -m "docs: partial audit pattern implementation"
```

## Dependencies

Should reference:
- `../mutations/mutation-result-pattern.md` - Audit info in mutations
- `../mutations/postgresql-function-based.md` - Function implementation
- `multi-tenancy.md` - Tenant-scoped audit trails
- `security.md` - Compliance and data protection

## Estimated Effort

**Large effort** - Comprehensive enterprise pattern:
- Complete audit pattern documentation
- Multiple SQL and GraphQL examples
- Compliance and legal considerations
- Performance and migration guidance

Target: 900-1200 lines of documentation
