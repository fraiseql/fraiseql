# FraiseQL Phase 5: Composite Type Input Generation - Implementation Plan

**Status**: Ready for Implementation
**Priority**: High
**Complexity**: Medium
**Estimated Time**: 8-12 hours
**Target Agent**: Junior/Mid-level Developer (Step-by-step guidance)

---

## ⚠️ IMPORTANT: Your Role

**You are implementing AutoFraiseQL introspection only.**

- ✅ **SpecQL creates the database** (composite types, functions, comments)
- ✅ **You introspect the database** (read metadata, generate GraphQL types)
- ❌ **You do NOT create or modify database objects**

### What Already Exists in the Database

When you connect to a SpecQL-generated database, you'll find:

```sql
-- ✅ Already exists (created by SpecQL)
CREATE TYPE app.type_create_contact_input AS (
    email TEXT,
    company_id UUID,
    status TEXT
);

CREATE FUNCTION app.create_contact(
    input_tenant_id UUID,
    input_user_id UUID,
    input_payload JSONB
) RETURNS app.mutation_result;

COMMENT ON TYPE app.type_create_contact_input IS '@fraiseql:input name=CreateContactInput';
COMMENT ON COLUMN app.type_create_contact_input.email IS '@fraiseql:field name=email,type=String!,required=true';
```

### Your Job

**Read** this metadata and **generate** GraphQL types:

```python
# ✅ Your code does this
@fraiseql.input
class CreateContactInput:
    email: str
    companyId: UUID
    status: str

@fraiseql.mutation(
    function="create_contact",
    schema="app",
    context_params={"tenant_id": "input_tenant_id", "user_id": "input_user_id"}
)
class CreateContact:
    input: CreateContactInput
    success: Contact
    failure: ContactError
```

---

## 📋 Table of Contents

1. [Executive Summary](#executive-summary)
2. [Prerequisites](#prerequisites)
3. [Phase Overview](#phase-overview)
4. [Detailed Implementation Steps](#detailed-implementation-steps)
5. [Testing Strategy](#testing-strategy)
6. [Validation Checklist](#validation-checklist)

---

## Executive Summary

### What You're Building

Currently, FraiseQL's AutoDiscovery generates GraphQL mutations by reading function parameters directly:

```sql
-- Pattern AutoFraiseQL currently expects
CREATE FUNCTION fn_create_user(p_name TEXT, p_email TEXT) ...
```

**Current behavior**: InputGenerator extracts `p_name` and `p_email` from function signature.

But SpecQL generates a different pattern:

```sql
-- Pattern SpecQL actually generates (already in database)
CREATE TYPE app.type_create_contact_input AS (
    email TEXT,
    company_id UUID
);

CREATE FUNCTION app.create_contact(
    input_tenant_id UUID,      -- Context (from GraphQL context)
    input_user_id UUID,         -- Context (from GraphQL context)
    input_payload JSONB         -- Business input (maps to composite type)
) RETURNS app.mutation_result;
```

**Your goal**: Make AutoFraiseQL introspect the composite type (not function parameters) and auto-detect context parameters.

### What Changes

| Component | Current Behavior | New Behavior |
|-----------|------------------|--------------|
| **PostgresIntrospector** | Introspects views + functions | + Introspects composite types |
| **InputGenerator** | Reads function parameters | Detects JSONB → introspects composite type |
| **MutationGenerator** | No context detection | Auto-detects `input_tenant_id`, `input_user_id` |

### Context Parameter Convention

**New Convention** (per your feedback):
- `input_tenant_id` → `context["tenant_id"]`
- `input_user_id` → `context["user_id"]`

This is clearer than the legacy PrintOptim convention (`input_pk_organization`, `input_created_by`).

---

## Prerequisites

### Knowledge Requirements

- [x] Basic Python (dataclasses, async/await, type hints)
- [x] Basic SQL (SELECT, JOIN, WHERE)
- [x] PostgreSQL system catalogs (`pg_type`, `pg_class`, `pg_attribute`)
- [x] FraiseQL codebase structure (Phase 1-4 already complete)

### Files You'll Modify

```
src/fraiseql/introspection/
├── postgres_introspector.py    # Add composite type introspection
├── input_generator.py           # Add composite type detection
├── mutation_generator.py        # Add context parameter extraction
├── metadata_parser.py           # Add field metadata parsing
└── type_mapper.py              # (Minor updates if needed)

tests/
├── unit/introspection/
│   ├── test_postgres_introspector.py
│   ├── test_input_generator.py
│   ├── test_mutation_generator.py
│   └── test_metadata_parser.py
└── integration/introspection/
    └── test_composite_type_generation.py
```

### Test Database Access

**You need access to a database with SpecQL-generated schema.**

Two options:

#### Option A: Use Existing PrintOptim Database

```bash
# Connect to PrintOptim (already has SpecQL pattern)
export DATABASE_URL="postgresql://user:password@localhost:5432/printoptim"
```

Verify SpecQL schema exists:
```sql
-- Check for composite types
\dT app.type_*

-- Should see:
-- app.type_create_contact_input
-- app.type_create_organizational_unit_input
-- etc.
```

#### Option B: Create Test Database with SpecQL Pattern

If you don't have access to a SpecQL-generated database, create a minimal test schema:

```bash
# Create test database
createdb fraiseql_test

# Apply test schema
psql fraiseql_test < tests/fixtures/specql_test_schema.sql
```

**File**: `tests/fixtures/specql_test_schema.sql`

```sql
-- ============================================================================
-- TEST SCHEMA: Minimal SpecQL Pattern
-- This simulates what SpecQL would generate
-- ============================================================================

CREATE SCHEMA IF NOT EXISTS app;

-- 1. Composite input type (SpecQL-generated)
CREATE TYPE app.type_create_contact_input AS (
    email TEXT,
    company_id UUID,
    status TEXT
);

-- 2. Standard output type (SpecQL-generated, used by all mutations)
CREATE TYPE app.mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);

-- 3. App layer function (SpecQL-generated)
CREATE OR REPLACE FUNCTION app.create_contact(
    input_tenant_id UUID,
    input_user_id UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql
AS $$
DECLARE
    input_data app.type_create_contact_input;
BEGIN
    -- Parse JSONB into typed composite
    input_data := jsonb_populate_record(
        NULL::app.type_create_contact_input,
        input_payload
    );

    -- Stub implementation (real SpecQL would call core layer)
    RETURN ROW(
        gen_random_uuid(),
        ARRAY['email', 'company_id', 'status'],
        'success',
        'Contact created successfully',
        jsonb_build_object('email', input_data.email),
        NULL
    )::app.mutation_result;
END;
$$;

-- 4. FraiseQL metadata (SpecQL-generated comments)
COMMENT ON TYPE app.type_create_contact_input IS
    '@fraiseql:input name=CreateContactInput';

COMMENT ON FUNCTION app.create_contact IS
    '@fraiseql:mutation
name: createContact
description: Create a new contact
input_type: app.type_create_contact_input
success_type: Contact
failure_type: ContactError';

-- 5. Field-level metadata (SpecQL-generated)
COMMENT ON COLUMN app.type_create_contact_input.email IS
    '@fraiseql:field name=email,type=String!,required=true';

COMMENT ON COLUMN app.type_create_contact_input.company_id IS
    '@fraiseql:field name=companyId,type=UUID,required=false';

COMMENT ON COLUMN app.type_create_contact_input.status IS
    '@fraiseql:field name=status,type=String!,required=true';
```

**Note**: This test schema is **read-only** for your implementation. You'll only query it, never modify it.

---

## Phase Overview

### Phase 5.1: Composite Type Introspection (Foundation)
**Time**: 2-3 hours
**Goal**: Query PostgreSQL to discover composite types that already exist

**What you're doing**: Reading `pg_type` and `pg_attribute` catalogs to discover composite types like `app.type_create_contact_input`.

### Phase 5.2: Field Metadata Parsing
**Time**: 1-2 hours
**Goal**: Parse `@fraiseql:field` annotations from column comments

**What you're doing**: Reading comments on composite type columns and extracting metadata (required, type, etc.).

### Phase 5.3: Input Generation from Composite Types
**Time**: 2-3 hours
**Goal**: Generate GraphQL input types from composite types (not function parameters)

**What you're doing**: When you see `input_payload JSONB`, introspect the corresponding composite type and generate the input class from it.

### Phase 5.4: Context Parameter Auto-Detection
**Time**: 1-2 hours
**Goal**: Extract `input_tenant_id` and `input_user_id` from function signatures

**What you're doing**: Scanning function parameters and building the `context_params` mapping automatically.

### Phase 5.5: Integration and Testing
**Time**: 2-3 hours
**Goal**: End-to-end tests with real SpecQL schema

**What you're doing**: Running AutoDiscovery against a SpecQL database and verifying generated mutations match expectations.

---

## Detailed Implementation Steps

---

## 🔧 PHASE 5.1: Composite Type Introspection

**What you're introspecting**: Composite types that SpecQL already created in the database.

### Step 1.1: Add Data Classes

**File**: `src/fraiseql/introspection/postgres_introspector.py`

**Location**: Add these dataclasses at the top of the file, after existing imports and before the `PostgresIntrospector` class (around line 55, after `ParameterInfo`).

```python
from dataclasses import dataclass
from typing import Optional

# ... existing imports ...

# ADD THESE NEW DATACLASSES

@dataclass
class CompositeAttribute:
    """Metadata for a single attribute in a PostgreSQL composite type.

    This represents one field within a composite type that SpecQL created.

    Example:
        For composite type:
            CREATE TYPE app.type_create_contact_input AS (email TEXT, ...);

        This would represent the 'email' attribute.
    """

    name: str                    # Attribute name (e.g., "email")
    pg_type: str                 # PostgreSQL type (e.g., "text", "uuid")
    ordinal_position: int        # Position in type (1, 2, 3, ...)
    comment: Optional[str]       # Column comment (contains @fraiseql:field metadata)


@dataclass
class CompositeTypeMetadata:
    """Metadata for a PostgreSQL composite type that SpecQL created.

    This represents an entire composite type (e.g., app.type_create_contact_input)
    with all its attributes.
    """

    schema_name: str             # Schema (e.g., "app")
    type_name: str               # Type name (e.g., "type_create_contact_input")
    attributes: list[CompositeAttribute]  # List of attributes/fields
    comment: Optional[str]       # Type comment (contains @fraiseql:input metadata)
```

**Why**: These dataclasses hold the information we read from PostgreSQL about composite types that SpecQL created.

---

### Step 1.2: Add Composite Type Discovery Method

**File**: `src/fraiseql/introspection/postgres_introspector.py`

**Location**: Add this method inside the `PostgresIntrospector` class, after the `discover_functions` method (around line 200).

```python
class PostgresIntrospector:
    # ... existing methods ...

    async def discover_composite_type(
        self,
        type_name: str,
        schema: str = "app"
    ) -> CompositeTypeMetadata | None:
        """
        Introspect a PostgreSQL composite type that SpecQL created.

        This method reads the database to discover composite types and their
        attributes. It does NOT create or modify anything.

        Args:
            type_name: Name of the composite type (e.g., "type_create_contact_input")
            schema: Schema name (default: "app" - where SpecQL puts composite types)

        Returns:
            CompositeTypeMetadata if type exists, None if not found

        Example:
            >>> introspector = PostgresIntrospector(pool)
            >>> # This reads from database (doesn't create anything)
            >>> metadata = await introspector.discover_composite_type(
            ...     "type_create_contact_input",
            ...     schema="app"
            ... )
            >>> print(metadata.attributes[0].name)  # "email"
        """
        async with self.pool.connection() as conn:
            # Step 1: Check if composite type exists (query pg_type catalog)
            type_query = """
                SELECT
                    t.typname AS type_name,
                    n.nspname AS schema_name,
                    obj_description(t.oid, 'pg_type') AS comment
                FROM pg_type t
                JOIN pg_namespace n ON n.oid = t.typnamespace
                WHERE t.typtype = 'c'         -- 'c' = composite type
                  AND n.nspname = %s          -- Schema filter
                  AND t.typname = %s          -- Type name filter
            """

            type_result = await conn.execute(type_query, (schema, type_name))
            type_row = await type_result.fetchone()

            if not type_row:
                # Composite type doesn't exist in database
                # (SpecQL hasn't generated it or wrong name)
                return None

            # Step 2: Get all attributes (fields) of the composite type
            attr_query = """
                SELECT
                    a.attname AS attribute_name,
                    format_type(a.atttypid, a.atttypmod) AS pg_type,
                    a.attnum AS ordinal_position,
                    col_description(c.oid, a.attnum) AS comment
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                JOIN pg_attribute a ON a.attrelid = c.oid
                WHERE c.relkind = 'c'         -- 'c' = composite type
                  AND n.nspname = %s          -- Schema filter
                  AND c.relname = %s          -- Type name filter
                  AND a.attnum > 0            -- Exclude system columns
                  AND NOT a.attisdropped      -- Exclude dropped columns
                ORDER BY a.attnum;            -- Order by position
            """

            attr_result = await conn.execute(attr_query, (schema, type_name))
            attr_rows = await attr_result.fetchall()

            # Step 3: Build list of attributes from query results
            attributes = [
                CompositeAttribute(
                    name=row['attribute_name'],
                    pg_type=row['pg_type'],
                    ordinal_position=row['ordinal_position'],
                    comment=row['comment']
                )
                for row in attr_rows
            ]

            # Step 4: Return complete metadata
            return CompositeTypeMetadata(
                schema_name=schema,
                type_name=type_name,
                attributes=attributes,
                comment=type_row['comment']
            )
```

**What this does**:
1. Queries `pg_type` to check if the composite type exists (reads, doesn't create)
2. Queries `pg_attribute` to get all fields in the composite type
3. Returns a `CompositeTypeMetadata` object with all the info

**Important**: This method only **reads** from the database. It doesn't create or modify anything.

**To test manually**:
```python
# In a Python REPL or test file
import asyncio
import psycopg_pool
from fraiseql.introspection import PostgresIntrospector

async def test_discovery():
    pool = psycopg_pool.AsyncConnectionPool(
        conninfo="postgresql://user:password@localhost:5432/testdb"
    )

    introspector = PostgresIntrospector(pool)

    # This READS the composite type that SpecQL created
    metadata = await introspector.discover_composite_type(
        "type_create_contact_input",
        "app"
    )

    if metadata:
        print(f"✅ Found type: {metadata.type_name}")
        print(f"   Attributes: {len(metadata.attributes)}")
        for attr in metadata.attributes:
            print(f"   - {attr.name}: {attr.pg_type}")
    else:
        print("❌ Type not found (check if SpecQL created it)")

    await pool.close()

asyncio.run(test_discovery())
```

**Expected output**:
```
✅ Found type: type_create_contact_input
   Attributes: 3
   - email: text
   - company_id: uuid
   - status: text
```

---

### Step 1.3: Update __init__.py to Export New Classes

**File**: `src/fraiseql/introspection/__init__.py`

**Change**: Add the new classes to the imports and `__all__`:

```python
from .postgres_introspector import (
    FunctionMetadata,
    ParameterInfo,
    PostgresIntrospector,
    ViewMetadata,
    CompositeTypeMetadata,      # ADD THIS
    CompositeAttribute,         # ADD THIS
)

__all__ = [
    "AutoDiscovery",
    "InputGenerator",
    "MetadataParser",
    "MutationGenerator",
    "PostgresIntrospector",
    "QueryGenerator",
    "TypeGenerator",
    "TypeMapper",
    "CompositeTypeMetadata",     # ADD THIS
    "CompositeAttribute",        # ADD THIS
]
```

---

### Step 1.4: Write Unit Test for Phase 5.1

**File**: `tests/unit/introspection/test_postgres_introspector.py`

**Location**: Add this test at the end of the file.

```python
import pytest


@pytest.mark.asyncio
async def test_discover_composite_type(test_db_pool):
    """Test composite type introspection.

    This test verifies we can READ composite types that SpecQL created.
    """
    # Given: Introspector with test database (that has SpecQL schema)
    introspector = PostgresIntrospector(test_db_pool)

    # When: Discover composite type (READ operation)
    metadata = await introspector.discover_composite_type(
        "type_create_contact_input",
        schema="app"
    )

    # Then: Metadata is returned
    assert metadata is not None, "Composite type should exist (created by SpecQL)"
    assert metadata.type_name == "type_create_contact_input"
    assert metadata.schema_name == "app"

    # Then: Has 3 attributes (email, company_id, status)
    assert len(metadata.attributes) == 3

    # Then: First attribute is email
    email_attr = metadata.attributes[0]
    assert email_attr.name == "email"
    assert email_attr.pg_type == "text"
    assert email_attr.ordinal_position == 1

    # Then: Second attribute is company_id
    company_attr = metadata.attributes[1]
    assert company_attr.name == "company_id"
    assert company_attr.pg_type == "uuid"

    # Then: Attributes have comments (SpecQL puts @fraiseql:field in comments)
    assert email_attr.comment is not None
    assert "@fraiseql:field" in email_attr.comment


@pytest.mark.asyncio
async def test_discover_composite_type_not_found(test_db_pool):
    """Test composite type discovery with non-existent type.

    This verifies graceful handling when type doesn't exist.
    """
    # Given: Introspector
    introspector = PostgresIntrospector(test_db_pool)

    # When: Try to discover non-existent type
    metadata = await introspector.discover_composite_type(
        "type_nonexistent_input",
        schema="app"
    )

    # Then: Returns None (not an error)
    assert metadata is None
```

**Run test**:
```bash
uv run pytest tests/unit/introspection/test_postgres_introspector.py::test_discover_composite_type -v
```

**Expected**: ✅ Test passes (assuming test database has SpecQL schema)

---

## 🔧 PHASE 5.2: Field Metadata Parsing

**What you're parsing**: Comments that SpecQL already added to composite type columns.

### Step 2.1: Add Field Metadata Dataclass

**File**: `src/fraiseql/introspection/metadata_parser.py`

**Location**: Add this dataclass after existing dataclasses (around line 30).

```python
@dataclass
class FieldMetadata:
    """Parsed @fraiseql:field annotation from composite type column comment.

    SpecQL puts this metadata in column comments. We parse it to understand
    field requirements (required, type, etc.).

    Example comment:
        @fraiseql:field name=email,type=String!,required=true

    Parses to:
        FieldMetadata(name="email", graphql_type="String!", required=True, ...)
    """

    name: str                    # GraphQL field name (camelCase)
    graphql_type: str            # GraphQL type (e.g., "String!", "UUID")
    required: bool               # Is field required (non-null)?
    is_enum: bool = False        # Is this an enum type?
    description: Optional[str] = None
```

---

### Step 2.2: Add Field Metadata Parser Method

**File**: `src/fraiseql/introspection/metadata_parser.py`

**Location**: Add this method inside the `MetadataParser` class (around line 150).

```python
class MetadataParser:
    # ... existing methods ...

    def parse_field_annotation(self, comment: str | None) -> FieldMetadata | None:
        """
        Parse @fraiseql:field annotation from composite type column comment.

        SpecQL puts this metadata in column comments when generating composite types.
        We read and parse it.

        Format (created by SpecQL):
            @fraiseql:field name=email,type=String!,required=true

        Args:
            comment: Column comment string (from pg_attribute)

        Returns:
            FieldMetadata if annotation found, None otherwise

        Example:
            >>> parser = MetadataParser()
            >>> # This comment was created by SpecQL
            >>> comment = "@fraiseql:field name=email,type=String!,required=true"
            >>> metadata = parser.parse_field_annotation(comment)
            >>> metadata.name
            'email'
            >>> metadata.required
            True
        """
        if not comment or "@fraiseql:field" not in comment:
            return None

        # Extract key-value pairs from annotation
        # Format: @fraiseql:field name=email,type=String!,required=true

        # Find the @fraiseql:field line
        lines = comment.split('\n')
        field_line = next((line for line in lines if '@fraiseql:field' in line), None)

        if not field_line:
            return None

        # Remove '@fraiseql:field' prefix
        content = field_line.split('@fraiseql:field', 1)[1].strip()

        # Parse key=value pairs
        params = {}
        current_key = None
        current_value = []

        # Split by comma, but handle values that might contain commas
        parts = content.split(',')

        for part in parts:
            if '=' in part and not current_key:
                # New key=value pair
                key, value = part.split('=', 1)
                current_key = key.strip()
                current_value = [value.strip()]
            elif '=' in part and current_key:
                # Save previous key-value, start new one
                params[current_key] = ','.join(current_value)
                key, value = part.split('=', 1)
                current_key = key.strip()
                current_value = [value.strip()]
            else:
                # Continuation of previous value
                current_value.append(part.strip())

        # Save last key-value
        if current_key:
            params[current_key] = ','.join(current_value)

        # Build FieldMetadata from parsed params
        name = params.get('name', '')
        graphql_type = params.get('type', 'String')
        required = params.get('required', 'false').lower() == 'true'
        is_enum = params.get('enum', 'false').lower() == 'true'
        description = params.get('description')

        return FieldMetadata(
            name=name,
            graphql_type=graphql_type,
            required=required,
            is_enum=is_enum,
            description=description
        )
```

**What this does**:
- Reads comments that SpecQL created on composite type columns
- Parses the `@fraiseql:field` format
- Extracts field name, type, required flag, etc.
- Returns structured metadata

**Important**: You're only **reading** comments that SpecQL already created.

---

### Step 2.3: Write Unit Test for Phase 5.2

**File**: `tests/unit/introspection/test_metadata_parser.py`

**Location**: Add at the end of the file.

```python
def test_parse_field_annotation_basic():
    """Test parsing basic field annotation (created by SpecQL)."""
    # Given: Parser
    parser = MetadataParser()

    # Given: Field comment (as SpecQL creates it)
    comment = "@fraiseql:field name=email,type=String!,required=true"

    # When: Parse annotation
    metadata = parser.parse_field_annotation(comment)

    # Then: Metadata is parsed correctly
    assert metadata is not None
    assert metadata.name == "email"
    assert metadata.graphql_type == "String!"
    assert metadata.required is True
    assert metadata.is_enum is False


def test_parse_field_annotation_with_enum():
    """Test parsing field annotation with enum flag."""
    # Given: Field comment with enum (SpecQL creates this for enum fields)
    comment = "@fraiseql:field name=status,type=ContactStatus,required=true,enum=true"

    # When: Parse
    metadata = MetadataParser().parse_field_annotation(comment)

    # Then: Enum flag is set
    assert metadata.name == "status"
    assert metadata.is_enum is True


def test_parse_field_annotation_optional():
    """Test parsing optional field (required=false)."""
    # Given: Optional field (SpecQL marks nullable fields this way)
    comment = "@fraiseql:field name=companyId,type=UUID,required=false"

    # When: Parse
    metadata = MetadataParser().parse_field_annotation(comment)

    # Then: Required is False
    assert metadata.required is False


def test_parse_field_annotation_no_annotation():
    """Test parsing comment without @fraiseql:field."""
    # Given: Regular comment (not from SpecQL)
    comment = "This is just a regular comment"

    # When: Parse
    metadata = MetadataParser().parse_field_annotation(comment)

    # Then: Returns None
    assert metadata is None
```

**Run test**:
```bash
uv run pytest tests/unit/introspection/test_metadata_parser.py::test_parse_field_annotation_basic -v
```

**Expected**: ✅ All tests pass

---

## 🔧 PHASE 5.3: Input Generation from Composite Types

**What you're doing**: Reading composite types from the database and generating Python classes.

### Step 3.1: Update InputGenerator to Accept Introspector

**File**: `src/fraiseql/introspection/input_generator.py`

**Change 1**: Update the `__init__` method to store `metadata_parser`:

```python
from .metadata_parser import MetadataParser  # Add import

class InputGenerator:
    """Generate GraphQL input types from PostgreSQL function parameters."""

    def __init__(self, type_mapper: TypeMapper):
        self.type_mapper = type_mapper
        self.metadata_parser = MetadataParser()  # ADD THIS LINE
```

---

### Step 3.2: Add Composite Type Detection Method

**File**: `src/fraiseql/introspection/input_generator.py`

**Location**: Add this method after `__init__` (around line 25).

```python
    def _find_jsonb_input_parameter(self, function_metadata: FunctionMetadata) -> ParameterInfo | None:
        """
        Find the JSONB input parameter that maps to a composite type.

        SpecQL creates functions with this signature:
            app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB)

        We detect the 'input_payload JSONB' parameter.

        Args:
            function_metadata: Function metadata from introspection

        Returns:
            ParameterInfo if found, None otherwise

        Example:
            Function signature (created by SpecQL):
                app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB)

            Returns: ParameterInfo(name='input_payload', pg_type='jsonb', ...)
        """
        for param in function_metadata.parameters:
            # Check if parameter is JSONB and named 'input_payload'
            if param.pg_type.lower() == 'jsonb' and param.name == 'input_payload':
                return param

        return None
```

---

### Step 3.3: Add Composite Type Name Extraction

**File**: `src/fraiseql/introspection/input_generator.py`

**Location**: Add after `_find_jsonb_input_parameter`.

```python
    def _extract_composite_type_name(
        self,
        function_metadata: FunctionMetadata,
        annotation: MutationAnnotation
    ) -> str | None:
        """
        Extract composite type name from annotation or convention.

        SpecQL follows a naming convention:
            Function: app.create_contact
            Composite type: app.type_create_contact_input

        We can either:
        1. Read explicit annotation (if SpecQL added it)
        2. Use naming convention to guess

        Priority:
        1. Explicit annotation: @fraiseql:mutation input_type=app.type_contact_input
        2. Convention: create_contact → type_create_contact_input

        Args:
            function_metadata: Function metadata
            annotation: Parsed mutation annotation

        Returns:
            Composite type name (without schema prefix) or None

        Example:
            function_name = "create_contact"
            → returns "type_create_contact_input"
        """
        # Priority 1: Check for explicit input_type in annotation (if SpecQL added it)
        if hasattr(annotation, 'input_type') and annotation.input_type:
            # Extract type name from fully qualified name
            # "app.type_contact_input" → "type_contact_input"
            if '.' in annotation.input_type:
                return annotation.input_type.split('.')[-1]
            return annotation.input_type

        # Priority 2: Convention-based extraction (SpecQL naming pattern)
        function_name = function_metadata.function_name

        # Convention: create_contact → type_create_contact_input
        return f"type_{function_name}_input"
```

---

### Step 3.4: Add Composite Type Input Generation Method

**File**: `src/fraiseql/introspection/input_generator.py`

**Location**: Add after `_extract_composite_type_name`.

```python
    async def _generate_from_composite_type(
        self,
        composite_type_name: str,
        schema_name: str,
        introspector: "PostgresIntrospector"
    ) -> Type:
        """
        Generate input class from PostgreSQL composite type (created by SpecQL).

        This method READS the composite type from the database and generates
        a Python class. It does NOT create or modify the database.

        Steps:
        1. Introspect composite type to get attributes (READ from database)
        2. Parse field metadata from column comments (READ comments SpecQL created)
        3. Map PostgreSQL types to Python types
        4. Create input class with proper annotations

        Args:
            composite_type_name: Name of composite type (e.g., "type_create_contact_input")
            schema_name: Schema where function is defined (will check "app" for type)
            introspector: PostgresIntrospector instance

        Returns:
            Dynamically created input class

        Example:
            Composite type (created by SpecQL):
                CREATE TYPE app.type_create_contact_input AS (
                    email TEXT,
                    company_id UUID,
                    status TEXT
                );

            Generates Python class:
                class CreateContactInput:
                    email: str
                    companyId: UUID  # Note: camelCase from metadata
                    status: str
        """
        # Step 1: Introspect composite type (READ from database)
        # SpecQL creates types in 'app' schema
        composite_metadata = await introspector.discover_composite_type(
            composite_type_name,
            schema="app"
        )

        if not composite_metadata:
            raise ValueError(
                f"Composite type '{composite_type_name}' not found in 'app' schema. "
                f"Expected by function '{schema_name}.{composite_type_name}'. "
                f"Check if SpecQL created this type."
            )

        # Step 2: Build annotations from composite type attributes
        annotations = {}

        for attr in composite_metadata.attributes:
            # Step 2a: Parse field metadata from comment (SpecQL puts metadata here)
            field_metadata = None
            if attr.comment:
                field_metadata = self.metadata_parser.parse_field_annotation(attr.comment)

            # Step 2b: Determine field name
            # Use metadata name (camelCase) if available, otherwise use attribute name
            field_name = field_metadata.name if field_metadata else attr.name

            # Step 2c: Map PostgreSQL type to Python type
            # Check if field is required (from SpecQL metadata)
            nullable = not field_metadata.required if field_metadata else True

            python_type = self.type_mapper.pg_type_to_python(
                attr.pg_type,
                nullable=nullable
            )

            # Step 2d: Add to annotations
            annotations[field_name] = python_type

        # Step 3: Generate class name from composite type name
        # "type_create_contact_input" → "CreateContactInput"
        class_name = self._composite_type_to_class_name(composite_type_name)

        # Step 4: Create input class dynamically
        input_cls = type(class_name, (object,), {"__annotations__": annotations})

        return input_cls

    def _composite_type_to_class_name(self, composite_type_name: str) -> str:
        """
        Convert composite type name to GraphQL input class name.

        SpecQL naming convention:
            type_create_contact_input → CreateContactInput

        Example:
            "type_create_contact_input" → "CreateContactInput"
        """
        # Remove "type_" prefix
        name = composite_type_name.replace("type_", "")

        # Remove "_input" suffix (we'll add it back as "Input")
        name = name.replace("_input", "")

        # Split by underscore and capitalize
        parts = name.split("_")
        class_name = "".join(part.capitalize() for part in parts)

        # Add "Input" suffix
        return f"{class_name}Input"
```

---

### Step 3.5: Update Main generate_input_type Method

**File**: `src/fraiseql/introspection/input_generator.py`

**Change**: Replace the existing `generate_input_type` method with this enhanced version:

```python
    async def generate_input_type(
        self,
        function_metadata: FunctionMetadata,
        annotation: MutationAnnotation,
        introspector: "PostgresIntrospector"
    ) -> Type:
        """
        Generate input class for mutation.

        Strategy:
        1. Look for JSONB parameter (SpecQL pattern: input_payload)
        2. If found, extract composite type name and introspect it (READ from DB)
        3. Otherwise, fall back to parameter-based generation (legacy)

        Args:
            function_metadata: Metadata from function introspection
            annotation: Parsed @fraiseql:mutation annotation
            introspector: PostgresIntrospector for composite type discovery

        Returns:
            Dynamically created input class

        Example (SpecQL pattern):
            Function (created by SpecQL):
                app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB)

            Generates from composite type (reads from DB):
                class CreateContactInput:
                    email: str
                    companyId: UUID | None
                    status: str

        Example (Legacy pattern):
            Function:
                fn_create_user(p_name TEXT, p_email TEXT)

            Generates from parameters:
                class CreateUserInput:
                    name: str
                    email: str
        """
        # STRATEGY 1: Try composite type-based generation (SpecQL pattern)
        jsonb_param = self._find_jsonb_input_parameter(function_metadata)

        if jsonb_param:
            # Found JSONB parameter → SpecQL pattern detected
            composite_type_name = self._extract_composite_type_name(
                function_metadata,
                annotation
            )

            if composite_type_name:
                try:
                    return await self._generate_from_composite_type(
                        composite_type_name,
                        function_metadata.schema_name,
                        introspector
                    )
                except ValueError as e:
                    # Composite type not found, fall back to parameter-based
                    logger.warning(
                        f"Composite type generation failed for {function_metadata.function_name}: {e}. "
                        f"Falling back to parameter-based generation."
                    )

        # STRATEGY 2: Fall back to parameter-based generation (legacy)
        return self._generate_from_parameters(function_metadata, annotation)

    def _generate_from_parameters(
        self,
        function_metadata: FunctionMetadata,
        annotation: MutationAnnotation
    ) -> Type:
        """
        Generate input class from function parameters (legacy pattern).

        This is the original implementation for backward compatibility.
        """
        class_name = self._function_to_input_name(function_metadata.function_name)

        annotations = {}
        for param in function_metadata.parameters:
            # Skip context parameters
            if param.name.startswith('input_tenant_') or param.name.startswith('input_user_'):
                continue
            if param.name == 'input_payload':
                continue

            # Skip output parameters
            if param.mode != "IN":
                continue

            # Map parameter to input field
            field_name = param.name.replace("p_", "")  # Remove p_ prefix
            python_type = self.type_mapper.pg_type_to_python(
                param.pg_type,
                nullable=(param.default_value is not None)
            )
            annotations[field_name] = python_type

        # Create input class
        input_cls = type(class_name, (object,), {"__annotations__": annotations})

        return input_cls
```

---

### Step 3.6: Add Missing Import

**File**: `src/fraiseql/introspection/input_generator.py`

**Location**: At the top of the file, add:

```python
import logging
from typing import TYPE_CHECKING, Type

from .metadata_parser import MutationAnnotation, MetadataParser
from .postgres_introspector import FunctionMetadata, ParameterInfo
from .type_mapper import TypeMapper

if TYPE_CHECKING:
    from .postgres_introspector import PostgresIntrospector

logger = logging.getLogger(__name__)
```

---

### Step 3.7: Write Unit Test for Phase 5.3

**File**: `tests/unit/introspection/test_input_generator.py`

**Location**: Add at the end of the file.

```python
import pytest
from fraiseql.introspection import (
    InputGenerator,
    TypeMapper,
    PostgresIntrospector,
    FunctionMetadata,
    ParameterInfo,
    MutationAnnotation,
)


@pytest.mark.asyncio
async def test_generate_input_from_composite_type(test_db_pool):
    """Test input generation from composite type (SpecQL pattern).

    This test verifies we can READ a composite type that SpecQL created
    and generate a Python class from it.
    """
    # Given: InputGenerator and introspector
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    introspector = PostgresIntrospector(test_db_pool)

    # Given: Function with JSONB parameter (as SpecQL creates it)
    function = FunctionMetadata(
        schema_name="app",
        function_name="create_contact",
        parameters=[
            ParameterInfo("input_tenant_id", "uuid", "IN", None),
            ParameterInfo("input_user_id", "uuid", "IN", None),
            ParameterInfo("input_payload", "jsonb", "IN", None),
        ],
        return_type="app.mutation_result",
        comment="@fraiseql:mutation ...",
        language="plpgsql"
    )

    # Given: Annotation
    annotation = MutationAnnotation(
        name="createContact",
        description="Create contact",
        success_type="Contact",
        failure_type="ContactError"
    )

    # When: Generate input type (READS composite type from database)
    input_cls = await input_generator.generate_input_type(
        function,
        annotation,
        introspector
    )

    # Then: Class name is correct
    assert input_cls.__name__ == "CreateContactInput"

    # Then: Has fields from composite type (that SpecQL created)
    assert "email" in input_cls.__annotations__
    assert "companyId" in input_cls.__annotations__  # camelCase from SpecQL metadata
    assert "status" in input_cls.__annotations__

    # Then: Types are correct
    assert input_cls.__annotations__["email"] == str


@pytest.mark.asyncio
async def test_generate_input_from_parameters_legacy(test_db_pool):
    """Test input generation from parameters (legacy pattern).

    Verifies backward compatibility with non-SpecQL functions.
    """
    # Given: InputGenerator
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    introspector = PostgresIntrospector(test_db_pool)

    # Given: Function with simple parameters (no JSONB)
    function = FunctionMetadata(
        schema_name="public",
        function_name="fn_create_user",
        parameters=[
            ParameterInfo("p_name", "text", "IN", None),
            ParameterInfo("p_email", "text", "IN", None),
        ],
        return_type="uuid",
        comment=None,
        language="plpgsql"
    )

    # Given: Annotation
    annotation = MutationAnnotation(
        name="createUser",
        description=None,
        success_type="User",
        failure_type="UserError"
    )

    # When: Generate input type
    input_cls = await input_generator.generate_input_type(
        function,
        annotation,
        introspector
    )

    # Then: Falls back to parameter-based generation
    assert input_cls.__name__ == "CreateUserInput"
    assert "name" in input_cls.__annotations__
    assert "email" in input_cls.__annotations__
```

**Run test**:
```bash
uv run pytest tests/unit/introspection/test_input_generator.py::test_generate_input_from_composite_type -v
```

**Expected**: ✅ Test passes (assuming test database has SpecQL schema)

---

## 🔧 PHASE 5.4: Context Parameter Auto-Detection

**What you're doing**: Reading function parameters and extracting context param names.

### Step 4.1: Add Context Parameter Extraction Method

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Location**: Add this method inside the `MutationGenerator` class (around line 75).

```python
    def _extract_context_params(
        self,
        function_metadata: FunctionMetadata
    ) -> dict[str, str]:
        """
        Auto-detect context parameters from function signature (created by SpecQL).

        SpecQL creates functions with context parameters:
            app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB)

        We detect these and build the context_params mapping.

        Convention:
            input_tenant_id UUID   → context["tenant_id"]
            input_user_id UUID     → context["user_id"]

        Args:
            function_metadata: Function metadata from introspection (READ from DB)

        Returns:
            Mapping of context_key → function_parameter_name

        Example:
            Function signature (created by SpecQL):
                app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB)

            Returns:
                {
                    "tenant_id": "input_tenant_id",
                    "user_id": "input_user_id"
                }
        """
        context_params = {}

        for param in function_metadata.parameters:
            # Pattern 1: input_tenant_id → tenant_id
            if param.name == 'input_tenant_id':
                context_params['tenant_id'] = param.name

            # Pattern 2: input_user_id → user_id
            elif param.name == 'input_user_id':
                context_params['user_id'] = param.name

            # Legacy patterns (for backward compatibility with PrintOptim)
            # input_pk_organization → organization_id
            elif param.name.startswith('input_pk_'):
                context_key = param.name.replace('input_pk_', '') + '_id'
                context_params[context_key] = param.name

            # input_created_by → user_id (legacy)
            elif param.name == 'input_created_by':
                if 'user_id' not in context_params:  # Don't override input_user_id
                    context_params['user_id'] = param.name

        return context_params
```

**What this does**:
- Reads function parameters (that SpecQL created)
- Detects context parameters (`input_tenant_id`, `input_user_id`)
- Builds a mapping for the `@fraiseql.mutation(context_params={...})` decorator
- Supports legacy patterns for backward compatibility

---

### Step 4.2: Update Mutation Generation to Use Context Params

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Change**: Update the `generate_mutation_for_function` method signature and add context param extraction.

Find the existing method (around line 23) and update it:

```python
    async def generate_mutation_for_function(  # ADD async
        self,
        function_metadata: FunctionMetadata,
        annotation: MutationAnnotation,
        type_registry: dict[str, Type],
        introspector: "PostgresIntrospector"  # ADD THIS PARAMETER
    ) -> Callable | None:
        """
        Generate mutation from function (created by SpecQL).

        This method READS function metadata and generates Python code.
        It does NOT create or modify the database.

        Steps:
        1. Generate input type (from composite type that SpecQL created)
        2. Resolve success/failure types
        3. Extract context parameters (READ from function signature)
        4. Create mutation function
        5. Apply @fraiseql.mutation decorator

        Args:
            function_metadata: Metadata from function introspection (READ from DB)
            annotation: Parsed @fraiseql:mutation annotation
            type_registry: Registry of available types
            introspector: PostgresIntrospector for composite type discovery (NEW)

        Returns:
            Decorated mutation function or None if generation fails
        """
        # 1. Generate input type (READS composite type from DB)
        input_cls = await self.input_generator.generate_input_type(
            function_metadata,
            annotation,
            introspector  # PASS INTROSPECTOR
        )

        # 2. Get success/failure types
        success_type = type_registry.get(annotation.success_type)
        failure_type = type_registry.get(annotation.failure_type)

        if not success_type or not failure_type:
            logger.warning(
                f"Cannot generate mutation {function_metadata.function_name}: "
                f"missing types {annotation.success_type} or {annotation.failure_type}"
            )
            return None

        # 3. Extract context parameters (NEW - auto-detect from function)
        context_params = self._extract_context_params(function_metadata)

        # 4. Create mutation class dynamically
        mutation_class = self._create_mutation_class(
            function_metadata,
            annotation,
            input_cls,
            success_type,
            failure_type
        )

        # 5. Apply @mutation decorator with context params
        from fraiseql import mutation

        decorated_mutation = mutation(
            mutation_class,
            function=function_metadata.function_name,
            schema=function_metadata.schema_name,
            context_params=context_params,  # ADD THIS
        )

        return decorated_mutation
```

**Add import at top of file**:
```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .postgres_introspector import PostgresIntrospector
```

---

### Step 4.3: Update AutoDiscovery to Pass Introspector

**File**: `src/fraiseql/introspection/auto_discovery.py`

**Change**: Update the `_generate_mutation_from_function` method to pass introspector:

```python
    async def _generate_mutation_from_function(
        self, function_metadata: FunctionMetadata
    ) -> Callable | None:
        """Generate a mutation from function metadata (SpecQL function).

        This method READS function metadata and delegates to MutationGenerator.
        It does NOT create or modify the database.
        """
        # Parse @fraiseql:mutation annotation (SpecQL adds this)
        annotation = self.metadata_parser.parse_mutation_annotation(function_metadata.comment)
        if not annotation:
            return None

        # Generate mutation (READS composite type from DB)
        try:
            mutation = await self.mutation_generator.generate_mutation_for_function(
                function_metadata,
                annotation,
                self.type_registry,
                self.introspector  # ADD THIS: Pass introspector for composite type discovery
            )

            logger.debug(f"Generated mutation: {mutation.__name__}")
            return mutation

        except Exception as e:
            logger.warning(
                f"Failed to generate mutation from function {function_metadata.function_name}: {e}"
            )
            return None
```

---

### Step 4.4: Write Unit Test for Phase 5.4

**File**: `tests/unit/introspection/test_mutation_generator.py`

**Location**: Add at the end of the file.

```python
import pytest
from fraiseql.introspection import (
    MutationGenerator,
    InputGenerator,
    TypeMapper,
    FunctionMetadata,
    ParameterInfo,
)


def test_extract_context_params_new_convention():
    """Test context parameter extraction with new convention.

    This verifies we can READ context params from SpecQL functions.
    """
    # Given: MutationGenerator
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)

    # Given: Function with new convention context params (SpecQL pattern)
    function = FunctionMetadata(
        schema_name="app",
        function_name="create_contact",
        parameters=[
            ParameterInfo("input_tenant_id", "uuid", "IN", None),
            ParameterInfo("input_user_id", "uuid", "IN", None),
            ParameterInfo("input_payload", "jsonb", "IN", None),
        ],
        return_type="app.mutation_result",
        comment=None,
        language="plpgsql"
    )

    # When: Extract context params (READ from function metadata)
    context_params = mutation_generator._extract_context_params(function)

    # Then: Correct mapping
    assert context_params == {
        "tenant_id": "input_tenant_id",
        "user_id": "input_user_id"
    }


def test_extract_context_params_legacy_convention():
    """Test context parameter extraction with legacy convention.

    Verifies backward compatibility with PrintOptim pattern.
    """
    # Given: MutationGenerator
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)

    # Given: Function with legacy convention
    function = FunctionMetadata(
        schema_name="app",
        function_name="create_organizational_unit",
        parameters=[
            ParameterInfo("input_pk_organization", "uuid", "IN", None),
            ParameterInfo("input_created_by", "uuid", "IN", None),
            ParameterInfo("input_payload", "jsonb", "IN", None),
        ],
        return_type="app.mutation_result",
        comment=None,
        language="plpgsql"
    )

    # When: Extract context params
    context_params = mutation_generator._extract_context_params(function)

    # Then: Legacy mapping still works
    assert context_params == {
        "organization_id": "input_pk_organization",
        "user_id": "input_created_by"
    }


def test_extract_context_params_no_context():
    """Test context parameter extraction with no context params."""
    # Given: Function without context parameters (legacy non-SpecQL function)
    function = FunctionMetadata(
        schema_name="public",
        function_name="fn_simple_mutation",
        parameters=[
            ParameterInfo("p_name", "text", "IN", None),
            ParameterInfo("p_value", "integer", "IN", None),
        ],
        return_type="uuid",
        comment=None,
        language="plpgsql"
    )

    # When: Extract context params
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)
    context_params = mutation_generator._extract_context_params(function)

    # Then: Empty dict (no context params)
    assert context_params == {}
```

**Run test**:
```bash
uv run pytest tests/unit/introspection/test_mutation_generator.py::test_extract_context_params_new_convention -v
```

**Expected**: ✅ All tests pass

---

## 🔧 PHASE 5.5: Integration and E2E Testing

**What you're doing**: Testing the full pipeline against a real SpecQL-generated database.

### Step 5.1: Create Integration Test

**File**: `tests/integration/introspection/test_composite_type_generation_integration.py`

**Location**: Create new file.

```python
"""Integration tests for composite type-based mutation generation.

These tests verify AutoFraiseQL can READ a SpecQL-generated database
and generate mutations correctly.

IMPORTANT: These tests assume a SpecQL-generated schema exists in the database.
"""

import pytest
from fraiseql.introspection import AutoDiscovery


@pytest.fixture
async def specql_test_schema_exists(test_db_pool):
    """
    Verify SpecQL test schema exists in database.

    This fixture does NOT create the schema - it only checks if it exists.
    The schema should be created by:
    1. Running SpecQL to generate it, OR
    2. Manually applying tests/fixtures/specql_test_schema.sql
    """
    async with test_db_pool.connection() as conn:
        # Check if composite type exists
        result = await conn.execute("""
            SELECT EXISTS (
                SELECT 1 FROM pg_type t
                JOIN pg_namespace n ON n.oid = t.typnamespace
                WHERE n.nspname = 'app'
                  AND t.typname = 'type_create_contact_input'
            )
        """)
        exists = await result.fetchone()

        if not exists[0]:
            pytest.skip("SpecQL test schema not found - run SpecQL or apply test schema SQL")

    yield


@pytest.mark.asyncio
async def test_end_to_end_composite_type_generation(test_db_pool, specql_test_schema_exists):
    """Test complete flow from database to generated mutation.

    This test READS a SpecQL-generated database and verifies AutoFraiseQL
    can generate mutations correctly.
    """
    # Given: AutoDiscovery with SpecQL schema (already in database)
    auto_discovery = AutoDiscovery(test_db_pool)

    # When: Discover all mutations (READ from database)
    result = await auto_discovery.discover_all(
        view_pattern="v_%",
        function_pattern="%",  # Discover all functions
        schemas=["app"]
    )

    # Then: Mutation was discovered
    assert len(result['mutations']) > 0, "Should find at least one mutation"

    # Find the create_contact mutation
    create_contact = next(
        (m for m in result['mutations'] if hasattr(m, '__name__') and 'createContact' in m.__name__),
        None
    )
    assert create_contact is not None, "createContact mutation should be generated"


@pytest.mark.asyncio
async def test_context_params_auto_detection(test_db_pool, specql_test_schema_exists):
    """Test that context parameters are automatically detected.

    Verifies that input_tenant_id and input_user_id are auto-detected
    from SpecQL function signatures.
    """
    # Given: AutoDiscovery
    auto_discovery = AutoDiscovery(test_db_pool)

    # When: Discover mutations (READ from database)
    result = await auto_discovery.discover_all(schemas=["app"])

    # Then: Mutations should be discovered
    assert result is not None
    assert len(result['mutations']) > 0

    # Note: Detailed assertion about context_params depends on
    # how @fraiseql.mutation exposes this information
    # You may need to add assertions here based on actual mutation structure
```

**Run integration test**:
```bash
# Make sure test database has SpecQL schema first!
psql fraiseql_test < tests/fixtures/specql_test_schema.sql

# Then run test
uv run pytest tests/integration/introspection/test_composite_type_generation_integration.py -v
```

**Expected**: ✅ Tests pass

---

### Step 5.2: Manual End-to-End Validation

**File**: Create `examples/test_composite_type_discovery.py`

```python
"""Manual test to verify composite type discovery works with real SpecQL database.

Usage:
    python examples/test_composite_type_discovery.py
"""

import asyncio
import os
import psycopg_pool
from fraiseql.introspection import AutoDiscovery


async def main():
    # Connect to database with SpecQL schema
    database_url = os.getenv(
        "DATABASE_URL",
        "postgresql://user:password@localhost:5432/printoptim"
    )

    print(f"🔌 Connecting to: {database_url}")

    connection_pool = psycopg_pool.AsyncConnectionPool(conninfo=database_url)

    # Initialize AutoDiscovery
    auto_discovery = AutoDiscovery(connection_pool)

    print("🔍 Discovering schema...")

    # Discover all (READ from database)
    result = await auto_discovery.discover_all(
        schemas=["app"]  # SpecQL puts things in 'app' schema
    )

    # Print results
    print(f"\n✅ Discovered {len(result['types'])} types")
    print(f"✅ Discovered {len(result['queries'])} queries")
    print(f"✅ Discovered {len(result['mutations'])} mutations")

    # Print mutation details
    if result['mutations']:
        print("\n📝 Mutations:")
        for mutation in result['mutations']:
            print(f"   - {mutation}")
    else:
        print("\n⚠️  No mutations discovered - check if functions exist in 'app' schema")

    await connection_pool.close()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except Exception as e:
        print(f"\n❌ Error: {e}")
        print("\nTroubleshooting:")
        print("1. Check DATABASE_URL is correct")
        print("2. Verify SpecQL schema exists: \\dT app.type_* in psql")
        print("3. Check functions exist: \\df app.* in psql")
```

**Run**:
```bash
# With PrintOptim database
DATABASE_URL="postgresql://user:password@localhost:5432/printoptim" python examples/test_composite_type_discovery.py

# With test database
DATABASE_URL="postgresql://user:password@localhost:5432/fraiseql_test" python examples/test_composite_type_discovery.py
```

**Expected output**:
```
🔌 Connecting to: postgresql://...
🔍 Discovering schema...

✅ Discovered 0 types
✅ Discovered 0 queries
✅ Discovered 5 mutations

📝 Mutations:
   - <function createContact at 0x...>
   - <function createOrganizationalUnit at 0x...>
   - ...
```

---

## Testing Strategy

### Unit Tests (Fast, Isolated)

**Run all unit tests**:
```bash
uv run pytest tests/unit/introspection/ -v --tb=short
```

**What's tested**:
- ✅ Composite type SQL queries return correct data
- ✅ Field metadata parsing extracts all fields
- ✅ Input type generation creates correct classes
- ✅ Context parameter extraction builds correct mapping
- ✅ Naming convention conversions work

---

### Integration Tests (Real Database)

**Run integration tests**:
```bash
# Ensure test database has SpecQL schema
psql fraiseql_test < tests/fixtures/specql_test_schema.sql

# Run tests
uv run pytest tests/integration/introspection/ -v --tb=short
```

**What's tested**:
- ✅ End-to-end discovery pipeline (real database)
- ✅ Generated mutations match expectations
- ✅ Works with actual SpecQL-generated schema

---

### Manual Testing (PrintOptim Database)

**Connect to PrintOptim**:
```bash
DATABASE_URL="postgresql://user:password@localhost:5432/printoptim" python examples/test_composite_type_discovery.py
```

**Validation checklist**:
- [ ] Discovers all mutations
- [ ] Generated input types match composite types
- [ ] Context params auto-detected correctly
- [ ] No errors in logs

---

## Validation Checklist

### Phase 5.1: Composite Type Introspection ✅
- [ ] `discover_composite_type()` returns correct metadata
- [ ] Handles non-existent types gracefully (returns None)
- [ ] Attributes are in correct order (ordinal_position)
- [ ] Column comments are retrieved
- [ ] **Only READS from database, never writes**

### Phase 5.2: Field Metadata Parsing ✅
- [ ] Parses `@fraiseql:field` annotations correctly
- [ ] Extracts name, type, required, enum flags
- [ ] Handles missing annotations (returns None)
- [ ] Handles malformed annotations gracefully
- [ ] **Only PARSES comments, never writes them**

### Phase 5.3: Input Generation ✅
- [ ] Detects JSONB `input_payload` parameter
- [ ] Extracts composite type name from convention
- [ ] Introspects composite type and generates input class
- [ ] Falls back to parameter-based generation when no JSONB
- [ ] Generated class name matches convention (CreateContactInput)
- [ ] **Only READS composite types, never creates them**

### Phase 5.4: Context Parameter Detection ✅
- [ ] Detects `input_tenant_id` → `tenant_id`
- [ ] Detects `input_user_id` → `user_id`
- [ ] Supports legacy `input_pk_*` pattern
- [ ] Supports legacy `input_created_by` pattern
- [ ] Returns empty dict when no context params
- [ ] **Only READS function parameters, never modifies them**

### Phase 5.5: Integration ✅
- [ ] Full discovery pipeline works end-to-end
- [ ] Generated mutations match hand-written equivalents
- [ ] Works with real PrintOptim/SpecQL schema
- [ ] No breaking changes to existing functionality
- [ ] **Never creates or modifies database objects**

---

## Common Issues and Solutions

### Issue 1: "Composite type not found"

**Symptom**:
```
ValueError: Composite type 'type_create_contact_input' not found in 'app' schema
```

**Root cause**: Composite type doesn't exist in database (SpecQL hasn't created it).

**Solution**:
1. Verify type exists: `\dT app.type_*` in psql
2. Check function name convention: `create_contact` → `type_create_contact_input`
3. If SpecQL uses different naming, add explicit annotation:
```sql
COMMENT ON FUNCTION app.create_contact IS '@fraiseql:mutation
input_type: app.type_contact_input_v2';
```

---

### Issue 2: "Column comments not retrieved"

**Symptom**:
```python
attr.comment is None  # Expected @fraiseql:field annotation
```

**Root cause**: SpecQL didn't add comments, or comment syntax is wrong.

**Solution**:
1. Check if SpecQL added comments: `\d+ app.type_create_contact_input` in psql
2. Verify SpecQL version supports field annotations
3. For testing, manually add comments:
```sql
COMMENT ON COLUMN app.type_create_contact_input.email IS '@fraiseql:field name=email,type=String!,required=true';
```

---

### Issue 3: "Async/await errors"

**Symptom**:
```
RuntimeWarning: coroutine 'generate_input_type' was never awaited
```

**Root cause**: Forgot to `await` async methods.

**Solution**:
```python
# ✅ Correct
input_cls = await input_generator.generate_input_type(...)

# ❌ Wrong
input_cls = input_generator.generate_input_type(...)
```

**Make sure**:
- `generate_input_type` is marked `async`
- `generate_mutation_for_function` is marked `async`
- All calls to these methods use `await`

---

### Issue 4: "Test database doesn't have SpecQL schema"

**Symptom**:
```
pytest.skip("SpecQL test schema not found")
```

**Root cause**: Test database missing composite types.

**Solution**:
```bash
# Apply test schema
psql fraiseql_test < tests/fixtures/specql_test_schema.sql

# Verify
psql fraiseql_test -c "\dT app.type_*"
```

---

## Performance Considerations

### Caching Strategy

**Problem**: Introspecting composite types on every mutation generation is slow.

**Solution**: Cache composite type metadata (this is read-only data).

```python
class AutoDiscovery:
    def __init__(self, connection_pool):
        # ...
        self._composite_type_cache: dict[str, CompositeTypeMetadata] = {}

    async def _get_composite_type_cached(
        self,
        type_name: str,
        schema: str
    ) -> CompositeTypeMetadata:
        """Get composite type with caching (READ-only operation)."""
        cache_key = f"{schema}.{type_name}"

        if cache_key not in self._composite_type_cache:
            # Read from database (cache miss)
            metadata = await self.introspector.discover_composite_type(type_name, schema)
            self._composite_type_cache[cache_key] = metadata

        return self._composite_type_cache[cache_key]
```

**When to invalidate**:
- Never during runtime (SpecQL generates schema at migration time)
- Only on server restart
- Or when explicitly requested via API

---

## Final Checklist

### Before Starting
- [ ] Read this entire document
- [ ] Have access to SpecQL-generated database (PrintOptim or test)
- [ ] Understand you're only READING database, never writing
- [ ] Existing Phase 1-4 code is working

### Phase 5.1 Complete
- [ ] Can introspect composite types from database
- [ ] Unit tests pass
- [ ] Manual test with real database works

### Phase 5.2 Complete
- [ ] Can parse field metadata from comments
- [ ] Unit tests pass
- [ ] Handles missing/malformed annotations

### Phase 5.3 Complete
- [ ] Can generate input classes from composite types
- [ ] Falls back to parameter-based for legacy functions
- [ ] Unit tests pass
- [ ] Integration test with real schema works

### Phase 5.4 Complete
- [ ] Can auto-detect context parameters
- [ ] Supports both new and legacy conventions
- [ ] Unit tests pass

### Phase 5.5 Complete
- [ ] End-to-end test passes
- [ ] Manual validation with PrintOptim successful
- [ ] No regression in existing functionality
- [ ] Documentation updated

### Production Ready
- [ ] All tests pass: `uv run pytest`
- [ ] Works with real SpecQL-generated schema
- [ ] Performance acceptable
- [ ] CHANGELOG updated
- [ ] README updated

---

## Success Metrics

**Definition of Done**:
1. ✅ All unit tests pass
2. ✅ All integration tests pass with SpecQL schema
3. ✅ Can discover and generate mutations from PrintOptim database
4. ✅ Generated mutations work correctly (no runtime errors)
5. ✅ No breaking changes to existing functionality
6. ✅ **Never creates or modifies database objects**

---

## Key Reminders

### ⚠️ YOU ARE ONLY READING THE DATABASE

- ✅ **DO**: Query `pg_type`, `pg_class`, `pg_attribute` catalogs
- ✅ **DO**: Read composite types, functions, comments
- ✅ **DO**: Parse metadata and generate Python code
- ❌ **DON'T**: Create types, functions, or comments
- ❌ **DON'T**: Modify database in any way
- ❌ **DON'T**: Execute DDL statements (CREATE, ALTER, DROP)

### 💡 What SpecQL Creates (You Just Read)

SpecQL generates:
- Composite types (`CREATE TYPE app.type_*_input`)
- Functions (`CREATE FUNCTION app.*`)
- Comments (`COMMENT ON TYPE/FUNCTION/COLUMN`)

You generate:
- Python classes (input types, mutations)
- GraphQL schema
- Decorator calls

---

## Congratulations! 🎉

Once complete, you'll have:
- ✅ Full composite type support in AutoFraiseQL
- ✅ Automatic context parameter detection
- ✅ Zero manual code for SpecQL-generated schemas
- ✅ Backward compatibility with existing patterns
- ✅ **100% read-only introspection (never touches database)**

**You've built a production-ready meta-framework feature that reads SpecQL schemas!**
