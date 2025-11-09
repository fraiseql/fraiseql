# FraiseQL Phase 5: Composite Type Input Generation - DETAILED IMPLEMENTATION PLAN

**Status**: Implementation Ready
**Priority**: High
**Complexity**: Complex - Requires Phased TDD Approach
**Estimated Time**: 2-3 weeks (8-12 hours active development + testing)
**Methodology**: Disciplined TDD Cycles per CLAUDE.md
**Target Agent**: Mid-level Developer with TDD Experience

---

## 📋 Table of Contents

1. [Executive Summary](#executive-summary)
2. [Complexity Assessment](#complexity-assessment)
3. [Prerequisites](#prerequisites)
4. [Phase Structure Overview](#phase-structure-overview)
5. [Detailed Phase Implementation](#detailed-phase-implementation)
6. [Testing Strategy](#testing-strategy)
7. [Success Criteria](#success-criteria)
8. [Common Issues and Solutions](#common-issues-and-solutions)

---

## Executive Summary

### What You're Building

AutoFraiseQL currently generates GraphQL mutations by reading function parameters directly. However, SpecQL (our database code generator) generates a different pattern using composite types. Your goal is to make AutoFraiseQL introspect composite types instead of function parameters.

**Current Pattern (Parameter-Based)**:
```sql
CREATE FUNCTION fn_create_user(p_name TEXT, p_email TEXT) ...
```
→ AutoFraiseQL extracts `p_name` and `p_email` from function signature

**SpecQL Pattern (Composite Type-Based)**:
```sql
CREATE TYPE app.type_create_contact_input AS (
    email TEXT,
    company_id UUID,
    status TEXT
);

CREATE FUNCTION app.create_contact(
    input_tenant_id UUID,      -- Context (from GraphQL context)
    input_user_id UUID,         -- Context (from GraphQL context)
    input_payload JSONB         -- Business input (maps to composite type)
) RETURNS app.mutation_result;
```
→ AutoFraiseQL should introspect the composite type and auto-detect context parameters

### What Changes

| Component | Current Behavior | New Behavior |
|-----------|------------------|--------------|
| **PostgresIntrospector** | Introspects views + functions | + Introspects composite types |
| **InputGenerator** | Reads function parameters | Detects JSONB → introspects composite type |
| **MutationGenerator** | No context detection | Auto-detects `input_tenant_id`, `input_user_id` |

### Key Constraints

⚠️ **CRITICAL**: You are implementing **introspection only**. You will **NEVER**:
- Create or modify database objects
- Execute DDL statements (CREATE, ALTER, DROP)
- Write to the database in any way

✅ **YOU WILL**:
- Query PostgreSQL system catalogs (`pg_type`, `pg_class`, `pg_attribute`)
- Read metadata that SpecQL already created
- Generate Python classes and GraphQL types
- Parse comments and annotations

---

## Complexity Assessment

**Classification**: **COMPLEX** - Multi-file, architecture changes, new patterns

**Why Complex**:
- Touches 4+ source files
- Requires deep PostgreSQL catalog knowledge
- New introspection patterns (composite types)
- Integration with existing codebase
- Backward compatibility required

**Development Approach**: **Phased TDD** (per CLAUDE.md)

---

## Prerequisites

### Knowledge Requirements

- [x] Python: dataclasses, async/await, type hints, dynamic class creation
- [x] PostgreSQL: System catalogs (`pg_type`, `pg_class`, `pg_attribute`)
- [x] SQL: SELECT, JOIN, WHERE, window functions
- [x] TDD: RED/GREEN/REFACTOR cycle discipline
- [x] FraiseQL: Phases 1-4 complete, codebase structure

### Files You'll Modify

```
src/fraiseql/introspection/
├── postgres_introspector.py    # Add composite type introspection
├── input_generator.py           # Add composite type detection
├── mutation_generator.py        # Add context parameter extraction
├── metadata_parser.py           # Add field metadata parsing
├── auto_discovery.py            # Wire everything together
└── __init__.py                  # Export new classes

tests/
├── unit/introspection/
│   ├── test_postgres_introspector.py
│   ├── test_input_generator.py
│   ├── test_mutation_generator.py
│   └── test_metadata_parser.py
├── integration/introspection/
│   └── test_composite_type_generation_integration.py
└── fixtures/
    └── specql_test_schema.sql   # Test database schema
```

### Test Database Setup

You **MUST** have access to a database with SpecQL-generated schema.

#### Option A: Use Existing PrintOptim Database
```bash
export DATABASE_URL="postgresql://user:password@localhost:5432/printoptim"
```

#### Option B: Create Test Database
```bash
createdb fraiseql_test
psql fraiseql_test < tests/fixtures/specql_test_schema.sql
```

**Verify Schema Exists**:
```bash
psql fraiseql_test -c "\dT app.type_*"
# Should show: app.type_create_contact_input, etc.
```

---

## Phase Structure Overview

This implementation follows the **Phased TDD Methodology** from CLAUDE.md.

### Development Phases

```
┌─────────────────────────────────────────────────────────────┐
│ PHASE 5.1: Composite Type Introspection (Foundation)        │
│ Time: 2-3 hours | Goal: Query PostgreSQL for composite types│
│                                                             │
│ ┌─────────┐  ┌─────────┐  ┌─────────────┐  ┌─────────┐     │
│ │   RED   │─▶│ GREEN   │─▶│  REFACTOR   │─▶│   QA    │     │
│ └─────────┘  └─────────┘  └─────────────┘  └─────────┘     │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ PHASE 5.2: Field Metadata Parsing                           │
│ Time: 1-2 hours | Goal: Parse @fraiseql:field annotations   │
│                                                             │
│ ┌─────────┐  ┌─────────┐  ┌─────────────┐  ┌─────────┐     │
│ │   RED   │─▶│ GREEN   │─▶│  REFACTOR   │─▶│   QA    │     │
│ └─────────┘  └─────────┘  └─────────────┘  └─────────┘     │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ PHASE 5.3: Input Generation from Composite Types            │
│ Time: 2-3 hours | Goal: Generate GraphQL inputs from types  │
│                                                             │
│ ┌─────────┐  ┌─────────┐  ┌─────────────┐  ┌─────────┐     │
│ │   RED   │─▶│ GREEN   │─▶│  REFACTOR   │─▶│   QA    │     │
│ └─────────┘  └─────────┘  └─────────────┘  └─────────┘     │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ PHASE 5.4: Context Parameter Auto-Detection                 │
│ Time: 1-2 hours | Goal: Extract context params from function│
│                                                             │
│ ┌─────────┐  ┌─────────┐  ┌─────────────┐  ┌─────────┐     │
│ │   RED   │─▶│ GREEN   │─▶│  REFACTOR   │─▶│   QA    │     │
│ └─────────┘  └─────────┘  └─────────────┘  └─────────┘     │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ PHASE 5.5: Integration and E2E Testing                      │
│ Time: 2-3 hours | Goal: Verify end-to-end with real schema  │
│                                                             │
│ ┌─────────┐  ┌─────────┐  ┌─────────────┐  ┌─────────┐     │
│ │   RED   │─▶│ GREEN   │─▶│  REFACTOR   │─▶│   QA    │     │
│ └─────────┘  └─────────┘  └─────────────┘  └─────────┘     │
└─────────────────────────────────────────────────────────────┘
```

---

## Detailed Phase Implementation

---

## 🔴🟢🔧✅ PHASE 5.1: Composite Type Introspection

**Objective**: Query PostgreSQL catalog to discover composite types that SpecQL created.

**Time Estimate**: 2-3 hours
**Files Modified**: `postgres_introspector.py`, `test_postgres_introspector.py`

---

### 🔴 RED: Write Failing Test

**Duration**: 15-20 minutes

#### Step 1.1: Write Failing Unit Test

**File**: `tests/unit/introspection/test_postgres_introspector.py`

**Add at the end of the file**:

```python
import pytest
from fraiseql.introspection import PostgresIntrospector


@pytest.mark.asyncio
async def test_discover_composite_type(db_pool):
    """Test composite type introspection.

    This test verifies we can READ composite types that SpecQL created.

    Expected to FAIL initially because discover_composite_type() doesn't exist yet.
    """
    # Given: Introspector with test database (has SpecQL schema)
    introspector = PostgresIntrospector(db_pool)

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


@pytest.mark.asyncio
async def test_discover_composite_type_not_found(db_pool):
    """Test composite type discovery with non-existent type.

    Expected to FAIL initially.
    """
    # Given: Introspector
    introspector = PostgresIntrospector(db_pool)

    # When: Try to discover non-existent type
    metadata = await introspector.discover_composite_type(
        "type_nonexistent_input",
        schema="app"
    )

    # Then: Returns None (not an error)
    assert metadata is None
```

#### Step 1.2: Run Test and Verify Failure

```bash
uv run pytest tests/unit/introspection/test_postgres_introspector.py::test_discover_composite_type -v
```

**Expected Output** (RED phase):
```
FAILED - AttributeError: 'PostgresIntrospector' object has no attribute 'discover_composite_type'
```

**✅ RED Phase Complete**: Test fails as expected (method doesn't exist).

---

### 🟢 GREEN: Minimal Implementation

**Duration**: 30-40 minutes

#### Step 1.3: Add Data Classes (if not already present)

**File**: `src/fraiseql/introspection/postgres_introspector.py`

**Check if these dataclasses already exist** (they might from Phase 4). If not, add after `ParameterInfo`:

```python
@dataclass
class CompositeAttribute:
    """Metadata for a single attribute in a PostgreSQL composite type.

    Represents one field within a composite type that SpecQL created.
    """
    name: str                    # Attribute name (e.g., "email")
    pg_type: str                 # PostgreSQL type (e.g., "text", "uuid")
    ordinal_position: int        # Position in type (1, 2, 3, ...)
    comment: Optional[str]       # Column comment (contains @fraiseql:field metadata)


@dataclass
class CompositeTypeMetadata:
    """Metadata for a PostgreSQL composite type that SpecQL created."""
    schema_name: str             # Schema (e.g., "app")
    type_name: str               # Type name (e.g., "type_create_contact_input")
    attributes: list[CompositeAttribute]  # List of attributes/fields
    comment: Optional[str]       # Type comment (contains @fraiseql:input metadata)
```

#### Step 1.4: Implement Minimal discover_composite_type Method

**File**: `src/fraiseql/introspection/postgres_introspector.py`

**Add this method inside the `PostgresIntrospector` class**:

```python
async def discover_composite_type(
    self,
    type_name: str,
    schema: str = "app"
) -> CompositeTypeMetadata | None:
    """
    Introspect a PostgreSQL composite type that SpecQL created.

    This method READS the database to discover composite types.
    It does NOT create or modify anything.

    Args:
        type_name: Name of the composite type (e.g., "type_create_contact_input")
        schema: Schema name (default: "app" - where SpecQL puts composite types)

    Returns:
        CompositeTypeMetadata if type exists, None if not found
    """
    async with self.pool.connection() as conn:
        # Step 1: Check if composite type exists
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
            return None  # Composite type doesn't exist

        # Step 2: Get all attributes of the composite type
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

        # Step 3: Build list of attributes
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

#### Step 1.5: Update __init__.py Exports

**File**: `src/fraiseql/introspection/__init__.py`

**Add to imports and __all__**:

```python
from .postgres_introspector import (
    # ... existing imports ...
    CompositeTypeMetadata,      # ADD THIS
    CompositeAttribute,         # ADD THIS
)

__all__ = [
    # ... existing exports ...
    "CompositeTypeMetadata",     # ADD THIS
    "CompositeAttribute",        # ADD THIS
]
```

#### Step 1.6: Run Test and Verify Pass

```bash
uv run pytest tests/unit/introspection/test_postgres_introspector.py::test_discover_composite_type -v
```

**Expected Output** (GREEN phase):
```
PASSED test_discover_composite_type
PASSED test_discover_composite_type_not_found
```

**✅ GREEN Phase Complete**: Tests pass with minimal implementation.

---

### 🔧 REFACTOR: Clean Up and Optimize

**Duration**: 20-30 minutes

#### Step 1.7: Code Quality Improvements

**Run linters**:
```bash
uv run ruff check src/fraiseql/introspection/postgres_introspector.py
uv run mypy src/fraiseql/introspection/postgres_introspector.py
```

**Refactor checklist**:
- [ ] Add comprehensive docstrings
- [ ] Extract magic strings to constants if needed
- [ ] Consider caching composite types (future optimization)
- [ ] Ensure error handling is graceful
- [ ] Add logging statements

**Example Refactoring**:

```python
import logging

logger = logging.getLogger(__name__)

async def discover_composite_type(
    self,
    type_name: str,
    schema: str = "app"
) -> CompositeTypeMetadata | None:
    """
    Introspect a PostgreSQL composite type that SpecQL created.

    This method queries the PostgreSQL system catalogs to discover
    composite types and their attributes. It is a read-only operation.

    Implementation Details:
    - Queries pg_type to check type existence
    - Queries pg_class and pg_attribute for attribute metadata
    - Retrieves comments that SpecQL added

    Args:
        type_name: Name of the composite type (e.g., "type_create_contact_input")
        schema: Schema name (default: "app" - SpecQL convention)

    Returns:
        CompositeTypeMetadata if type exists, None if not found

    Example:
        >>> introspector = PostgresIntrospector(pool)
        >>> metadata = await introspector.discover_composite_type(
        ...     "type_create_contact_input",
        ...     schema="app"
        ... )
        >>> print(metadata.attributes[0].name)  # "email"
    """
    logger.debug(f"Discovering composite type: {schema}.{type_name}")

    # ... rest of implementation ...

    if not type_row:
        logger.debug(f"Composite type {schema}.{type_name} not found")
        return None

    logger.info(f"Discovered composite type {schema}.{type_name} with {len(attributes)} attributes")
    return CompositeTypeMetadata(...)
```

#### Step 1.8: Run Tests After Refactoring

```bash
uv run pytest tests/unit/introspection/test_postgres_introspector.py -v
```

**Expected**: ✅ All tests still pass after refactoring.

**✅ REFACTOR Phase Complete**: Code is clean and maintainable.

---

### ✅ QA: Verify Phase Completion

**Duration**: 15-20 minutes

#### Step 1.9: Run Full Test Suite

```bash
# Run all introspection tests
uv run pytest tests/unit/introspection/ -v --tb=short

# Run linting
uv run ruff check src/fraiseql/introspection/

# Run type checking
uv run mypy src/fraiseql/introspection/
```

#### Step 1.10: Manual Integration Test

**Test against real database**:

```python
# Create file: examples/test_phase_5_1.py
import asyncio
import psycopg_pool
from fraiseql.introspection import PostgresIntrospector

async def main():
    pool = psycopg_pool.AsyncConnectionPool(
        conninfo="postgresql://user:password@localhost:5432/printoptim"
    )

    introspector = PostgresIntrospector(pool)

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
        print("❌ Type not found")

    await pool.close()

asyncio.run(main())
```

Run: `python examples/test_phase_5_1.py`

**Expected Output**:
```
✅ Found type: type_create_contact_input
   Attributes: 3
   - email: text
   - company_id: uuid
   - status: text
```

#### Step 1.11: Phase 5.1 Completion Checklist

- [ ] All unit tests pass
- [ ] Linting passes (ruff)
- [ ] Type checking passes (mypy)
- [ ] Manual test with real database succeeds
- [ ] Code is documented
- [ ] Only reads from database (never writes)
- [ ] No breaking changes to existing code

**✅ QA Phase Complete**: Phase 5.1 is ready for production.

---

## 🔴🟢🔧✅ PHASE 5.2: Field Metadata Parsing

**Objective**: Parse `@fraiseql:field` annotations from composite type column comments.

**Time Estimate**: 1-2 hours
**Files Modified**: `metadata_parser.py`, `test_metadata_parser.py`

---

### 🔴 RED: Write Failing Test

**Duration**: 10-15 minutes

#### Step 2.1: Write Failing Unit Test

**File**: `tests/unit/introspection/test_metadata_parser.py`

**Add at the end of the file**:

```python
from fraiseql.introspection import MetadataParser


def test_parse_field_annotation_basic():
    """Test parsing basic field annotation (created by SpecQL).

    Expected to FAIL initially because parse_field_annotation() doesn't exist.
    """
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
    """Test parsing field annotation with enum flag.

    Expected to FAIL initially.
    """
    comment = "@fraiseql:field name=status,type=ContactStatus,required=true,enum=true"

    metadata = MetadataParser().parse_field_annotation(comment)

    assert metadata.name == "status"
    assert metadata.is_enum is True


def test_parse_field_annotation_optional():
    """Test parsing optional field (required=false).

    Expected to FAIL initially.
    """
    comment = "@fraiseql:field name=companyId,type=UUID,required=false"

    metadata = MetadataParser().parse_field_annotation(comment)

    assert metadata.required is False


def test_parse_field_annotation_no_annotation():
    """Test parsing comment without @fraiseql:field.

    Expected to FAIL initially.
    """
    comment = "This is just a regular comment"

    metadata = MetadataParser().parse_field_annotation(comment)

    assert metadata is None
```

#### Step 2.2: Run Test and Verify Failure

```bash
uv run pytest tests/unit/introspection/test_metadata_parser.py::test_parse_field_annotation_basic -v
```

**Expected Output** (RED phase):
```
FAILED - AttributeError: 'MetadataParser' object has no attribute 'parse_field_annotation'
```

**✅ RED Phase Complete**: Test fails as expected.

---

### 🟢 GREEN: Minimal Implementation

**Duration**: 25-35 minutes

#### Step 2.3: Add FieldMetadata Dataclass

**File**: `src/fraiseql/introspection/metadata_parser.py`

**Add after existing dataclasses**:

```python
@dataclass
class FieldMetadata:
    """Parsed @fraiseql:field annotation from composite type column comment.

    SpecQL puts this metadata in column comments. We parse it to understand
    field requirements.

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

#### Step 2.4: Implement parse_field_annotation Method

**File**: `src/fraiseql/introspection/metadata_parser.py`

**Add this method inside the `MetadataParser` class**:

```python
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
        >>> comment = "@fraiseql:field name=email,type=String!,required=true"
        >>> metadata = parser.parse_field_annotation(comment)
        >>> metadata.name
        'email'
        >>> metadata.required
        True
    """
    if not comment or "@fraiseql:field" not in comment:
        return None

    # Find the @fraiseql:field line
    lines = comment.split('\n')
    field_line = next((line for line in lines if '@fraiseql:field' in line), None)

    if not field_line:
        return None

    # Remove '@fraiseql:field' prefix
    content = field_line.split('@fraiseql:field', 1)[1].strip()

    # Parse key=value pairs
    params = {}
    parts = content.split(',')

    for part in parts:
        if '=' in part:
            key, value = part.split('=', 1)
            params[key.strip()] = value.strip()

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

#### Step 2.5: Run Test and Verify Pass

```bash
uv run pytest tests/unit/introspection/test_metadata_parser.py::test_parse_field_annotation_basic -v
```

**Expected Output** (GREEN phase):
```
PASSED test_parse_field_annotation_basic
PASSED test_parse_field_annotation_with_enum
PASSED test_parse_field_annotation_optional
PASSED test_parse_field_annotation_no_annotation
```

**✅ GREEN Phase Complete**: Tests pass with minimal implementation.

---

### 🔧 REFACTOR: Clean Up and Optimize

**Duration**: 15-20 minutes

#### Step 2.6: Improve Parsing Logic

**Refactor for edge cases**:
- Handle descriptions with commas
- Handle multiline comments
- Add error handling for malformed annotations

**Example improved implementation**:

```python
def parse_field_annotation(self, comment: str | None) -> FieldMetadata | None:
    """
    Parse @fraiseql:field annotation from composite type column comment.

    [Full docstring...]
    """
    if not comment or "@fraiseql:field" not in comment:
        return None

    try:
        # Find the @fraiseql:field line
        lines = comment.split('\n')
        field_line = next((line for line in lines if '@fraiseql:field' in line), None)

        if not field_line:
            return None

        # Remove '@fraiseql:field' prefix
        content = field_line.split('@fraiseql:field', 1)[1].strip()

        # Parse key=value pairs (improved logic)
        params = self._parse_key_value_pairs(content)

        # Build FieldMetadata with validation
        name = params.get('name')
        if not name:
            logger.warning(f"Field annotation missing 'name' parameter: {comment}")
            return None

        return FieldMetadata(
            name=name,
            graphql_type=params.get('type', 'String'),
            required=params.get('required', 'false').lower() == 'true',
            is_enum=params.get('enum', 'false').lower() == 'true',
            description=params.get('description')
        )
    except Exception as e:
        logger.error(f"Failed to parse field annotation '{comment}': {e}")
        return None

def _parse_key_value_pairs(self, content: str) -> dict[str, str]:
    """Parse comma-separated key=value pairs, handling quoted values."""
    params = {}
    current_key = None
    current_value = []
    in_quotes = False

    for char in content:
        # Handle quotes for values with commas
        if char == '"':
            in_quotes = not in_quotes
        # ... parsing logic ...

    return params
```

#### Step 2.7: Run Tests After Refactoring

```bash
uv run pytest tests/unit/introspection/test_metadata_parser.py -v
```

**Expected**: ✅ All tests still pass.

**✅ REFACTOR Phase Complete**: Code handles edge cases gracefully.

---

### ✅ QA: Verify Phase Completion

**Duration**: 10-15 minutes

#### Step 2.8: Run Full Test Suite

```bash
uv run pytest tests/unit/introspection/ -v --tb=short
uv run ruff check src/fraiseql/introspection/metadata_parser.py
uv run mypy src/fraiseql/introspection/metadata_parser.py
```

#### Step 2.9: Phase 5.2 Completion Checklist

- [ ] All unit tests pass
- [ ] Handles malformed annotations gracefully
- [ ] Linting passes
- [ ] Type checking passes
- [ ] Only parses comments (never writes)

**✅ QA Phase Complete**: Phase 5.2 is ready for production.

---

## 🔴🟢🔧✅ PHASE 5.3: Input Generation from Composite Types

**Objective**: Generate GraphQL input types from composite types (not function parameters).

**Time Estimate**: 2-3 hours
**Files Modified**: `input_generator.py`, `test_input_generator.py`

---

### 🔴 RED: Write Failing Test

**Duration**: 15-20 minutes

#### Step 3.1: Write Failing Unit Test

**File**: `tests/unit/introspection/test_input_generator.py`

**Add at the end of the file**:

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
async def test_generate_input_from_composite_type(db_pool):
    """Test input generation from composite type (SpecQL pattern).

    Expected to FAIL initially because composite type detection doesn't exist.
    """
    # Given: InputGenerator and introspector
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    introspector = PostgresIntrospector(db_pool)

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

    # When: Generate input type (should READ composite type from database)
    input_cls = await input_generator.generate_input_type(
        function,
        annotation,
        introspector
    )

    # Then: Class name is correct
    assert input_cls.__name__ == "CreateContactInput"

    # Then: Has fields from composite type (that SpecQL created)
    assert "email" in input_cls.__annotations__
    assert "companyId" in input_cls.__annotations__  # camelCase from metadata
    assert "status" in input_cls.__annotations__

    # Then: Types are correct
    assert input_cls.__annotations__["email"] == str
```

#### Step 3.2: Run Test and Verify Failure

```bash
uv run pytest tests/unit/introspection/test_input_generator.py::test_generate_input_from_composite_type -v
```

**Expected Output** (RED phase):
```
FAILED - TypeError: generate_input_type() missing 1 required positional argument: 'introspector'
```
or
```
FAILED - ValueError: Composite type 'type_create_contact_input' not found
```

**✅ RED Phase Complete**: Test fails as expected.

---

### 🟢 GREEN: Minimal Implementation

**Duration**: 40-50 minutes

#### Step 3.3: Update InputGenerator __init__

**File**: `src/fraiseql/introspection/input_generator.py`

**Add import and store metadata_parser**:

```python
from .metadata_parser import MetadataParser  # Add import

class InputGenerator:
    """Generate GraphQL input types from PostgreSQL function parameters."""

    def __init__(self, type_mapper: TypeMapper):
        self.type_mapper = type_mapper
        self.metadata_parser = MetadataParser()  # ADD THIS LINE
```

#### Step 3.4: Add Helper Methods

**File**: `src/fraiseql/introspection/input_generator.py`

**Add these methods to the class**:

```python
def _find_jsonb_input_parameter(self, function_metadata: FunctionMetadata) -> ParameterInfo | None:
    """
    Find the JSONB input parameter that maps to a composite type.

    SpecQL creates functions with signature:
        app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB)

    We detect the 'input_payload JSONB' parameter.
    """
    for param in function_metadata.parameters:
        if param.pg_type.lower() == 'jsonb' and param.name == 'input_payload':
            return param
    return None


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

    Priority:
    1. Explicit annotation: @fraiseql:mutation input_type=app.type_contact_input
    2. Convention: create_contact → type_create_contact_input
    """
    # Priority 1: Check for explicit input_type in annotation
    if hasattr(annotation, 'input_type') and annotation.input_type:
        if '.' in annotation.input_type:
            return annotation.input_type.split('.')[-1]
        return annotation.input_type

    # Priority 2: Convention-based extraction
    function_name = function_metadata.function_name
    return f"type_{function_name}_input"


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
    # Remove "_input" suffix
    name = name.replace("_input", "")
    # Split by underscore and capitalize
    parts = name.split("_")
    class_name = "".join(part.capitalize() for part in parts)
    # Add "Input" suffix
    return f"{class_name}Input"
```

#### Step 3.5: Implement Composite Type Input Generation

**File**: `src/fraiseql/introspection/input_generator.py`

**Add this method**:

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
    """
    # Step 1: Introspect composite type (READ from database)
    composite_metadata = await introspector.discover_composite_type(
        composite_type_name,
        schema="app"
    )

    if not composite_metadata:
        raise ValueError(
            f"Composite type '{composite_type_name}' not found in 'app' schema. "
            f"Check if SpecQL created this type."
        )

    # Step 2: Build annotations from composite type attributes
    annotations = {}

    for attr in composite_metadata.attributes:
        # Parse field metadata from comment
        field_metadata = None
        if attr.comment:
            field_metadata = self.metadata_parser.parse_field_annotation(attr.comment)

        # Determine field name (camelCase from metadata, or attribute name)
        field_name = field_metadata.name if field_metadata else attr.name

        # Map PostgreSQL type to Python type
        nullable = not field_metadata.required if field_metadata else True
        python_type = self.type_mapper.pg_type_to_python(
            attr.pg_type,
            nullable=nullable
        )

        annotations[field_name] = python_type

    # Step 3: Generate class name
    class_name = self._composite_type_to_class_name(composite_type_name)

    # Step 4: Create input class dynamically
    input_cls = type(class_name, (object,), {"__annotations__": annotations})

    return input_cls
```

#### Step 3.6: Update Main generate_input_type Method

**File**: `src/fraiseql/introspection/input_generator.py`

**Replace the existing method signature and add composite type detection**:

```python
async def generate_input_type(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation,
    introspector: "PostgresIntrospector"  # ADD THIS PARAMETER
) -> Type:
    """
    Generate input class for mutation.

    Strategy:
    1. Look for JSONB parameter (SpecQL pattern: input_payload)
    2. If found, introspect composite type (READ from DB)
    3. Otherwise, fall back to parameter-based generation (legacy)
    """
    # STRATEGY 1: Try composite type-based generation (SpecQL pattern)
    jsonb_param = self._find_jsonb_input_parameter(function_metadata)

    if jsonb_param:
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
                logger.warning(
                    f"Composite type generation failed: {e}. "
                    f"Falling back to parameter-based generation."
                )

    # STRATEGY 2: Fall back to parameter-based generation (legacy)
    return self._generate_from_parameters(function_metadata, annotation)

def _generate_from_parameters(
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation
) -> Type:
    """Generate input class from function parameters (legacy pattern)."""
    # ... existing implementation ...
```

#### Step 3.7: Add TYPE_CHECKING Import

**File**: `src/fraiseql/introspection/input_generator.py`

**At the top of the file**:

```python
import logging
from typing import TYPE_CHECKING, Type

if TYPE_CHECKING:
    from .postgres_introspector import PostgresIntrospector

logger = logging.getLogger(__name__)
```

#### Step 3.8: Run Test and Verify Pass

```bash
uv run pytest tests/unit/introspection/test_input_generator.py::test_generate_input_from_composite_type -v
```

**Expected Output** (GREEN phase):
```
PASSED test_generate_input_from_composite_type
```

**✅ GREEN Phase Complete**: Test passes with minimal implementation.

---

### 🔧 REFACTOR: Clean Up and Optimize

**Duration**: 20-30 minutes

#### Step 3.9: Code Quality Improvements

**Refactor checklist**:
- [ ] Extract magic strings ("app", "input_payload") to constants
- [ ] Add comprehensive error handling
- [ ] Add logging statements
- [ ] Consider caching composite type metadata
- [ ] Improve naming convention flexibility

**Run tests after each refactoring**:
```bash
uv run pytest tests/unit/introspection/test_input_generator.py -v
```

**✅ REFACTOR Phase Complete**: Code is clean and maintainable.

---

### ✅ QA: Verify Phase Completion

**Duration**: 15-20 minutes

```bash
uv run pytest tests/unit/introspection/ -v --tb=short
uv run ruff check src/fraiseql/introspection/input_generator.py
uv run mypy src/fraiseql/introspection/input_generator.py
```

#### Phase 5.3 Completion Checklist

- [ ] All unit tests pass
- [ ] Composite type detection works
- [ ] Falls back to parameter-based for legacy functions
- [ ] Linting passes
- [ ] Type checking passes

**✅ QA Phase Complete**: Phase 5.3 is ready for production.

---

## 🔴🟢🔧✅ PHASE 5.4: Context Parameter Auto-Detection

**Objective**: Extract context parameters (`input_tenant_id`, `input_user_id`) from function signatures.

**Time Estimate**: 1-2 hours
**Files Modified**: `mutation_generator.py`, `test_mutation_generator.py`, `auto_discovery.py`

---

### 🔴 RED: Write Failing Test

**Duration**: 10-15 minutes

#### Step 4.1: Write Failing Unit Test

**File**: `tests/unit/introspection/test_mutation_generator.py`

**Add at the end of the file**:

```python
from fraiseql.introspection import (
    MutationGenerator,
    InputGenerator,
    TypeMapper,
    FunctionMetadata,
    ParameterInfo,
)


def test_extract_context_params_new_convention():
    """Test context parameter extraction with new convention.

    Expected to FAIL initially because _extract_context_params() doesn't exist.
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

    Expected to FAIL initially.
    """
    type_mapper = TypeMapper()
    input_generator = InputGenerator(type_mapper)
    mutation_generator = MutationGenerator(input_generator)

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

    context_params = mutation_generator._extract_context_params(function)

    assert context_params == {
        "organization_id": "input_pk_organization",
        "user_id": "input_created_by"
    }
```

#### Step 4.2: Run Test and Verify Failure

```bash
uv run pytest tests/unit/introspection/test_mutation_generator.py::test_extract_context_params_new_convention -v
```

**Expected Output** (RED phase):
```
FAILED - AttributeError: 'MutationGenerator' object has no attribute '_extract_context_params'
```

**✅ RED Phase Complete**: Test fails as expected.

---

### 🟢 GREEN: Minimal Implementation

**Duration**: 20-30 minutes

#### Step 4.3: Implement _extract_context_params Method

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Add this method inside the `MutationGenerator` class**:

```python
def _extract_context_params(
    self,
    function_metadata: FunctionMetadata
) -> dict[str, str]:
    """
    Auto-detect context parameters from function signature (created by SpecQL).

    SpecQL creates functions with context parameters:
        app.create_contact(input_tenant_id UUID, input_user_id UUID, input_payload JSONB)

    Convention:
        input_tenant_id UUID   → context["tenant_id"]
        input_user_id UUID     → context["user_id"]

    Args:
        function_metadata: Function metadata from introspection (READ from DB)

    Returns:
        Mapping of context_key → function_parameter_name

    Example:
        Returns: {"tenant_id": "input_tenant_id", "user_id": "input_user_id"}
    """
    context_params = {}

    for param in function_metadata.parameters:
        # Pattern 1: input_tenant_id → tenant_id
        if param.name == 'input_tenant_id':
            context_params['tenant_id'] = param.name

        # Pattern 2: input_user_id → user_id
        elif param.name == 'input_user_id':
            context_params['user_id'] = param.name

        # Legacy pattern: input_pk_organization → organization_id
        elif param.name.startswith('input_pk_'):
            context_key = param.name.replace('input_pk_', '') + '_id'
            context_params[context_key] = param.name

        # Legacy pattern: input_created_by → user_id
        elif param.name == 'input_created_by':
            if 'user_id' not in context_params:  # Don't override input_user_id
                context_params['user_id'] = param.name

    return context_params
```

#### Step 4.4: Update generate_mutation_for_function

**File**: `src/fraiseql/introspection/mutation_generator.py`

**Update method signature and add context param extraction**:

```python
async def generate_mutation_for_function(  # ADD async
    self,
    function_metadata: FunctionMetadata,
    annotation: MutationAnnotation,
    type_registry: dict[str, Type],
    introspector: "PostgresIntrospector"  # ADD THIS PARAMETER
) -> Callable | None:
    """Generate mutation from function (created by SpecQL)."""

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
        logger.warning(f"Cannot generate mutation: missing types")
        return None

    # 3. Extract context parameters (NEW - auto-detect from function)
    context_params = self._extract_context_params(function_metadata)

    # 4. Create mutation class
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

**Add TYPE_CHECKING import**:

```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .postgres_introspector import PostgresIntrospector
```

#### Step 4.5: Update AutoDiscovery to Pass Introspector

**File**: `src/fraiseql/introspection/auto_discovery.py`

**Update the `_generate_mutation_from_function` method**:

```python
async def _generate_mutation_from_function(
    self, function_metadata: FunctionMetadata
) -> Callable | None:
    """Generate a mutation from function metadata."""
    annotation = self.metadata_parser.parse_mutation_annotation(function_metadata.comment)
    if not annotation:
        return None

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
        logger.warning(f"Failed to generate mutation: {e}")
        return None
```

#### Step 4.6: Run Test and Verify Pass

```bash
uv run pytest tests/unit/introspection/test_mutation_generator.py::test_extract_context_params_new_convention -v
```

**Expected Output** (GREEN phase):
```
PASSED test_extract_context_params_new_convention
PASSED test_extract_context_params_legacy_convention
```

**✅ GREEN Phase Complete**: Tests pass.

---

### 🔧 REFACTOR: Clean Up and Optimize

**Duration**: 15-20 minutes

Run linting and improve code quality.

```bash
uv run pytest tests/unit/introspection/test_mutation_generator.py -v
```

**✅ REFACTOR Phase Complete**: Code is clean.

---

### ✅ QA: Verify Phase Completion

**Duration**: 10-15 minutes

```bash
uv run pytest tests/unit/introspection/ -v --tb=short
uv run ruff check src/fraiseql/introspection/
uv run mypy src/fraiseql/introspection/
```

**✅ QA Phase Complete**: Phase 5.4 is ready for production.

---

## 🔴🟢🔧✅ PHASE 5.5: Integration and E2E Testing

**Objective**: Verify end-to-end flow with real SpecQL-generated schema.

**Time Estimate**: 2-3 hours
**Files Modified**: `test_composite_type_generation_integration.py`, test fixtures

---

### 🔴 RED: Write Failing Integration Test

**Duration**: 20-30 minutes

#### Step 5.1: Create Test Schema Fixture

**File**: `tests/fixtures/specql_test_schema.sql`

**Create this file with minimal SpecQL pattern**:

```sql
-- ============================================================================
-- TEST SCHEMA: Minimal SpecQL Pattern
-- ============================================================================

CREATE SCHEMA IF NOT EXISTS app;

-- Composite input type (SpecQL-generated)
CREATE TYPE app.type_create_contact_input AS (
    email TEXT,
    company_id UUID,
    status TEXT
);

-- Standard output type (SpecQL-generated)
CREATE TYPE app.mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB
);

-- App layer function (SpecQL-generated)
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
    input_data := jsonb_populate_record(
        NULL::app.type_create_contact_input,
        input_payload
    );

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

-- FraiseQL metadata (SpecQL-generated comments)
COMMENT ON TYPE app.type_create_contact_input IS
    '@fraiseql:input name=CreateContactInput';

COMMENT ON FUNCTION app.create_contact IS
    '@fraiseql:mutation
name: createContact
description: Create a new contact
success_type: Contact
failure_type: ContactError';

-- Field-level metadata (SpecQL-generated)
COMMENT ON COLUMN app.type_create_contact_input.email IS
    '@fraiseql:field name=email,type=String!,required=true';

COMMENT ON COLUMN app.type_create_contact_input.company_id IS
    '@fraiseql:field name=companyId,type=UUID,required=false';

COMMENT ON COLUMN app.type_create_contact_input.status IS
    '@fraiseql:field name=status,type=String!,required=true';
```

#### Step 5.2: Write Failing E2E Test

**File**: `tests/integration/introspection/test_composite_type_generation_integration.py`

(Already exists from earlier check, verify it's complete)

#### Step 5.3: Run Test and Verify Failure

```bash
# Apply test schema
psql fraiseql_test < tests/fixtures/specql_test_schema.sql

# Run test
uv run pytest tests/integration/introspection/test_composite_type_generation_integration.py -v
```

**Expected Output** (RED phase):
```
FAILED - Some assertion fails or mutation not generated
```

**✅ RED Phase Complete**: E2E test fails as expected.

---

### 🟢 GREEN: Make E2E Test Pass

**Duration**: 30-40 minutes

This phase involves fixing any integration issues discovered during E2E testing.

**Common issues to fix**:
1. Async/await inconsistencies
2. Missing imports
3. Type registry not populated
4. Context params not wired correctly

**Run E2E test iteratively**:

```bash
uv run pytest tests/integration/introspection/test_composite_type_generation_integration.py -v
```

**Debug failures and fix until GREEN**.

**✅ GREEN Phase Complete**: E2E test passes.

---

### 🔧 REFACTOR: Optimize Integration

**Duration**: 20-30 minutes

**Optimization opportunities**:
- Add caching for composite type metadata
- Improve error messages
- Add performance logging

```bash
uv run pytest tests/integration/introspection/ -v
```

**✅ REFACTOR Phase Complete**: Integration is optimized.

---

### ✅ QA: Final Validation

**Duration**: 30-40 minutes

#### Step 5.4: Run Full Test Suite

```bash
# All tests
uv run pytest --tb=short

# Coverage
uv run pytest --cov=src/fraiseql/introspection --cov-report=term

# Linting
uv run ruff check

# Type checking
uv run mypy
```

#### Step 5.5: Manual Validation Against PrintOptim

**Create manual test script**:

```python
# examples/test_phase_5_complete.py
import asyncio
import os
import psycopg_pool
from fraiseql.introspection import AutoDiscovery

async def main():
    database_url = os.getenv("DATABASE_URL", "postgresql://localhost/printoptim")

    pool = psycopg_pool.AsyncConnectionPool(conninfo=database_url)
    auto_discovery = AutoDiscovery(pool)

    print("🔍 Discovering schema...")
    result = await auto_discovery.discover_all(schemas=["app"])

    print(f"\n✅ Discovered {len(result['mutations'])} mutations")

    for mutation in result['mutations']:
        print(f"   - {mutation}")

    await pool.close()

asyncio.run(main())
```

**Run against PrintOptim**:

```bash
DATABASE_URL="postgresql://localhost/printoptim" python examples/test_phase_5_complete.py
```

**Expected**: All mutations discovered successfully.

#### Step 5.6: Final Completion Checklist

- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Manual test with PrintOptim succeeds
- [ ] All mutations auto-generate correctly
- [ ] Context params auto-detected
- [ ] Composite types introspected successfully
- [ ] No breaking changes to existing functionality
- [ ] Linting passes
- [ ] Type checking passes
- [ ] Performance acceptable
- [ ] Documentation updated

**✅ QA Phase Complete**: Phase 5 is production-ready.

---

## Testing Strategy

### Unit Tests (Fast, Isolated)

```bash
# Run specific phase tests
uv run pytest tests/unit/introspection/test_postgres_introspector.py -v
uv run pytest tests/unit/introspection/test_metadata_parser.py -v
uv run pytest tests/unit/introspection/test_input_generator.py -v
uv run pytest tests/unit/introspection/test_mutation_generator.py -v

# Run all unit tests
uv run pytest tests/unit/introspection/ -v --tb=short
```

### Integration Tests (Real Database)

```bash
# Setup test database
createdb fraiseql_test
psql fraiseql_test < tests/fixtures/specql_test_schema.sql

# Run integration tests
uv run pytest tests/integration/introspection/ -v --tb=short
```

### Manual Tests (PrintOptim Database)

```bash
DATABASE_URL="postgresql://localhost/printoptim" python examples/test_phase_5_complete.py
```

---

## Success Criteria

### Phase 5 Complete When:

1. ✅ All unit tests pass (100% coverage of new code)
2. ✅ All integration tests pass with SpecQL schema
3. ✅ Can discover and generate mutations from PrintOptim database
4. ✅ Generated mutations work correctly at runtime
5. ✅ No breaking changes to existing functionality
6. ✅ Context parameters auto-detected correctly
7. ✅ Composite types introspected successfully
8. ✅ Falls back to parameter-based for legacy functions
9. ✅ Linting and type checking pass
10. ✅ **Never creates or modifies database objects**

### Definition of Done:

```bash
# This command should succeed
uv run pytest --tb=short && \
uv run ruff check && \
uv run mypy && \
DATABASE_URL="postgresql://localhost/printoptim" python examples/test_phase_5_complete.py
```

**Output**: All green ✅

---

## Common Issues and Solutions

### Issue 1: "Composite type not found"

**Symptom**:
```
ValueError: Composite type 'type_create_contact_input' not found in 'app' schema
```

**Solution**:
1. Verify type exists: `psql -c "\dT app.type_*"`
2. Check naming convention: `create_contact` → `type_create_contact_input`
3. Add explicit annotation if different naming

---

### Issue 2: "Column comments not retrieved"

**Symptom**:
```python
attr.comment is None  # Expected @fraiseql:field annotation
```

**Solution**:
1. Check SpecQL added comments: `psql -c "\d+ app.type_create_contact_input"`
2. Manually add for testing:
```sql
COMMENT ON COLUMN app.type_create_contact_input.email IS '@fraiseql:field name=email,type=String!,required=true';
```

---

### Issue 3: "Async/await errors"

**Symptom**:
```
RuntimeWarning: coroutine was never awaited
```

**Solution**:
- Ensure all methods are marked `async`
- Use `await` for all async calls
- Update callers to be async

---

### Issue 4: "Test database doesn't have SpecQL schema"

**Symptom**:
```
pytest.skip("SpecQL test schema not found")
```

**Solution**:
```bash
psql fraiseql_test < tests/fixtures/specql_test_schema.sql
psql fraiseql_test -c "\dT app.type_*"  # Verify
```

---

## Final Reminders

### ⚠️ YOU ARE ONLY READING THE DATABASE

- ✅ Query `pg_type`, `pg_class`, `pg_attribute` catalogs
- ✅ Read composite types, functions, comments
- ✅ Parse metadata and generate Python code
- ❌ **NEVER** create types, functions, or comments
- ❌ **NEVER** modify database in any way
- ❌ **NEVER** execute DDL statements

### 💡 Disciplined TDD Approach

**For each phase**:
1. 🔴 RED: Write failing test first
2. 🟢 GREEN: Minimal implementation to pass
3. 🔧 REFACTOR: Clean up and optimize
4. ✅ QA: Verify quality and integration

**Never skip phases** - Each builds confidence.

---

## Congratulations! 🎉

Once complete, you'll have:
- ✅ Full composite type support in AutoFraiseQL
- ✅ Automatic context parameter detection
- ✅ Zero manual code for SpecQL-generated schemas
- ✅ Backward compatibility with existing patterns
- ✅ 100% read-only introspection (never touches database)

**You've built a production-ready meta-framework feature!**

---

**Status**: Implementation Ready
**Next Step**: Begin Phase 5.1 (RED phase - write failing test)
**Estimated Total Time**: 2-3 weeks (8-12 hours active development)
