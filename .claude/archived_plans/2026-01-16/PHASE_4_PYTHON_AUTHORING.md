# Phase 4: Python Authoring Layer - Complete Implementation Guide

**Status**: Ready to Execute
**Date**: January 14, 2026
**Current Implementation**: Foundation classes exist, needs completion and testing

---

## Overview

Phase 4 completes the Python SDK for schema authoring. The foundation is already in place:

- ✅ `fraiseql-python` package directory created
- ✅ `pyproject.toml` configured
- ✅ Basic decorator structure sketched (`decorators.py`, `analytics.py`)
- ✅ Schema export system (`schema.py`)
- ⏳ **NEEDS**: Implementation completion, testing, and PyPI publication

**Goal**: Enable Python developers to define GraphQL schemas without writing JSON, with full support for types, queries, mutations, and analytics decorators.

---

## Architecture

```
User Code (Python)
    ↓
    @fraiseql.type class User
    @fraiseql.query def users()
    ↓
FraiseQL Python SDK
    ↓
    fraiseql.export_schema() → schema.json
    ↓
fraiseql-cli compile
    ↓
    schema.compiled.json (optimized SQL)
    ↓
Rust Runtime (fraiseql-server)
    ↓
    GraphQL query execution
```

**Key Principle**: Python is **authoring only**. No runtime FFI, no language bindings needed.

---

## Current Implementation Status

### Files Present

```
fraiseql-python/
├── pyproject.toml         # ✅ Configured
├── README.md              # ✅ Exists, needs update
├── src/fraiseql/
│   ├── __init__.py        # ✅ Basic setup
│   ├── decorators.py      # ⏳ Skeleton exists
│   ├── schema.py          # ⏳ Skeleton exists
│   ├── analytics.py       # ⏳ Skeleton exists
│   ├── types.py           # ⏳ Skeleton exists
│   └── registry.py        # ⏳ Skeleton exists
├── tests/
│   ├── test_decorators.py # ✅ Tests exist
│   ├── test_analytics.py  # ✅ Tests exist
│   └── test_types.py      # ✅ Tests exist
└── examples/
    ├── basic_schema.py    # ✅ Examples exist
    └── analytics_schema.py # ✅ Examples exist
```

### What's Working

- Package structure created
- Tests written (waiting for implementation)
- Examples created (waiting for implementation)
- `pyproject.toml` properly configured

### What Needs Implementation

- Core decorator implementations
- Type system mapping
- Schema JSON generation
- Field validation directives
- Analytics decorators
- Registry and introspection
- CLI integration

---

## Phase 4 Implementation Plan

### Phase 4.1: Core Type System (Days 1-2)

**Objective**: Implement the foundation for all type mappings and field definitions.

#### 4.1.1: Implement `types.py`

**Key Classes to Implement**:

```python
class FieldDefinition:
    """Represents a GraphQL field."""
    name: str
    type_: str  # GraphQL type name (e.g., "String", "Int", "User")
    is_nullable: bool
    is_list: bool
    default_value: Any | None
    validation: dict[str, Any]  # For custom validators
    directives: list[str]  # For @index, @cache, etc.

class TypeDefinition:
    """Represents a GraphQL type (object type)."""
    name: str
    fields: dict[str, FieldDefinition]
    description: str | None
    directives: dict[str, Any]

class InputTypeDefinition:
    """Represents a GraphQL input type."""
    name: str
    fields: dict[str, FieldDefinition]
    description: str | None
```

**Type Mapping**:

```python
PYTHON_TO_GRAPHQL_TYPE_MAP = {
    str: "String",
    int: "Int",
    float: "Float",
    bool: "Boolean",
    datetime.datetime: "DateTime",
    datetime.date: "Date",
    datetime.time: "Time",
    decimal.Decimal: "Decimal",
    uuid.UUID: "UUID",
    dict: "JSON",
    list: "List",
}
```

**Implementation Tasks**:

1. Create `FieldDefinition` class with:
   - Initialization with Python type hints
   - Validation rules support
   - Directive support
   - Conversion to JSON schema

2. Create `TypeDefinition` class with:
   - Field registration
   - JSON schema generation
   - Introspection support

3. Create type mapping utilities:
   - `python_type_to_graphql(type_hint) -> str`
   - Support for `list[T]`, `T | None`, etc.
   - Custom scalar support

**Files Modified**:
- `src/fraiseql/types.py` - Complete implementation

**Tests**:
- Type mapping for all Python types
- Nullable types (`T | None`)
- List types (`list[T]`)
- Custom scalars
- JSON schema generation

---

### Phase 4.2: Core Decorators (Days 2-3)

**Objective**: Implement `@type`, `@query`, `@mutation` decorators.

#### 4.2.1: `@type` Decorator

```python
@fraiseql.type
class User:
    id: str  # Automatically primary key
    name: str
    email: str = fraiseql.Field(
        validation=fraiseql.rules.Email(),
        index=True,
    )
```

**Implementation**:

```python
def type(cls: type) -> type:
    """Convert a class into a GraphQL type."""
    # Extract type name
    # Extract fields from class annotations
    # Create TypeDefinition
    # Register in global registry
    # Store metadata on class
    # Return original class (decorators are non-intrusive)
```

**What it needs to do**:

1. Parse class name → GraphQL type name
2. Parse class annotations → fields
3. Extract default values → field defaults
4. Create `TypeDefinition` instance
5. Register in global registry (`fraiseql.registry`)
6. Attach metadata to class

**Key Features**:

- Support for type inheritance (not yet - keep simple)
- Support for `fraiseql.Field()` for custom field options
- Primary key detection (first `id` field or explicit)
- Auto-generation of `__init__` with type hints

#### 4.2.2: `@query` Decorator

```python
@fraiseql.query
def get_user(id: str) -> User:
    """Get a single user by ID."""
    pass  # No implementation needed - decorators only
```

OR with class syntax:

```python
@fraiseql.query
class UserQueries:
    def get_user(self, id: str) -> User:
        pass

    def list_users(self, limit: int = 10) -> list[User]:
        pass
```

**Implementation**:

```python
def query(func_or_class):
    """Register a query operation."""
    if inspect.isclass(func_or_class):
        # Class-based: multiple queries
        # Extract methods, convert to queries
        # Register each as separate query
    else:
        # Function-based: single query
        # Extract signature
        # Create QueryDefinition
        # Register in registry
```

**What it needs to do**:

1. Parse function/method signatures
2. Extract return type → GraphQL type
3. Extract parameters → GraphQL input arguments
4. Create `QueryDefinition` instance
5. Register in global registry

#### 4.2.3: `@mutation` Decorator

Similar to `@query`, but creates mutation operations.

```python
@fraiseql.mutation
class UserMutations:
    def create_user(self, name: str, email: str) -> User:
        pass

    def update_user(self, id: str, name: str | None = None) -> User:
        pass
```

**Implementation Tasks**:

1. Implement `type` decorator
2. Implement `query` decorator
3. Implement `mutation` decorator
4. Implement `subscription` decorator (basic)
5. Test all decorator combinations

**Files Modified**:
- `src/fraiseql/decorators.py` - Complete implementation

**Tests**:
- Type decorator registration
- Query/mutation signature parsing
- Return type extraction
- Argument type mapping
- Registry operations

---

### Phase 4.3: Field Configuration (Days 3-4)

**Objective**: Implement `fraiseql.Field()` for advanced field options.

#### 4.3.1: Field Descriptor

```python
class Field:
    """Configure a field with validation, indexing, caching, etc."""

    def __init__(
        self,
        *,
        primary_key: bool = False,
        validation: ValidationRule | None = None,
        index: bool = False,
        cache_ttl: int | None = None,
        security: SecurityRule | None = None,
        description: str | None = None,
        default: Any = None,
    ):
        ...
```

**Usage**:

```python
@fraiseql.type
class User:
    id: str = fraiseql.Field(primary_key=True)
    email: str = fraiseql.Field(
        validation=fraiseql.rules.Email(),
        index=True,
        cache_ttl=3600,
    )
    password: str = fraiseql.Field(
        security=fraiseql.rules.RequireAuth(),
    )
```

**Validation Rules**:

```python
class ValidationRule:
    """Base validation rule."""
    pass

class Email(ValidationRule):
    """Email format validation."""
    pass

class Unique(ValidationRule):
    """Database uniqueness constraint."""
    pass

class Length(ValidationRule):
    """String length validation."""
    def __init__(self, min: int | None = None, max: int | None = None):
        pass

class Pattern(ValidationRule):
    """Regex pattern validation."""
    def __init__(self, pattern: str):
        pass
```

**Security Rules**:

```python
class SecurityRule:
    """Base security rule."""
    pass

class RequireAuth(SecurityRule):
    """Require authentication."""
    pass

class RequireRole(SecurityRule):
    """Require specific role."""
    def __init__(self, role: str):
        pass
```

**Implementation Tasks**:

1. Create `Field` descriptor class
2. Create `ValidationRule` base class
3. Implement validation rules: `Email`, `Unique`, `Length`, `Pattern`
4. Create `SecurityRule` base class
5. Implement security rules: `RequireAuth`, `RequireRole`
6. Integration with `@type` decorator

**Files Modified**:
- `src/fraiseql/types.py` - Add `Field`, validation rules
- `src/fraiseql/decorators.py` - Use Field in decorator

**Tests**:
- Field descriptor behavior
- Validation rule application
- Security rule application
- Schema generation with field options

---

### Phase 4.4: Schema Generation (Days 4-5)

**Objective**: Implement JSON schema generation from decorated classes.

#### 4.4.1: Schema Generator

```python
class SchemaGenerator:
    """Generate JSON schema from decorated classes."""

    def __init__(self):
        self.registry = fraiseql.registry

    def generate(self) -> dict:
        """Generate complete schema JSON."""
        return {
            "version": "1.0.0",
            "types": self._generate_types(),
            "queries": self._generate_queries(),
            "mutations": self._generate_mutations(),
            "subscriptions": self._generate_subscriptions(),
        }

    def _generate_types(self) -> dict:
        """Generate type definitions."""
        # Iterate over registered types
        # Convert each TypeDefinition to JSON
        # Return dict of type name → type schema

    def _generate_queries(self) -> dict:
        """Generate query operations."""
        # Similar for queries

    def _generate_mutations(self) -> dict:
        """Similar for mutations."""

    def _generate_subscriptions(self) -> dict:
        """Similar for subscriptions."""
```

**Schema JSON Format**:

```json
{
  "version": "1.0.0",
  "types": {
    "User": {
      "kind": "OBJECT",
      "fields": {
        "id": {
          "type": "String!",
          "directives": []
        },
        "name": {
          "type": "String!",
          "directives": []
        },
        "email": {
          "type": "String!",
          "directives": [
            {
              "name": "email",
              "args": {}
            },
            {
              "name": "index",
              "args": {}
            }
          ]
        }
      }
    }
  },
  "queries": {
    "getUser": {
      "args": {
        "id": {"type": "String!"}
      },
      "returns": "User"
    }
  },
  "mutations": {},
  "subscriptions": {}
}
```

**Implementation Tasks**:

1. Create `SchemaGenerator` class
2. Implement `_generate_types()`
3. Implement `_generate_queries()`
4. Implement `_generate_mutations()`
5. Implement `_generate_subscriptions()`
6. Create `config()` function for query/mutation configuration

**Config Function**:

```python
@fraiseql.query
def get_user(id: str) -> User:
    return fraiseql.config(
        sql_source="v_user",  # View or table name
        sql_where="id = $1",  # SQL WHERE clause
        returns_one=True,      # Returns single row vs list
    )

@fraiseql.query
def list_users(limit: int = 10) -> list[User]:
    return fraiseql.config(
        sql_source="v_user",
        sql_limit="$1",
        sql_order_by="id ASC",
        returns_list=True,
    )
```

**Files Modified**:
- `src/fraiseql/schema.py` - Complete SchemaGenerator
- `src/fraiseql/decorators.py` - Add config() function

**Tests**:
- Schema generation for types
- Schema generation for queries
- Schema generation for mutations
- JSON structure validation
- Config function behavior

---

### Phase 4.5: Analytics Decorators (Days 5-6)

**Objective**: Implement analytics-specific decorators for fact tables and aggregations.

#### 4.5.1: Fact Table Decorator

```python
from fraiseql import FactTable, Dimension, Measure

@fraiseql.type
@FactTable
class TfSales:
    id: str = fraiseql.Field(primary_key=True)
    product_id: str = Dimension()
    category: str = Dimension()
    date: str = Dimension()

    revenue: decimal.Decimal = Measure()
    quantity: int = Measure()
```

**Implementation**:

```python
class Dimension:
    """Mark a field as a dimension for grouping."""
    pass

class Measure:
    """Mark a field as a measure for aggregation."""
    pass

def fact_table(cls: type) -> type:
    """Mark a type as a fact table."""
    # Store fact table metadata
    # Validate dimensions and measures exist
    # Register in analytics registry
    return cls
```

#### 4.5.2: Aggregate Query Decorator

```python
from fraiseql import AggregateQuery, GroupBy

@fraiseql.query
@AggregateQuery(fact_table=TfSales)
class SalesAnalytics:
    by_product: GroupBy = GroupBy(
        dimensions=['product_id'],
        measures=['revenue', 'quantity'],
    )

    by_category_date: GroupBy = GroupBy(
        dimensions=['category', 'date'],
        measures=['revenue'],
        filters=[
            {'field': 'date', 'op': 'gte', 'value': '2024-01-01'}
        ]
    )
```

**Implementation**:

```python
class GroupBy:
    """Specify dimensions, measures, and filters for aggregation."""
    def __init__(
        self,
        dimensions: list[str],
        measures: list[str],
        filters: list[dict] | None = None,
        order_by: str | None = None,
    ):
        ...

def aggregate_query(cls: type) -> type:
    """Create aggregate queries from fact table."""
    # Extract GroupBy fields
    # Validate dimensions/measures exist in fact table
    # Generate aggregate query definitions
    # Register in registry
    return cls
```

**Implementation Tasks**:

1. Create `Dimension` and `Measure` markers
2. Create `@fact_table` decorator
3. Create `GroupBy` specification class
4. Create `@aggregate_query` decorator
5. Validation of dimensions/measures
6. Schema generation for aggregate queries

**Files Modified**:
- `src/fraiseql/analytics.py` - Complete implementation

**Tests**:
- Dimension/measure detection
- Fact table validation
- Aggregate query generation
- Filter specification
- Schema generation for analytics

---

### Phase 4.6: Registry & Introspection (Day 6)

**Objective**: Implement global registry for introspection and schema lookup.

#### 4.6.1: Global Registry

```python
# fraiseql/registry.py

class Registry:
    """Global registry for all decorated definitions."""

    def __init__(self):
        self.types: dict[str, TypeDefinition] = {}
        self.queries: dict[str, QueryDefinition] = {}
        self.mutations: dict[str, MutationDefinition] = {}
        self.subscriptions: dict[str, SubscriptionDefinition] = {}

    def register_type(self, type_def: TypeDefinition):
        """Register a type definition."""
        if type_def.name in self.types:
            raise ValueError(f"Type {type_def.name} already registered")
        self.types[type_def.name] = type_def

    def register_query(self, query_def: QueryDefinition):
        """Register a query definition."""
        ...

    def get_type(self, name: str) -> TypeDefinition | None:
        """Get a type definition by name."""
        return self.types.get(name)

    def to_dict(self) -> dict:
        """Export registry as dict."""
        ...

# Global instance
_global_registry = Registry()

def get_registry() -> Registry:
    """Get the global registry."""
    return _global_registry

def clear_registry():
    """Clear the global registry (for testing)."""
    global _global_registry
    _global_registry = Registry()
```

**Implementation Tasks**:

1. Create `Registry` class with type/query/mutation storage
2. Implement registration methods
3. Implement lookup methods
4. Implement `to_dict()` export
5. Create global registry instance
6. Update decorators to use registry

**Files Modified**:
- `src/fraiseql/registry.py` - Complete implementation
- `src/fraiseql/decorators.py` - Use registry
- `src/fraiseql/schema.py` - Use registry

**Tests**:
- Registry storage
- Type registration
- Query/mutation registration
- Lookup methods
- Registry export

---

### Phase 4.7: Export & CLI Integration (Day 7)

**Objective**: Implement `export_schema()` function and integration with CLI.

#### 4.7.1: Export Function

```python
def export_schema(
    filepath: str = "schema.json",
    pretty: bool = True,
) -> None:
    """Export schema to JSON file.

    This is called after all decorators have been applied.

    Example:
        @fraiseql.type
        class User:
            ...

        @fraiseql.query
        def get_user(id: str) -> User:
            ...

        # Export to schema.json
        fraiseql.export_schema()
    """
    generator = SchemaGenerator()
    schema = generator.generate()

    import json
    with open(filepath, 'w') as f:
        if pretty:
            json.dump(schema, f, indent=2)
        else:
            json.dump(schema, f)

    print(f"Schema exported to {filepath}")
```

**Implementation Tasks**:

1. Create `export_schema()` function
2. Handle file I/O
3. Handle JSON serialization
4. Pretty printing support
5. Error handling

**Files Modified**:
- `src/fraiseql/schema.py` - Add export_schema()

**Tests**:
- Export to file
- JSON validity
- Pretty printing
- Error handling

---

### Phase 4.8: Testing & Quality (Days 7-8)

**Objective**: Comprehensive testing and validation.

#### 4.8.1: Unit Tests

Ensure all tests in `tests/` pass:

```bash
cd fraiseql-python
python -m pytest tests/ -v
```

**Test Coverage Areas**:

1. **Type System** (`test_types.py`):
   - Type mapping (all Python types)
   - Nullable types
   - List types
   - Custom scalars
   - Field definitions

2. **Decorators** (`test_decorators.py`):
   - `@type` decorator
   - `@query` decorator
   - `@mutation` decorator
   - `@subscription` decorator
   - Decorator combinations

3. **Analytics** (`test_analytics.py`):
   - `@fact_table` decorator
   - `@aggregate_query` decorator
   - Dimension/measure detection
   - Filter validation

4. **Schema Generation**:
   - JSON structure
   - Type definitions
   - Query/mutation definitions
   - Analytics definitions

5. **Registry**:
   - Registration
   - Lookup
   - Export

#### 4.8.2: Integration Tests

```python
# Example integration test
def test_end_to_end_schema_export(tmp_path):
    """Test: decorators → schema.json"""
    fraiseql.clear_registry()

    @fraiseql.type
    class User:
        id: str
        name: str

    @fraiseql.query
    def get_user(id: str) -> User:
        pass

    output_file = tmp_path / "schema.json"
    fraiseql.export_schema(str(output_file))

    assert output_file.exists()

    import json
    schema = json.loads(output_file.read_text())
    assert "User" in schema["types"]
    assert "getUser" in schema["queries"]
```

#### 4.8.3: Code Quality

```bash
# Linting
ruff check src/ tests/

# Type checking (optional)
mypy src/ --ignore-missing-imports

# Format check
ruff format --check src/ tests/
```

**Implementation Tasks**:

1. Run all tests
2. Fix failures
3. Achieve 90%+ code coverage
4. Fix all lint warnings
5. Document any known limitations

**Files to Create/Update**:
- `tests/test_decorators.py` - Complete
- `tests/test_types.py` - Complete
- `tests/test_analytics.py` - Complete
- `tests/test_schema.py` - New
- `tests/test_registry.py` - New
- `tests/test_integration.py` - New

---

### Phase 4.9: Documentation (Days 8-9)

**Objective**: Complete documentation for Python SDK.

#### 4.9.1: Update README.md

Include:

```markdown
# FraiseQL Python SDK

Schema authoring for FraiseQL v2.

## Installation

```bash
pip install fraiseql
```

## Quick Start

```python
import fraiseql

@fraiseql.type
class User:
    id: str
    name: str
    email: str

@fraiseql.query
def get_user(id: str) -> User:
    pass

fraiseql.export_schema()
```

## Full Documentation

See `docs/python/` for:
- [Getting Started](docs/python/GETTING_STARTED.md)
- [API Reference](docs/python/API_REFERENCE.md)
- [Examples](docs/python/EXAMPLES.md)
```

#### 4.9.2: Create `docs/python/` Documentation

**Files to Create**:

1. **INSTALLATION.md** (300 words)
   - pip install instructions
   - Python version requirements
   - Dependency list
   - Troubleshooting

2. **GETTING_STARTED.md** (800 words)
   - Basic schema definition
   - Running first example
   - Compiling to schema.json
   - Integration with fraiseql-cli

3. **DECORATORS_REFERENCE.md** (1000 words)
   - `@type` decorator
   - `@query` decorator
   - `@mutation` decorator
   - `@subscription` decorator
   - Field configuration
   - Validation rules
   - Security rules

4. **ANALYTICS_GUIDE.md** (1000 words)
   - Fact tables
   - Dimensions and measures
   - Aggregate queries
   - Filter specification
   - Example analytics schemas

5. **EXAMPLES.md** (1500 words)
   - Basic CRUD schema
   - E-commerce schema
   - Analytics schema
   - Federation patterns
   - Enterprise RBAC

6. **TROUBLESHOOTING.md** (500 words)
   - Common errors
   - Debug tips
   - FAQ

**Implementation Tasks**:

1. Update `README.md`
2. Create `docs/python/INSTALLATION.md`
3. Create `docs/python/GETTING_STARTED.md`
4. Create `docs/python/DECORATORS_REFERENCE.md`
5. Create `docs/python/ANALYTICS_GUIDE.md`
6. Create `docs/python/EXAMPLES.md`
7. Create `docs/python/TROUBLESHOOTING.md`

---

### Phase 4.10: PyPI Publication & Release (Day 9)

**Objective**: Publish Python SDK to PyPI.

#### 4.10.1: Pre-Publication Checklist

- [ ] All tests passing
- [ ] No lint warnings
- [ ] Version updated in `pyproject.toml` (suggest v2.0.0-beta.1)
- [ ] CHANGELOG.md updated
- [ ] All documentation complete
- [ ] Examples working correctly

#### 4.10.2: Build & Test Distribution

```bash
cd fraiseql-python

# Install build tools
pip install build twine

# Build distribution
python -m build

# Verify build
twine check dist/*

# (Optional) Upload to TestPyPI first
twine upload --repository testpypi dist/*
pip install --index-url https://test.pypi.org/simple/ fraiseql
```

#### 4.10.3: Publish to PyPI

```bash
# Upload to PyPI
twine upload dist/*

# Verify installation
pip install fraiseql
python -c "import fraiseql; print(fraiseql.__version__)"
```

#### 4.10.4: Create Release

- [ ] Create GitHub release with changelog
- [ ] Link to PyPI package
- [ ] Add installation instructions
- [ ] Include example code

**Implementation Tasks**:

1. Verify all quality gates passed
2. Update version to v2.0.0-beta.1
3. Build distribution
4. Test distribution locally
5. Upload to PyPI
6. Create GitHub release
7. Announce release (blog post, Twitter, etc.)

---

## Quality Gates

**All of these must pass before moving to Phase 5**:

```bash
# 1. All tests pass
cd fraiseql-python
python -m pytest tests/ -v --cov=src/fraiseql --cov-report=term-missing

# Expected: 90%+ coverage

# 2. No lint warnings
ruff check src/ tests/
ruff format --check src/ tests/

# Expected: 0 warnings

# 3. Build succeeds
python -m build

# 4. Documentation builds (if using Sphinx)
cd docs/python
sphinx-build -b html . _build

# 5. Examples run without error
python examples/basic_schema.py
python examples/analytics_schema.py
```

---

## Success Criteria

✅ **Phase 4 is complete when**:

1. **Implementation**:
   - All decorators working (`@type`, `@query`, `@mutation`, `@subscription`)
   - Field configuration system complete
   - Analytics decorators complete
   - Registry system functional
   - JSON schema generation working

2. **Testing**:
   - 95%+ test coverage
   - All integration tests passing
   - All examples working
   - No flaky tests

3. **Documentation**:
   - README updated
   - API reference complete
   - Getting started guide written
   - Analytics guide written
   - 5+ examples provided
   - Troubleshooting section complete

4. **Quality**:
   - All lint checks passing
   - No type errors
   - Code formatted consistently
   - All dependencies specified

5. **Distribution**:
   - Published to PyPI
   - Installation tested
   - Package metadata correct
   - GitHub release created

---

## Estimated Timeline

| Phase | Days | Tasks |
|-------|------|-------|
| 4.1 | 2 | Type system, field definitions |
| 4.2 | 1.5 | Core decorators |
| 4.3 | 1 | Field configuration |
| 4.4 | 1.5 | Schema generation |
| 4.5 | 1 | Analytics decorators |
| 4.6 | 1 | Registry system |
| 4.7 | 1 | Export & CLI integration |
| 4.8 | 1.5 | Testing & quality |
| 4.9 | 1.5 | Documentation |
| 4.10 | 1 | PyPI publication |
| **Total** | **~12 days** | **Full Phase 4** |

---

## Rollout Strategy

### Phase 4.1-4.7: Core Implementation (7 days)

- Daily commits for each sub-phase
- Continuous testing as you go
- Rapid iteration

### Phase 4.8: Quality Gate Verification (1-2 days)

- Run full test suite
- Fix any issues
- Verify coverage targets

### Phase 4.9: Documentation (1-2 days)

- Write comprehensive guides
- Create examples
- Review and polish

### Phase 4.10: Release (1 day)

- Final verification
- Publish to PyPI
- Create GitHub release

---

## Dependencies

**Runtime**:
- Python 3.10+
- No external dependencies (pure Python)

**Development**:
- `pytest` - Testing
- `ruff` - Linting and formatting
- `hatchling` - Build system
- `build` - Building distributions
- `twine` - PyPI upload

---

## Key Design Decisions

1. **No Runtime FFI**: Decorators generate JSON only. No Python-Rust bridge.
2. **Pure Python**: Single dependency-free package (no external libs).
3. **Class-based decorators**: Optional, for better IDE support and type hints.
4. **Registry pattern**: Global registry for schema introspection.
5. **Config function**: Separate `fraiseql.config()` for SQL hints (not in decorator).

---

## Next After Phase 4

**Phase 5: TypeScript/JavaScript Authoring**

Once Python SDK is published, similar effort for TypeScript:
- TypeScript decorators (experimental decorators)
- npm package (`@fraiseql/core`)
- Similar feature set to Python
- Published to npm registry

---

## Notes for Implementation

1. **Start with types.py**: This is foundational for everything else.
2. **Tests first**: Example tests are already written - implement code to pass them.
3. **Type hints everywhere**: Use Python 3.10+ type hints (`X | None`, `list[T]`).
4. **Registry pattern**: Makes it easy to add features later.
5. **JSON schema**: Keep close to GraphQL schema format for easy compilation.

---

## Files Checklist

### Implement

- [ ] `src/fraiseql/types.py` - Type system, Field descriptor, validation rules
- [ ] `src/fraiseql/decorators.py` - @type, @query, @mutation, @subscription, config()
- [ ] `src/fraiseql/analytics.py` - @fact_table, @aggregate_query, Dimension, Measure
- [ ] `src/fraiseql/schema.py` - SchemaGenerator, export_schema()
- [ ] `src/fraiseql/registry.py` - Global registry

### Test

- [ ] `tests/test_types.py` - Complete and pass
- [ ] `tests/test_decorators.py` - Complete and pass
- [ ] `tests/test_analytics.py` - Complete and pass
- [ ] `tests/test_schema.py` - New, comprehensive schema tests
- [ ] `tests/test_registry.py` - New, registry tests
- [ ] `tests/test_integration.py` - New, end-to-end tests

### Document

- [ ] `README.md` - Update
- [ ] `docs/python/INSTALLATION.md` - Create
- [ ] `docs/python/GETTING_STARTED.md` - Create
- [ ] `docs/python/DECORATORS_REFERENCE.md` - Create
- [ ] `docs/python/ANALYTICS_GUIDE.md` - Create
- [ ] `docs/python/EXAMPLES.md` - Create
- [ ] `docs/python/TROUBLESHOOTING.md` - Create

### Release

- [ ] Version update to v2.0.0-beta.1
- [ ] GitHub release created
- [ ] Package published to PyPI
- [ ] Installation verified

---

## Summary

Phase 4 is a well-scoped 10-12 day effort to complete the Python SDK. The foundation is already in place - you're primarily implementing the decorators, type system, and schema generation. The tests are already written, so you know exactly what needs to work. Publication to PyPI is straightforward and gives immediate value to Python developers.

**Ready to start?** Begin with Phase 4.1 (Type System implementation) and work through systematically, one sub-phase per day.
