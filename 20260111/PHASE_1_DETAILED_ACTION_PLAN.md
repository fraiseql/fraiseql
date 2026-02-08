# Phase 1: Establish Clean Schema Authoring Layer
## Detailed Action Plan

**Duration**: 2-3 weeks
**Effort**: Medium (40-60 hours)
**Risk**: Low
**Outcome**: Clean, documented Python authoring APIs that produce clean JSON schemas

---

## Overview

### Current State
- Python type system is mixed with execution logic
- Schema compilation spreads across multiple modules
- Configuration scattered throughout codebase
- JSON schema format is undocumented

### Target State
- Pure Python type/decorator system (no execution)
- Single `SchemaCompiler` entry point
- Centralized configuration layer
- Documented, versioned JSON schema format

### Success Criteria
- [ ] SchemaCompiler produces clean JSON
- [ ] All type definitions work via decorators only
- [ ] Configuration is separate, serializable
- [ ] PrintOptim tests pass
- [ ] Documentation complete

---

## Week 1: Audit & Design

### Day 1-2: Audit types/ Module (892KB)

#### Task 1.1: List all type-related files
```bash
find /home/lionel/code/fraiseql/fraiseql-python/src/fraiseql/types -name "*.py" | xargs wc -l | tail -1
```

#### Task 1.2: Categorize what each file does
Create spreadsheet:
| File | Lines | Purpose | Keep? | Move? | Delete? |
|------|-------|---------|-------|-------|---------|
| __init__.py | ??? | ? | ? | ? | ? |
| ... | | | | | |

**Specific files to audit**:
- `types/__init__.py` - Entry point
- `types/definitions.py` - Type classes?
- `types/scalars.py` - Scalar types
- `types/inputs.py` - Input types
- `types/errors.py` - Error types
- `types/decorators.py` - Decorator implementation?
- All other files in `types/`

#### Task 1.3: Document what's currently happening
For each file, answer:
- What does it do?
- Does it have execution logic or just definitions?
- Is it used during schema compilation?
- Is it used during request handling?
- Could a user extend it?

**Deliverable**: Audit report (markdown file)

---

### Day 3: Audit decorators.py (40KB)

#### Task 1.4: Understand decorator system
```python
# Understand what these decorators do:
@fraiseql.type
@fraiseql.query
@fraiseql.mutation
@fraiseql.subscription
@fraiseql.input
@fraiseql.field
@fraiseql.scalar
# ... any others?
```

**Specific questions**:
- How does `@fraiseql.type` work?
- What metadata does it capture?
- What happens when a type is decorated?
- How is the registry populated?
- What execution logic is in decorators.py?

**Deliverable**: Decorator behavior documentation

---

### Day 4: Audit gql/ Module (244KB) - Schema Part

#### Task 1.5: Separate schema from execution
```bash
find /home/lionel/code/fraiseql/fraiseql-python/src/fraiseql/gql -name "*.py" | xargs wc -l
```

**Questions**:
- What files define schema structure?
- What files have execution logic (query building, resolution)?
- What's the relationship between gql/ and types/?
- Where are field definitions stored?
- How are schemas built/compiled?

**Deliverable**: gql/ module analysis (schema vs execution split)

---

### Day 5: Audit config & setup

#### Task 1.6: Find current configuration
```bash
grep -r "class.*Config\|class.*Settings" /home/lionel/code/fraiseql/fraiseql-python/src/fraiseql --include="*.py" | head -20
```

**Find**:
- All config classes
- Where are they used?
- Are they serializable?
- Do they contain execution logic?

#### Task 1.7: Understand PrintOptim usage
```bash
grep -r "@fraiseql\." /home/lionel/code/printoptim_backend/src --include="*.py" | head -10
grep -r "create_fraiseql_app" /home/lionel/code/printoptim_backend/src --include="*.py"
grep -r "SchemaCompiler" /home/lionel/code/printoptim_backend/src --include="*.py"
```

**Questions**:
- How does PrintOptim define schemas?
- What APIs does it use?
- What would break if we change the API?

**Deliverable**: PrintOptim usage analysis

---

### Design Session: Define Clean Authoring Layer

#### Task 1.8: Design Python → JSON schema format

**Create document**: `SCHEMA_JSON_FORMAT.md`

Specify:
```json
{
  "version": "1.0",
  "types": [
    {
      "name": "User",
      "sql_source": "public.users",
      "fields": [
        {
          "name": "id",
          "field_type": "ID",
          "nullable": false
        },
        {
          "name": "name",
          "field_type": "String",
          "nullable": false
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true
    }
  ],
  "mutations": [],
  "subscriptions": []
}
```

**Questions to answer**:
- What fields are required?
- What's the version strategy?
- Can Rust `CompiledSchema::from_json()` parse this?
- What validation is needed?

**Deliverable**: Complete JSON schema specification

---

## Week 2: Refactor types/ & decorators.py

### Day 1-2: Clean types/ Module

#### Task 2.1: Remove execution logic from types/
For each file in types/:
- Remove any code that:
  - Builds queries
  - Executes SQL
  - Transforms results
  - Handles database operations
- Keep only:
  - Type definitions
  - Field metadata
  - Decorator implementations
  - Type conversion helpers (if pure functions)

#### Task 2.2: Add clear docstrings
For each type:
```python
@dataclass
class User:
    """User type definition for GraphQL schema.

    This type is purely declarative. It defines the shape of a User
    in the GraphQL schema, with no execution logic.

    SQL Source: public.users

    Example:
        @fraiseql.type
        class User:
            id: ID
            name: str
    """
    id: ID
    name: str
```

#### Task 2.3: Audit test coverage
```bash
find /home/lionel/code/fraiseql -path "*/test*" -name "*type*" -o -name "*decorator*" | wc -l
```

Identify tests that:
- Test type definitions ✓ (keep)
- Test decorator registration ✓ (keep)
- Test execution logic ✗ (move to Rust tests)

**Deliverable**: Cleaned types/ module with zero execution logic

---

### Day 3: Clean decorators.py

#### Task 2.4: Ensure decorators only register
Check that `@fraiseql.type` etc. only:
- Register type metadata
- Store field information
- Don't execute anything

Remove any:
- Query building
- Result mapping
- Database operations

#### Task 2.5: Document decorator behavior
```python
@fraiseql.type
class User:
    """This decorator registers a GraphQL type with FraiseQL.

    It captures:
    - Type name (User)
    - All fields with type hints
    - Docstring as description
    - Field defaults as metadata

    The registration is purely informational - it doesn't execute.
    """
    id: ID
    name: str
```

**Deliverable**: Cleaned decorators.py, well-documented

---

### Day 4-5: Create SchemaCompiler

#### Task 2.6: Create new SchemaCompiler class
Location: `fraiseql/schema/compiler.py`

```python
from dataclasses import dataclass
from typing import Any
import json

@dataclass
class CompiledType:
    name: str
    sql_source: str
    fields: list[dict]

@dataclass
class CompiledQuery:
    name: str
    return_type: str
    returns_list: bool

@dataclass
class CompiledSchema:
    """Schema compiled from Python decorators to JSON.

    This is what gets passed to Rust at startup.
    No execution logic - purely declarative.
    """
    types: list[CompiledType]
    queries: list[CompiledQuery]
    mutations: list
    subscriptions: list

    def to_json(self) -> str:
        """Convert to JSON for Rust CompiledSchema::from_json()"""
        return json.dumps({
            'version': '1.0',
            'types': [t.__dict__ for t in self.types],
            'queries': [q.__dict__ for q in self.queries],
            'mutations': [m.__dict__ for m in self.mutations],
            'subscriptions': [s.__dict__ for s in self.subscriptions],
        })

class SchemaCompiler:
    """Compile Python decorators to FraiseQL schema."""

    def __init__(self):
        self.types: dict = {}
        self.queries: dict = {}
        self.mutations: dict = {}

    def compile(self) -> CompiledSchema:
        """Compile to Rust-compatible schema."""
        return CompiledSchema(
            types=[CompiledType(...) for t in self.types.values()],
            queries=[CompiledQuery(...) for q in self.queries.values()],
            mutations=[],
            subscriptions=[],
        )
```

#### Task 2.7: Integrate with registry
How does the type/query registry populate SchemaCompiler?

Current (unknown):
```python
# How are types registered now?
@fraiseql.type
class User: ...
# What happens here?
```

New (design):
```python
# Get compiler instance
compiler = SchemaCompiler.get_default()

# Types register themselves
@fraiseql.type
class User: ...
# This calls: compiler.register_type(User)

# Compile when ready
schema = compiler.compile()
json_schema = schema.to_json()
```

**Deliverable**: Working SchemaCompiler with tests

---

## Week 3: Configuration & Integration

### Day 1-2: Create Config Layer

#### Task 3.1: Centralize configuration
Create: `fraiseql/config/` directory

```python
@dataclass
class DatabaseConfig:
    """Database connection configuration."""
    url: str
    pool_size: int = 20
    timeout: int = 30

@dataclass
class SecurityConfig:
    """Security policies."""
    require_authentication: bool = False
    enable_introspection: bool = True
    rate_limit: int = 1000

@dataclass
class AuditConfig:
    """Audit event configuration."""
    enabled: bool = False
    backends: list[str] = None

@dataclass
class FraiseQLConfig:
    """Complete FraiseQL configuration."""
    database: DatabaseConfig
    security: SecurityConfig
    audit: AuditConfig
    # ... others

    def to_json(self) -> str:
        """Convert to JSON for Rust"""
        # Serialize all config
        pass
```

#### Task 3.2: Consolidate from existing modules
Move config from:
- enterprise/
- security/
- monitoring/
- cli/
- etc.

Into centralized `config/` module.

**Deliverable**: Clean, centralized configuration

---

### Day 3: Document Everything

#### Task 3.3: Write Python authoring guide
Create: `docs/PYTHON_AUTHORING_GUIDE.md`

```markdown
# FraiseQL Python Authoring Guide

## Quick Start

### 1. Define Types

@fraiseql.type
class User:
    id: ID
    name: str

### 2. Define Queries

@fraiseql.query
def users() -> list[User]:
    # No implementation needed!
    # Rust generates SQL automatically
    pass

### 3. Compile Schema

from fraiseql.schema.compiler import SchemaCompiler
compiler = SchemaCompiler.get_default()
schema = compiler.compile()

### 4. Start Server

from fraiseql.axum import create_axum_app
app = create_axum_app(schema)
# Or FastAPI:
from fraiseql.fastapi import create_fraiseql_app
app = create_fraiseql_app(schema)
```

**Deliverable**: Clear, complete documentation

---

### Day 4-5: Validation & Testing

#### Task 3.4: Test with PrintOptim
```bash
cd /home/lionel/code/printoptim_backend
pytest tests/ -v
# All tests should pass with new Python APIs
```

#### Task 3.5: Create comprehensive tests
Location: `tests/unit/schema/test_compiler.py`

```python
def test_schema_compiler_simple():
    """Test compiling a simple schema."""
    compiler = SchemaCompiler()

    @fraiseql.type
    class User:
        id: ID
        name: str

    schema = compiler.compile()
    json_schema = schema.to_json()

    # Verify structure
    assert schema.types[0].name == 'User'
    assert len(schema.types[0].fields) == 2

    # Verify JSON is valid
    import json
    parsed = json.loads(json_schema)
    assert parsed['version'] == '1.0'

def test_schema_compiler_with_queries():
    """Test schema with queries."""
    # Similar structure
    pass

def test_schema_json_compatibility():
    """Test JSON can be loaded by Rust."""
    schema = compiler.compile()
    json_str = schema.to_json()

    # This would require FFI, but we can at least
    # verify the JSON structure is correct
    import json
    parsed = json.loads(json_str)

    # Validate against expected schema
    assert 'version' in parsed
    assert 'types' in parsed
    assert 'queries' in parsed
```

**Deliverable**: Full test coverage for Phase 1

---

#### Task 3.6: Document JSON schema format
Create: `docs/SCHEMA_JSON_FORMAT.md`

Example:
```json
{
  "version": "1.0",
  "types": [
    {
      "name": "User",
      "sql_source": "public.users",
      "fields": [
        {
          "name": "id",
          "field_type": "ID",
          "nullable": false
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true
    }
  ]
}
```

**Deliverable**: Documented, versioned schema format

---

## Deliverables Checklist

### Week 1 (Audit & Design)
- [ ] types/ module audit report
- [ ] decorators.py analysis
- [ ] gql/ module schema vs execution analysis
- [ ] Configuration audit
- [ ] PrintOptim usage analysis
- [ ] SCHEMA_JSON_FORMAT.md (design)

### Week 2 (Refactoring)
- [ ] Cleaned types/ module (no execution logic)
- [ ] Cleaned decorators.py (no execution logic)
- [ ] SchemaCompiler class (working)
- [ ] Integration tests for SchemaCompiler
- [ ] Updated docstrings throughout

### Week 3 (Integration & Testing)
- [ ] Centralized config/ module
- [ ] Configuration serialization to JSON
- [ ] Python authoring guide
- [ ] Schema JSON format documentation
- [ ] Full test coverage
- [ ] PrintOptim compatibility verified

---

## Success Criteria

### Code Quality
- [ ] Zero execution logic in Python type system
- [ ] All public APIs documented
- [ ] Type hints throughout
- [ ] Clear separation: Authoring vs Execution

### Functionality
- [ ] SchemaCompiler produces valid JSON
- [ ] Rust can parse output: `CompiledSchema::from_json()`
- [ ] PrintOptim tests pass (100%)
- [ ] Schema format is stable and versioned

### Testing
- [ ] 95%+ code coverage for schema/compiler
- [ ] 100+ new tests for Phase 1
- [ ] All existing tests pass
- [ ] Integration tests with Rust

### Documentation
- [ ] Authoring guide (user-facing)
- [ ] Schema format specification
- [ ] API documentation (docstrings)
- [ ] Migration guide (if API changed)

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Breaking PrintOptim | Test continuously; maintain backward compat |
| Incorrect schema format | Validate JSON structure; test with Rust |
| Missing metadata | Audit all decorator behaviors first |
| Performance regression | No execution logic added, so no perf impact |

---

## Rollback Plan

If Phase 1 goes wrong:
1. Keep old code in `_legacy/` directory
2. Revert to previous commit
3. Continue with incremental approach

All work is additive (new SchemaCompiler) not destructive.

---

## Definition of Done

✅ All deliverables complete
✅ All tests passing (100%)
✅ PrintOptim tests passing (100%)
✅ Documentation complete
✅ Code review approved
✅ Merged to `dev` branch
✅ Commit message: "refactor(python): establish clean schema authoring layer [Phase 1]"

---

## Next Phase (Phase 2 Prep)

Once Phase 1 is done:
1. Audit sql/ module (1.1MB)
2. Design Rust query builder FFI
3. Plan SQL elimination
4. Prepare Phase 2 checklist

---

**Phase 1 Status**: Ready to begin
**Estimated Start**: Week of January 13, 2026
**Estimated Completion**: Week of January 27, 2026
