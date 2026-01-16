# Phase 4: Quick Start Guide

**Where to Start**: `fraiseql-python/src/fraiseql/types.py`

---

## What You're Building

A Python SDK that lets developers write GraphQL schemas like this:

```python
import fraiseql

@fraiseql.type
class User:
    id: str
    name: str
    email: str = fraiseql.Field(validation=fraiseql.rules.Email())

@fraiseql.query
def get_user(id: str) -> User:
    pass

@fraiseql.mutation
def create_user(name: str, email: str) -> User:
    pass

# Export to schema.json
fraiseql.export_schema()
```

**Goal**: Users never write JSON by hand. Just Python decorators.

---

## 10-Phase Roadmap (12 days)

```
Phase 4.1: Type System (2 days)
    ├─ FieldDefinition class
    ├─ TypeDefinition class
    └─ Type mapping utilities

Phase 4.2: Core Decorators (1.5 days)
    ├─ @type decorator
    ├─ @query decorator
    ├─ @mutation decorator
    └─ @subscription decorator (basic)

Phase 4.3: Field Configuration (1 day)
    ├─ fraiseql.Field() descriptor
    ├─ Validation rules (Email, Unique, Length, Pattern)
    └─ Security rules (RequireAuth, RequireRole)

Phase 4.4: Schema Generation (1.5 days)
    ├─ SchemaGenerator class
    ├─ JSON output (types, queries, mutations)
    └─ fraiseql.config() function

Phase 4.5: Analytics (1 day)
    ├─ @fact_table decorator
    ├─ Dimension & Measure markers
    ├─ @aggregate_query decorator
    └─ GroupBy specification

Phase 4.6: Registry (1 day)
    ├─ Global registry
    ├─ Type/query/mutation lookup
    └─ Registry export

Phase 4.7: Export & CLI (1 day)
    ├─ export_schema() function
    └─ File I/O and JSON serialization

Phase 4.8: Testing (1.5 days)
    ├─ Unit tests
    ├─ Integration tests
    └─ Code coverage verification

Phase 4.9: Documentation (1.5 days)
    ├─ README update
    ├─ API reference
    ├─ Getting started guide
    └─ Examples

Phase 4.10: PyPI Release (1 day)
    ├─ Build distribution
    ├─ Upload to PyPI
    └─ GitHub release

TOTAL: 12 days
```

---

## Implementation Order

### Day 1-2: Types (Foundation)

**File**: `fraiseql-python/src/fraiseql/types.py`

```python
class FieldDefinition:
    name: str
    type_: str  # GraphQL type
    is_nullable: bool
    is_list: bool
    default_value: Any | None
    validation: dict
    directives: list[str]

class TypeDefinition:
    name: str
    fields: dict[str, FieldDefinition]
    description: str | None

# Utilities
PYTHON_TO_GRAPHQL_TYPE_MAP = {...}
def python_type_to_graphql(hint) -> str: ...
def field_to_json(field: FieldDefinition) -> dict: ...
```

**Tests**: Tests in `tests/test_types.py` already written - make them pass.

**Done when**:
```bash
pytest tests/test_types.py -v  # All green
```

### Day 2-3: Decorators

**File**: `fraiseql-python/src/fraiseql/decorators.py`

```python
def type(cls: type) -> type:
    """Convert class to GraphQL type."""
    # Parse annotations
    # Create TypeDefinition
    # Register in global registry
    return cls

def query(func_or_class):
    """Register a query."""
    # Parse signature
    # Create QueryDefinition
    # Register in registry

def mutation(func_or_class):
    """Register a mutation."""
    # Similar to query

def config(sql_source: str, **kwargs) -> None:
    """Configure SQL source for query/mutation."""
    pass  # Just a marker function
```

**Tests**: Tests in `tests/test_decorators.py` - make them pass.

### Day 3: Field Configuration

**File**: `fraiseql-python/src/fraiseql/types.py` (add to)

```python
class Field:
    def __init__(
        self,
        *,
        primary_key: bool = False,
        validation: ValidationRule | None = None,
        index: bool = False,
        cache_ttl: int | None = None,
        security: SecurityRule | None = None,
    ):
        ...

class Email(ValidationRule): pass
class Unique(ValidationRule): pass
class Length(ValidationRule): pass
class Pattern(ValidationRule): pass

class RequireAuth(SecurityRule): pass
class RequireRole(SecurityRule): pass
```

### Day 4-5: Schema Generation

**File**: `fraiseql-python/src/fraiseql/schema.py`

```python
class SchemaGenerator:
    def generate(self) -> dict:
        """Generate complete schema JSON."""
        return {
            "version": "1.0.0",
            "types": self._generate_types(),
            "queries": self._generate_queries(),
            "mutations": self._generate_mutations(),
            "subscriptions": self._generate_subscriptions(),
        }

def export_schema(filepath: str = "schema.json") -> None:
    """Save schema to JSON file."""
    generator = SchemaGenerator()
    schema = generator.generate()
    with open(filepath, 'w') as f:
        json.dump(schema, f, indent=2)
```

### Day 5-6: Analytics

**File**: `fraiseql-python/src/fraiseql/analytics.py`

```python
class Dimension:
    """Mark field as dimension."""
    pass

class Measure:
    """Mark field as measure."""
    pass

def fact_table(cls: type) -> type:
    """Mark type as fact table."""
    # Store metadata
    # Validate dimensions/measures
    return cls

class GroupBy:
    def __init__(
        self,
        dimensions: list[str],
        measures: list[str],
        filters: list[dict] | None = None,
    ):
        ...

def aggregate_query(cls: type) -> type:
    """Create aggregate queries."""
    # Extract GroupBy fields
    # Validate against fact table
    # Register aggregate queries
    return cls
```

### Day 6: Registry

**File**: `fraiseql-python/src/fraiseql/registry.py`

```python
class Registry:
    def __init__(self):
        self.types: dict[str, TypeDefinition] = {}
        self.queries: dict[str, QueryDefinition] = {}
        self.mutations: dict[str, MutationDefinition] = {}

    def register_type(self, type_def: TypeDefinition): ...
    def register_query(self, query_def: QueryDefinition): ...
    def get_type(self, name: str) -> TypeDefinition | None: ...

# Global instance
_global_registry = Registry()
def get_registry() -> Registry: ...
def clear_registry(): ...
```

### Day 7-8: Testing & Quality

```bash
# Run all tests
cd fraiseql-python
python -m pytest tests/ -v --cov=src/fraiseql

# Lint
ruff check src/ tests/
ruff format --check src/ tests/

# Fix issues
pytest tests/  # Fix failures
ruff format src/ tests/  # Auto-format
```

**Success criteria**:
- All tests pass
- 90%+ coverage
- Zero lint warnings

### Day 8-9: Documentation

Create files:
- `docs/python/INSTALLATION.md`
- `docs/python/GETTING_STARTED.md`
- `docs/python/DECORATORS_REFERENCE.md`
- `docs/python/ANALYTICS_GUIDE.md`
- `docs/python/EXAMPLES.md`

### Day 9-10: PyPI Release

```bash
# Build
python -m build

# Test build
twine check dist/*

# Upload to PyPI
twine upload dist/*

# Verify
pip install --upgrade fraiseql
python -c "import fraiseql; print(fraiseql.__version__)"
```

---

## Key Implementation Tips

### 1. Start with Skeleton

The files have skeletons. Flesh them out, don't rewrite.

```python
# Current state (skeleton)
def type(cls: type) -> type:
    """Convert class to GraphQL type."""
    pass  # TODO: Implement

# Expected state
def type(cls: type) -> type:
    """Convert class to GraphQL type."""
    # Extract name
    type_name = cls.__name__

    # Extract fields from annotations
    fields = {}
    for field_name, field_type in cls.__annotations__.items():
        fields[field_name] = FieldDefinition(...)

    # Create definition
    type_def = TypeDefinition(name=type_name, fields=fields)

    # Register
    registry.register_type(type_def)

    return cls
```

### 2. Tests First

Tests are already written. Read them to understand requirements:

```bash
cat tests/test_decorators.py | head -50
```

Then implement code to make tests pass.

### 3. Use Python 3.10+ Syntax

```python
# ✅ Good
def get_user(user_id: int) -> User | None:
    ...

items: list[str] | None = None

# ❌ Bad (old style)
from typing import Optional, List
def get_user(user_id: int) -> Optional[User]:
    ...
```

### 4. Global Registry Pattern

All decorators register with the global registry:

```python
from fraiseql.registry import get_registry

def type(cls: type) -> type:
    registry = get_registry()
    type_def = TypeDefinition(...)
    registry.register_type(type_def)  # <- Register here
    return cls
```

### 5. JSON Schema Format

Keep JSON close to GraphQL schema spec:

```json
{
  "version": "1.0.0",
  "types": {
    "User": {
      "kind": "OBJECT",
      "fields": {
        "id": {"type": "String!"},
        "name": {"type": "String!"}
      }
    }
  },
  "queries": {
    "getUser": {
      "args": {"id": {"type": "String!"}},
      "returns": "User"
    }
  }
}
```

---

## Daily Checkpoint

Each day, verify:

```bash
# Tests pass
cd fraiseql-python
pytest tests/ -v

# Code quality
ruff check src/
ruff format src/

# Can import
python -c "from fraiseql import type, query, mutation, export_schema; print('✅ OK')"
```

---

## End-to-End Test (Day 10)

```python
# example_phase_4_test.py
import fraiseql

@fraiseql.type
class User:
    id: str
    name: str
    email: str

@fraiseql.query
def get_user(id: str) -> User:
    pass

@fraiseql.query
def list_users(limit: int = 10) -> list[User]:
    pass

# Export
fraiseql.export_schema("schema.json")

# Verify
import json
with open("schema.json") as f:
    schema = json.load(f)

assert "User" in schema["types"]
assert "getUser" in schema["queries"]
assert "listUsers" in schema["queries"]
print("✅ Phase 4 complete!")
```

---

## Files to Implement (Checklist)

**Core Implementation** (in order):

- [ ] `src/fraiseql/types.py` - FieldDefinition, TypeDefinition, type mapping
- [ ] `src/fraiseql/decorators.py` - @type, @query, @mutation, @subscription
- [ ] `src/fraiseql/registry.py` - Global registry
- [ ] `src/fraiseql/schema.py` - SchemaGenerator, export_schema()
- [ ] `src/fraiseql/analytics.py` - @fact_table, @aggregate_query

**Testing**:

- [ ] `tests/test_types.py` - Pass all tests
- [ ] `tests/test_decorators.py` - Pass all tests
- [ ] `tests/test_analytics.py` - Pass all tests
- [ ] `tests/test_schema.py` - New comprehensive tests
- [ ] `tests/test_registry.py` - New registry tests
- [ ] `tests/test_integration.py` - End-to-end tests

**Documentation**:

- [ ] Update `README.md`
- [ ] Create `docs/python/INSTALLATION.md`
- [ ] Create `docs/python/GETTING_STARTED.md`
- [ ] Create `docs/python/DECORATORS_REFERENCE.md`
- [ ] Create `docs/python/ANALYTICS_GUIDE.md`
- [ ] Create `docs/python/EXAMPLES.md`

**Release**:

- [ ] Update version to v2.0.0-beta.1
- [ ] Run `python -m build`
- [ ] Publish to PyPI: `twine upload dist/*`
- [ ] Create GitHub release

---

## Success Criteria

**Implementation**: ✅
- All decorators working
- Schema JSON generation working
- Registry system functional
- Analytics support functional

**Testing**: ✅
- All tests passing
- 90%+ coverage
- No flaky tests

**Quality**: ✅
- Zero lint warnings
- Proper type hints everywhere
- All dependencies specified

**Documentation**: ✅
- README updated
- API reference complete
- 5+ examples provided

**Release**: ✅
- Published to PyPI
- Installation verified
- GitHub release created

---

## Still Need Help?

- Full details: `/home/lionel/code/fraiseql/.claude/PHASE_4_PYTHON_AUTHORING.md`
- Example tests: `fraiseql-python/tests/`
- Example schemas: `fraiseql-python/examples/`

---

**Ready? Start with `src/fraiseql/types.py` and work through 4.1-4.10 systematically.**

Each phase builds on the previous. Don't skip ahead. Focus on quality over speed.

Good luck! 🚀
