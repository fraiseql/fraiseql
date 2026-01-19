# Phase 8: Python Authoring SDK Implementation Plan

**Phase**: 8 - Authoring Layer
**Objective**: Build Python decorators and schema generation for FraiseQL
**Estimated Duration**: 7-10 days
**Status**: 📋 PLANNING

---

## 🎯 Executive Summary

Phase 8 implements the **authoring layer** - a complete Python SDK that lets developers define FraiseQL schemas using Python decorators. Once defined, the schema generates JSON that can be compiled by `fraiseql-cli` and executed by `fraiseql-server`.

This phase completes the **left side of the architecture**:

```
┌─────────────────────┐
│ Python Authoring    │  ← YOU ARE HERE (Phase 8)
│ @Type, @Query, etc  │
└──────────┬──────────┘
           │
           ↓ (generates)
┌─────────────────────┐
│ schema.json         │
└──────────┬──────────┘
           │
           ↓ (fraiseql-cli)
┌─────────────────────┐
│ schema.compiled.json│  ← Existing (Phases 4-5)
└──────────┬──────────┘
           │
           ↓ (fraiseql-server)
┌─────────────────────┐
│ GraphQL Execution   │  ← Existing (Phase 6)
└─────────────────────┘
```

**Key Principle**: Python → JSON → SQL (no runtime Rust bindings)

---

## 📊 Current Status

### What's Completed

- ✅ Phase 1-7: Core engine, database, caching, cascade
- ✅ Phase 4-6: Schema compilation and GraphQL execution
- ✅ Phase 7: Entity-level cache invalidation

### What's Missing (Phase 8)

- ❌ Python decorator layer (`@Type`, `@Field`, `@Query`, `@Mutation`)
- ❌ Schema generator (Python → JSON)
- ❌ Type system mapping
- ❌ Analytics support (`@FactTable`, `@Dimension`, `@Measure`)
- ❌ Python package (`pyproject.toml`, `setup.py`)
- ❌ Examples and documentation

---

## 🏗️ Phase 8 Architecture

### Directory Structure

```
fraiseql_python/
├── __init__.py                      # Package entry point
│
├── fraiseql/                        # Main package
│   ├── __init__.py                  # Exports: Type, Field, Query, Mutation
│   ├── core/                        # Core decorators
│   │   ├── __init__.py
│   │   ├── decorators.py            # @Type, @Field base classes
│   │   ├── type_system.py           # Python ↔ JSON type mapping
│   │   └── field.py                 # Field metadata & constraints
│   │
│   ├── query/                       # Query/Mutation builders
│   │   ├── __init__.py
│   │   ├── query.py                 # @Query decorator
│   │   ├── mutation.py              # @Mutation decorator
│   │   └── resolver.py              # Resolver metadata
│   │
│   ├── schema/                      # Schema generation
│   │   ├── __init__.py
│   │   ├── generator.py             # Main schema generator
│   │   ├── validator.py             # Validate generated schema
│   │   ├── exporter.py              # Export to JSON/file
│   │   └── resolver_detector.py     # Auto-detect SQL resolvers
│   │
│   ├── analytics/                   # Analytics support
│   │   ├── __init__.py
│   │   ├── fact_table.py            # @FactTable decorator
│   │   ├── dimensions.py            # @Dimension marker
│   │   ├── measures.py              # @Measure marker
│   │   ├── aggregate.py             # Aggregate query generator
│   │   └── rollup.py                # Rollup & drill-down
│   │
│   ├── types/                       # Built-in types
│   │   ├── __init__.py
│   │   ├── scalars.py               # String, Int, Boolean, Float, UUID
│   │   ├── objects.py               # Object types
│   │   ├── enums.py                 # Enum types
│   │   ├── lists.py                 # List types
│   │   └── json_mapping.py          # JSON serialization
│   │
│   └── utils/                       # Utilities
│       ├── __init__.py
│       ├── naming.py                # snake_case ↔ camelCase
│       ├── validation.py            # Input validation
│       └── logging.py               # Debug logging
│
├── tests/                           # Test suite
│   ├── __init__.py
│   ├── test_decorators.py           # @Type, @Field tests
│   ├── test_type_mapping.py         # Python → JSON type conversion
│   ├── test_query_builder.py        # @Query/@Mutation tests
│   ├── test_schema_generator.py     # Schema generation tests
│   ├── test_analytics.py            # @FactTable, @Dimension tests
│   ├── test_json_export.py          # JSON export validation
│   ├── test_integration.py          # E2E: decorators → JSON → CLI
│   └── fixtures/                    # Test schemas
│       ├── basic_schema.py
│       ├── analytics_schema.py
│       └── complex_schema.py
│
├── examples/                        # Example schemas
│   ├── basic.py                     # Simple CRUD
│   ├── blog.py                      # Blog app
│   ├── ecommerce.py                 # E-commerce analytics
│   ├── social_media.py              # Complex relationships
│   └── README.md                    # Example guide
│
├── docs/                            # Documentation
│   ├── GETTING_STARTED.md           # Quick start
│   ├── API_REFERENCE.md             # Full API docs
│   ├── ANALYTICS_GUIDE.md           # Analytics walkthrough
│   ├── TYPE_SYSTEM.md               # Type mapping docs
│   ├── MIGRATION.md                 # v1 → v2 migration
│   └── examples.md                  # Example walkthroughs
│
├── pyproject.toml                   # Modern Python packaging
├── setup.py                         # Legacy support (still used)
├── setup.cfg                        # Build config
├── MANIFEST.in                      # File inclusion
├── README.md                        # Package README
├── LICENSE                          # Same as FraiseQL
├── .gitignore                       # Python-specific excludes
├── requirements.txt                 # Dependencies
├── requirements-dev.txt             # Dev dependencies
├── tox.ini                          # Test matrix
├── .pre-commit-config.yaml          # Pre-commit hooks
└── conftest.py                      # Pytest configuration

```

### Key Dependencies

```toml
# pyproject.toml
[build-system]
requires = ["setuptools>=65.0", "wheel"]
build-backend = "setuptools.build_meta"

[project]
name = "fraiseql"
version = "2.0.0"
description = "Compiled GraphQL execution engine - Python authoring SDK"
requires-python = ">=3.10"

[project.optional-dependencies]
dev = [
    "pytest>=7.0",
    "pytest-cov>=4.0",
    "black>=23.0",
    "ruff>=0.1",
    "mypy>=1.0",
    "sphinx>=6.0",
]
analytics = ["numpy>=1.20", "pandas>=1.3"]  # Optional
```

---

## 📋 Implementation Tasks

### Phase 8.1: Core Decorators (2 days)

**Objective**: Implement basic type system and field definitions

#### Task 8.1.1: Type System Foundation

**File**: `fraiseql/core/type_system.py`

```python
# Type mapping: Python → JSON
PYTHON_TO_JSON_TYPE = {
    str: "string",
    int: "integer",
    float: "float",
    bool: "boolean",
    UUID: "uuid",
    List[str]: "string[]",
    dict: "json",
}

# JSON field schema structure
@dataclass
class FieldSchema:
    name: str
    type: str
    nullable: bool = False
    primary_key: bool = False
    description: str | None = None
    default: Any | None = None
    constraints: dict[str, Any] = field(default_factory=dict)

# Type registry
class TypeRegistry:
    def __init__(self):
        self.types: dict[str, TypeDef] = {}

    def register(self, cls, fields):
        """Register a @Type class"""

    def get_type(self, name: str) -> TypeDef | None:
        """Retrieve registered type"""
```

**Tests** (`test_type_mapping.py`):

- ✅ Map Python str → JSON string
- ✅ Map Python int → JSON integer
- ✅ Map Python List[str] → JSON string[]
- ✅ Map Python UUID → JSON uuid
- ✅ Handle Optional[T] → nullable: true
- ✅ Reject unsupported types

#### Task 8.1.2: Field Decorator

**File**: `fraiseql/core/field.py`

```python
@dataclass
class Field:
    """Mark a field and add metadata"""
    primary_key: bool = False
    nullable: bool = False
    description: str | None = None
    default: Any | None = None
    index: bool = False
    unique: bool = False

    # Custom constraints
    min_length: int | None = None
    max_length: int | None = None
    pattern: str | None = None
    min_value: float | None = None
    max_value: float | None = None

# Usage:
@Type
class User:
    id: str = Field(primary_key=True)
    name: str = Field(min_length=1, max_length=255)
    email: str = Field(unique=True)
    age: int | None = Field(nullable=True, min_value=0)
```

**Tests** (`test_decorators.py`):

- ✅ Parse Field metadata from class
- ✅ Extract primary key designation
- ✅ Handle nullable fields
- ✅ Store constraints (min_length, etc.)
- ✅ Support default values
- ✅ Reject invalid constraint combinations

#### Task 8.1.3: Type Decorator

**File**: `fraiseql/core/decorators.py`

```python
def Type(cls: type) -> type:
    """Decorator to mark a class as a GraphQL type"""

    # Extract fields from class annotations
    fields = {}
    for field_name, field_type in cls.__annotations__.items():
        field_default = getattr(cls, field_name, Field())
        fields[field_name] = FieldSchema(
            name=field_name,
            type=python_type_to_json(field_type),
            ...
        )

    # Store metadata on class
    cls._fraiseql_fields = fields
    cls._fraiseql_type_name = cls.__name__

    # Register globally
    GLOBAL_TYPE_REGISTRY.register(cls, fields)

    return cls

# Usage:
@Type
class User:
    id: str = Field(primary_key=True)
    name: str
    email: str
```

**Tests**:

- ✅ Decorator preserves class functionality
- ✅ Extracts all annotated fields
- ✅ Registers in global registry
- ✅ Handles inheritance
- ✅ Works with dataclasses

---

### Phase 8.2: Query & Mutation Builders (2 days)

**Objective**: Implement query and mutation definitions

#### Task 8.2.1: Query Decorator

**File**: `fraiseql/query/query.py`

```python
def Query(cls: type) -> type:
    """Mark a class as containing query definitions"""

    cls._fraiseql_queries = {}

    # Extract methods that return Types
    for method_name, method in inspect.getmembers(cls, predicate=inspect.isfunction):
        if method_name.startswith('_'):
            continue

        # Get return type
        return_type = get_type_hints(method).get('return')

        # Create query definition
        cls._fraiseql_queries[method_name] = QueryDef(
            name=method_name,
            return_type=return_type.__name__,
            arguments=extract_method_arguments(method),
            description=method.__doc__,
        )

    GLOBAL_QUERY_REGISTRY.register(cls, cls._fraiseql_queries)
    return cls

# Usage:
@Query
class UserQueries:
    def get_user(user_id: str) -> User:
        """Get a user by ID"""
        pass

    def list_users(limit: int = 10) -> [User]:
        """List all users with optional limit"""
        pass
```

**Tests** (`test_query_builder.py`):

- ✅ Extract query methods from class
- ✅ Infer return types
- ✅ Parse method arguments
- ✅ Store description docstrings
- ✅ Handle optional parameters
- ✅ Support list returns

#### Task 8.2.2: Mutation Decorator

**File**: `fraiseql/query/mutation.py`

```python
def Mutation(cls: type) -> type:
    """Mark a class as containing mutation definitions"""

    # Similar to Query but tracks mutations separately
    cls._fraiseql_mutations = {}

    for method_name, method in inspect.getmembers(cls, predicate=inspect.isfunction):
        if method_name.startswith('_'):
            continue

        cls._fraiseql_mutations[method_name] = MutationDef(
            name=method_name,
            return_type=get_type_hints(method).get('return').__name__,
            arguments=extract_method_arguments(method),
            side_effects=extract_cascade_metadata(method),  # Phase 7 integration!
        )

    GLOBAL_MUTATION_REGISTRY.register(cls, cls._fraiseql_mutations)
    return cls

# Usage:
@Mutation
class UserMutations:
    def create_user(name: str, email: str) -> User:
        """Create a new user"""
        pass

    def update_user(user_id: str, name: str) -> User:
        """Update user details"""
        pass
```

**Tests**:

- ✅ Extract mutation methods
- ✅ Track mutation side effects
- ✅ Support input objects
- ✅ Infer return types
- ✅ Handle error cases

---

### Phase 8.3: Schema Generator (2 days)

**Objective**: Convert decorated classes → JSON schema

#### Task 8.3.1: Schema Generator Core

**File**: `fraiseql/schema/generator.py`

```python
class SchemaGenerator:
    """Generate FraiseQL JSON schema from decorated Python classes"""

    def __init__(self):
        self.types = {}
        self.queries = {}
        self.mutations = {}

    def generate(self, classes: list[type]) -> SchemaDict:
        """Generate schema from list of decorated classes"""

        # Collect all types
        for cls in classes:
            if hasattr(cls, '_fraiseql_fields'):
                self.types[cls.__name__] = self._generate_type(cls)

            if hasattr(cls, '_fraiseql_queries'):
                self.queries.update(self._generate_queries(cls))

            if hasattr(cls, '_fraiseql_mutations'):
                self.mutations.update(self._generate_mutations(cls))

        return {
            "types": self.types,
            "queries": self.queries,
            "mutations": self.mutations,
            "version": "2.0.0",
        }

    def _generate_type(self, cls: type) -> dict:
        """Convert @Type class to JSON type def"""
        return {
            "name": cls.__name__,
            "fields": [
                {
                    "name": field.name,
                    "type": field.type,
                    "nullable": field.nullable,
                    "primaryKey": field.primary_key,
                    "constraints": field.constraints,
                }
                for field in cls._fraiseql_fields.values()
            ],
        }

    def _generate_queries(self, cls: type) -> dict:
        """Convert @Query class to queries"""
        queries = {}
        for query_name, query_def in cls._fraiseql_queries.items():
            queries[query_name] = {
                "name": query_def.name,
                "returnType": query_def.return_type,
                "arguments": query_def.arguments,
                "description": query_def.description,
            }
        return queries

    def _generate_mutations(self, cls: type) -> dict:
        """Convert @Mutation class to mutations"""
        mutations = {}
        for mut_name, mut_def in cls._fraiseql_mutations.items():
            mutations[mut_name] = {
                "name": mut_def.name,
                "returnType": mut_def.return_type,
                "arguments": mut_def.arguments,
                "cascade": mut_def.side_effects,  # Phase 7!
            }
        return mutations

# Usage:
generator = SchemaGenerator()
schema = generator.generate([User, UserQueries, UserMutations])
```

**Tests** (`test_schema_generator.py`):

- ✅ Generate type definitions
- ✅ Generate query definitions
- ✅ Generate mutation definitions
- ✅ Handle nested types
- ✅ Validate type references
- ✅ Export valid JSON

#### Task 8.3.2: Schema Exporter

**File**: `fraiseql/schema/exporter.py`

```python
class SchemaExporter:
    """Export generated schema to JSON file"""

    @staticmethod
    def to_json(schema: SchemaDict) -> str:
        """Convert schema dict to JSON string"""
        return json.dumps(schema, indent=2)

    @staticmethod
    def to_file(schema: SchemaDict, path: str) -> None:
        """Write schema to file"""
        with open(path, 'w') as f:
            json.dump(schema, f, indent=2)

    @staticmethod
    def validate(schema: SchemaDict) -> None:
        """Validate schema structure"""
        if not schema.get('types'):
            raise ValueError("Schema must have 'types'")

        if not schema.get('queries'):
            raise ValueError("Schema must have 'queries'")

        # Validate all type references
        type_names = set(schema['types'].keys())

        for query in schema['queries'].values():
            if query['returnType'] not in type_names:
                raise ValueError(f"Unknown return type: {query['returnType']}")

# Usage:
generator = SchemaGenerator()
schema = generator.generate([User, UserQueries, UserMutations])
SchemaExporter.to_file(schema, 'schema.json')
```

**Tests**:

- ✅ Generate valid JSON
- ✅ Write to file with correct formatting
- ✅ Validate schema structure
- ✅ Detect missing types
- ✅ Detect circular references

---

### Phase 8.4: Analytics Support (2 days)

**Objective**: Implement data warehouse patterns

#### Task 8.4.1: Fact Table Decorator

**File**: `fraiseql/analytics/fact_table.py`

```python
@dataclass
class FactTableMeta:
    """Metadata for fact tables"""
    grain: list[str]  # Unique identifier columns
    description: str | None = None

def FactTable(grain: list[str]):
    """Decorator for fact tables (many rows, aggregatable data)"""

    def decorator(cls: type) -> type:
        # Mark fields as measures vs dimensions
        cls._fraiseql_fact_table = FactTableMeta(grain=grain)

        # Mark this as both a Type and a fact table
        return Type(cls)

    return decorator

# Usage:
@FactTable(grain=['date', 'product_id', 'store_id'])
class SalesFactTable:
    date: str = Field()
    product_id: str = Field()
    store_id: str = Field()
    revenue: float = Field(description="Total revenue")
    units_sold: int = Field(description="Units sold")
    transactions: int = Field(description="Number of transactions")
```

#### Task 8.4.2: Dimension & Measure Markers

**File**: `fraiseql/analytics/dimensions.py` & `measures.py`

```python
class Dimension:
    """Mark a field as a dimension (categorical)"""
    pass

class Measure:
    """Mark a field as a measure (quantitative, aggregatable)"""

    def __init__(self, aggregation: str = 'sum'):
        self.aggregation = aggregation  # sum, count, avg, min, max

# Usage:
@FactTable(grain=['date', 'product_id'])
class SalesMetrics:
    date: str = Dimension()
    product_id: str = Dimension()
    revenue: float = Measure(aggregation='sum')
    units: int = Measure(aggregation='sum')
    transactions: int = Measure(aggregation='count')
```

#### Task 8.4.3: Automatic Aggregate Queries

**File**: `fraiseql/analytics/aggregate.py`

```python
class AggregateQueryGenerator:
    """Generate rollup and drill-down queries automatically"""

    def generate_aggregates(self, fact_table_cls: type) -> dict:
        """Create GROUP BY queries automatically"""

        aggregates = {}
        fact_name = fact_table_cls.__name__

        # Single-level aggregates
        for field_name, field_def in fact_table_cls._fraiseql_fields.items():
            if isinstance(field_def, Dimension):
                aggregate_name = f'{fact_name}_by_{field_name}'
                aggregates[aggregate_name] = {
                    "name": aggregate_name,
                    "baseTable": fact_name,
                    "groupBy": [field_name],
                    "measures": self._get_measures(fact_table_cls),
                }

        return aggregates

# Auto-generate:
# - SalesMetrics_by_date
# - SalesMetrics_by_product_id
# - SalesMetrics_by_store_id
```

**Tests** (`test_analytics.py`):

- ✅ Mark fact tables correctly
- ✅ Distinguish dimensions from measures
- ✅ Generate rollup queries
- ✅ Support hierarchical drilling
- ✅ Validate grain constraints

---

### Phase 8.5: Examples & Documentation (1 day)

**Objective**: Provide working examples and complete documentation

#### Example 8.5.1: Basic CRUD (`examples/basic.py`)

```python
from fraiseql import Type, Field, Query, Mutation

@Type
class User:
    id: str = Field(primary_key=True)
    name: str = Field(min_length=1, max_length=255)
    email: str = Field(unique=True)
    created_at: str

@Type
class Post:
    id: str = Field(primary_key=True)
    user_id: str
    title: str
    content: str
    created_at: str

@Query
class UserQueries:
    def get_user(id: str) -> User:
        """Get user by ID"""
        pass

    def list_users(limit: int = 10) -> [User]:
        """List all users"""
        pass

@Query
class PostQueries:
    def get_post(id: str) -> Post:
        """Get post by ID"""
        pass

    def user_posts(user_id: str, limit: int = 10) -> [Post]:
        """Get all posts by user"""
        pass

@Mutation
class UserMutations:
    def create_user(name: str, email: str) -> User:
        """Create new user"""
        pass

    def update_user(id: str, name: str, email: str) -> User:
        """Update user"""
        pass

    def delete_user(id: str) -> bool:
        """Delete user"""
        pass

# Generate schema
if __name__ == '__main__':
    from fraiseql.schema import SchemaGenerator, SchemaExporter

    generator = SchemaGenerator()
    schema = generator.generate([
        User, Post,
        UserQueries, PostQueries,
        UserMutations,
    ])

    SchemaExporter.to_file(schema, 'schema.json')
    print("✅ Generated schema.json")
```

#### Example 8.5.2: Analytics (`examples/ecommerce.py`)

```python
from fraiseql import Type, Field
from fraiseql.analytics import FactTable, Dimension, Measure

@Type
class Product:
    id: str = Field(primary_key=True)
    name: str
    category: str

@Type
class Customer:
    id: str = Field(primary_key=True)
    name: str
    country: str

@FactTable(grain=['date', 'product_id', 'customer_id'])
class SalesMetrics:
    date: str = Dimension()
    product_id: str = Dimension()
    customer_id: str = Dimension()
    revenue: float = Measure(aggregation='sum')
    units_sold: int = Measure(aggregation='sum')
    transactions: int = Measure(aggregation='count')

# Generated queries:
# - SalesMetrics_by_date
# - SalesMetrics_by_product_id
# - SalesMetrics_by_customer_id
# - SalesMetrics_by_date_product_id
```

#### Documentation: `docs/GETTING_STARTED.md`

```markdown
# Getting Started with FraiseQL Python SDK

## Installation

```bash
pip install fraiseql
```

## Quick Start

1. Define your schema:

```python
from fraiseql import Type, Field, Query, Mutation

@Type
class User:
    id: str = Field(primary_key=True)
    name: str

@Query
class UserQueries:
    def get_user(id: str) -> User:
        pass

# Generate schema
schema = SchemaGenerator().generate([User, UserQueries])
SchemaExporter.to_file(schema, 'schema.json')
```

2. Compile with FraiseQL CLI:

```bash
fraiseql-cli compile schema.json > schema.compiled.json
```

3. Run server:

```bash
fraiseql-server schema.compiled.json
```

4. Query:

```bash
curl -X POST http://localhost:3000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ getUser(id: \"1\") { id name } }"}'
```

```

---

### Phase 8.6: Testing & CI/CD (1 day)

**Objective**: Complete test coverage and package automation

#### Task 8.6.1: Test Suite
**File**: `tests/` directory

```bash
# Run all tests
pytest tests/ -v

# With coverage
pytest tests/ --cov=fraiseql --cov-report=html

# Type checking
mypy fraiseql/ --strict

# Code style
ruff check fraiseql/
black --check fraiseql/
```

**Test Coverage Target**: 85%+

- Unit tests: 500+ assertions
- Integration tests: 50+ E2E scenarios
- Analytics tests: 100+ assertions

#### Task 8.6.2: Package Publication

**File**: `setup.py` / `pyproject.toml`

```bash
# Build
python -m build

# Test locally
pip install dist/fraiseql-2.0.0-py3-none-any.whl

# Publish to PyPI
twine upload dist/*
```

**Publication Checklist**:

- ✅ Build succeeds
- ✅ Tests pass
- ✅ Documentation generated
- ✅ README parses correctly
- ✅ Keywords & classifiers set
- ✅ License file included

---

## 🧪 Testing Strategy

### Unit Tests (By Component)

| Component | Tests | Coverage |
|-----------|-------|----------|
| Type system | 25 | 95% |
| Decorators | 30 | 90% |
| Query builder | 20 | 85% |
| Schema generator | 40 | 90% |
| Analytics | 25 | 85% |
| Exporters | 15 | 95% |
| **Total** | **155** | **89%** |

### Integration Tests

```python
# E2E: Python → JSON → CLI
def test_end_to_end_basic_schema():
    # 1. Define schema in Python
    schema = SchemaGenerator().generate([User, UserQueries])

    # 2. Export to JSON
    json_str = SchemaExporter.to_json(schema)

    # 3. Validate JSON
    assert is_valid_json(json_str)

    # 4. Run CLI compiler
    result = subprocess.run(['fraiseql-cli', 'compile', 'schema.json'])
    assert result.returncode == 0

    # 5. Check compiled schema exists
    assert Path('schema.compiled.json').exists()

# Similar tests for:
# - Complex types
# - Analytics schemas
# - Error cases
```

### Performance Tests

```python
def test_schema_generation_performance():
    """Schema generation should be fast"""

    # Generate large schema (100 types, 500 fields)
    start = time.time()
    schema = SchemaGenerator().generate(LARGE_CLASSES)
    elapsed = time.time() - start

    # Should complete in < 100ms
    assert elapsed < 0.1
```

---

## 📅 Implementation Timeline

### Week 1 (Days 1-5)

| Day | Task | Hours | Status |
|-----|------|-------|--------|
| 1   | Phase 8.1.1: Type System | 4 | 📋 |
| 1   | Phase 8.1.2: Field Decorator | 4 | 📋 |
| 2   | Phase 8.1.3: Type Decorator | 4 | 📋 |
| 2   | Phase 8.1: Testing & Debug | 4 | 📋 |
| 3   | Phase 8.2.1: Query Decorator | 4 | 📋 |
| 3   | Phase 8.2.2: Mutation Decorator | 4 | 📋 |
| 4   | Phase 8.3.1: Schema Generator | 4 | 📋 |
| 4   | Phase 8.3.2: Schema Exporter | 4 | 📋 |
| 5   | Phase 8.3: Testing & Debug | 4 | 📋 |
| 5   | Phase 8.4: Analytics Core | 4 | 📋 |

### Week 2 (Days 6-10)

| Day | Task | Hours | Status |
|-----|------|-------|--------|
| 6   | Phase 8.4: Aggregate Queries | 4 | 📋 |
| 6   | Phase 8.4: Testing | 4 | 📋 |
| 7   | Phase 8.5.1: Examples | 4 | 📋 |
| 7   | Phase 8.5: Documentation | 4 | 📋 |
| 8   | Phase 8.6: Test Suite | 4 | 📋 |
| 8   | Phase 8.6: Coverage & CI | 4 | 📋 |
| 9   | Phase 8.6: Package Setup | 4 | 📋 |
| 9   | Phase 8.6: PyPI Prep | 4 | 📋 |
| 10  | Buffer & Final Testing | 8 | 📋 |

**Total**: 72 hours = 9 days

---

## 🎯 Success Criteria

### Must-Have (MVP)

- ✅ All decorators work (`@Type`, `@Field`, `@Query`, `@Mutation`)
- ✅ Schema generation produces valid JSON matching Phase 4 schema
- ✅ E2E test: Python decorators → `fraiseql-cli compile` → `fraiseql-server`
- ✅ 85%+ code coverage
- ✅ Examples: basic.py, blog.py, ecommerce.py work end-to-end
- ✅ Package installable: `pip install fraiseql`

### Nice-to-Have

- ✅ Analytics support (`@FactTable`, `@Dimension`, `@Measure`)
- ✅ Automatic aggregate query generation
- ✅ Type validation with `mypy`
- ✅ CLI command: `fraiseql init-schema` to scaffold

### Out of Scope

- TypeScript SDK (Phase 9)
- Go/Java SDKs (Phase 10)
- Runtime FFI/bindings (keeps Python decoupled)

---

## 🔀 Integration Points

### With Phase 7 (Cache)

- Cascade metadata in mutations (already defined)
- Schema includes cascade field

### With Phase 4 (Compiler)

- Generated JSON must match compiler's expected schema format
- Verify with compiler tests

### With Phase 6 (Server)

- E2E test: Generate → Compile → Serve → Query

---

## ⚠️ Known Risks & Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| Type mapping incomplete | Medium | High | Create comprehensive type matrix early |
| Schema validation mismatch | Medium | High | Test against Phase 4 schemas |
| JSON export invalid | Low | High | Pre-validate with Phase 4 validator |
| Package conflicts | Low | Medium | Test in clean virtualenv |
| Import cycles | Low | High | Use clear module structure |
| Performance degradation | Low | Low | Benchmark schema generation |

---

## 📚 Reference Documentation

### Phase 4 Schema Format

- See: `crates/fraiseql-core/src/schema/compiled.rs`
- Key fields: `types`, `queries`, `mutations`, `version`

### Type System

- See: Phase 4 docs for canonical type names
- Mapping: `UUID` → `"uuid"`, `str` → `"string"`, etc.

### Examples

- See: `.claude/examples/` for expected JSON format
- See: `crates/fraiseql-core/tests/` for E2E patterns

---

## 🚀 Execution Instructions

### Before Starting

```bash
# Create Phase 8 branch
git checkout -b feature/phase-8-python-authoring

# Create package structure
mkdir fraiseql_python
cd fraiseql_python

# Initialize git
git init
```

### Development Loop

```bash
# Edit & test
pytest tests/ -v

# Type check
mypy fraiseql/ --strict

# Code style
black fraiseql/ && ruff check fraiseql/

# Commit frequently
git add -A && git commit -m "feat(phase-8): ..."
```

### Final Submission

```bash
# Build package
python -m build

# Test installation
pip install dist/fraiseql-*.whl
python -c "from fraiseql import Type, Field; print('✅')"

# Run E2E tests
pytest tests/test_integration.py -v

# Push to repo
git push origin feature/phase-8-python-authoring
```

---

## 📞 Support & Questions

### Key Reference Files

- Phase 4 Schema: `crates/fraiseql-core/src/schema/`
- Type System: `crates/fraiseql-core/src/schema/types.rs`
- Existing Examples: `crates/fraiseql-core/tests/fixtures/`

### Testing Patterns

- See: `crates/fraiseql-core/tests/` for integration patterns
- See: `tests/test_schemas/` for example inputs/outputs

---

**Plan Status**: ✅ READY TO IMPLEMENT

**Next Steps**:

1. Create Python package structure
2. Implement Phase 8.1 (Type System)
3. Run daily E2E tests against Phase 4 compiler
