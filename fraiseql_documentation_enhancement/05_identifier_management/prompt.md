# Prompt: Implement Identifier Management Pattern Documentation

## Objective

Create comprehensive documentation for FraiseQL's identifier management pattern, covering the **Triple ID Pattern** and identifier recalculation strategies. This pattern is crucial for enterprise applications that need both database efficiency (UUID primary keys) and user-friendly business identifiers.

## Current State

**Status: MINIMAL DOCUMENTATION (10% coverage)**
- Basic UUID primary keys mentioned
- No documentation of business identifier patterns
- Missing triple ID pattern (id, pk_*, identifier)
- No identifier recalculation strategies

## Target Documentation

Create new documentation file: `docs/advanced/identifier-management.md`

## Implementation Requirements

### 1. Document Triple ID Pattern

**Three types of identifiers for each entity:**
```sql
-- Standard identifier pattern for all entities
CREATE TABLE tenant.tb_contract (
    -- 1. Internal sequence ID (for JOINs and performance)
    id SERIAL NOT NULL,

    -- 2. Primary Key UUID (exposed to GraphQL as 'id')
    pk_contract UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 3. Business Identifier (human-readable, optional)
    -- Stored in data column, exposed via views
    data JSONB NOT NULL,  -- Contains: {"identifier": "CONTRACT-2024-001"}

    -- Multi-tenant context
    fk_customer_org UUID NOT NULL,

    -- Standard audit fields
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL
);
```

**ID Type Usage:**

| ID Type | Purpose | Visibility | Example |
|---------|---------|------------|---------|
| `id` (SERIAL) | Internal joins, performance | Never exposed | 12345 |
| `pk_*` (UUID) | GraphQL ID, external references | Always exposed as `id` | `123e4567-...` |
| `identifier` (TEXT) | Human-readable business ID | Optional, user-facing | `CONTRACT-2024-001` |

### 2. Document Identifier Generation Strategies

**Business identifier patterns:**
```sql
-- Identifier generation function
CREATE OR REPLACE FUNCTION core.generate_contract_identifier(
    input_pk_organization UUID,
    input_contract_type TEXT DEFAULT NULL
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
        ELSE 'CON'
    END;

    -- Get next sequence number for this org/year/type
    WITH sequence_calc AS (
        SELECT COALESCE(
            MAX((data->>'sequence_number')::INTEGER), 0
        ) + 1 as next_seq
        FROM tenant.tb_contract
        WHERE fk_customer_org = input_pk_organization
        AND data->>'identifier_year' = v_year
        AND data->>'contract_type' = COALESCE(input_contract_type, 'default')
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

### 3. Document Identifier Recalculation Pattern

**Recalculation context and triggers:**
```sql
-- Recalculation context type
CREATE TYPE core.recalculation_context AS (
    trigger_source TEXT,        -- 'api', 'import', 'migration', 'system'
    trigger_reason TEXT,        -- 'creation', 'update', 'org_settings_changed'
    batch_operation_id UUID,    -- For tracking bulk operations
    affected_entity_count INTEGER
);

-- Contract identifier recalculation
CREATE OR REPLACE FUNCTION core.recalcid_contract(
    context core.recalculation_context
) RETURNS TABLE(
    pk_contract UUID,
    old_identifier TEXT,
    new_identifier TEXT,
    recalc_needed BOOLEAN
) AS $$
DECLARE
    v_contract RECORD;
    v_new_identifier TEXT;
    v_org_settings JSONB;
BEGIN
    -- Process each contract that might need recalculation
    FOR v_contract IN
        SELECT
            c.pk_contract,
            c.fk_customer_org,
            c.data->>'identifier' as current_identifier,
            c.data->>'contract_type' as contract_type,
            c.created_at,
            c.data
        FROM tenant.tb_contract c
        WHERE
            -- Only recalculate if identifier format might be outdated
            CASE context.trigger_reason
                WHEN 'org_settings_changed' THEN
                    c.data->>'identifier_format_version' != '2024.1'
                WHEN 'creation' THEN FALSE  -- New contracts already have correct ID
                ELSE TRUE  -- For migrations, check all
            END
        ORDER BY c.created_at  -- Maintain chronological order
    LOOP
        -- Generate new identifier based on current rules
        v_new_identifier := core.generate_contract_identifier(
            v_contract.fk_customer_org,
            v_contract.contract_type
        );

        -- Check if recalculation needed
        IF v_contract.current_identifier != v_new_identifier THEN
            -- Update the contract
            UPDATE tenant.tb_contract
            SET
                data = data || jsonb_build_object(
                    'identifier', v_new_identifier,
                    'identifier_format_version', '2024.1',
                    'previous_identifier', v_contract.current_identifier,
                    'identifier_recalculated_at', NOW(),
                    'recalculation_context', row_to_json(context)
                ),
                updated_at = NOW(),
                updated_by = COALESCE(
                    (context.batch_operation_id)::UUID,
                    '00000000-0000-0000-0000-000000000000'::UUID
                ),
                version = version + 1
            WHERE pk_contract = v_contract.pk_contract;

            -- Return the change
            pk_contract := v_contract.pk_contract;
            old_identifier := v_contract.current_identifier;
            new_identifier := v_new_identifier;
            recalc_needed := TRUE;
            RETURN NEXT;
        ELSE
            -- No change needed, but log for completeness
            pk_contract := v_contract.pk_contract;
            old_identifier := v_contract.current_identifier;
            new_identifier := v_contract.current_identifier;
            recalc_needed := FALSE;
            RETURN NEXT;
        END IF;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
```

### 4. Document Identifier Lookup Patterns

**Flexible entity lookup by any ID type:**
```sql
-- Unified lookup function
CREATE OR REPLACE FUNCTION core.find_contract_by_any_id(
    input_pk_organization UUID,
    input_id_value TEXT  -- Can be UUID or business identifier
) RETURNS UUID AS $$
DECLARE
    v_pk_contract UUID;
BEGIN
    -- Try UUID first (fastest)
    BEGIN
        IF input_id_value ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN
            SELECT pk_contract INTO v_pk_contract
            FROM tenant.tb_contract
            WHERE pk_contract = input_id_value::UUID
            AND fk_customer_org = input_pk_organization;

            IF v_pk_contract IS NOT NULL THEN
                RETURN v_pk_contract;
            END IF;
        END IF;
    EXCEPTION WHEN invalid_text_representation THEN
        -- Not a valid UUID, continue to business identifier lookup
        NULL;
    END;

    -- Try business identifier (indexed lookup)
    SELECT pk_contract INTO v_pk_contract
    FROM tenant.tb_contract
    WHERE fk_customer_org = input_pk_organization
    AND data->>'identifier' = input_id_value;

    RETURN v_pk_contract;
END;
$$ LANGUAGE plpgsql STABLE;
```

### 5. Document View Patterns with Identifier Exposure

**Query views exposing appropriate identifiers:**
```sql
-- Standard view exposing both UUID and business identifier
CREATE OR REPLACE VIEW public.v_contract AS
SELECT
    -- Transform pk_contract → id for GraphQL
    c.pk_contract::TEXT AS id,
    c.fk_customer_org::TEXT AS tenant_id,

    -- Business identifier (user-friendly)
    c.data->>'identifier' AS identifier,

    -- Entity data
    c.data->>'name' AS name,
    c.data->>'contract_type' AS contract_type,
    c.data->>'status' AS status,

    -- Identifier metadata (for debugging/admin)
    c.data->>'identifier_format_version' AS identifier_format_version,
    c.data->>'previous_identifier' AS previous_identifier,
    (c.data->>'identifier_recalculated_at')::TIMESTAMPTZ AS identifier_recalculated_at,

    -- Audit fields
    c.created_at,
    c.updated_at,
    c.version,

    -- Complete data for APIs
    jsonb_build_object(
        'id', c.pk_contract,
        'identifier', c.data->>'identifier',
        'name', c.data->>'name',
        'contract_type', c.data->>'contract_type',
        'status', c.data->>'status',
        'created_at', c.created_at,
        'updated_at', c.updated_at
    ) AS data

FROM tenant.tb_contract c
WHERE c.deleted_at IS NULL;

-- Admin view with internal IDs (for debugging)
CREATE OR REPLACE VIEW admin.v_contract_with_internal_ids AS
SELECT
    c.id AS internal_id,           -- SERIAL id
    c.pk_contract AS primary_key,  -- UUID
    c.data->>'identifier' AS business_identifier,
    c.data->>'name' AS name,
    -- Full identifier history
    jsonb_build_object(
        'current', c.data->>'identifier',
        'previous', c.data->>'previous_identifier',
        'format_version', c.data->>'identifier_format_version',
        'recalculated_at', c.data->>'identifier_recalculated_at'
    ) AS identifier_history
FROM tenant.tb_contract c;
```

### 6. Document GraphQL Integration

**GraphQL types with identifier management:**
```python
@fraiseql.type
class Contract:
    """Contract with multiple identifier types."""
    # Primary ID (pk_contract exposed as 'id')
    id: UUID

    # Business identifier (human-readable)
    identifier: str

    # Contract data
    name: str
    contract_type: str
    status: str

    # Identifier metadata (admin only)
    identifier_format_version: Optional[str] = None
    previous_identifier: Optional[str] = None
    identifier_recalculated_at: Optional[datetime] = None

@fraiseql.input
class ContractLookupInput:
    """Flexible contract lookup by any ID type."""
    # Can be either UUID or business identifier
    id: Optional[str] = None
    identifier: Optional[str] = None

@fraiseql.query
async def contract(
    info: GraphQLResolveInfo,
    lookup: ContractLookupInput
) -> Optional[Contract]:
    """Find contract by UUID or business identifier."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]

    # Determine lookup value
    lookup_value = lookup.id or lookup.identifier
    if not lookup_value:
        raise ValueError("Either id or identifier must be provided")

    # Use unified lookup function
    pk_contract = await db.call_function(
        "core.find_contract_by_any_id",
        input_pk_organization=tenant_id,
        input_id_value=lookup_value
    )

    if not pk_contract:
        return None

    # Get contract data from view
    result = await db.find_one(
        "v_contract",
        where={"id": pk_contract, "tenant_id": tenant_id}
    )

    return Contract.from_dict(result) if result else None
```

### 7. Document Migration Patterns

**Migrating existing systems to triple ID pattern:**
```sql
-- Migration: Add identifier support to existing tables
ALTER TABLE tenant.tb_contract
ADD COLUMN pk_contract UUID DEFAULT gen_random_uuid();

-- Create unique constraint
ALTER TABLE tenant.tb_contract
ADD CONSTRAINT uq_contract_pk UNIQUE (pk_contract);

-- Backfill business identifiers
UPDATE tenant.tb_contract
SET data = data || jsonb_build_object(
    'identifier', core.generate_contract_identifier(fk_customer_org, data->>'contract_type'),
    'identifier_format_version', '2024.1',
    'identifier_migration_date', NOW()
)
WHERE data->>'identifier' IS NULL;

-- Create indexes for efficient lookups
CREATE INDEX idx_contract_identifier ON tenant.tb_contract
USING gin ((data->>'identifier') gin_trgm_ops)
WHERE data->>'identifier' IS NOT NULL;

CREATE INDEX idx_contract_pk ON tenant.tb_contract (pk_contract);
```

### 8. Document Performance Considerations

**Indexing strategies:**
```sql
-- Primary key index (automatic)
-- pk_contract already has unique index

-- Business identifier lookup index
CREATE INDEX idx_contract_business_id ON tenant.tb_contract
(fk_customer_org, (data->>'identifier'))
WHERE data->>'identifier' IS NOT NULL;

-- Sequence tracking index for identifier generation
CREATE INDEX idx_contract_sequence_tracking ON tenant.tb_contract
(fk_customer_org, (data->>'identifier_year'), (data->>'contract_type'));

-- Full-text search on identifiers
CREATE INDEX idx_contract_identifier_search ON tenant.tb_contract
USING gin ((data->>'identifier') gin_trgm_ops);
```

**Query optimization:**
```sql
-- Optimized lookup with proper index usage
EXPLAIN (ANALYZE, BUFFERS)
SELECT pk_contract
FROM tenant.tb_contract
WHERE fk_customer_org = $1
AND data->>'identifier' = $2;
```

### 9. Document Identifier Uniqueness Constraints

**Ensuring business identifier uniqueness:**
```sql
-- Unique constraint on business identifiers within organization
CREATE UNIQUE INDEX uq_contract_identifier_per_org
ON tenant.tb_contract (fk_customer_org, (data->>'identifier'))
WHERE data->>'identifier' IS NOT NULL
AND deleted_at IS NULL;

-- Function to validate identifier uniqueness before assignment
CREATE OR REPLACE FUNCTION core.validate_identifier_uniqueness(
    input_pk_organization UUID,
    input_entity_type TEXT,
    input_identifier TEXT,
    input_exclude_pk UUID DEFAULT NULL
) RETURNS BOOLEAN AS $$
DECLARE
    v_exists BOOLEAN;
BEGIN
    -- Check based on entity type
    CASE input_entity_type
        WHEN 'contract' THEN
            SELECT EXISTS(
                SELECT 1 FROM tenant.tb_contract
                WHERE fk_customer_org = input_pk_organization
                AND data->>'identifier' = input_identifier
                AND deleted_at IS NULL
                AND (input_exclude_pk IS NULL OR pk_contract != input_exclude_pk)
            ) INTO v_exists;

        WHEN 'user' THEN
            SELECT EXISTS(
                SELECT 1 FROM tenant.tb_user
                WHERE fk_customer_org = input_pk_organization
                AND data->>'identifier' = input_identifier
                AND deleted_at IS NULL
                AND (input_exclude_pk IS NULL OR pk_user != input_exclude_pk)
            ) INTO v_exists;

        ELSE
            RAISE EXCEPTION 'Unknown entity type: %', input_entity_type;
    END CASE;

    RETURN NOT v_exists;  -- Return true if identifier is available
END;
$$ LANGUAGE plpgsql STABLE;
```

### 10. Documentation Structure

Create comprehensive sections:
1. **Overview** - Why triple ID pattern matters
2. **ID Types** - Internal, primary key, business identifier
3. **Generation Strategies** - Auto-generation patterns
4. **Recalculation Pattern** - When and how to recalculate
5. **Lookup Patterns** - Finding entities by any ID type
6. **View Design** - Exposing appropriate identifiers
7. **GraphQL Integration** - Types and resolvers
8. **Migration Guides** - Adding identifier support
9. **Performance** - Indexing and optimization
10. **Uniqueness Constraints** - Ensuring identifier integrity
11. **Best Practices** - Do's and don'ts
12. **Troubleshooting** - Common identifier issues

## Success Criteria

After implementation:
- [ ] Complete triple ID pattern documentation
- [ ] Identifier generation strategies covered
- [ ] Recalculation patterns documented
- [ ] GraphQL integration examples provided
- [ ] Migration guidance included
- [ ] Performance optimization covered
- [ ] Follows FraiseQL documentation style

## File Location

Create: `docs/advanced/identifier-management.md`

Update: `docs/advanced/index.md` to include link

## Implementation Methodology

### Development Workflow

**Critical: Systematic ID Pattern Documentation**

Break this complex identifier pattern into logical commits:

1. **Core Pattern Foundation Commit** (15-25 minutes)
   ```bash
   # Establish triple ID pattern fundamentals
   git add docs/advanced/identifier-management.md
   git commit -m "docs: initialize identifier management pattern guide

   - Define triple ID pattern (id, pk_*, identifier)
   - Document ID type usage table and examples
   - Show database table structures
   - Explain ID visibility and exposure rules
   - References #[issue-number]"
   ```

2. **Generation Strategies Commit** (30-40 minutes)
   ```bash
   # Complete identifier generation patterns
   git add docs/advanced/identifier-management.md
   git commit -m "docs: add identifier generation strategies

   - Document auto-generation patterns
   - Show sequential, hierarchical, and custom formats
   - Include collision detection and retry logic
   - Add business rule examples for different entities"
   ```

3. **Recalculation System Commit** (25-35 minutes)
   ```bash
   # Complete recalculation patterns and functions
   git add docs/advanced/identifier-management.md
   git commit -m "docs: add identifier recalculation patterns

   - Document recalculation trigger conditions
   - Show recalculation function implementations
   - Include batch recalculation strategies
   - Add validation and verification patterns"
   ```

4. **Lookup and Query Patterns Commit** (20-30 minutes)
   ```bash
   # Complete lookup and view patterns
   git add docs/advanced/identifier-management.md
   git commit -m "docs: add identifier lookup and view patterns

   - Document lookup by any ID type patterns
   - Show view designs for identifier exposure
   - Include uniqueness validation functions
   - Add GraphQL resolver examples"
   ```

5. **Performance and Migration Commit** (25-35 minutes)
   ```bash
   # Complete optimization and migration guidance
   git add docs/advanced/identifier-management.md
   git commit -m "docs: add identifier performance and migration

   - Document indexing strategies for all ID types
   - Include migration patterns for existing systems
   - Add performance benchmarks and optimizations
   - Show identifier collision handling"
   ```

6. **Integration and Finalization Commit** (15-20 minutes)
   ```bash
   # Finalize with best practices and cross-references
   git add docs/advanced/identifier-management.md docs/advanced/index.md
   git commit -m "docs: complete identifier management guide

   - Add troubleshooting and best practices
   - Include testing patterns for identifier systems
   - Update advanced patterns index
   - Add cross-references to related patterns
   - Ready for review"
   ```

### Quality Validation

After each commit:
- [ ] Build documentation (`mkdocs serve`)
- [ ] Validate SQL function syntax
- [ ] Test identifier generation examples
- [ ] Verify uniqueness constraint logic
- [ ] Check GraphQL type definitions
- [ ] Ensure examples match PrintOptim patterns

### Risk Management

**For complex ID generation:**
```bash
# Test identifier generation in isolation
# CREATE TEMPORARY TABLE test_identifiers AS...
# Verify collision detection works correctly
# Test recalculation performance on large datasets
```

**For migration examples:**
```bash
# Validate migration scripts carefully
# Include rollback strategies
# Test with various data scenarios
# Document downtime requirements
```

**Recovery strategy:**
```bash
# Complex examples should be validated separately
git stash  # Save work in progress
# Test SQL in development database first
git stash pop  # Resume documentation
```

## Dependencies

Should reference:
- `../core-concepts/database-views.md` - How IDs are exposed in views
- `../mutations/postgresql-function-based.md` - Using IDs in functions
- `multi-tenancy.md` - Tenant-scoped identifiers
- `audit-field-patterns.md` - Audit trail for identifier changes

## Estimated Effort

**Large effort** - Complex enterprise pattern:
- Triple ID pattern explanation
- Multiple generation strategies
- Recalculation and migration patterns
- Performance optimization guidance

Target: 800-1000 lines of documentation
