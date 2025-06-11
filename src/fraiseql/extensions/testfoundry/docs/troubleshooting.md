# TestFoundry Troubleshooting Guide

This guide helps you diagnose and fix common issues when using TestFoundry.

## Table of Contents

1. [Installation Issues](#installation-issues)
2. [Metadata Configuration Issues](#metadata-configuration-issues)
3. [Generation Errors](#generation-errors)
4. [Test Execution Issues](#test-execution-issues)
5. [Performance Issues](#performance-issues)
6. [Common Error Messages](#common-error-messages)
7. [Debugging Techniques](#debugging-techniques)

## Installation Issues

### Schema Not Found

**Error:**
```
ERROR: schema "testfoundry" does not exist
```

**Solution:**
```sql
-- Create the schema
CREATE SCHEMA IF NOT EXISTS testfoundry;

-- Verify it exists
SELECT schema_name FROM information_schema.schemata
WHERE schema_name = 'testfoundry';
```

### Missing Functions

**Error:**
```
ERROR: function testfoundry_generate_random_input(text) does not exist
```

**Solution:**
Ensure all SQL files are executed in order:
```python
setup = FoundrySetup(repository)
await setup.install()
```

Or manually:
```bash
# Execute in order
psql -f 1_tables/*.sql
psql -f 3_functions/*.sql
psql -f 590_test_generation/*.sql
```

### Permission Denied

**Error:**
```
ERROR: permission denied for schema testfoundry
```

**Solution:**
```sql
-- Grant necessary permissions
GRANT ALL ON SCHEMA testfoundry TO your_user;
GRANT ALL ON ALL TABLES IN SCHEMA testfoundry TO your_user;
GRANT ALL ON ALL FUNCTIONS IN SCHEMA testfoundry TO your_user;
```

## Metadata Configuration Issues

### Missing Input Type Mapping

**Error:**
```
ERROR: No field mappings found for input type 'user_input'
```

**Solution:**
Check if mappings exist:
```sql
SELECT * FROM testfoundry.testfoundry_tb_input_field_mapping
WHERE input_type = 'user_input';
```

If empty, add mappings:
```sql
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping
(input_type, field_name, generator_type, required)
VALUES
('user_input', 'email', 'random', TRUE),
('user_input', 'name', 'random', TRUE);
```

### Foreign Key Not Resolving

**Error:**
```
ERROR: FK mapping not found for key: user_id
```

**Solution:**
1. Check FK mapping exists:
```sql
SELECT * FROM testfoundry.testfoundry_tb_fk_mapping
WHERE input_type = 'user_id';
```

2. Add if missing:
```sql
INSERT INTO testfoundry.testfoundry_tb_fk_mapping
(input_type, from_expression, select_field, random_pk_field, random_value_field)
VALUES
('user_id', 'tb_users', 'id', 'id', 'email');
```

### Circular Dependencies

**Error:**
```
ERROR: Circular dependency detected
```

**Solution:**
Review your FK dependencies:
```sql
-- Find circular references
WITH RECURSIVE deps AS (
    SELECT input_type, fk_dependency_fields, 1 as level
    FROM testfoundry.testfoundry_tb_input_field_mapping
    WHERE input_type = 'your_type'

    UNION ALL

    SELECT m.input_type, m.fk_dependency_fields, d.level + 1
    FROM testfoundry.testfoundry_tb_input_field_mapping m
    JOIN deps d ON m.input_type = ANY(d.fk_dependency_fields)
    WHERE d.level < 10
)
SELECT * FROM deps WHERE level > 5;
```

## Generation Errors

### Random Function Not Found

**Error:**
```
ERROR: function testfoundry_random_email() does not exist
```

**Solution:**
Create the missing function:
```sql
CREATE OR REPLACE FUNCTION testfoundry_random_email()
RETURNS TEXT AS $$
BEGIN
    RETURN 'user_' || gen_random_uuid()::TEXT || '@example.com';
END;
$$ LANGUAGE plpgsql;
```

### Invalid Input Type

**Error:**
```
ERROR: type "user_input" does not exist
```

**Solution:**
Create the composite type:
```sql
CREATE TYPE type_user_input AS (
    email TEXT,
    name TEXT,
    bio TEXT
);
```

### Group Leader Configuration Error

**Error:**
```
ERROR: Group leader field 'country' does not have group_dependency_fields defined
```

**Solution:**
Update the group leader configuration:
```sql
UPDATE testfoundry.testfoundry_tb_input_field_mapping
SET group_dependency_fields = ARRAY['country', 'postal_code', 'city_code']
WHERE input_type = 'address_input'
  AND field_name = 'country'
  AND group_leader = TRUE;
```

## Test Execution Issues

### Test Function Not Found

**Error:**
```
ERROR: function test_users_happy_create() does not exist
```

**Solution:**
1. Generate the test:
```sql
SELECT testfoundry_generate_happy_create('users');
```

2. Execute the generated SQL to create the function

### Transaction Rollback Issues

**Error:**
```
ERROR: current transaction is aborted
```

**Solution:**
Ensure tests are run in isolated transactions:
```sql
BEGIN;
SELECT test_users_happy_create();
ROLLBACK;
```

### pgTAP Not Installed

**Error:**
```
ERROR: function plan(integer) does not exist
```

**Solution:**
Install pgTAP:
```sql
CREATE EXTENSION IF NOT EXISTS pgtap;
```

## Performance Issues

### Slow Random Generation

**Symptom:** Generation takes several seconds per record

**Solutions:**

1. Add indexes:
```sql
CREATE INDEX idx_fk_mapping_type
ON testfoundry.testfoundry_tb_fk_mapping(input_type);

CREATE INDEX idx_field_mapping_type
ON testfoundry.testfoundry_tb_input_field_mapping(input_type);
```

2. Optimize FK queries:
```sql
-- Add indexes on frequently queried columns
CREATE INDEX idx_users_deleted_at ON tb_users(deleted_at);
```

3. Use batch generation:
```sql
-- Generate multiple at once
SELECT testfoundry_generate_random_input('user_input')
FROM generate_series(1, 100);
```

### Memory Issues

**Symptom:** Out of memory errors

**Solutions:**

1. Limit batch sizes:
```python
# Process in chunks
for i in range(0, total, 100):
    await generator.generate_batch(100)
```

2. Increase PostgreSQL memory:
```sql
-- In postgresql.conf
work_mem = '256MB'
shared_buffers = '1GB'
```

## Common Error Messages

### "No random value could be generated"

**Cause:** FK table is empty or all records are filtered out

**Debug:**
```sql
-- Check if source table has data
SELECT COUNT(*) FROM tb_users WHERE deleted_at IS NULL;

-- Test FK mapping directly
SELECT testfoundry_random_value_from_mapping('user_id', NULL);
```

### "Field type not supported"

**Cause:** Unknown PostgreSQL type

**Solution:**
Add custom handler:
```sql
-- In testfoundry_random_value function
WHEN p_field_type = 'your_custom_type' THEN
    RETURN your_custom_generator();
```

### "Dependency not satisfied"

**Cause:** Required field not generated before dependent field

**Debug:**
```sql
-- Check dependency order
SELECT field_name, generator_type, fk_dependency_fields
FROM testfoundry.testfoundry_tb_input_field_mapping
WHERE input_type = 'your_input'
ORDER BY
    CASE generator_type
        WHEN 'random' THEN 1
        WHEN 'resolve_fk' THEN 2
        ELSE 3
    END;
```

## Debugging Techniques

### Enable Debug Mode

```sql
-- See step-by-step generation
SELECT testfoundry_generate_random_input('user_input', true);
```

### Trace FK Resolution

```sql
-- Add debug output to FK mapping function
CREATE OR REPLACE FUNCTION testfoundry_random_value_from_mapping_debug(
    p_fk_mapping_key TEXT,
    VARIADIC p_args TEXT[]
)
RETURNS JSONB AS $$
DECLARE
    v_result JSONB;
BEGIN
    RAISE NOTICE 'Resolving FK: %, Args: %', p_fk_mapping_key, p_args;

    -- Original function logic
    v_result := testfoundry_random_value_from_mapping(p_fk_mapping_key, VARIADIC p_args);

    RAISE NOTICE 'Result: %', v_result;
    RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

### Validate Metadata

```sql
-- Check for common issues
SELECT
    'Missing FK mapping' as issue,
    field_name,
    fk_mapping_key
FROM testfoundry.testfoundry_tb_input_field_mapping
WHERE generator_type = 'resolve_fk'
  AND fk_mapping_key NOT IN (
    SELECT input_type FROM testfoundry.testfoundry_tb_fk_mapping
  );
```

### Test Individual Components

```sql
-- Test random function
SELECT testfoundry_random_email();

-- Test FK resolution
SELECT * FROM testfoundry.testfoundry_tb_fk_mapping WHERE input_type = 'user_id';

-- Test group leader
SELECT field_name, generator_group, group_leader, group_dependency_fields
FROM testfoundry.testfoundry_tb_input_field_mapping
WHERE generator_group IS NOT NULL
ORDER BY generator_group, group_leader DESC;
```

## Best Practices for Avoiding Issues

### 1. Validate Before Generation

```sql
-- Check all mappings are complete
SELECT input_type, COUNT(*) as field_count
FROM testfoundry.testfoundry_tb_input_field_mapping
GROUP BY input_type;

-- Verify FK mappings exist
SELECT DISTINCT fk_mapping_key
FROM testfoundry.testfoundry_tb_input_field_mapping
WHERE generator_type = 'resolve_fk'
  AND fk_mapping_key IS NOT NULL;
```

### 2. Start Simple

1. Begin with random fields only
2. Add FK relationships one at a time
3. Implement group leaders last
4. Test each addition

### 3. Use Transactions

```sql
BEGIN;
-- Insert metadata
INSERT INTO testfoundry.testfoundry_tb_input_field_mapping ...;

-- Test immediately
SELECT testfoundry_generate_random_input('your_input');

-- Commit only if successful
COMMIT;
```

### 4. Document Complex Mappings

```sql
COMMENT ON COLUMN testfoundry.testfoundry_tb_fk_mapping.dependency_field_mapping IS
'Maps country field to c.name in the JOIN for postal code resolution';
```

## Getting Help

If you're still stuck:

1. **Check logs**: PostgreSQL logs often have detailed error information
2. **Enable verbose mode**: Set `client_min_messages = DEBUG1`
3. **Isolate the issue**: Test components individually
4. **Review examples**: Check the examples directory for working configurations
5. **Ask for help**: Include your metadata setup and full error message

## Quick Reference Card

| Issue | Quick Check | Quick Fix |
|-------|------------|-----------|
| Function not found | `\df testfoundry_*` | Run installation script |
| No field mappings | `SELECT * FROM testfoundry.testfoundry_tb_input_field_mapping` | Insert mappings |
| FK not resolving | `SELECT * FROM testfoundry.testfoundry_tb_fk_mapping` | Add FK mapping |
| Slow generation | `EXPLAIN ANALYZE` on FK queries | Add indexes |
| Type not found | `\dT *input*` | Create composite type |
| Permission denied | `\dn+ testfoundry` | Grant permissions |
