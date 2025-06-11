# TestFoundry Installation Guide

This guide walks you through installing TestFoundry in your PostgreSQL database.

## Prerequisites

### Required

- PostgreSQL 14 or higher
- Superuser or database owner privileges
- pgTAP extension (for test execution)

### Recommended

- Python 3.8+ (for Python API)
- FraiseQL (if using with FraiseQL)
- Docker/Podman (for isolated testing)

## Installation Methods

### Method 1: Using Python API (Recommended)

If you're using TestFoundry with FraiseQL:

```python
import asyncio
from fraiseql.db import FraiseQLRepository
from fraiseql.extensions.testfoundry import FoundrySetup

async def install():
    # Create your database connection
    pool = await asyncpg.create_pool(
        "postgresql://user:password@localhost/dbname"
    )
    repository = FraiseQLRepository(pool=pool)

    # Install TestFoundry
    setup = FoundrySetup(repository)
    await setup.install()

    print("TestFoundry installed successfully!")

    await pool.close()

# Run installation
asyncio.run(install())
```

### Method 2: Manual SQL Installation

#### Step 1: Create Schema

```sql
-- Create TestFoundry schema
CREATE SCHEMA IF NOT EXISTS testfoundry;

-- Set search path
SET search_path TO testfoundry, public;
```

#### Step 2: Install Tables

Execute these SQL files in order:

```bash
# Navigate to TestFoundry directory
cd /path/to/fraiseql/src/fraiseql/extensions/testfoundry

# Install tables
psql -U postgres -d your_database -f 1_tables/5001_testfoundry_tb_field_mapping.sql
psql -U postgres -d your_database -f 1_tables/5002_test_entity_dependents.sql
psql -U postgres -d your_database -f 1_tables/5003_test_manual_scenarios.sql
psql -U postgres -d your_database -f 1_tables/5003_testfoundry_tb_entity_period_fields.sql

# Install metadata tables
psql -U postgres -d your_database -f rootextension/testfoundry/testfoundry_tb_fk_mapping.sql
psql -U postgres -d your_database -f rootextension/testfoundry/testfoundry_tb_input_field_mapping.sql
```

#### Step 3: Install Functions

```bash
# Core functions
psql -U postgres -d your_database -f 3_functions/5030_get_entity_structure.sql
psql -U postgres -d your_database -f 3_functions/5036_testfoundry_insert_entity.sql

# Randomizer functions
psql -U postgres -d your_database -f 3_functions/randomize/5031_testfoundry_random_value.sql
psql -U postgres -d your_database -f 3_functions/randomize/testfoundry_generate_random_input.sql
psql -U postgres -d your_database -f 3_functions/randomize/testfoundry_random_value_from_mapping.sql

# Type introspection
psql -U postgres -d your_database -f 3_functions/type_introspection/testfoundry_list_input_fields.sql
```

#### Step 4: Install Generators

```bash
# Test generators
psql -U postgres -d your_database -f 590_test_generation/51_create/501_generate_happy_create.sql
psql -U postgres -d your_database -f 590_test_generation/51_create/502_generate_duplicate_create.sql
psql -U postgres -d your_database -f 590_test_generation/52_delete/521_generate_happy_delete.sql
```

### Method 3: Using Make/Script

Create an installation script:

```bash
#!/bin/bash
# install_testfoundry.sh

DB_NAME=${1:-your_database}
DB_USER=${2:-postgres}
DB_HOST=${3:-localhost}

echo "Installing TestFoundry in database: $DB_NAME"

# Function to execute SQL file
execute_sql() {
    psql -h $DB_HOST -U $DB_USER -d $DB_NAME -f "$1" || exit 1
}

# Create schema
psql -h $DB_HOST -U $DB_USER -d $DB_NAME -c "CREATE SCHEMA IF NOT EXISTS testfoundry;"

# Install tables
for file in 1_tables/*.sql rootextension/testfoundry/*.sql; do
    echo "Installing: $file"
    execute_sql "$file"
done

# Install functions
for file in 3_functions/*.sql 3_functions/*/*.sql; do
    echo "Installing: $file"
    execute_sql "$file"
done

# Install generators
for file in 590_test_generation/*/*.sql; do
    echo "Installing: $file"
    execute_sql "$file"
done

echo "TestFoundry installation complete!"
```

## Verification

### Check Installation

```sql
-- Verify schema exists
SELECT schema_name
FROM information_schema.schemata
WHERE schema_name = 'testfoundry';

-- Check tables
SELECT table_name
FROM information_schema.tables
WHERE table_schema = 'testfoundry'
ORDER BY table_name;

-- Check functions
SELECT routine_name
FROM information_schema.routines
WHERE routine_schema = 'testfoundry'
  AND routine_name LIKE 'testfoundry_%'
ORDER BY routine_name;
```

### Test Basic Functionality

```sql
-- Test random value generation
SELECT testfoundry.testfoundry_random_value('text');
SELECT testfoundry.testfoundry_random_value('uuid');
SELECT testfoundry.testfoundry_random_value('integer');

-- Test entity structure analysis
SELECT * FROM testfoundry.testfoundry_get_entity_structure('test_entity');
```

## Installing pgTAP

TestFoundry generates pgTAP tests, so you'll need pgTAP installed:

### Ubuntu/Debian

```bash
sudo apt-get install postgresql-14-pgtap
```

### macOS with Homebrew

```bash
brew install pgtap
```

### From Source

```bash
git clone https://github.com/theory/pgtap.git
cd pgtap
make
make install
```

### Enable in Database

```sql
CREATE EXTENSION IF NOT EXISTS pgtap;
```

## Post-Installation Setup

### 1. Grant Permissions

```sql
-- Grant usage on schema
GRANT USAGE ON SCHEMA testfoundry TO your_app_user;

-- Grant permissions on tables
GRANT SELECT, INSERT, UPDATE, DELETE
ON ALL TABLES IN SCHEMA testfoundry
TO your_app_user;

-- Grant permissions on functions
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA testfoundry
TO your_app_user;

-- Grant permissions on sequences
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA testfoundry
TO your_app_user;
```

### 2. Configure Search Path

```sql
-- Add testfoundry to search path
ALTER DATABASE your_database SET search_path TO public, testfoundry;

-- Or for specific user
ALTER USER your_app_user SET search_path TO public, testfoundry;
```

### 3. Create Custom Functions

Add any domain-specific random generators:

```sql
-- Email generator
CREATE OR REPLACE FUNCTION testfoundry.testfoundry_random_email()
RETURNS TEXT AS $$
BEGIN
    RETURN 'user_' || gen_random_uuid()::TEXT || '@example.com';
END;
$$ LANGUAGE plpgsql;

-- URL generator
CREATE OR REPLACE FUNCTION testfoundry.testfoundry_random_url()
RETURNS TEXT AS $$
BEGIN
    RETURN 'https://example.com/' || SUBSTRING(MD5(RANDOM()::TEXT), 1, 8);
END;
$$ LANGUAGE plpgsql;
```

## Upgrading TestFoundry

### Backup First

```bash
pg_dump -U postgres -d your_database -n testfoundry > testfoundry_backup.sql
```

### Method 1: Clean Reinstall

```sql
-- Remove old version
DROP SCHEMA testfoundry CASCADE;

-- Install new version
-- Follow installation steps above
```

### Method 2: Incremental Update

```sql
-- Run only new/changed files
-- Check release notes for specific files
```

## Uninstalling TestFoundry

### Complete Removal

```sql
-- Remove schema and all objects
DROP SCHEMA IF EXISTS testfoundry CASCADE;

-- Remove from search path
ALTER DATABASE your_database RESET search_path;
```

### Partial Removal (Keep Data)

```sql
-- Remove functions only
DROP FUNCTION IF EXISTS testfoundry.testfoundry_generate_random_input(TEXT, BOOLEAN);
DROP FUNCTION IF EXISTS testfoundry.testfoundry_generate_happy_create(TEXT);
-- ... etc

-- Keep tables with metadata
```

## Docker Installation

### Dockerfile

```dockerfile
FROM postgres:14

# Install pgTAP
RUN apt-get update && apt-get install -y postgresql-14-pgtap

# Copy TestFoundry files
COPY testfoundry /docker-entrypoint-initdb.d/testfoundry/

# Installation script
COPY install_testfoundry.sh /docker-entrypoint-initdb.d/99_install_testfoundry.sh
RUN chmod +x /docker-entrypoint-initdb.d/99_install_testfoundry.sh
```

### Docker Compose

```yaml
version: '3.8'

services:
  postgres:
    build: .
    environment:
      POSTGRES_DB: testdb
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
    volumes:
      - ./testfoundry:/testfoundry
    ports:
      - "5432:5432"
```

## Troubleshooting Installation

### Common Issues

1. **Permission Denied**
   ```sql
   -- Check current user
   SELECT current_user, current_database();

   -- Grant superuser temporarily
   ALTER USER your_user WITH SUPERUSER;
   ```

2. **File Not Found**
   - Ensure you're in the correct directory
   - Check file paths are relative to current location

3. **Function Already Exists**
   ```sql
   -- Drop existing function
   DROP FUNCTION IF EXISTS testfoundry.function_name CASCADE;
   ```

4. **Extension Not Found**
   ```sql
   -- Check available extensions
   SELECT * FROM pg_available_extensions WHERE name = 'pgtap';
   ```

### Verify Installation

Run the verification script:

```sql
DO $$
DECLARE
    v_count INTEGER;
BEGIN
    -- Check tables
    SELECT COUNT(*) INTO v_count
    FROM information_schema.tables
    WHERE table_schema = 'testfoundry';

    RAISE NOTICE 'TestFoundry tables: %', v_count;

    -- Check functions
    SELECT COUNT(*) INTO v_count
    FROM information_schema.routines
    WHERE routine_schema = 'testfoundry';

    RAISE NOTICE 'TestFoundry functions: %', v_count;

    -- Test basic functionality
    PERFORM testfoundry.testfoundry_random_value('text');
    RAISE NOTICE 'Basic functionality: OK';

EXCEPTION
    WHEN OTHERS THEN
        RAISE NOTICE 'Installation incomplete: %', SQLERRM;
END;
$$;
```

## Next Steps

1. Read the [User Guide](./user-guide.md) to start using TestFoundry
2. Check [Examples](./examples.md) for common patterns
3. Configure metadata for your entities
4. Generate your first tests!

## Getting Help

- Check the [Troubleshooting Guide](./troubleshooting.md)
- Review installation logs for errors
- Ensure all prerequisites are met
- Verify file permissions and paths
