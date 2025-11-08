# FraiseQL Phase 5: Composite Type Input Generation - Implementation Plan

**Status**: Ready for Implementation
**Priority**: High
**Complexity**: Medium
**Estimated Time**: 8-12 hours
**Target Agent**: Junior/Mid-level Developer (Step-by-step guidance)

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
-- Current assumption
CREATE FUNCTION fn_create_user(p_name TEXT, p_email TEXT) ...
```

But real enterprise PostgreSQL (SpecQL pattern) uses **composite types** with **context injection**:

```sql
-- Real pattern
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

**Your goal**: Make FraiseQL introspect the composite type and auto-detect context parameters.

### What Changes

| Component | Current Behavior | New Behavior |
|-----------|------------------|--------------|
| **InputGenerator** | Reads function parameters | Detects JSONB → introspects composite type |
| **MutationGenerator** | No context detection | Auto-detects `input_tenant_id`, `input_user_id` |
| **PostgresIntrospector** | Views + Functions only | + Composite Types |

### Context Parameter Convention Change

**Updated Convention** (per your feedback):
- `input_tenant_id` → `context["tenant_id"]` (clearer than `input_pk_organization`)
- `input_user_id` → `context["user_id"]` (clearer than `input_created_by`)

This is more explicit and follows common patterns (Django, FastAPI, etc.).

---

## Prerequisites

### Knowledge Requirements

- [x] Basic Python (dataclasses, async/await, type hints)
- [x] Basic SQL (SELECT, JOIN, WHERE)
- [x] PostgreSQL system catalogs (`pg_type`, `pg_class`, `pg_attribute`)
- [x] FraiseQL codebase structure (already have Phase 1-4 complete)

### Files You'll Modify

```
src/fraiseql/introspection/
├── postgres_introspector.py    # Add composite type queries
├── input_generator.py           # Add composite type detection
├── mutation_generator.py        # Add context parameter extraction
├── metadata_parser.py           # Add field metadata parsing
└── type_mapper.py              # (Minor update for UUID handling)

tests/
├── unit/introspection/
│   ├── test_postgres_introspector.py
│   ├── test_input_generator.py
│   └── test_mutation_generator.py
└── integration/introspection/
    └── test_composite_type_generation.py
```

### Testing Database

You'll need a test database with SpecQL-style schema:

```sql
-- Create this in your test database
CREATE SCHEMA IF NOT EXISTS app;

CREATE TYPE app.type_create_contact_input AS (
    email TEXT,
    company_id UUID,
    status TEXT
);

CREATE TYPE app.mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);

CREATE FUNCTION app.create_contact(
    input_tenant_id UUID,
    input_user_id UUID,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql
AS $$ BEGIN /* stub */ END; $$;

COMMENT ON TYPE app.type_create_contact_input IS '@fraiseql:input name=CreateContactInput';
COMMENT ON COLUMN app.type_create_contact_input.email IS '@fraiseql:field name=email,type=String!,required=true';
COMMENT ON COLUMN app.type_create_contact_input.company_id IS '@fraiseql:field name=companyId,type=UUID,required=false';
COMMENT ON COLUMN app.type_create_contact_input.status IS '@fraiseql:field name=status,type=String!,required=true';

COMMENT ON FUNCTION app.create_contact IS '@fraiseql:mutation
name: createContact
description: Create a new contact
input_type: app.type_create_contact_input
success_type: Contact
failure_type: ContactError';
```

---

## Phase Overview

### Phase 5.1: Composite Type Introspection (Foundation)
**Time**: 2-3 hours
**Goal**: Query PostgreSQL to discover composite types and their fields

### Phase 5.2: Field Metadata Parsing
**Time**: 1-2 hours
**Goal**: Parse `@fraiseql:field` annotations from column comments

### Phase 5.3: Input Generation from Composite Types
**Time**: 2-3 hours
**Goal**: Generate GraphQL input types from composite types (not function parameters)

### Phase 5.4: Context Parameter Auto-Detection
**Time**: 1-2 hours
**Goal**: Extract `input_tenant_id` and `input_user_id` from function signatures

### Phase 5.5: Integration and Testing
**Time**: 2-3 hours
**Goal**: End-to-end tests with real SpecQL schema

---

## Detailed Implementation Steps

---

## 🔧 PHASE 5.1: Composite Type Introspection

### Step 1.1: Add Data Classes

**File**: `src/fraiseql/introspection/postgres_introspector.py`

**Location**: Add these dataclasses at the top of the file, after existing imports and before the `PostgresIntrospector` class.

```python
from dataclasses import dataclass
from typing import Optional

# ... existing imports ...

# ADD THESE NEW DATACLASSES (around line 55, after ParameterInfo)

@dataclass
class CompositeAttribute:
    """Metadata for a single attribute in a PostgreSQL composite type."""

    name: str                    # Attribute name (e.g., "email")
    pg_type: str                 # PostgreSQL type (e.g., "text", "uuid")
    ordinal_position: int        # Position in type (1, 2, 3, ...)
    comment: Optional[str]       # Column comment (contains @fraiseql:field metadata)


@dataclass
class CompositeTypeMetadata:
    """Metadata for a PostgreSQL composite type."""

    schema_name: str             # Schema (e.g., "app")
    type_name: str               # Type name (e.g., "type_create_contact_input")
    attributes: list[CompositeAttribute]  # List of attributes/fields
    comment: Optional[str]       # Type comment (contains @fraiseql:input metadata)
```

**Why**: These dataclasses hold the information we get from PostgreSQL about composite types.

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
        Introspect a PostgreSQL composite type.

        Args:
            type_name: Name of the composite type (e.g., "type_create_contact_input")
            schema: Schema name (default: "app")

        Returns:
            CompositeTypeMetadata if type exists, None otherwise

        Example:
            >>> introspector = PostgresIntrospector(pool)
            >>> metadata = await introspector.discover_composite_type(
            ...     "type_create_contact_input",
            ...     schema="app"
            ... )
            >>> print(metadata.attributes[0].name)  # "email"
        """
        async with self.pool.connection() as conn:
            # Step 1: Get type-level metadata (comment)
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
                return None  # Composite type not found

            # Step 2: Get attribute-level metadata (fields)
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
                ORDER BY a.attnum;
            """

            attr_result = await conn.execute(attr_query, (schema, type_name))
            attr_rows = await attr_result.fetchall()

            # Step 3: Build attribute list
            attributes = [
                CompositeAttribute(
                    name=row['attribute_name'],
                    pg_type=row['pg_type'],
                    ordinal_position=row['ordinal_position'],
                    comment=row['comment']
                )
                for row in attr_rows
            ]

            # Step 4: Return composite type metadata
            return CompositeTypeMetadata(
                schema_name=schema,
                type_name=type_name,
                attributes=attributes,
                comment=type_row['comment']
            )
```

**What this does**:
1. Queries `pg_type` to check if the composite type exists
2. Queries `pg_attribute` to get all fields in the composite type
3. Returns a `CompositeTypeMetadata` object with all the info

**To test manually**:
```python
# In a Python REPL or test file
introspector = PostgresIntrospector(connection_pool)
metadata = await introspector.discover_composite_type("type_create_contact_input", "app")
print(metadata.type_name)  # "type_create_contact_input"
print(metadata.attributes[0].name)  # "email"
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
    """Test composite type introspection."""
    # Given: Introspector with test database
    introspector = PostgresIntrospector(test_db_pool)

    # When: Discover composite type
    metadata = await introspector.discover_composite_type(
        "type_create_contact_input",
        schema="app"
    )

    # Then: Metadata is returned
    assert metadata is not None
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

    # Then: Attributes have comments
    assert email_attr.comment is not None
    assert "@fraiseql:field" in email_attr.comment


@pytest.mark.asyncio
async def test_discover_composite_type_not_found(test_db_pool):
    """Test composite type discovery with non-existent type."""
    # Given: Introspector
    introspector = PostgresIntrospector(test_db_pool)

    # When: Try to discover non-existent type
    metadata = await introspector.discover_composite_type(
        "type_nonexistent_input",
        schema="app"
    )

    # Then: Returns None
    assert metadata is None
```

**Run test**:
```bash
uv run pytest tests/unit/introspection/test_postgres_introspector.py::test_discover_composite_type -v
```

**Expected**: ✅ Test passes

---

## 🔧 PHASE 5.2: Field Metadata Parsing

### Step 2.1: Add Field Metadata Dataclass

**File**: `src/fraiseql/introspection/metadata_parser.py`

**Location**: Add this dataclass after existing dataclasses (around line 30).

```python
@dataclass
class FieldMetadata:
    """Parsed @fraiseql:field annotation from composite type column comment."""

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

        Format:
            @fraiseql:field name=email,type=String!,required=true

        Args:
            comment: Column comment string

        Returns:
            FieldMetadata if annotation found, None otherwise

        Example:
            >>> parser = MetadataParser()
            >>> metadata = parser.parse_field_annotation(
            ...     "@fraiseql:field name=email,type=String!,required=true"
            ... )
            >>> metadata.name
            'email'
            >>> metadata.required
            True
        """
        if not comment or "@fraiseql:field" not in comment:
            return None

        # Extract key-value pairs from annotation
        # Format: @fraiseql:field name=email,type=String!,required=true,description=User email

        # Find the @fraiseql:field line
        lines = comment.split('\n')
        field_line = next((line for line in lines if '@fraiseql:field' in line), None)

        if not field_line:
            return None

        # Remove '@fraiseql:field' prefix
        content = field_line.split('@fraiseql:field', 1)[1].strip()

        # Parse key=value pairs
        # Handle: name=email,type=String!,required=true,description=Some desc with spaces
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
                # This is a new key, save previous one
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

        # Build FieldMetadata
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
- Parses the `@fraiseql:field` comment format
- Extracts field name, type, required flag, etc.
- Returns structured metadata

---

### Step 2.3: Write Unit Test for Phase 5.2

**File**: `tests/unit/introspection/test_metadata_parser.py`

**Location**: Add at the end of the file.

```python
def test_parse_field_annotation_basic():
    """Test parsing basic field annotation."""
    # Given: Parser
    parser = MetadataParser()

    # Given: Field comment
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
    # Given: Field comment with enum
    comment = "@fraiseql:field name=status,type=ContactStatus,required=true,enum=true"

    # When: Parse
    metadata = MetadataParser().parse_field_annotation(comment)

    # Then: Enum flag is set
    assert metadata.name == "status"
    assert metadata.is_enum is True


def test_parse_field_annotation_optional():
    """Test parsing optional field (required=false)."""
    # Given: Optional field
    comment = "@fraiseql:field name=companyId,type=UUID,required=false"

    # When: Parse
    metadata = MetadataParser().parse_field_annotation(comment)

    # Then: Required is False
    assert metadata.required is False


def test_parse_field_annotation_with_description():
    """Test parsing field with description."""
    # Given: Field with description
    comment = "@fraiseql:field name=email,type=String!,required=true,description=User email address"

    # When: Parse
    metadata = MetadataParser().parse_field_annotation(comment)

    # Then: Description is extracted
    assert metadata.description == "User email address"


def test_parse_field_annotation_no_annotation():
    """Test parsing comment without @fraiseql:field."""
    # Given: Regular comment
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

### Step 3.1: Update InputGenerator to Accept Introspector

**File**: `src/fraiseql/introspection/input_generator.py`

**Change 1**: Update the `__init__` method to store `metadata_parser`:

```python
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

        Convention: Look for parameter named 'input_payload' with type 'jsonb'

        Args:
            function_metadata: Function metadata from introspection

        Returns:
            ParameterInfo if found, None otherwise

        Example:
            Function signature:
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
        # Priority 1: Check for explicit input_type in annotation
        if hasattr(annotation, 'input_type') and annotation.input_type:
            # Extract type name from fully qualified name
            # "app.type_contact_input" → "type_contact_input"
            if '.' in annotation.input_type:
                return annotation.input_type.split('.')[-1]
            return annotation.input_type

        # Priority 2: Convention-based extraction
        # Remove common prefixes: fn_, app.
        function_name = function_metadata.function_name

        # Remove 'fn_' prefix if present
        if function_name.startswith('fn_'):
            function_name = function_name[3:]

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
        Generate input class from PostgreSQL composite type.

        Steps:
        1. Introspect composite type to get attributes
        2. Parse field metadata from column comments
        3. Map PostgreSQL types to Python types
        4. Create input class with proper annotations

        Args:
            composite_type_name: Name of composite type (e.g., "type_create_contact_input")
            schema_name: Schema where function is defined (will check "app" schema for type)
            introspector: PostgresIntrospector instance

        Returns:
            Dynamically created input class

        Example:
            Composite type:
                CREATE TYPE app.type_create_contact_input AS (
                    email TEXT,
                    company_id UUID,
                    status TEXT
                );

            Generates:
                class CreateContactInput:
                    email: str
                    company_id: UUID
                    status: str
        """
        # Step 1: Introspect composite type (always look in 'app' schema per SpecQL convention)
        composite_metadata = await introspector.discover_composite_type(
            composite_type_name,
            schema="app"
        )

        if not composite_metadata:
            raise ValueError(
                f"Composite type '{composite_type_name}' not found in 'app' schema. "
                f"Expected by function in '{schema_name}' schema."
            )

        # Step 2: Build annotations from composite type attributes
        annotations = {}

        for attr in composite_metadata.attributes:
            # Step 2a: Parse field metadata from comment (if exists)
            field_metadata = None
            if attr.comment:
                field_metadata = self.metadata_parser.parse_field_annotation(attr.comment)

            # Step 2b: Determine field name (use metadata name if available)
            field_name = field_metadata.name if field_metadata else attr.name

            # Step 2c: Map PostgreSQL type to Python type
            # Check if field is required (from metadata or assume optional)
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
        2. If found, extract composite type name and introspect it
        3. Otherwise, fall back to parameter-based generation (legacy)

        Args:
            function_metadata: Metadata from function introspection
            annotation: Parsed @fraiseql:mutation annotation
            introspector: PostgresIntrospector for composite type discovery

        Returns:
            Dynamically created input class

        Example (SpecQL pattern):
            Function:
                app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB)

            Generates from composite type:
                class CreateContactInput:
                    email: str
                    company_id: UUID | None
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
            # Found JSONB parameter → use composite type
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
    """Test input generation from composite type (SpecQL pattern)."""
    # Given: InputGenerator and introspector
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    introspector = PostgresIntrospector(test_db_pool)

    # Given: Function with JSONB parameter
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

    # When: Generate input type
    input_cls = await input_generator.generate_input_type(
        function,
        annotation,
        introspector
    )

    # Then: Class name is correct
    assert input_cls.__name__ == "CreateContactInput"

    # Then: Has fields from composite type
    assert "email" in input_cls.__annotations__
    assert "companyId" in input_cls.__annotations__  # camelCase from metadata
    assert "status" in input_cls.__annotations__

    # Then: Types are correct
    assert input_cls.__annotations__["email"] == str
    # UUID type will be from type_mapper
    assert "UUID" in str(input_cls.__annotations__["companyId"])


@pytest.mark.asyncio
async def test_generate_input_from_parameters_legacy(test_db_pool):
    """Test input generation from parameters (legacy pattern)."""
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

**Expected**: ✅ Test passes

---

## 🔧 PHASE 5.4: Context Parameter Auto-Detection

### Step 4.1: Add Context Parameter Extraction Method

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Location**: Add this method inside the `MutationGenerator` class (around line 75).

```python
    def _extract_context_params(
        self,
        function_metadata: FunctionMetadata
    ) -> dict[str, str]:
        """
        Auto-detect context parameters from function signature.

        Convention (updated based on feedback):
            input_tenant_id UUID   → context["tenant_id"]
            input_user_id UUID     → context["user_id"]

        Args:
            function_metadata: Function metadata from introspection

        Returns:
            Mapping of context_key → function_parameter_name

        Example:
            Function signature:
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

            # Legacy patterns (for backward compatibility)
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
- Scans function parameters for context patterns
- Builds a mapping for the `@fraiseql.mutation(context_params={...})` decorator
- Supports both new convention (`input_tenant_id`, `input_user_id`) and legacy patterns

---

### Step 4.2: Update Mutation Generation to Use Context Params

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Change**: Update the `generate_mutation_for_function` method to extract and use context params:

```python
    def generate_mutation_for_function(
        self,
        function_metadata: FunctionMetadata,
        annotation: MutationAnnotation,
        type_registry: dict[str, Type],
        introspector: "PostgresIntrospector"  # ADD THIS PARAMETER
    ) -> Callable | None:
        """
        Generate mutation from function.

        Steps:
        1. Generate input type (from composite type or parameters)
        2. Resolve success/failure types
        3. Extract context parameters (NEW)
        4. Create mutation function
        5. Handle JSONB return parsing

        Args:
            function_metadata: Metadata from function introspection
            annotation: Parsed @fraiseql:mutation annotation
            type_registry: Registry of available types
            introspector: PostgresIntrospector for composite type discovery (NEW)

        Returns:
            Decorated mutation function or None if generation fails
        """
        # 1. Generate input type (now supports composite types)
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

        # 3. Extract context parameters (NEW)
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

---

### Step 4.3: Update AutoDiscovery to Pass Introspector

**File**: `src/fraiseql/introspection/auto_discovery.py`

**Change**: Update the `_generate_mutation_from_function` method to pass introspector:

```python
    async def _generate_mutation_from_function(
        self, function_metadata: FunctionMetadata
    ) -> Callable | None:
        """Generate a mutation from function metadata."""
        # Parse @fraiseql:mutation annotation
        annotation = self.metadata_parser.parse_mutation_annotation(function_metadata.comment)
        if not annotation:
            return None

        # Generate mutation
        try:
            mutation = await self.mutation_generator.generate_mutation_for_function(
                function_metadata,
                annotation,
                self.type_registry,
                self.introspector  # ADD THIS: Pass introspector
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
    """Test context parameter extraction with new convention."""
    # Given: MutationGenerator
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)

    # Given: Function with new convention context params
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

    # When: Extract context params
    context_params = mutation_generator._extract_context_params(function)

    # Then: Correct mapping
    assert context_params == {
        "tenant_id": "input_tenant_id",
        "user_id": "input_user_id"
    }


def test_extract_context_params_legacy_convention():
    """Test context parameter extraction with legacy convention."""
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
    # Given: Function without context parameters (legacy style)
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

### Step 5.1: Create Integration Test Database Schema

**File**: `tests/integration/introspection/test_composite_type_generation_integration.py`

**Location**: Create new file.

```python
"""Integration tests for composite type-based mutation generation."""

import pytest
from fraiseql.introspection import AutoDiscovery


@pytest.fixture
async def specql_test_schema(test_db_pool):
    """Create SpecQL-style test schema."""
    async with test_db_pool.connection() as conn:
        await conn.execute("""
            CREATE SCHEMA IF NOT EXISTS app;

            -- Composite input type
            CREATE TYPE app.type_create_contact_input AS (
                email TEXT,
                company_id UUID,
                status TEXT
            );

            -- Standard output type
            CREATE TYPE app.mutation_result AS (
                id UUID,
                updated_fields TEXT[],
                status TEXT,
                message TEXT,
                object_data JSONB,
                extra_metadata JSONB
            );

            -- Function with SpecQL pattern
            CREATE OR REPLACE FUNCTION app.create_contact(
                input_tenant_id UUID,
                input_user_id UUID,
                input_payload JSONB
            ) RETURNS app.mutation_result
            LANGUAGE plpgsql
            AS $$
            BEGIN
                -- Stub implementation
                RETURN ROW(
                    gen_random_uuid(),
                    ARRAY['email', 'company_id', 'status'],
                    'success',
                    'Contact created',
                    '{}'::JSONB,
                    '{}'::JSONB
                )::app.mutation_result;
            END;
            $$;

            -- Add FraiseQL metadata
            COMMENT ON TYPE app.type_create_contact_input IS
                '@fraiseql:input name=CreateContactInput';

            COMMENT ON FUNCTION app.create_contact IS
                '@fraiseql:mutation
name: createContact
description: Create a new contact
input_type: app.type_create_contact_input
success_type: Contact
failure_type: ContactError';

            -- Field metadata
            -- Note: Column comments on composite types require special syntax
            COMMENT ON COLUMN app.type_create_contact_input.email IS
                '@fraiseql:field name=email,type=String!,required=true';

            COMMENT ON COLUMN app.type_create_contact_input.company_id IS
                '@fraiseql:field name=companyId,type=UUID,required=false';

            COMMENT ON COLUMN app.type_create_contact_input.status IS
                '@fraiseql:field name=status,type=String!,required=true';
        """)

        yield

        # Cleanup
        await conn.execute("""
            DROP FUNCTION IF EXISTS app.create_contact;
            DROP TYPE IF EXISTS app.mutation_result;
            DROP TYPE IF EXISTS app.type_create_contact_input;
            DROP SCHEMA IF EXISTS app CASCADE;
        """)


@pytest.mark.asyncio
async def test_end_to_end_composite_type_generation(test_db_pool, specql_test_schema):
    """Test complete flow from database to generated mutation."""
    # Given: AutoDiscovery with SpecQL schema
    auto_discovery = AutoDiscovery(test_db_pool)

    # When: Discover all mutations
    result = await auto_discovery.discover_all(
        view_pattern="v_%",
        function_pattern="%",  # Discover all functions
        schemas=["app"]
    )

    # Then: Mutation was discovered
    assert len(result['mutations']) > 0

    # Find the create_contact mutation
    create_contact = next(
        (m for m in result['mutations'] if 'createContact' in str(m)),
        None
    )
    assert create_contact is not None, "createContact mutation not found"

    # Then: Mutation has correct structure
    # (Exact assertions depend on how @fraiseql.mutation structures the result)
    # This is a basic smoke test - adjust based on actual mutation structure


@pytest.mark.asyncio
async def test_composite_type_input_fields(test_db_pool, specql_test_schema):
    """Test that input type has correct fields from composite type."""
    # Given: AutoDiscovery
    auto_discovery = AutoDiscovery(test_db_pool)

    # When: Discover mutations
    result = await auto_discovery.discover_all(schemas=["app"])

    # Then: Can access the generated input type
    # (Implementation-specific - you may need to adjust how to access the input type)
    # For now, this is a placeholder for more detailed assertions
    assert result is not None


@pytest.mark.asyncio
async def test_context_params_auto_detection(test_db_pool, specql_test_schema):
    """Test that context parameters are automatically detected."""
    # Given: AutoDiscovery
    auto_discovery = AutoDiscovery(test_db_pool)

    # When: Discover mutations
    result = await auto_discovery.discover_all(schemas=["app"])

    # Then: Context params should be auto-detected
    # (Check that tenant_id and user_id are in context_params)
    # Implementation-specific assertions here
    assert result is not None
```

---

### Step 5.2: Run Full Test Suite

```bash
# Run all unit tests
uv run pytest tests/unit/introspection/ -v

# Run integration tests
uv run pytest tests/integration/introspection/test_composite_type_generation_integration.py -v

# Run full test suite
uv run pytest tests/ -v
```

**Expected**: ✅ All tests pass

---

### Step 5.3: Manual Validation with Real Database

**File**: Create `examples/composite_type_example.py`

```python
"""Manual example to test composite type generation."""

import asyncio
import psycopg_pool
from fraiseql.introspection import AutoDiscovery


async def main():
    # Connect to your test database
    connection_pool = psycopg_pool.AsyncConnectionPool(
        conninfo="postgresql://user:password@localhost:5432/testdb"
    )

    # Initialize AutoDiscovery
    auto_discovery = AutoDiscovery(connection_pool)

    # Discover all
    result = await auto_discovery.discover_all(
        schemas=["app"]
    )

    # Print results
    print(f"✅ Discovered {len(result['types'])} types")
    print(f"✅ Discovered {len(result['queries'])} queries")
    print(f"✅ Discovered {len(result['mutations'])} mutations")

    # Print mutation details
    for mutation in result['mutations']:
        print(f"\n📝 Mutation: {mutation}")

    await connection_pool.close()


if __name__ == "__main__":
    asyncio.run(main())
```

**Run**:
```bash
uv run python examples/composite_type_example.py
```

**Expected output**:
```
✅ Discovered 0 types
✅ Discovered 0 queries
✅ Discovered 1 mutations

📝 Mutation: <function createContact at 0x...>
```

---

## Testing Strategy

### Unit Tests (Fast, Isolated)

**What to test**:
- ✅ Composite type introspection SQL queries
- ✅ Field metadata parsing from comments
- ✅ Input type generation logic
- ✅ Context parameter extraction
- ✅ Naming convention conversions

**How to run**:
```bash
uv run pytest tests/unit/introspection/ -v --tb=short
```

---

### Integration Tests (Slower, Real Database)

**What to test**:
- ✅ End-to-end discovery pipeline
- ✅ Real PostgreSQL composite types
- ✅ Generated mutations work with FraiseQL decorators

**How to run**:
```bash
uv run pytest tests/integration/introspection/ -v --tb=short
```

---

### Manual Testing (With PrintOptim Schema)

**Steps**:
1. Connect to PrintOptim database
2. Run `AutoDiscovery.discover_all(schemas=["app"])`
3. Verify mutations are generated
4. Compare with hand-written mutations

**Validation**:
```python
# Check input type matches
generated_input = mutation_generator.generate_input_type(...)
assert generated_input.__annotations__ == CreateOrganizationalUnitInput.__annotations__

# Check context params match
context_params = mutation_generator._extract_context_params(...)
assert context_params == {"tenant_id": "input_tenant_id", "user_id": "input_user_id"}
```

---

## Validation Checklist

### Phase 5.1: Composite Type Introspection ✅
- [ ] `discover_composite_type()` returns correct metadata
- [ ] Handles non-existent types gracefully (returns None)
- [ ] Attributes are in correct order (ordinal_position)
- [ ] Column comments are retrieved

### Phase 5.2: Field Metadata Parsing ✅
- [ ] Parses `@fraiseql:field` annotations correctly
- [ ] Extracts name, type, required, enum flags
- [ ] Handles missing annotations (returns None)
- [ ] Handles malformed annotations gracefully

### Phase 5.3: Input Generation ✅
- [ ] Detects JSONB `input_payload` parameter
- [ ] Extracts composite type name from convention
- [ ] Introspects composite type and generates input class
- [ ] Falls back to parameter-based generation when no JSONB
- [ ] Generated class name matches convention (CreateContactInput)

### Phase 5.4: Context Parameter Detection ✅
- [ ] Detects `input_tenant_id` → `tenant_id`
- [ ] Detects `input_user_id` → `user_id`
- [ ] Supports legacy `input_pk_*` pattern
- [ ] Supports legacy `input_created_by` pattern
- [ ] Returns empty dict when no context params

### Phase 5.5: Integration ✅
- [ ] Full discovery pipeline works end-to-end
- [ ] Generated mutations match hand-written equivalents
- [ ] Works with real PrintOptim schema
- [ ] No breaking changes to existing functionality

---

## Common Issues and Solutions

### Issue 1: Composite Type Not Found

**Symptom**:
```
ValueError: Composite type 'type_create_contact_input' not found in 'app' schema
```

**Solution**:
1. Check type name convention: `type_{function_name}_input`
2. Verify type exists in `app` schema: `\dT app.type_*` in psql
3. Add explicit `input_type` annotation to function comment

---

### Issue 2: Column Comments Not Retrieved

**Symptom**:
```python
attr.comment is None
```

**Solution**:
PostgreSQL requires special syntax for composite type column comments:
```sql
COMMENT ON COLUMN app.type_contact_input.email IS '@fraiseql:field ...';
```

Note the schema prefix: `app.type_contact_input` not just `type_contact_input`.

---

### Issue 3: Async/Await Errors

**Symptom**:
```
RuntimeWarning: coroutine 'generate_input_type' was never awaited
```

**Solution**:
Remember to `await` async methods:
```python
# ✅ Correct
input_cls = await input_generator.generate_input_type(...)

# ❌ Wrong
input_cls = input_generator.generate_input_type(...)
```

---

### Issue 4: Type Mapping Errors

**Symptom**:
```
KeyError: 'uuid'
```

**Solution**:
Ensure `TypeMapper.pg_type_to_python()` handles all PostgreSQL types:
```python
# Check if type mapper has UUID support
type_mapper = TypeMapper()
python_type = type_mapper.pg_type_to_python("uuid", nullable=False)
```

If missing, add to `type_mapper.py`:
```python
"uuid": UUID,
"uuid[]": list[UUID],
```

---

## Performance Considerations

### Caching Strategy

**Problem**: Introspecting composite types on every mutation generation is slow.

**Solution**: Cache composite type metadata on startup.

```python
class AutoDiscovery:
    def __init__(self, connection_pool):
        # ...
        self._composite_type_cache: dict[str, CompositeTypeMetadata] = {}

    async def _get_composite_type(self, type_name: str, schema: str) -> CompositeTypeMetadata:
        cache_key = f"{schema}.{type_name}"

        if cache_key not in self._composite_type_cache:
            metadata = await self.introspector.discover_composite_type(type_name, schema)
            self._composite_type_cache[cache_key] = metadata

        return self._composite_type_cache[cache_key]
```

**When to invalidate**:
- Schema migrations
- Server restart
- Listen to PostgreSQL notifications: `LISTEN schema_changes`

---

### Batch Loading

**Problem**: Discovering 100+ functions with composite types requires 100+ queries.

**Solution**: Batch-load all composite types in `app` schema at startup.

```python
async def preload_composite_types(self, schema: str = "app") -> None:
    """Preload all composite types in schema into cache."""
    # Query all composite types at once
    query = """
        SELECT typname
        FROM pg_type t
        JOIN pg_namespace n ON n.oid = t.typnamespace
        WHERE n.nspname = %s
          AND t.typtype = 'c'
          AND t.typname LIKE 'type_%_input'
    """

    async with self.pool.connection() as conn:
        result = await conn.execute(query, (schema,))
        rows = await result.fetchall()

    # Load each composite type
    for row in rows:
        type_name = row['typname']
        await self.discover_composite_type(type_name, schema)
```

Call during initialization:
```python
auto_discovery = AutoDiscovery(pool)
await auto_discovery.preload_composite_types("app")
```

---

## Documentation Updates

### Update README.md

Add section explaining composite type support:

```markdown
## AutoFraiseQL: Composite Type Support

AutoFraiseQL supports two patterns for mutation input generation:

### Pattern 1: Parameter-Based (Legacy)

```sql
CREATE FUNCTION fn_create_user(p_name TEXT, p_email TEXT) ...
```

→ Generates input from function parameters

### Pattern 2: Composite Type-Based (SpecQL)

```sql
CREATE TYPE app.type_create_contact_input AS (email TEXT, company_id UUID);
CREATE FUNCTION app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB) ...
```

→ Generates input from composite type

**Context parameters** (`input_tenant_id`, `input_user_id`) are automatically detected and mapped to GraphQL context.
```

---

### Update CHANGELOG.md

```markdown
## [Unreleased]

### Added
- **Phase 5: Composite Type Input Generation**
  - Support for PostgreSQL composite types as mutation inputs
  - Automatic context parameter detection (`input_tenant_id`, `input_user_id`)
  - Field metadata parsing from `@fraiseql:field` annotations
  - Backward compatibility with parameter-based input generation

### Changed
- `InputGenerator.generate_input_type()` now accepts `introspector` parameter
- `MutationGenerator.generate_mutation_for_function()` now accepts `introspector` parameter

### Fixed
- None

### Breaking Changes
- None (fully backward compatible)
```

---

## Final Checklist

### Before Starting Implementation
- [ ] Read this entire document
- [ ] Set up test database with SpecQL schema
- [ ] Understand existing Phase 1-4 codebase
- [ ] Have PrintOptim database access for validation

### Phase 5.1 Complete
- [ ] `CompositeTypeMetadata` and `CompositeAttribute` dataclasses added
- [ ] `discover_composite_type()` method implemented
- [ ] Unit tests pass
- [ ] Manual test with real composite type works

### Phase 5.2 Complete
- [ ] `FieldMetadata` dataclass added
- [ ] `parse_field_annotation()` method implemented
- [ ] Unit tests pass
- [ ] Can parse required/optional/enum metadata

### Phase 5.3 Complete
- [ ] `_find_jsonb_input_parameter()` method added
- [ ] `_extract_composite_type_name()` method added
- [ ] `_generate_from_composite_type()` method added
- [ ] `generate_input_type()` updated to support both patterns
- [ ] Unit tests pass
- [ ] Integration test with real composite type works

### Phase 5.4 Complete
- [ ] `_extract_context_params()` method added
- [ ] `generate_mutation_for_function()` passes context params
- [ ] Unit tests pass
- [ ] Context params auto-detected correctly

### Phase 5.5 Complete
- [ ] Integration tests pass
- [ ] Manual validation with PrintOptim schema successful
- [ ] No regression in existing functionality
- [ ] Performance acceptable (<2s for 100+ functions)
- [ ] Documentation updated

### Ready for Production
- [ ] All tests pass (`uv run pytest`)
- [ ] Code review complete
- [ ] CHANGELOG updated
- [ ] README updated
- [ ] Deployed to staging environment
- [ ] Validated with real SpecQL-generated schema

---

## Success Metrics

**Definition of Done**:
1. ✅ All unit tests pass
2. ✅ All integration tests pass
3. ✅ Can auto-generate all PrintOptim mutations from database
4. ✅ Generated mutations match hand-written equivalents
5. ✅ No breaking changes to existing functionality
6. ✅ Documentation complete

**Quantitative Goals**:
- [ ] 100% test coverage for new code
- [ ] <2s discovery time for 100 functions
- [ ] Zero manual code required for SpecQL schemas

---

## Getting Help

### Debugging Steps

1. **Enable verbose logging**:
```python
import logging
logging.basicConfig(level=logging.DEBUG)
```

2. **Test components in isolation**:
```python
# Test composite type introspection
metadata = await introspector.discover_composite_type("type_create_contact_input", "app")
print(metadata)

# Test field parsing
field_meta = parser.parse_field_annotation("@fraiseql:field name=email,type=String!,required=true")
print(field_meta)
```

3. **Check PostgreSQL directly**:
```sql
-- List composite types
\dT app.type_*

-- View composite type structure
\d+ app.type_create_contact_input

-- Check function signature
\df+ app.create_contact
```

### Questions to Ask

- ❓ Is the composite type in the `app` schema?
- ❓ Does the function have `input_payload JSONB` parameter?
- ❓ Are column comments formatted correctly?
- ❓ Is the type name following convention (`type_{action}_input`)?

---

## Congratulations! 🎉

Once you complete all phases, you'll have:
- ✅ Full composite type support in AutoFraiseQL
- ✅ Automatic context parameter detection
- ✅ Zero manual code for SpecQL-generated schemas
- ✅ Backward compatibility with existing patterns

**You've built a production-ready meta-framework feature!**
