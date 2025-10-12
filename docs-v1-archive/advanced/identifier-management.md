# Identifier Management Patterns

FraiseQL implements a sophisticated **Triple ID Pattern** that balances database performance, API usability, and enterprise requirements. This guide covers comprehensive identifier management strategies for PostgreSQL-backed GraphQL APIs.

## Overview

Modern enterprise applications require multiple identifier types to serve different purposes:

- **Performance**: Efficient joins and indexing with sequential IDs
- **Security**: Non-guessable external references with UUIDs
- **Usability**: Human-readable business identifiers for users

FraiseQL's identifier management system addresses all these needs through a standardized pattern that scales across complex multi-tenant applications.

## The Triple ID Pattern

Every entity in FraiseQL uses three distinct identifier types, each optimized for specific use cases:

### ID Type Breakdown

| ID Type | Purpose | Visibility | Example | Performance |
|---------|---------|------------|---------|-------------|
| `id` (SERIAL) | Internal joins, performance | Never exposed | 12345 | Fastest joins |
| `pk_*` (UUID) | GraphQL ID, external refs | Always as `id` | `123e4567-...` | Secure, cacheable |
| `identifier` (TEXT) | Human-readable business ID | User-facing | `CONTRACT-2024-001` | Searchable |

### Database Schema Pattern

```sql
-- Standard table structure following triple ID pattern
CREATE TABLE tenant.tb_contract (
    -- 1. Internal sequence ID (for JOINs and performance)
    id SERIAL NOT NULL,

    -- 2. Primary Key UUID (exposed to GraphQL as 'id')
    pk_contract UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 3. Business Identifier (stored in data column)
    data JSONB NOT NULL,  -- Contains: {"identifier": "CONTRACT-2024-001"}

    -- Multi-tenant context
    fk_customer_org UUID NOT NULL,

    -- Standard audit fields
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL,
    updated_at TIMESTAMPTZ,
    updated_by UUID,
    version INTEGER NOT NULL DEFAULT 1,
    deleted_at TIMESTAMPTZ
);
```

### ID Transformation Rules

**Command Side (Internal)**:
```sql
-- Uses pk_* primary keys for all operations
SELECT * FROM tenant.tb_contract WHERE pk_contract = $1;
```

**Query Side (GraphQL)**:
```sql
-- Exposes pk_* as 'id' field
CREATE VIEW v_contract AS
SELECT
    pk_contract::TEXT AS id,  -- UUID → id transformation
    data->>'identifier' AS identifier,
    data->>'name' AS name
FROM tenant.tb_contract;
```

**Business Logic**:
```python
# GraphQL resolvers work with transformed IDs
@fraiseql.query
async def contract(info: GraphQLResolveInfo, id: UUID) -> Contract:
    # 'id' parameter is actually pk_contract UUID
    db = info.context["db"]
    result = await db.find_one("v_contract", id=id)
    return Contract.from_dict(result)
```

## ID Exposure Patterns

### Never Expose Internal IDs

```sql
-- ❌ WRONG: Exposing internal SERIAL id
SELECT id, name FROM tenant.tb_contract;

-- ✅ CORRECT: Expose UUID as 'id', business identifier separately
SELECT
    pk_contract::TEXT AS id,
    data->>'identifier' AS identifier,
    data->>'name' AS name
FROM tenant.tb_contract;
```

### GraphQL Type Definitions

```python
@fraiseql.type
class Contract:
    """Contract with proper identifier exposure."""
    # Primary ID (pk_contract UUID exposed as 'id')
    id: UUID

    # Business identifier (human-readable)
    identifier: str

    # Contract data
    name: str
    status: str

    # Metadata (optional for debugging)
    identifier_format_version: Optional[str] = None
```

### Admin vs Public Views

```sql
-- Public view: Clean identifier exposure
CREATE VIEW public.v_contract AS
SELECT
    pk_contract::TEXT AS id,
    data->>'identifier' AS identifier,
    data->>'name' AS name,
    data->>'status' AS status
FROM tenant.tb_contract
WHERE deleted_at IS NULL;

-- Admin view: Full identifier debugging
CREATE VIEW admin.v_contract_debug AS
SELECT
    id AS internal_serial_id,      -- For debugging only
    pk_contract AS uuid_primary_key,
    data->>'identifier' AS business_identifier,
    data->>'previous_identifier' AS previous_identifier,
    jsonb_build_object(
        'format_version', data->>'identifier_format_version',
        'recalculated_at', data->>'identifier_recalculated_at',
        'generation_method', data->>'identifier_generation_method'
    ) AS identifier_metadata
FROM tenant.tb_contract;
```

## Identifier Uniqueness Constraints

### Organization-Scoped Business Identifiers

```sql
-- Ensure business identifiers are unique within organization
CREATE UNIQUE INDEX uq_contract_identifier_per_org
ON tenant.tb_contract (fk_customer_org, (data->>'identifier'))
WHERE data->>'identifier' IS NOT NULL
AND deleted_at IS NULL;

-- UUID primary keys are globally unique (automatic)
-- No additional constraint needed for pk_contract
```

### Validation Functions

```sql
CREATE OR REPLACE FUNCTION core.validate_identifier_uniqueness(
    input_pk_organization UUID,
    input_entity_type TEXT,
    input_identifier TEXT,
    input_exclude_pk UUID DEFAULT NULL
) RETURNS BOOLEAN AS $$
DECLARE
    v_exists BOOLEAN;
    v_table_name TEXT;
    v_pk_column TEXT;
BEGIN
    -- Determine table and primary key column
    CASE input_entity_type
        WHEN 'contract' THEN
            v_table_name := 'tenant.tb_contract';
            v_pk_column := 'pk_contract';
        WHEN 'user' THEN
            v_table_name := 'tenant.tb_user';
            v_pk_column := 'pk_user';
        ELSE
            RAISE EXCEPTION 'Unsupported entity type: %', input_entity_type;
    END CASE;

    -- Dynamic query for existence check
    EXECUTE format('
        SELECT EXISTS(
            SELECT 1 FROM %s
            WHERE fk_customer_org = $1
            AND data->>''identifier'' = $2
            AND deleted_at IS NULL
            AND ($3 IS NULL OR %s != $3)
        )', v_table_name, v_pk_column)
    INTO v_exists
    USING input_pk_organization, input_identifier, input_exclude_pk;

    RETURN NOT v_exists;  -- Return true if identifier is available
END;
$$ LANGUAGE plpgsql STABLE;
```

## Performance Optimization

### Indexing Strategy

```sql
-- 1. Primary key index (automatic with UUID primary key)
-- pk_contract already has unique B-tree index

-- 2. Business identifier lookup (organization + identifier)
CREATE INDEX idx_contract_business_id ON tenant.tb_contract
(fk_customer_org, (data->>'identifier'))
WHERE data->>'identifier' IS NOT NULL;

-- 3. Full-text search on identifiers
CREATE INDEX idx_contract_identifier_search ON tenant.tb_contract
USING gin ((data->>'identifier') gin_trgm_ops);

-- 4. Internal ID for joins (automatic with SERIAL)
-- SERIAL 'id' column has automatic index

-- 5. Multi-column for identifier generation
CREATE INDEX idx_contract_sequence_tracking ON tenant.tb_contract
(fk_customer_org, (data->>'identifier_year'), (data->>'contract_type'));
```

### Query Optimization Examples

```sql
-- Optimized UUID lookup (uses primary key)
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM tenant.tb_contract
WHERE pk_contract = '123e4567-e89b-12d3-a456-426614174000';
-- Result: Index Scan on tb_contract_pkey (cost=0.29..8.30)

-- Optimized business identifier lookup
EXPLAIN (ANALYZE, BUFFERS)
SELECT pk_contract FROM tenant.tb_contract
WHERE fk_customer_org = '...'
AND data->>'identifier' = 'CONTRACT-2024-001';
-- Result: Index Scan on idx_contract_business_id (cost=0.42..8.44)

-- Inefficient scan (avoid this)
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM tenant.tb_contract
WHERE data->>'identifier' LIKE 'CONTRACT%';
-- Result: Seq Scan (cost=0.00..15000.00) - BAD!
```

## Identifier Generation Strategies

FraiseQL supports multiple business identifier generation patterns, from simple sequences to complex hierarchical formats.

### Sequential Generation Pattern

```sql
CREATE OR REPLACE FUNCTION core.generate_contract_identifier(
    input_pk_organization UUID,
    input_contract_type TEXT DEFAULT 'general'
) RETURNS TEXT AS $$
DECLARE
    v_org_code TEXT;
    v_year TEXT;
    v_sequence INTEGER;
    v_type_prefix TEXT;
    v_identifier TEXT;
BEGIN
    -- Get organization code
    SELECT data->>'code' INTO v_org_code
    FROM tenant.tb_organization
    WHERE pk_organization = input_pk_organization;

    v_org_code := COALESCE(v_org_code, 'ORG');
    v_year := EXTRACT(YEAR FROM NOW())::TEXT;

    -- Type-specific prefix
    v_type_prefix := CASE input_contract_type
        WHEN 'service' THEN 'SVC'
        WHEN 'product' THEN 'PRD'
        WHEN 'lease' THEN 'LSE'
        WHEN 'maintenance' THEN 'MNT'
        ELSE 'CON'
    END;

    -- Get next sequence number (atomic operation)
    WITH sequence_calc AS (
        SELECT COALESCE(
            MAX((data->>'sequence_number')::INTEGER), 0
        ) + 1 as next_seq
        FROM tenant.tb_contract
        WHERE fk_customer_org = input_pk_organization
        AND data->>'identifier_year' = v_year
        AND data->>'contract_type' = input_contract_type
        AND deleted_at IS NULL
    )
    SELECT next_seq INTO v_sequence FROM sequence_calc;

    -- Build identifier: ORG-SVC-2024-001
    v_identifier := format('%s-%s-%s-%s',
        v_org_code,
        v_type_prefix,
        v_year,
        lpad(v_sequence::TEXT, 3, '0')
    );

    RETURN v_identifier;
END;
$$ LANGUAGE plpgsql;
```

### Hierarchical Generation Pattern

```sql
CREATE OR REPLACE FUNCTION core.generate_hierarchical_identifier(
    input_pk_organization UUID,
    input_parent_identifier TEXT DEFAULT NULL,
    input_entity_type TEXT DEFAULT 'item'
) RETURNS TEXT AS $$
DECLARE
    v_parent_parts TEXT[];
    v_base_identifier TEXT;
    v_child_sequence INTEGER;
    v_identifier TEXT;
BEGIN
    IF input_parent_identifier IS NOT NULL THEN
        -- Parse parent identifier
        v_parent_parts := string_to_array(input_parent_identifier, '-');
        v_base_identifier := input_parent_identifier;

        -- Get next child sequence
        WITH child_calc AS (
            SELECT COALESCE(
                MAX(
                    (regexp_match(data->>'identifier', v_base_identifier || '-(\d+)'))[1]::INTEGER
                ), 0
            ) + 1 as next_child
            FROM tenant.tb_contract_item
            WHERE fk_customer_org = input_pk_organization
            AND data->>'parent_identifier' = input_parent_identifier
            AND deleted_at IS NULL
        )
        SELECT next_child INTO v_child_sequence FROM child_calc;

        -- Build hierarchical identifier: CONTRACT-2024-001-01
        v_identifier := format('%s-%s',
            v_base_identifier,
            lpad(v_child_sequence::TEXT, 2, '0')
        );
    ELSE
        -- Generate root identifier
        v_identifier := core.generate_contract_identifier(
            input_pk_organization,
            input_entity_type
        );
    END IF;

    RETURN v_identifier;
END;
$$ LANGUAGE plpgsql;
```

### Custom Format Generation

```sql
CREATE OR REPLACE FUNCTION core.generate_custom_format_identifier(
    input_pk_organization UUID,
    input_format_template TEXT,
    input_context JSONB DEFAULT '{}'::JSONB
) RETURNS TEXT AS $$
DECLARE
    v_identifier TEXT;
    v_org_data JSONB;
    v_current_date DATE := NOW()::DATE;
    v_sequence INTEGER;
    v_format_variables JSONB;
BEGIN
    -- Get organization context
    SELECT data INTO v_org_data
    FROM tenant.tb_organization
    WHERE pk_organization = input_pk_organization;

    -- Build format variables
    v_format_variables := jsonb_build_object(
        'org_code', COALESCE(v_org_data->>'code', 'ORG'),
        'org_short', COALESCE(v_org_data->>'short_name', 'O'),
        'year', EXTRACT(YEAR FROM v_current_date),
        'month', lpad(EXTRACT(MONTH FROM v_current_date)::TEXT, 2, '0'),
        'day', lpad(EXTRACT(DAY FROM v_current_date)::TEXT, 2, '0'),
        'quarter', 'Q' || EXTRACT(QUARTER FROM v_current_date)
    ) || input_context;

    -- Get sequence for this format
    WITH sequence_calc AS (
        SELECT COALESCE(
            MAX((data->>'format_sequence')::INTEGER), 0
        ) + 1 as next_seq
        FROM tenant.tb_contract
        WHERE fk_customer_org = input_pk_organization
        AND data->>'format_template' = input_format_template
        AND data->>'format_date' = v_current_date::TEXT
        AND deleted_at IS NULL
    )
    SELECT next_seq INTO v_sequence FROM sequence_calc;

    -- Add sequence to variables
    v_format_variables := v_format_variables || jsonb_build_object(
        'seq', lpad(v_sequence::TEXT, 3, '0'),
        'seq_2', lpad(v_sequence::TEXT, 2, '0'),
        'seq_4', lpad(v_sequence::TEXT, 4, '0')
    );

    -- Process template: {org_code}-{year}-{seq}
    v_identifier := input_format_template;

    -- Replace all variables
    FOR key, value IN (SELECT * FROM jsonb_each_text(v_format_variables))
    LOOP
        v_identifier := replace(v_identifier, '{' || key || '}', value::TEXT);
    END LOOP;

    -- Validate no unreplaced variables remain
    IF v_identifier ~ '\{[^}]+\}' THEN
        RAISE EXCEPTION 'Unreplaced template variables in: %', v_identifier;
    END IF;

    RETURN v_identifier;
END;
$$ LANGUAGE plpgsql;
```

### Generation Strategy Examples

```sql
-- Example usage of different generation strategies

-- 1. Simple sequential
SELECT core.generate_contract_identifier(
    '12345678-1234-1234-1234-123456789012'::UUID,
    'service'
);
-- Returns: ORG-SVC-2024-001

-- 2. Hierarchical parent-child
SELECT core.generate_hierarchical_identifier(
    '12345678-1234-1234-1234-123456789012'::UUID,
    'CONTRACT-2024-001',
    'item'
);
-- Returns: CONTRACT-2024-001-01

-- 3. Custom format template
SELECT core.generate_custom_format_identifier(
    '12345678-1234-1234-1234-123456789012'::UUID,
    '{org_code}-{quarter}-{year}-{seq}',
    '{"project": "ALPHA"}'::JSONB
);
-- Returns: ORG-Q1-2024-001
```

### Collision Detection and Retry

```sql
CREATE OR REPLACE FUNCTION core.generate_safe_identifier(
    input_pk_organization UUID,
    input_generation_function TEXT,
    input_parameters JSONB DEFAULT '{}'::JSONB,
    max_retries INTEGER DEFAULT 5
) RETURNS TEXT AS $$
DECLARE
    v_identifier TEXT;
    v_retry_count INTEGER := 0;
    v_base_identifier TEXT;
    v_is_unique BOOLEAN;
BEGIN
    LOOP
        -- Generate identifier
        CASE input_generation_function
            WHEN 'sequential' THEN
                v_identifier := core.generate_contract_identifier(
                    input_pk_organization,
                    COALESCE(input_parameters->>'contract_type', 'general')
                );
            WHEN 'hierarchical' THEN
                v_identifier := core.generate_hierarchical_identifier(
                    input_pk_organization,
                    input_parameters->>'parent_identifier',
                    COALESCE(input_parameters->>'entity_type', 'item')
                );
            WHEN 'custom' THEN
                v_identifier := core.generate_custom_format_identifier(
                    input_pk_organization,
                    input_parameters->>'format_template',
                    COALESCE(input_parameters->'context', '{}'::JSONB)
                );
            ELSE
                RAISE EXCEPTION 'Unknown generation function: %', input_generation_function;
        END CASE;

        -- Check uniqueness
        SELECT core.validate_identifier_uniqueness(
            input_pk_organization,
            COALESCE(input_parameters->>'entity_type', 'contract'),
            v_identifier
        ) INTO v_is_unique;

        -- If unique, return
        IF v_is_unique THEN
            RETURN v_identifier;
        END IF;

        -- Handle collision
        v_retry_count := v_retry_count + 1;

        IF v_retry_count >= max_retries THEN
            RAISE EXCEPTION 'Failed to generate unique identifier after % retries. Last attempt: %',
                max_retries, v_identifier;
        END IF;

        -- Add retry suffix for next attempt
        input_parameters := input_parameters || jsonb_build_object(
            'retry_suffix', '-R' || v_retry_count::TEXT
        );

    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

## Identifier Recalculation Patterns

Enterprise systems often need to recalculate identifiers when business rules change, organizations merge, or data migrations occur. FraiseQL provides systematic recalculation patterns.

### Recalculation Context System

```sql
-- Context type for tracking recalculation operations
CREATE TYPE core.recalculation_context AS (
    trigger_source TEXT,        -- 'api', 'import', 'migration', 'system'
    trigger_reason TEXT,        -- 'creation', 'update', 'org_settings_changed'
    batch_operation_id UUID,    -- For tracking bulk operations
    affected_entity_count INTEGER,
    dry_run BOOLEAN            -- Preview changes without applying
);

-- Recalculation result type
CREATE TYPE core.recalculation_result AS (
    pk_entity UUID,
    entity_type TEXT,
    old_identifier TEXT,
    new_identifier TEXT,
    recalc_needed BOOLEAN,
    recalc_reason TEXT,
    recalc_timestamp TIMESTAMPTZ
);
```

### Contract Identifier Recalculation

```sql
CREATE OR REPLACE FUNCTION core.recalcid_contract(
    context core.recalculation_context
) RETURNS SETOF core.recalculation_result AS $$
DECLARE
    v_contract RECORD;
    v_new_identifier TEXT;
    v_org_settings JSONB;
    v_result core.recalculation_result;
    v_recalc_reason TEXT;
BEGIN
    -- Log recalculation start
    INSERT INTO admin.tb_identifier_recalculation_log (
        operation_id, entity_type, trigger_source, trigger_reason,
        started_at, dry_run
    ) VALUES (
        context.batch_operation_id, 'contract', context.trigger_source,
        context.trigger_reason, NOW(), context.dry_run
    );

    -- Process each contract that might need recalculation
    FOR v_contract IN
        SELECT
            c.pk_contract,
            c.fk_customer_org,
            c.data->>'identifier' as current_identifier,
            c.data->>'contract_type' as contract_type,
            c.data->>'identifier_format_version' as format_version,
            c.created_at,
            c.data,
            c.version
        FROM tenant.tb_contract c
        WHERE
            -- Filter based on recalculation trigger
            CASE context.trigger_reason
                WHEN 'org_settings_changed' THEN
                    -- Only contracts with outdated format version
                    c.data->>'identifier_format_version' != '2024.1'
                WHEN 'migration' THEN
                    -- All contracts during migration
                    TRUE
                WHEN 'format_update' THEN
                    -- Contracts with specific old format
                    c.data->>'identifier_format_version' < '2024.1'
                ELSE
                    -- Creation/update - usually no recalculation needed
                    FALSE
            END
        ORDER BY c.created_at  -- Maintain chronological sequence
    LOOP
        -- Determine recalculation reason
        v_recalc_reason := CASE
            WHEN v_contract.format_version IS NULL THEN 'missing_format_version'
            WHEN v_contract.format_version < '2024.1' THEN 'outdated_format'
            WHEN context.trigger_reason = 'org_settings_changed' THEN 'org_settings_updated'
            ELSE context.trigger_reason
        END;

        -- Generate new identifier using current rules
        v_new_identifier := core.generate_safe_identifier(
            v_contract.fk_customer_org,
            'sequential',
            jsonb_build_object(
                'contract_type', v_contract.contract_type,
                'entity_type', 'contract',
                'preserve_sequence', true  -- Try to maintain sequence if possible
            )
        );

        -- Prepare result record
        v_result.pk_entity := v_contract.pk_contract;
        v_result.entity_type := 'contract';
        v_result.old_identifier := v_contract.current_identifier;
        v_result.new_identifier := v_new_identifier;
        v_result.recalc_reason := v_recalc_reason;
        v_result.recalc_timestamp := NOW();

        -- Check if recalculation is needed
        IF v_contract.current_identifier != v_new_identifier THEN
            v_result.recalc_needed := TRUE;

            -- Apply changes if not dry run
            IF NOT context.dry_run THEN
                UPDATE tenant.tb_contract
                SET
                    data = data || jsonb_build_object(
                        'identifier', v_new_identifier,
                        'identifier_format_version', '2024.1',
                        'previous_identifier', v_contract.current_identifier,
                        'identifier_recalculated_at', NOW(),
                        'recalculation_context', row_to_json(context)::JSONB,
                        'recalculation_reason', v_recalc_reason
                    ),
                    updated_at = NOW(),
                    updated_by = COALESCE(
                        context.batch_operation_id,
                        '00000000-0000-0000-0000-000000000000'::UUID
                    ),
                    version = version + 1
                WHERE pk_contract = v_contract.pk_contract;

                -- Refresh cache for this contract
                PERFORM app.refresh_contract_cache(v_contract.pk_contract);
            END IF;
        ELSE
            v_result.recalc_needed := FALSE;
        END IF;

        RETURN NEXT v_result;
    END LOOP;

    -- Update completion log
    UPDATE admin.tb_identifier_recalculation_log
    SET
        completed_at = NOW(),
        total_processed = (
            SELECT COUNT(*) FROM core.recalcid_contract(context)
        ),
        total_changed = (
            SELECT COUNT(*) FROM core.recalcid_contract(context)
            WHERE recalc_needed = TRUE
        )
    WHERE operation_id = context.batch_operation_id;
END;
$$ LANGUAGE plpgsql;
```

### Batch Recalculation Operations

```sql
-- Batch recalculation orchestrator
CREATE OR REPLACE FUNCTION core.recalculate_identifiers_batch(
    input_entity_types TEXT[] DEFAULT ARRAY['contract', 'user', 'order'],
    input_pk_organization UUID DEFAULT NULL,  -- NULL = all organizations
    input_trigger_reason TEXT DEFAULT 'manual_batch',
    input_dry_run BOOLEAN DEFAULT TRUE
) RETURNS TABLE(
    batch_id UUID,
    entity_type TEXT,
    total_processed INTEGER,
    total_changed INTEGER,
    processing_time_ms INTEGER
) AS $$
DECLARE
    v_batch_id UUID := gen_random_uuid();
    v_context core.recalculation_context;
    v_entity_type TEXT;
    v_start_time TIMESTAMPTZ;
    v_end_time TIMESTAMPTZ;
    v_processed INTEGER;
    v_changed INTEGER;
BEGIN
    -- Initialize batch operation
    INSERT INTO admin.tb_batch_operations (
        batch_id, operation_type, parameters, started_at, dry_run
    ) VALUES (
        v_batch_id, 'identifier_recalculation',
        jsonb_build_object(
            'entity_types', input_entity_types,
            'organization_filter', input_pk_organization,
            'trigger_reason', input_trigger_reason
        ),
        NOW(), input_dry_run
    );

    -- Setup recalculation context
    v_context.trigger_source := 'batch_operation';
    v_context.trigger_reason := input_trigger_reason;
    v_context.batch_operation_id := v_batch_id;
    v_context.dry_run := input_dry_run;

    -- Process each entity type
    FOREACH v_entity_type IN ARRAY input_entity_types
    LOOP
        v_start_time := NOW();

        -- Call appropriate recalculation function
        CASE v_entity_type
            WHEN 'contract' THEN
                SELECT
                    COUNT(*) as processed,
                    COUNT(*) FILTER (WHERE recalc_needed) as changed
                INTO v_processed, v_changed
                FROM core.recalcid_contract(v_context);

            WHEN 'user' THEN
                SELECT
                    COUNT(*) as processed,
                    COUNT(*) FILTER (WHERE recalc_needed) as changed
                INTO v_processed, v_changed
                FROM core.recalcid_user(v_context);

            WHEN 'order' THEN
                SELECT
                    COUNT(*) as processed,
                    COUNT(*) FILTER (WHERE recalc_needed) as changed
                INTO v_processed, v_changed
                FROM core.recalcid_order(v_context);

            ELSE
                RAISE WARNING 'Unknown entity type: %', v_entity_type;
                CONTINUE;
        END CASE;

        v_end_time := NOW();

        -- Return results
        batch_id := v_batch_id;
        entity_type := v_entity_type;
        total_processed := v_processed;
        total_changed := v_changed;
        processing_time_ms := EXTRACT(MILLISECONDS FROM v_end_time - v_start_time)::INTEGER;
        RETURN NEXT;
    END LOOP;

    -- Complete batch operation
    UPDATE admin.tb_batch_operations
    SET completed_at = NOW()
    WHERE batch_id = v_batch_id;
END;
$$ LANGUAGE plpgsql;
```

### Sequence Preservation During Recalculation

```sql
-- Advanced recalculation that preserves sequence integrity
CREATE OR REPLACE FUNCTION core.recalcid_with_sequence_preservation(
    input_pk_organization UUID,
    input_entity_type TEXT,
    context core.recalculation_context
) RETURNS SETOF core.recalculation_result AS $$
DECLARE
    v_entity RECORD;
    v_sequence_map JSONB := '{}'::JSONB;
    v_new_identifier TEXT;
    v_result core.recalculation_result;
BEGIN
    -- First pass: Build sequence mapping
    FOR v_entity IN
        EXECUTE format('
            SELECT
                pk_%I,
                data->>''identifier'' as current_identifier,
                data->>''contract_type'' as entity_subtype,
                data->>''identifier_year'' as identifier_year,
                created_at
            FROM tenant.tb_%I
            WHERE fk_customer_org = $1
            AND deleted_at IS NULL
            ORDER BY created_at
        ', input_entity_type, input_entity_type)
        USING input_pk_organization
    LOOP
        -- Extract sequence number from current identifier
        DECLARE
            v_current_seq INTEGER;
            v_seq_key TEXT;
        BEGIN
            -- Parse sequence from identifier (assumes format: PREFIX-TYPE-YEAR-SEQ)
            v_current_seq := (
                regexp_match(v_entity.current_identifier, '-(\d+)$')
            )[1]::INTEGER;

            v_seq_key := format('%s_%s_%s',
                input_entity_type,
                COALESCE(v_entity.entity_subtype, 'default'),
                COALESCE(v_entity.identifier_year, EXTRACT(YEAR FROM v_entity.created_at))
            );

            -- Track maximum sequence for each type/year combo
            v_sequence_map := jsonb_set(
                v_sequence_map,
                ARRAY[v_seq_key],
                GREATEST(
                    COALESCE((v_sequence_map->v_seq_key)::INTEGER, 0),
                    COALESCE(v_current_seq, 0)
                )::TEXT::JSONB
            );
        EXCEPTION WHEN OTHERS THEN
            -- Skip if identifier doesn't match expected format
            CONTINUE;
        END;
    END LOOP;

    -- Second pass: Generate new identifiers preserving sequences
    FOR v_entity IN
        EXECUTE format('
            SELECT
                pk_%I as pk_entity,
                data->>''identifier'' as current_identifier,
                data->>''contract_type'' as entity_subtype,
                data->>''identifier_format_version'' as format_version,
                created_at
            FROM tenant.tb_%I
            WHERE fk_customer_org = $1
            AND deleted_at IS NULL
            ORDER BY created_at
        ', input_entity_type, input_entity_type)
        USING input_pk_organization
    LOOP
        -- Generate new identifier using preserved sequence context
        v_new_identifier := core.generate_custom_format_identifier(
            input_pk_organization,
            '{org_code}-{type_prefix}-{year}-{seq}',
            jsonb_build_object(
                'contract_type', v_entity.entity_subtype,
                'sequence_context', v_sequence_map
            )
        );

        -- Build result
        v_result.pk_entity := v_entity.pk_entity;
        v_result.entity_type := input_entity_type;
        v_result.old_identifier := v_entity.current_identifier;
        v_result.new_identifier := v_new_identifier;
        v_result.recalc_needed := (v_entity.current_identifier != v_new_identifier);
        v_result.recalc_reason := 'sequence_preserved_recalc';
        v_result.recalc_timestamp := NOW();

        RETURN NEXT v_result;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

### Recalculation Triggers and Automation

```sql
-- Trigger function for automatic recalculation
CREATE OR REPLACE FUNCTION trigger_recalculate_dependent_identifiers()
RETURNS TRIGGER AS $$
DECLARE
    v_context core.recalculation_context;
    v_affected_count INTEGER;
BEGIN
    -- Only trigger on organization settings changes that affect identifiers
    IF TG_TABLE_NAME = 'tb_organization' AND
       (OLD.data->>'identifier_format' IS DISTINCT FROM NEW.data->>'identifier_format' OR
        OLD.data->>'code' IS DISTINCT FROM NEW.data->>'code') THEN

        -- Setup recalculation context
        v_context.trigger_source := 'trigger';
        v_context.trigger_reason := 'org_settings_changed';
        v_context.batch_operation_id := gen_random_uuid();
        v_context.dry_run := FALSE;

        -- Count affected entities
        SELECT COUNT(*) INTO v_affected_count
        FROM tenant.tb_contract
        WHERE fk_customer_org = NEW.pk_organization
        AND data->>'identifier_format_version' < '2024.1';

        v_context.affected_entity_count := v_affected_count;

        -- Trigger background recalculation if significant impact
        IF v_affected_count > 100 THEN
            -- Queue for background processing
            INSERT INTO admin.tb_background_jobs (
                job_type, parameters, priority, created_at
            ) VALUES (
                'identifier_recalculation',
                jsonb_build_object(
                    'organization_id', NEW.pk_organization,
                    'context', row_to_json(v_context)
                ),
                'medium',
                NOW()
            );
        ELSE
            -- Process immediately for small datasets
            PERFORM core.recalcid_contract(v_context);
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Install trigger
CREATE TRIGGER trigger_org_identifier_recalc
    AFTER UPDATE ON tenant.tb_organization
    FOR EACH ROW
    EXECUTE FUNCTION trigger_recalculate_dependent_identifiers();
```

## Lookup Patterns and View Design

FraiseQL supports flexible entity lookup by any identifier type, with optimized views that expose appropriate identifiers to different consumers.

### Universal Entity Lookup

```sql
-- Unified lookup function that handles any identifier type
CREATE OR REPLACE FUNCTION core.find_contract_by_any_id(
    input_pk_organization UUID,
    input_id_value TEXT  -- Can be UUID or business identifier
) RETURNS UUID AS $$
DECLARE
    v_pk_contract UUID;
BEGIN
    -- First try UUID lookup (fastest path)
    BEGIN
        IF input_id_value ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN
            SELECT pk_contract INTO v_pk_contract
            FROM tenant.tb_contract
            WHERE pk_contract = input_id_value::UUID
            AND fk_customer_org = input_pk_organization
            AND deleted_at IS NULL;

            IF v_pk_contract IS NOT NULL THEN
                RETURN v_pk_contract;
            END IF;
        END IF;
    EXCEPTION WHEN invalid_text_representation THEN
        -- Not a valid UUID, continue to business identifier lookup
        NULL;
    END;

    -- Try business identifier lookup (indexed)
    SELECT pk_contract INTO v_pk_contract
    FROM tenant.tb_contract
    WHERE fk_customer_org = input_pk_organization
    AND data->>'identifier' = input_id_value
    AND deleted_at IS NULL;

    IF v_pk_contract IS NOT NULL THEN
        RETURN v_pk_contract;
    END IF;

    -- Try previous identifier lookup (for backward compatibility)
    SELECT pk_contract INTO v_pk_contract
    FROM tenant.tb_contract
    WHERE fk_customer_org = input_pk_organization
    AND data->>'previous_identifier' = input_id_value
    AND deleted_at IS NULL;

    RETURN v_pk_contract;  -- NULL if not found
END;
$$ LANGUAGE plpgsql STABLE;
```

### Multi-Entity Lookup Function

```sql
-- Generic multi-entity lookup
CREATE OR REPLACE FUNCTION core.find_entity_by_any_id(
    input_pk_organization UUID,
    input_entity_type TEXT,
    input_id_value TEXT
) RETURNS TABLE(
    pk_entity UUID,
    entity_type TEXT,
    business_identifier TEXT,
    found_by TEXT  -- 'uuid', 'identifier', 'previous_identifier'
) AS $$
DECLARE
    v_table_name TEXT;
    v_pk_column TEXT;
    v_result_record RECORD;
BEGIN
    -- Validate and get table information
    CASE input_entity_type
        WHEN 'contract' THEN
            v_table_name := 'tenant.tb_contract';
            v_pk_column := 'pk_contract';
        WHEN 'user' THEN
            v_table_name := 'tenant.tb_user';
            v_pk_column := 'pk_user';
        WHEN 'order' THEN
            v_table_name := 'tenant.tb_order';
            v_pk_column := 'pk_order';
        ELSE
            RAISE EXCEPTION 'Unsupported entity type: %', input_entity_type;
    END CASE;

    -- Try UUID lookup first
    BEGIN
        IF input_id_value ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN
            EXECUTE format('
                SELECT %I as pk_entity, data->>''identifier'' as business_identifier
                FROM %s
                WHERE %I = $1::UUID
                AND fk_customer_org = $2
                AND deleted_at IS NULL
            ', v_pk_column, v_table_name, v_pk_column)
            INTO v_result_record
            USING input_id_value, input_pk_organization;

            IF v_result_record.pk_entity IS NOT NULL THEN
                pk_entity := v_result_record.pk_entity;
                entity_type := input_entity_type;
                business_identifier := v_result_record.business_identifier;
                found_by := 'uuid';
                RETURN NEXT;
                RETURN;
            END IF;
        END IF;
    EXCEPTION WHEN invalid_text_representation THEN
        -- Continue to business identifier lookup
        NULL;
    END;

    -- Try business identifier lookup
    EXECUTE format('
        SELECT %I as pk_entity, data->>''identifier'' as business_identifier
        FROM %s
        WHERE fk_customer_org = $1
        AND data->>''identifier'' = $2
        AND deleted_at IS NULL
    ', v_pk_column, v_table_name)
    INTO v_result_record
    USING input_pk_organization, input_id_value;

    IF v_result_record.pk_entity IS NOT NULL THEN
        pk_entity := v_result_record.pk_entity;
        entity_type := input_entity_type;
        business_identifier := v_result_record.business_identifier;
        found_by := 'identifier';
        RETURN NEXT;
        RETURN;
    END IF;

    -- Try previous identifier lookup
    EXECUTE format('
        SELECT %I as pk_entity, data->>''identifier'' as business_identifier
        FROM %s
        WHERE fk_customer_org = $1
        AND data->>''previous_identifier'' = $2
        AND deleted_at IS NULL
    ', v_pk_column, v_table_name)
    INTO v_result_record
    USING input_pk_organization, input_id_value;

    IF v_result_record.pk_entity IS NOT NULL THEN
        pk_entity := v_result_record.pk_entity;
        entity_type := input_entity_type;
        business_identifier := v_result_record.business_identifier;
        found_by := 'previous_identifier';
        RETURN NEXT;
    END IF;
END;
$$ LANGUAGE plpgsql STABLE;
```

### Standard Query Views

```sql
-- Public API view: Clean identifier exposure
CREATE OR REPLACE VIEW public.v_contract AS
SELECT
    -- Primary ID transformation: pk_contract → id
    c.pk_contract::TEXT AS id,
    c.fk_customer_org::TEXT AS tenant_id,

    -- Business identifier (user-friendly)
    c.data->>'identifier' AS identifier,

    -- Core entity data
    c.data->>'name' AS name,
    c.data->>'contract_type' AS contract_type,
    c.data->>'status' AS status,
    c.data->>'description' AS description,

    -- Financial data
    (c.data->>'total_amount')::DECIMAL AS total_amount,
    c.data->>'currency' AS currency,

    -- Date fields
    (c.data->>'start_date')::DATE AS start_date,
    (c.data->>'end_date')::DATE AS end_date,

    -- Audit fields
    c.created_at,
    c.updated_at,
    c.version,

    -- Complete data object (for APIs that need full context)
    jsonb_build_object(
        'id', c.pk_contract,
        'identifier', c.data->>'identifier',
        'name', c.data->>'name',
        'contract_type', c.data->>'contract_type',
        'status', c.data->>'status',
        'total_amount', (c.data->>'total_amount')::DECIMAL,
        'currency', c.data->>'currency',
        'start_date', c.data->>'start_date',
        'end_date', c.data->>'end_date',
        'created_at', c.created_at,
        'updated_at', c.updated_at
    ) AS data

FROM tenant.tb_contract c
WHERE c.deleted_at IS NULL;

-- Performance-optimized cache view (TurboRouter)
CREATE OR REPLACE VIEW public.tv_contract AS
SELECT
    -- Same structure as v_contract but optimized for caching
    c.pk_contract::TEXT AS id,
    c.fk_customer_org::TEXT AS tenant_id,
    c.data->>'identifier' AS identifier,
    c.data->>'name' AS name,
    c.data->>'status' AS status,

    -- Cached timestamp for TurboRouter
    NOW() AS cached_at,

    -- Essential data only (minimize cache size)
    jsonb_build_object(
        'id', c.pk_contract,
        'identifier', c.data->>'identifier',
        'name', c.data->>'name',
        'status', c.data->>'status'
    ) AS data

FROM tenant.tb_contract c
WHERE c.deleted_at IS NULL;
```

### Admin and Debug Views

```sql
-- Admin view: Full identifier debugging information
CREATE OR REPLACE VIEW admin.v_contract_identifier_debug AS
SELECT
    -- All ID types exposed
    c.id AS internal_serial_id,
    c.pk_contract AS uuid_primary_key,
    c.data->>'identifier' AS current_business_identifier,

    -- Identifier history and metadata
    c.data->>'previous_identifier' AS previous_business_identifier,
    c.data->>'identifier_format_version' AS format_version,
    (c.data->>'identifier_recalculated_at')::TIMESTAMPTZ AS recalculated_at,
    c.data->>'recalculation_reason' AS recalculation_reason,

    -- Generation metadata
    jsonb_build_object(
        'generation_method', c.data->>'identifier_generation_method',
        'generation_context', c.data->'identifier_generation_context',
        'sequence_number', c.data->>'sequence_number',
        'identifier_year', c.data->>'identifier_year'
    ) AS generation_metadata,

    -- Entity context
    c.data->>'name' AS name,
    c.fk_customer_org AS organization_id,
    c.created_at,
    c.updated_at,
    c.version

FROM tenant.tb_contract c;

-- Search-optimized view for identifier lookups
CREATE OR REPLACE VIEW public.v_contract_searchable AS
SELECT
    c.pk_contract::TEXT AS id,
    c.fk_customer_org::TEXT AS tenant_id,

    -- Searchable identifier fields
    c.data->>'identifier' AS identifier,
    c.data->>'previous_identifier' AS previous_identifier,
    c.data->>'name' AS name,

    -- Search vectors for full-text search
    to_tsvector('english',
        COALESCE(c.data->>'identifier', '') || ' ' ||
        COALESCE(c.data->>'previous_identifier', '') || ' ' ||
        COALESCE(c.data->>'name', '')
    ) AS search_vector,

    -- Trigram search optimization
    lower(c.data->>'identifier') AS identifier_lower,
    lower(c.data->>'name') AS name_lower

FROM tenant.tb_contract c
WHERE c.deleted_at IS NULL;

-- Index for full-text search
CREATE INDEX idx_contract_search_vector
ON public.v_contract_searchable
USING gin(search_vector);

-- Index for trigram similarity search
CREATE INDEX idx_contract_identifier_trgm
ON tenant.tb_contract
USING gin ((lower(data->>'identifier')) gin_trgm_ops);
```

### GraphQL Integration Patterns

```python
# GraphQL types with identifier support

@fraiseql.input
class ContractLookupInput:
    """Flexible contract lookup supporting multiple identifier types."""
    # Can be UUID (pk_contract) or business identifier
    id: Optional[str] = None
    identifier: Optional[str] = None
    # Legacy support
    legacy_id: Optional[str] = None

@fraiseql.type
class Contract:
    """Contract with proper identifier exposure."""
    # Primary ID (UUID transformed for GraphQL)
    id: UUID

    # Business identifier (human-readable)
    identifier: str

    # Contract data
    name: str
    contract_type: str
    status: str

    # Optional identifier metadata (admin users only)
    identifier_format_version: Optional[str] = None
    previous_identifier: Optional[str] = None
    identifier_recalculated_at: Optional[datetime] = None

@fraiseql.query
async def contract(
    info: GraphQLResolveInfo,
    lookup: ContractLookupInput
) -> Optional[Contract]:
    """Find contract by any identifier type."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    # Determine lookup value (priority: id > identifier > legacy_id)
    lookup_value = lookup.id or lookup.identifier or lookup.legacy_id
    if not lookup_value:
        raise ValueError("Must provide id, identifier, or legacy_id")

    # Use unified lookup function
    pk_contract = await db.call_function(
        "core.find_contract_by_any_id",
        input_pk_organization=tenant_id,
        input_id_value=lookup_value
    )

    if not pk_contract:
        return None

    # Get contract data from optimized view
    result = await db.find_one(
        "v_contract",
        where={"id": str(pk_contract), "tenant_id": tenant_id}
    )

    return Contract.from_dict(result) if result else None

@fraiseql.query
async def contracts(
    info: GraphQLResolveInfo,
    where: Optional[ContractWhereInput] = None,
    search: Optional[str] = None,
    limit: int = 100,
    offset: int = 0
) -> List[Contract]:
    """Query contracts with identifier search support."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    view_name = "tv_contract"  # Use cached view for lists

    # Handle identifier-based search
    if search:
        # Use search-optimized view for full-text search
        view_name = "v_contract_searchable"

        # Add search condition to where clause
        search_condition = {
            "OR": [
                {"identifier": {"contains": search}},
                {"name": {"contains": search}},
                {"search_vector": {"matches": search}}
            ]
        }

        if where:
            where = {"AND": [where, search_condition]}
        else:
            where = search_condition

    results = await db.find(
        view_name,
        where={"tenant_id": tenant_id, **where} if where else {"tenant_id": tenant_id},
        limit=limit,
        offset=offset,
        order_by="created_at DESC"
    )

    return [Contract.from_dict(r) for r in results]

# Advanced lookup with metadata
@fraiseql.query
async def find_entity_by_identifier(
    info: GraphQLResolveInfo,
    entity_type: str,
    identifier: str
) -> Optional[EntityLookupResult]:
    """Advanced entity lookup with metadata about how the entity was found."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    result = await db.call_function(
        "core.find_entity_by_any_id",
        input_pk_organization=tenant_id,
        input_entity_type=entity_type,
        input_id_value=identifier
    )

    if not result:
        return None

    return EntityLookupResult(
        entity_id=result["pk_entity"],
        entity_type=result["entity_type"],
        business_identifier=result["business_identifier"],
        found_by=result["found_by"]
    )

@fraiseql.type
class EntityLookupResult:
    """Result of advanced entity lookup."""
    entity_id: UUID
    entity_type: str
    business_identifier: str
    found_by: str  # 'uuid', 'identifier', 'previous_identifier'
```

## Migration Strategies

Migrating existing systems to the triple ID pattern requires careful planning to maintain data integrity and minimize downtime.

### Migration from Single ID Systems

```sql
-- Phase 1: Add UUID primary keys to existing tables
ALTER TABLE tenant.tb_contract
ADD COLUMN pk_contract UUID DEFAULT gen_random_uuid();

-- Create unique constraint
ALTER TABLE tenant.tb_contract
ADD CONSTRAINT uq_contract_pk UNIQUE (pk_contract);

-- Phase 2: Migrate business identifiers to data column
UPDATE tenant.tb_contract
SET data = COALESCE(data, '{}'::JSONB) || jsonb_build_object(
    'identifier',
    CASE
        WHEN legacy_identifier IS NOT NULL THEN legacy_identifier
        ELSE core.generate_contract_identifier(fk_customer_org, 'general')
    END,
    'identifier_format_version', '2024.1',
    'identifier_migration_date', NOW(),
    'legacy_id_preserved', legacy_identifier
)
WHERE data->>'identifier' IS NULL;

-- Phase 3: Update views to expose new ID structure
CREATE OR REPLACE VIEW public.v_contract AS
SELECT
    pk_contract::TEXT AS id,  -- New UUID exposed as id
    data->>'identifier' AS identifier,
    -- ... rest of view definition
FROM tenant.tb_contract;

-- Phase 4: Create compatibility view for legacy systems
CREATE OR REPLACE VIEW legacy.v_contract_legacy AS
SELECT
    id AS legacy_id,          -- Old SERIAL id for backward compatibility
    pk_contract::TEXT AS id,  -- New UUID
    data->>'identifier' AS identifier,
    -- ... rest of fields
FROM tenant.tb_contract;
```

### Migration from External ID Systems

```sql
-- Migration function for external systems
CREATE OR REPLACE FUNCTION migration.migrate_external_identifiers(
    input_mapping_table TEXT,  -- Table with external_id -> internal_id mapping
    input_entity_type TEXT
) RETURNS TABLE(
    external_id TEXT,
    new_pk_entity UUID,
    new_identifier TEXT,
    migration_status TEXT
) AS $$
DECLARE
    v_mapping_record RECORD;
    v_new_pk UUID;
    v_new_identifier TEXT;
    v_table_name TEXT;
    v_pk_column TEXT;
BEGIN
    -- Validate entity type and get table info
    CASE input_entity_type
        WHEN 'contract' THEN
            v_table_name := 'tenant.tb_contract';
            v_pk_column := 'pk_contract';
        ELSE
            RAISE EXCEPTION 'Unsupported entity type: %', input_entity_type;
    END CASE;

    -- Process each mapping
    FOR v_mapping_record IN
        EXECUTE format('SELECT external_id, internal_id FROM %s', input_mapping_table)
    LOOP
        BEGIN
            -- Generate new UUID and business identifier
            v_new_pk := gen_random_uuid();

            -- Get organization from existing record
            DECLARE
                v_org_id UUID;
            BEGIN
                EXECUTE format('SELECT fk_customer_org FROM %s WHERE id = $1', v_table_name)
                INTO v_org_id
                USING v_mapping_record.internal_id;

                -- Generate business identifier
                v_new_identifier := core.generate_contract_identifier(
                    v_org_id,
                    'general'
                );
            END;

            -- Update the record with new identifiers
            EXECUTE format('
                UPDATE %s
                SET
                    %s = $1,
                    data = COALESCE(data, ''{}''::JSONB) || jsonb_build_object(
                        ''identifier'', $2,
                        ''external_id'', $3,
                        ''migration_date'', NOW(),
                        ''identifier_format_version'', ''2024.1''
                    )
                WHERE id = $4
            ', v_table_name, v_pk_column)
            USING v_new_pk, v_new_identifier, v_mapping_record.external_id, v_mapping_record.internal_id;

            -- Return success
            external_id := v_mapping_record.external_id;
            new_pk_entity := v_new_pk;
            new_identifier := v_new_identifier;
            migration_status := 'success';
            RETURN NEXT;

        EXCEPTION WHEN OTHERS THEN
            -- Return failure
            external_id := v_mapping_record.external_id;
            new_pk_entity := NULL;
            new_identifier := NULL;
            migration_status := 'error: ' || SQLERRM;
            RETURN NEXT;
        END;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

### Zero-Downtime Migration Strategy

```sql
-- Migration orchestrator for zero-downtime deployment
CREATE OR REPLACE FUNCTION migration.zero_downtime_id_migration(
    input_entity_type TEXT,
    input_batch_size INTEGER DEFAULT 1000,
    input_dry_run BOOLEAN DEFAULT TRUE
) RETURNS TABLE(
    batch_number INTEGER,
    records_processed INTEGER,
    processing_time_ms INTEGER,
    migration_status TEXT
) AS $$
DECLARE
    v_table_name TEXT;
    v_pk_column TEXT;
    v_batch_number INTEGER := 1;
    v_offset INTEGER := 0;
    v_records_in_batch INTEGER;
    v_start_time TIMESTAMPTZ;
    v_end_time TIMESTAMPTZ;
    v_total_records INTEGER;
BEGIN
    -- Validate entity type
    CASE input_entity_type
        WHEN 'contract' THEN
            v_table_name := 'tenant.tb_contract';
            v_pk_column := 'pk_contract';
        ELSE
            RAISE EXCEPTION 'Unsupported entity type: %', input_entity_type;
    END CASE;

    -- Get total record count
    EXECUTE format('SELECT COUNT(*) FROM %s WHERE %s IS NULL', v_table_name, v_pk_column)
    INTO v_total_records;

    RAISE NOTICE 'Starting zero-downtime migration for % records', v_total_records;

    -- Process in batches
    WHILE v_offset < v_total_records
    LOOP
        v_start_time := NOW();

        -- Process batch
        IF NOT input_dry_run THEN
            EXECUTE format('
                UPDATE %s
                SET
                    %s = gen_random_uuid(),
                    data = COALESCE(data, ''{}''::JSONB) || jsonb_build_object(
                        ''identifier'', core.generate_contract_identifier(fk_customer_org, ''general''),
                        ''identifier_format_version'', ''2024.1'',
                        ''batch_migration_date'', NOW()
                    )
                WHERE id IN (
                    SELECT id FROM %s
                    WHERE %s IS NULL
                    ORDER BY id
                    LIMIT $1 OFFSET $2
                )
            ', v_table_name, v_pk_column, v_table_name, v_pk_column)
            USING input_batch_size, v_offset;

            GET DIAGNOSTICS v_records_in_batch = ROW_COUNT;
        ELSE
            -- Dry run: just count records that would be processed
            EXECUTE format('
                SELECT COUNT(*) FROM %s
                WHERE %s IS NULL
                AND id IN (
                    SELECT id FROM %s
                    WHERE %s IS NULL
                    ORDER BY id
                    LIMIT $1 OFFSET $2
                )
            ', v_table_name, v_pk_column, v_table_name, v_pk_column)
            INTO v_records_in_batch
            USING input_batch_size, v_offset;
        END IF;

        v_end_time := NOW();

        -- Return batch results
        batch_number := v_batch_number;
        records_processed := v_records_in_batch;
        processing_time_ms := EXTRACT(MILLISECONDS FROM v_end_time - v_start_time)::INTEGER;
        migration_status := CASE
            WHEN input_dry_run THEN 'dry_run'
            WHEN v_records_in_batch > 0 THEN 'completed'
            ELSE 'no_records'
        END;
        RETURN NEXT;

        -- Exit if no more records to process
        EXIT WHEN v_records_in_batch = 0;

        -- Move to next batch
        v_offset := v_offset + input_batch_size;
        v_batch_number := v_batch_number + 1;

        -- Brief pause to reduce system load
        PERFORM pg_sleep(0.1);
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

## Advanced Performance Optimization

### Comprehensive Indexing Strategy

```sql
-- 1. Primary key performance (automatic with UUID)
-- UUID primary keys have automatic B-tree index

-- 2. Business identifier lookups (most critical)
CREATE INDEX CONCURRENTLY idx_contract_org_identifier
ON tenant.tb_contract (fk_customer_org, (data->>'identifier'))
WHERE data->>'identifier' IS NOT NULL
AND deleted_at IS NULL;

-- 3. Previous identifier backward compatibility
CREATE INDEX CONCURRENTLY idx_contract_org_previous_identifier
ON tenant.tb_contract (fk_customer_org, (data->>'previous_identifier'))
WHERE data->>'previous_identifier' IS NOT NULL
AND deleted_at IS NULL;

-- 4. Identifier generation sequence tracking
CREATE INDEX CONCURRENTLY idx_contract_sequence_generation
ON tenant.tb_contract (
    fk_customer_org,
    (data->>'contract_type'),
    (data->>'identifier_year'),
    ((data->>'sequence_number')::INTEGER)
)
WHERE data->>'identifier' IS NOT NULL;

-- 5. Full-text search optimization
CREATE INDEX CONCURRENTLY idx_contract_identifier_fulltext
ON tenant.tb_contract
USING gin (to_tsvector('english', data->>'identifier' || ' ' || data->>'name'));

-- 6. Trigram similarity search
CREATE INDEX CONCURRENTLY idx_contract_identifier_trigram
ON tenant.tb_contract
USING gin ((lower(data->>'identifier')) gin_trgm_ops);

-- 7. Format version tracking (for migrations)
CREATE INDEX CONCURRENTLY idx_contract_format_version
ON tenant.tb_contract ((data->>'identifier_format_version'))
WHERE data->>'identifier_format_version' IS NOT NULL;

-- 8. Recalculation tracking
CREATE INDEX CONCURRENTLY idx_contract_recalc_tracking
ON tenant.tb_contract ((data->>'identifier_recalculated_at'))
WHERE data->>'identifier_recalculated_at' IS NOT NULL;
```

### Query Performance Analysis

```sql
-- Performance analysis for different lookup patterns
CREATE OR REPLACE FUNCTION admin.analyze_identifier_performance(
    input_pk_organization UUID,
    input_sample_size INTEGER DEFAULT 1000
) RETURNS TABLE(
    lookup_type TEXT,
    avg_time_ms NUMERIC,
    min_time_ms NUMERIC,
    max_time_ms NUMERIC,
    total_queries INTEGER,
    cache_hit_ratio NUMERIC
) AS $$
DECLARE
    v_uuid_sample UUID[];
    v_identifier_sample TEXT[];
    v_start_time TIMESTAMPTZ;
    v_end_time TIMESTAMPTZ;
    v_times NUMERIC[];
    v_test_uuid UUID;
    v_test_identifier TEXT;
BEGIN
    -- Prepare test samples
    SELECT array_agg(pk_contract) INTO v_uuid_sample
    FROM (
        SELECT pk_contract
        FROM tenant.tb_contract
        WHERE fk_customer_org = input_pk_organization
        ORDER BY random()
        LIMIT input_sample_size
    ) s;

    SELECT array_agg(data->>'identifier') INTO v_identifier_sample
    FROM (
        SELECT data->>'identifier'
        FROM tenant.tb_contract
        WHERE fk_customer_org = input_pk_organization
        AND data->>'identifier' IS NOT NULL
        ORDER BY random()
        LIMIT input_sample_size
    ) s;

    -- Test UUID lookups
    v_times := ARRAY[]::NUMERIC[];
    FOREACH v_test_uuid IN ARRAY v_uuid_sample
    LOOP
        v_start_time := clock_timestamp();

        PERFORM pk_contract FROM tenant.tb_contract
        WHERE pk_contract = v_test_uuid
        AND fk_customer_org = input_pk_organization;

        v_end_time := clock_timestamp();
        v_times := v_times || EXTRACT(MILLISECONDS FROM v_end_time - v_start_time);
    END LOOP;

    -- Return UUID performance
    lookup_type := 'uuid_primary_key';
    avg_time_ms := (SELECT avg(t) FROM unnest(v_times) t);
    min_time_ms := (SELECT min(t) FROM unnest(v_times) t);
    max_time_ms := (SELECT max(t) FROM unnest(v_times) t);
    total_queries := array_length(v_times, 1);
    cache_hit_ratio := NULL;  -- Would need cache monitoring
    RETURN NEXT;

    -- Test business identifier lookups
    v_times := ARRAY[]::NUMERIC[];
    FOREACH v_test_identifier IN ARRAY v_identifier_sample
    LOOP
        v_start_time := clock_timestamp();

        PERFORM pk_contract FROM tenant.tb_contract
        WHERE fk_customer_org = input_pk_organization
        AND data->>'identifier' = v_test_identifier;

        v_end_time := clock_timestamp();
        v_times := v_times || EXTRACT(MILLISECONDS FROM v_end_time - v_start_time);
    END LOOP;

    -- Return identifier performance
    lookup_type := 'business_identifier';
    avg_time_ms := (SELECT avg(t) FROM unnest(v_times) t);
    min_time_ms := (SELECT min(t) FROM unnest(v_times) t);
    max_time_ms := (SELECT max(t) FROM unnest(v_times) t);
    total_queries := array_length(v_times, 1);
    cache_hit_ratio := NULL;
    RETURN NEXT;
END;
$$ LANGUAGE plpgsql;
```

### Identifier Cache Optimization

```sql
-- Identifier cache for frequent lookups
CREATE TABLE IF NOT EXISTS cache.tb_identifier_lookup_cache (
    cache_key TEXT PRIMARY KEY,
    pk_organization UUID NOT NULL,
    entity_type TEXT NOT NULL,
    identifier_value TEXT NOT NULL,
    pk_entity UUID NOT NULL,
    business_identifier TEXT NOT NULL,
    cached_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '1 hour',
    hit_count INTEGER DEFAULT 1,
    last_hit_at TIMESTAMPTZ DEFAULT NOW()
);

-- Cache lookup function
CREATE OR REPLACE FUNCTION cache.get_identifier_cache(
    input_pk_organization UUID,
    input_entity_type TEXT,
    input_identifier_value TEXT
) RETURNS UUID AS $$
DECLARE
    v_cache_key TEXT;
    v_pk_entity UUID;
BEGIN
    v_cache_key := format('%s:%s:%s', input_pk_organization, input_entity_type, input_identifier_value);

    -- Check cache
    SELECT pk_entity INTO v_pk_entity
    FROM cache.tb_identifier_lookup_cache
    WHERE cache_key = v_cache_key
    AND expires_at > NOW();

    IF v_pk_entity IS NOT NULL THEN
        -- Update hit statistics
        UPDATE cache.tb_identifier_lookup_cache
        SET
            hit_count = hit_count + 1,
            last_hit_at = NOW()
        WHERE cache_key = v_cache_key;

        RETURN v_pk_entity;
    END IF;

    RETURN NULL;  -- Cache miss
END;
$$ LANGUAGE plpgsql;

-- Cache population function
CREATE OR REPLACE FUNCTION cache.populate_identifier_cache(
    input_pk_organization UUID,
    input_entity_type TEXT,
    input_identifier_value TEXT,
    input_pk_entity UUID,
    input_business_identifier TEXT
) RETURNS VOID AS $$
DECLARE
    v_cache_key TEXT;
BEGIN
    v_cache_key := format('%s:%s:%s', input_pk_organization, input_entity_type, input_identifier_value);

    -- Upsert cache entry
    INSERT INTO cache.tb_identifier_lookup_cache (
        cache_key, pk_organization, entity_type, identifier_value,
        pk_entity, business_identifier, cached_at, expires_at
    ) VALUES (
        v_cache_key, input_pk_organization, input_entity_type, input_identifier_value,
        input_pk_entity, input_business_identifier, NOW(), NOW() + INTERVAL '1 hour'
    )
    ON CONFLICT (cache_key) DO UPDATE SET
        pk_entity = EXCLUDED.pk_entity,
        business_identifier = EXCLUDED.business_identifier,
        cached_at = NOW(),
        expires_at = NOW() + INTERVAL '1 hour',
        hit_count = tb_identifier_lookup_cache.hit_count + 1;
END;
$$ LANGUAGE plpgsql;

-- Enhanced lookup function with caching
CREATE OR REPLACE FUNCTION core.find_contract_by_any_id_cached(
    input_pk_organization UUID,
    input_id_value TEXT
) RETURNS UUID AS $$
DECLARE
    v_pk_contract UUID;
BEGIN
    -- Check cache first
    v_pk_contract := cache.get_identifier_cache(
        input_pk_organization,
        'contract',
        input_id_value
    );

    IF v_pk_contract IS NOT NULL THEN
        RETURN v_pk_contract;
    END IF;

    -- Cache miss - do database lookup
    v_pk_contract := core.find_contract_by_any_id(input_pk_organization, input_id_value);

    -- Populate cache if found
    IF v_pk_contract IS NOT NULL THEN
        DECLARE
            v_business_identifier TEXT;
        BEGIN
            SELECT data->>'identifier' INTO v_business_identifier
            FROM tenant.tb_contract
            WHERE pk_contract = v_pk_contract;

            PERFORM cache.populate_identifier_cache(
                input_pk_organization,
                'contract',
                input_id_value,
                v_pk_contract,
                v_business_identifier
            );
        END;
    END IF;

    RETURN v_pk_contract;
END;
$$ LANGUAGE plpgsql;
```

## Best Practices

### Identifier Design Principles

1. **Consistency**: Use the same identifier format across all entities of the same type
2. **Readability**: Business identifiers should be human-readable and meaningful
3. **Uniqueness**: Ensure uniqueness within appropriate scope (organization/tenant)
4. **Immutability**: Avoid changing business identifiers once assigned
5. **Versioning**: Track identifier format versions for future migrations

### Implementation Guidelines

```python
# ✅ GOOD: Consistent identifier patterns
class ContractIdentifier:
    """Standardized contract identifier management."""

    @staticmethod
    def generate(org_code: str, contract_type: str, year: int, sequence: int) -> str:
        return f"{org_code}-{contract_type.upper()}-{year}-{sequence:03d}"

    @staticmethod
    def validate(identifier: str) -> bool:
        pattern = r'^[A-Z0-9]+-[A-Z]+-\d{4}-\d{3}$'
        return bool(re.match(pattern, identifier))

# ❌ AVOID: Inconsistent identifier generation
def make_id(org, type, num):  # Unclear, inconsistent
    return f"{org}{type}{num}"  # No separators, hard to read
```

### Database Design Best Practices

```sql
-- ✅ GOOD: Proper constraint naming and indexing
CREATE UNIQUE INDEX uq_contract_business_id_per_org
ON tenant.tb_contract (fk_customer_org, (data->>'identifier'))
WHERE data->>'identifier' IS NOT NULL AND deleted_at IS NULL;

-- ✅ GOOD: Comprehensive validation
CREATE OR REPLACE FUNCTION validate_business_identifier(input_identifier TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    RETURN input_identifier ~ '^[A-Z0-9]+-[A-Z]+-\d{4}-\d{3}$'
        AND LENGTH(input_identifier) BETWEEN 10 AND 50;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- ❌ AVOID: Generic naming and missing validation
CREATE INDEX idx_contract_1 ON tenant.tb_contract ((data->>'identifier'));  -- Bad name
-- No validation or constraints
```

### GraphQL Integration Best Practices

```python
# ✅ GOOD: Clear input types and validation
@fraiseql.input
class ContractLookupInput:
    """Contract lookup with clear identifier types."""
    id: Optional[UUID] = None  # Primary UUID
    business_id: Optional[str] = None  # Business identifier

    def __post_init__(self):
        if not (self.id or self.business_id):
            raise ValueError("Either id or business_id must be provided")

# ✅ GOOD: Consistent error handling
@fraiseql.query
async def contract(info: GraphQLResolveInfo, lookup: ContractLookupInput) -> Optional[Contract]:
    try:
        pk_contract = await find_contract_by_lookup(lookup)
        if not pk_contract:
            return None  # Not found, return None (not error)
        return await get_contract(pk_contract)
    except ValueError as e:
        raise GraphQLError(f"Invalid lookup parameters: {e}")

# ❌ AVOID: Confusing parameter naming
async def get_thing(info, id_or_name):  # Unclear what type of ID
    pass
```

## Troubleshooting

### Common Issues and Solutions

#### Identifier Collision Detection

```sql
-- Problem: Identifier collisions during generation
-- Solution: Implement collision detection with retry logic

CREATE OR REPLACE FUNCTION debug_identifier_collisions(
    input_pk_organization UUID
) RETURNS TABLE(
    entity_type TEXT,
    identifier TEXT,
    collision_count INTEGER,
    first_occurrence TIMESTAMPTZ,
    last_occurrence TIMESTAMPTZ
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        'contract'::TEXT as entity_type,
        c.data->>'identifier' as identifier,
        COUNT(*)::INTEGER as collision_count,
        MIN(c.created_at) as first_occurrence,
        MAX(c.created_at) as last_occurrence
    FROM tenant.tb_contract c
    WHERE c.fk_customer_org = input_pk_organization
    AND c.data->>'identifier' IS NOT NULL
    GROUP BY c.data->>'identifier'
    HAVING COUNT(*) > 1
    ORDER BY collision_count DESC;
END;
$$ LANGUAGE plpgsql;
```

#### Performance Debugging

```sql
-- Problem: Slow identifier lookups
-- Solution: Performance analysis and optimization

CREATE OR REPLACE FUNCTION debug_identifier_lookup_performance()
RETURNS TABLE(
    index_name TEXT,
    index_size TEXT,
    index_scans BIGINT,
    tuples_read BIGINT,
    tuples_fetched BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        indexrelname::TEXT,
        pg_size_pretty(pg_relation_size(indexrelid))::TEXT,
        idx_scan,
        idx_tup_read,
        idx_tup_fetch
    FROM pg_stat_user_indexes
    WHERE schemaname = 'tenant'
    AND indexrelname LIKE '%identifier%'
    ORDER BY idx_scan DESC;
END;
$$ LANGUAGE plpgsql;
```

#### Migration Issues

```sql
-- Problem: Identifier migration failures
-- Solution: Detailed migration logging and rollback

CREATE TABLE admin.tb_identifier_migration_log (
    log_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    migration_batch_id UUID NOT NULL,
    entity_type TEXT NOT NULL,
    pk_entity UUID,
    old_identifier TEXT,
    new_identifier TEXT,
    migration_status TEXT NOT NULL, -- 'success', 'error', 'skipped'
    error_message TEXT,
    migrated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Rollback function for failed migrations
CREATE OR REPLACE FUNCTION admin.rollback_identifier_migration(
    input_batch_id UUID
) RETURNS TABLE(
    pk_entity UUID,
    rollback_status TEXT
) AS $$
DECLARE
    v_log_record RECORD;
BEGIN
    FOR v_log_record IN
        SELECT * FROM admin.tb_identifier_migration_log
        WHERE migration_batch_id = input_batch_id
        AND migration_status = 'success'
    LOOP
        BEGIN
            -- Restore original identifier
            UPDATE tenant.tb_contract
            SET data = data || jsonb_build_object(
                'identifier', v_log_record.old_identifier,
                'rollback_date', NOW(),
                'rollback_reason', 'migration_rollback'
            )
            WHERE pk_contract = v_log_record.pk_entity;

            pk_entity := v_log_record.pk_entity;
            rollback_status := 'success';
            RETURN NEXT;

        EXCEPTION WHEN OTHERS THEN
            pk_entity := v_log_record.pk_entity;
            rollback_status := 'error: ' || SQLERRM;
            RETURN NEXT;
        END;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

### Diagnostic Queries

```sql
-- Check identifier distribution
SELECT
    data->>'contract_type' as contract_type,
    data->>'identifier_format_version' as format_version,
    COUNT(*) as count,
    MIN(created_at) as earliest,
    MAX(created_at) as latest
FROM tenant.tb_contract
WHERE data->>'identifier' IS NOT NULL
GROUP BY data->>'contract_type', data->>'identifier_format_version'
ORDER BY count DESC;

-- Find orphaned or malformed identifiers
SELECT
    pk_contract,
    data->>'identifier' as identifier,
    CASE
        WHEN data->>'identifier' !~ '^[A-Z0-9]+-[A-Z]+-\d{4}-\d{3}$' THEN 'invalid_format'
        WHEN data->>'identifier' IS NULL THEN 'missing_identifier'
        ELSE 'valid'
    END as identifier_status
FROM tenant.tb_contract
WHERE data->>'identifier' IS NULL
   OR data->>'identifier' !~ '^[A-Z0-9]+-[A-Z]+-\d{4}-\d{3}$';

-- Analyze identifier generation sequence gaps
WITH identifier_sequences AS (
    SELECT
        fk_customer_org,
        data->>'contract_type' as contract_type,
        data->>'identifier_year' as identifier_year,
        (data->>'sequence_number')::INTEGER as sequence_number,
        data->>'identifier' as identifier
    FROM tenant.tb_contract
    WHERE data->>'sequence_number' IS NOT NULL
),
sequence_analysis AS (
    SELECT
        fk_customer_org,
        contract_type,
        identifier_year,
        sequence_number,
        LAG(sequence_number) OVER (
            PARTITION BY fk_customer_org, contract_type, identifier_year
            ORDER BY sequence_number
        ) as prev_sequence
    FROM identifier_sequences
)
SELECT
    fk_customer_org,
    contract_type,
    identifier_year,
    sequence_number,
    sequence_number - prev_sequence - 1 as gap_size
FROM sequence_analysis
WHERE sequence_number - prev_sequence > 1
ORDER BY gap_size DESC;
```

## Testing Patterns

### Unit Testing Identifier Functions

```python
import pytest
from uuid import uuid4

class TestIdentifierManagement:
    """Test suite for identifier management patterns."""

    @pytest.fixture
    async def test_organization_id(self):
        """Create test organization for identifier testing."""
        return str(uuid4())

    async def test_identifier_generation(self, db, test_organization_id):
        """Test business identifier generation."""
        # Test sequential generation
        identifier1 = await db.call_function(
            "core.generate_contract_identifier",
            input_pk_organization=test_organization_id,
            input_contract_type='service'
        )

        identifier2 = await db.call_function(
            "core.generate_contract_identifier",
            input_pk_organization=test_organization_id,
            input_contract_type='service'
        )

        # Verify format and sequence
        assert identifier1.endswith('-001')
        assert identifier2.endswith('-002')
        assert identifier1.startswith(identifier2.split('-')[0])  # Same org code

    async def test_identifier_uniqueness_validation(self, db, test_organization_id):
        """Test identifier uniqueness constraints."""
        # Create contract with identifier
        contract_id = str(uuid4())
        test_identifier = "TEST-SVC-2024-001"

        # First insert should succeed
        result1 = await db.call_function(
            "core.validate_identifier_uniqueness",
            input_pk_organization=test_organization_id,
            input_entity_type='contract',
            input_identifier=test_identifier
        )
        assert result1 is True

        # Create the contract
        await db.execute("""
            INSERT INTO tenant.tb_contract (pk_contract, fk_customer_org, data)
            VALUES ($1, $2, $3)
        """, contract_id, test_organization_id, {"identifier": test_identifier})

        # Second validation should fail
        result2 = await db.call_function(
            "core.validate_identifier_uniqueness",
            input_pk_organization=test_organization_id,
            input_entity_type='contract',
            input_identifier=test_identifier
        )
        assert result2 is False

    async def test_flexible_lookup(self, db, test_organization_id):
        """Test lookup by different identifier types."""
        # Create contract
        pk_contract = str(uuid4())
        business_identifier = "TEST-CON-2024-001"

        await db.execute("""
            INSERT INTO tenant.tb_contract (pk_contract, fk_customer_org, data)
            VALUES ($1, $2, $3)
        """, pk_contract, test_organization_id, {"identifier": business_identifier})

        # Test UUID lookup
        result_uuid = await db.call_function(
            "core.find_contract_by_any_id",
            input_pk_organization=test_organization_id,
            input_id_value=pk_contract
        )
        assert result_uuid == pk_contract

        # Test business identifier lookup
        result_business = await db.call_function(
            "core.find_contract_by_any_id",
            input_pk_organization=test_organization_id,
            input_id_value=business_identifier
        )
        assert result_business == pk_contract

    async def test_recalculation_dry_run(self, db, test_organization_id):
        """Test identifier recalculation in dry-run mode."""
        context = {
            'trigger_source': 'test',
            'trigger_reason': 'format_update',
            'batch_operation_id': str(uuid4()),
            'affected_entity_count': 0,
            'dry_run': True
        }

        results = await db.call_function(
            "core.recalcid_contract",
            context=context
        )

        # Verify dry run doesn't modify data
        for result in results:
            if result['recalc_needed']:
                # Verify original identifier unchanged
                original = await db.find_one(
                    "tenant.tb_contract",
                    pk_contract=result['pk_entity']
                )
                assert original['data']['identifier'] == result['old_identifier']
```

## Summary

The FraiseQL Triple ID Pattern provides a comprehensive solution for identifier management in PostgreSQL-backed GraphQL applications:

- **Performance**: Sequential internal IDs for efficient joins
- **Security**: UUID primary keys for external references
- **Usability**: Human-readable business identifiers for users
- **Flexibility**: Multiple lookup patterns and migration strategies
- **Scalability**: Optimized indexing and caching patterns

This pattern scales from simple applications to complex multi-tenant enterprise systems, providing the foundation for robust, maintainable identifier management.

---

**Related Documentation:**

- [Database Views](database-views.md) - How identifiers are exposed in query views
- [PostgreSQL Function-Based Mutations](../mutations/postgresql-function-based.md) - Using identifiers in mutations
- [Multi-Tenancy](multi-tenancy.md) - Tenant-scoped identifier patterns
- [Audit Field Patterns](audit-field-patterns.md) - Tracking identifier changes
