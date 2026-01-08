# Type System Decision for v4.0 - APPROVED

**Date**: January 8, 2026
**Task**: Task 1.1 - Complete Type System Evaluation
**Status**: ✅ DECISION APPROVED AND LOCKED IN
**Decision**: Option A - Keep Python Types (with Library-Internal Rust Types)

---

## Architectural Requirements (Confirmed)

### FraiseQL v4.0 Design Principles

1. **Users write Python code explicitly**
   - Define types with `@fraiseql.type` decorators
   - Users see and control their type definitions
   - Python code is the source of truth for users

2. **Users define database views explicitly**
   - Views created in actual PostgreSQL
   - Users create views themselves
   - Not auto-generated or hidden

3. **Library execution is 100% Rust**
   - All query parsing, building, execution in Rust
   - Single FFI boundary for performance
   - No Python in the hot path

4. **Rust types are library-internal only**
   - No `#[fraiseql::type]` macros exposed to users
   - Rust type system for internal use only
   - Users only see Python decorators

---

## Approved Decision: Option A

**Keep Python Types for User API + Library-Internal Rust Types for Execution**

### Summary

Users define types using `@fraiseql.type` decorators in Python (explicit, visible code). The library internally converts this to a JSON schema that Rust consumes for validation and execution. Users never see or interact with Rust type macros.

### Why This Matches Your Architecture

1. **✅ Users write Python explicitly**
   - Users define types with decorators
   - Python is the user-facing API
   - No code generation, no hidden infrastructure
   - Pure, explicit Python code

2. **✅ Library is 100% Rust**
   - Type system implementation stays internal in Rust
   - Users never see Rust type macros
   - All execution is Rust (via single FFI)
   - Clean separation: user code vs library code

3. **✅ Views defined by users**
   - Database views are user-created in PostgreSQL
   - Not auto-generated from Python types
   - Users have full control
   - Python types don't drive schema

4. **✅ Rust types library-internal**
   - Rust has full type system internally
   - Validates Python types at runtime
   - Users never exposed to Rust
   - Perfect abstraction boundary

### Implementation Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      USER CODE (Python)                         │
│                                                                 │
│  @fraiseql.type                                                │
│  class User:                                                   │
│      id: ID                                                    │
│      name: str                                                │
│      email: str | None = None                                 │
│                                                                │
│  @fraiseql.view (or actual DB views created by user)          │
│  users_view = "SELECT * FROM users"                           │
│                                                                │
│  engine = GraphQLEngine(schema_json)                          │
│  result = await engine.execute(query, context)               │
└─────────────────────────────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────────────────────────────┐
│                  FRAISEQL LIBRARY (Internal)                    │
│                                                                 │
│  Python Layer:                                                 │
│  ├─ @fraiseql.type decorator (introspects)                    │
│  ├─ Schema builder (type to JSON)                             │
│  └─ GraphQLEngine wrapper                                     │
│         ↓ (Single FFI Call)                                    │
│  Rust Layer:                                                   │
│  ├─ Type validation (internal)                                │
│  ├─ Query parsing                                             │
│  ├─ SQL generation                                            │
│  ├─ Execution                                                 │
│  └─ Response building                                         │
│                                                                │
│  Key Point: Rust types are NEVER exposed to users             │
└─────────────────────────────────────────────────────────────────┘
         ↓
┌─────────────────────────────────────────────────────────────────┐
│                    DATABASE (PostgreSQL)                        │
│                                                                 │
│  Views created and managed by users                           │
│  Types don't auto-generate schema                             │
│  Users control the actual database                            │
└─────────────────────────────────────────────────────────────────┘
```

### Advantages of This Approach

1. **Zero User Impact**
   - All existing `@fraiseql.type` code works unchanged
   - No migration burden
   - Users don't need to learn Rust

2. **Perfect Rust Library**
   - Type validation in Rust (fast)
   - Query building in Rust (optimized)
   - Single FFI boundary (minimal overhead)
   - All benefits of Rust without exposing complexity

3. **Explicit User Code**
   - Python decorators are visible
   - Users see what they write
   - No generated code
   - Source of truth in user repository

4. **Minimal Code Changes**
   - No new Rust code for user-facing types
   - Python types remain unchanged
   - Fast Phase A implementation (4 weeks)

5. **Database Control**
   - Views are actual database views (user-created)
   - Not auto-generated
   - Users have full schema control
   - Flexibility for complex schemas

### Timeline

- **Phase A (v2.5.0)**: 4 weeks
  - Keep Python types as-is
  - Consolidate to single FFI
  - Users get unified execution

- **Phase B-D (v3.0-4.0)**: Optional
  - If desired, can add Rust type system improvements
  - Users always have Python decorator option
  - Never forced to use Rust

---

## Current State Analysis

### Python Type System (74 files, ~2592 LOC)

**Location**: `src/fraiseql/types/`

**Key Components**:
- `fraise_type.py` - Main `@fraiseql.type` decorator
- `fraise_input.py` - `@fraiseql.input` decorator
- `enum.py` - `@fraiseql.enum` decorator
- `context.py` - Execution context
- `constructor.py` - Type construction logic
- `generic.py` - Generic type support
- `scalars/` - Custom scalar types (Date, Time, UUID, etc.)

**Current Usage**:
```python
@fraiseql.type
class User:
    """A user in the system."""
    id: ID
    name: str
    email: str | None = None
    roles: list[str] = []
    metadata: dict[str, Any] = {}
```

### Rust Type System (Internal)

**Current State**: Partial, library-internal implementation

**What Rust Has**:
- `graphql/types.rs` - GraphQL AST types
- `schema_registry.rs` - Schema metadata storage
- `graphql/parser.rs` - Query parsing
- JSON schema acceptance in FFI

**Current Type Flow**:
1. Python type decorated with `@fraiseql.type`
2. Decorator introspects and builds schema
3. Schema exported to JSON
4. Via FFI call, passed to Rust
5. Rust validates and uses for execution
6. Users never see this Rust work

---

## Implementation Plan for Phase A

**Week 1**: Type system decision + planning (THIS TASK)
- ✅ Task 1.1: Type system evaluation - APPROVED
- Next: Task 1.2: Schema building audit

**Week 2-3**: Integration with FFI consolidation
- Types work with GraphQLEngine
- Schema exports as JSON
- Rust consumes schema

**Week 4**: Testing and release
- All 5991 tests pass
- v2.5.0 released
- Users get unified FFI

### What Doesn't Change

- Python `@fraiseql.type` decorator (unchanged)
- User type definitions (unchanged)
- Database views (user-controlled)
- Type features (all still work)

### What Gets Enhanced

- Unified FFI boundary (single call)
- Rust execution (already happening)
- GraphQLEngine API (new thin wrapper)
- Performance (marginal improvement from cleaner code path)

---

## Rejected Alternatives

### Option B: Rust Macros for Users ❌ REJECTED

Why this doesn't work:
- Users would need to learn Rust
- Violates "users write Python explicitly"
- Violates "Rust types library-internal"
- Adds no value (views are already user-created)
- Breaks backward compatibility

### Option C: Code Generation ❌ REJECTED

Why this doesn't work:
- Users don't write explicit Python code (it's generated)
- Violates "users write Python explicitly"
- More infrastructure to maintain
- No benefit (types are simple, not complex)

---

## Decision Rationale

**This decision is LOCKED IN** because it:

1. ✅ Matches all architectural requirements
2. ✅ Enables Phase A to complete in 4 weeks
3. ✅ Requires zero user migration
4. ✅ Provides perfect library abstraction
5. ✅ Achieves 100% Rust execution
6. ✅ Keeps user code explicit and visible

**This is the foundation for v4.0 production release.**

---

## Next Steps

### Task 1.1 Complete ✅
- [x] Analyzed Python type system
- [x] Analyzed Rust type system
- [x] Evaluated architectural requirements
- [x] Made decision based on design principles
- [x] Locked in Option A
- [x] Created implementation plan

### Proceed to Task 1.2
**Schema Building Audit** - Evaluate how schema building works with this type system

---

## Appendix: Code Example

### User Code (Unchanged)

```python
# User writes pure Python
from fraiseql import type, field, GraphQLEngine

@type
class User:
    """A user in the system."""
    id: ID
    name: str
    email: str | None = field(default=None, description="User's email")
    roles: list[str] = []

# User creates views in database (or via ORM)
# CREATE VIEW users_view AS SELECT id, name, email, roles FROM users;

# User executes queries
engine = GraphQLEngine(schema_json)
result = await engine.execute(
    query="{ user { id name email } }",
    context={"user_id": "123"}
)
```

### Library Code (Rust + Python)

```python
# Python: fraise_type.py (unchanged, user-facing)
def fraise_type(cls: Type) -> Type:
    """Decorator users see and understand."""
    # Introspect class annotations
    # Build GraphQL type info
    # Register with registry
    return cls

# Python: engine.py (new wrapper)
class GraphQLEngine:
    def __init__(self, schema: str):
        self.schema = schema

    async def execute(self, query: str, context=None):
        # Simple wrapper, just calls Rust
        request_json = json.dumps({"query": query})
        context_json = json.dumps(context) if context else None

        response_json = await _ffi(
            self.schema,
            request_json,
            context_json
        )
        return json.loads(response_json)
```

```rust
// Rust: lib.rs (internal, users never see)
#[pyfunction]
pub async fn process_graphql_request(
    py: Python,
    schema_json: String,
    request_json: String,
    context_json: Option<String>,
) -> PyResult<String> {
    // All execution here
    // Type validation
    // Query building
    // Execution
    // Response building
    // All internal to library
}
```

---

**Document**: v4-TYPE_SYSTEM_DECISION.md
**Status**: ✅ APPROVED - DECISION LOCKED
**Next**: Task 1.2 - Schema Building Audit
**Timeline**: Phase A - 4 weeks to v2.5.0
