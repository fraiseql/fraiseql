# Valid Pattern Variations - Edge Cases

## Overview

During manual review, several valid deviations from strict Trinity patterns were identified. These represent acceptable variations for specific use cases.

## Edge Case Categories

### 1. Projection Tables (tv_*)

**Pattern:** Simple structure without Trinity identifiers

**Valid Example:**
```sql
-- ✅ CORRECT: Projection tables don't need Trinity
CREATE TABLE tv_user (
    id UUID PRIMARY KEY,  -- Simple UUID PK
    data JSONB            -- Pre-computed JSONB
);
```

**Why Valid:** Projection tables are materialized caches of view data. They don't need the Trinity pattern because:
- They only store `id` (for lookups) and `data` (pre-computed JSONB)
- No business logic operates directly on `tv_*` tables
- They exist solely for query performance

**Rule Exception:** TR-001 does not apply to `tv_*` tables

### 2. Hierarchical Data Structures

**Pattern:** Views include `pk_*` for recursive operations

**Valid Example:**
```sql
-- ✅ CORRECT: Recursive CTE needs pk_* for path construction
CREATE VIEW v_comment AS
WITH RECURSIVE comment_tree AS (
    SELECT
        c.pk_comment,  -- ✅ Needed for recursive JOINs
        c.id,
        -- ... other fields
    FROM tb_comment c
    WHERE c.fk_parent_comment IS NULL

    UNION ALL

    SELECT
        c.pk_comment,
        c.id,
        -- ...
    FROM tb_comment c
    JOIN comment_tree ct ON ct.pk_comment = c.fk_parent_comment  -- ✅ Uses pk_*
)
SELECT
    ct.id,
    jsonb_build_object(...) AS data  -- ✅ pk_* NOT in JSONB
FROM comment_tree ct;
```

**Why Valid:** Hierarchical data (trees, graphs, recursive structures) requires `pk_*` for:
- Path construction in ltree columns
- Recursive CTE operations
- Parent-child relationship traversals

**Rule Exception:** VW-002 allows `pk_*` in views with recursive/hierarchical operations

### 3. Optional identifier Fields

**Pattern:** Some entities don't need human-readable slugs

**Valid Examples:**
```sql
-- ✅ CORRECT: Audit logs don't need identifiers
CREATE TABLE tb_audit_log (
    pk_audit_log INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    -- No identifier - audit entries don't need slugs
    action TEXT NOT NULL,
    details JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- ✅ CORRECT: Sensor readings don't need identifiers
CREATE TABLE tb_reading (
    pk_reading INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE,
    -- No identifier - readings identified by timestamp/location
    sensor_id INTEGER REFERENCES tb_sensor(pk_sensor),
    value NUMERIC,
    recorded_at TIMESTAMPTZ DEFAULT NOW()
);
```

**Why Valid:** Not all entities need SEO-friendly URLs or business keys:
- Internal/system tables
- High-volume transactional data
- Lookup tables with <100 rows
- Time-series or event data

**Rule Adjustment:** TR-003 remains INFO level (not ERROR)

### 4. Layered Function Architecture

**Pattern:** Core functions return simple types, app functions return JSONB

**Valid Example:**
```sql
-- ✅ CORRECT: Core function returns simple type
CREATE FUNCTION core.create_customer(...) RETURNS UUID AS $$
BEGIN
    INSERT INTO tb_customer (...) VALUES (...);
    PERFORM app.sync_tv_customer();
    RETURN v_customer_id;
END;
$$ LANGUAGE plpgsql;

-- ✅ CORRECT: App function returns JSONB
CREATE FUNCTION app.create_customer(input_payload JSONB) RETURNS JSONB AS $$
BEGIN
    v_id := core.create_customer(...);
    RETURN app.build_mutation_response(true, 'SUCCESS', 'Created', jsonb_build_object('id', v_id));
END;
$$ LANGUAGE plpgsql;
```

**Why Valid:** Clean separation of concerns:
- Core functions: Business logic, return essential data
- App functions: API formatting, return structured responses

**Rule Exception:** MF-001 allows simple types for `core.*` functions

### 5. Delete Operations and CASCADE

**Pattern:** DELETE operations don't need explicit sync calls

**Valid Example:**
```sql
-- ✅ CORRECT: CASCADE handles tv_* cleanup
CREATE FUNCTION delete_user(user_id UUID) RETURNS BOOLEAN AS $$
BEGIN
    DELETE FROM tb_user WHERE id = user_id;
    -- No explicit sync needed - CASCADE constraint handles tv_user cleanup
    RETURN true;
END;
$$ LANGUAGE plpgsql;
```

**Why Valid:** Foreign key CASCADE constraints automatically clean up related `tv_*` records when base records are deleted.

**Rule Exception:** MF-002 does not apply to DELETE operations

## Implementation Notes

### Exception Handling in Code

```python
def verify_table(table: TableDefinition) -> List[ViolationReport]:
    violations = []

    # Check for tv_* table exception
    if table.name.startswith('tv_'):
        return violations  # Skip Trinity checks for projection tables

    # Normal Trinity checks...
```

### Documentation Updates Needed

1. **concepts-glossary.md:** Add section on "Pattern Variations"
2. **README.md:** Document layered function architecture
3. **API docs:** Explain when identifier fields are optional

### Future Rule Enhancements

1. **Context-aware rules:** Rules that consider table purpose (projection vs base)
2. **Relationship analysis:** Rules that check for CASCADE constraints
3. **Function classification:** Rules that distinguish core vs app functions

## Summary

These edge cases represent **valid architectural choices** rather than errors. The verification system should accommodate these patterns while maintaining strict enforcement of core Trinity principles for base tables and views.
