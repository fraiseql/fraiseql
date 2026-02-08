# FraiseQL Clean Python Architecture Plan
## Building the Ideal Long-Term Layer

**Status**: Architectural Vision Document
**Date**: January 10, 2026
**Philosophy**: Build it right, not fast. Quality first, timeline second.
**Time Constraint**: None - we have all the time required for excellence

---

## Executive Vision

We're not doing incremental deprecation or migration. We're **building the ideal Python layer from first principles**, understanding that:

1. **Rust is the execution engine** - All query execution, DB operations, HTTP serving
2. **Python is the schema authoring DSL** - Clean, elegant, developer-friendly
3. **Clear boundary** - CompiledSchema JSON at startup; zero Python in request path
4. **Zero compromises** - We'll build it right, even if it takes longer

This is an architectural refactoring to create the **cleanest possible Python API** that properly reflects the "Python author, Rust execute" model.

---

## Part 1: Vision of the Final State

### The Ideal Python API

```python
# Clean, intentional, pure schema authoring
from fraiseql import (
    type,
    query,
    mutation,
    subscription,
    ID,
    String,
    Int,
    Field,
    Arg,
)

# 1. Define types (pure data definitions)
@type
class User:
    """A user in the system."""
    id: ID
    name: str
    email: str | None = None
    roles: list[str] = []

# 2. Define queries (schema only, no logic)
@query
class Query:
    """Root query type."""

    @Query.field
    def users() -> list[User]:
        """Get all users."""
        # Zero implementation - Rust generates SQL automatically
        ...

    @Query.field
    def user(id: ID) -> User | None:
        """Get a user by ID."""
        # Zero implementation - Rust handles it
        ...

# 3. Define mutations (schema only)
@mutation
class Mutation:
    """Root mutation type."""

    @Mutation.field
    def create_user(name: str, email: str | None = None) -> User:
        """Create a new user."""
        # Zero implementation - Rust generates INSERT
        ...

    @Mutation.field
    def update_user(id: ID, **changes) -> User | None:
        """Update a user."""
        # Zero implementation - Rust generates UPDATE
        ...

# 4. Compile schema (one-time, at startup)
from fraiseql.schema import compile_schema

schema = compile_schema(
    types=[User],
    queries=[Query],
    mutations=[Mutation],
)

# 5. Start server (run in Rust)
from fraiseql.axum import create_server

server = create_server(schema)
# Server runs entirely in Rust
# Python is finished - no further involvement
```

### What Makes This "Clean"

1. **No execution logic in Python** - Only declarations
2. **No SQL anywhere** - Rust handles all SQL generation
3. **No result mapping** - Rust handles all transformations
4. **No database calls** - Rust owns the connection pool
5. **No middleware logic** - Rust handles HTTP
6. **Pure data definitions** - Types are just data classes
7. **Single responsibility** - Python: author; Rust: execute
8. **Clear boundaries** - CompiledSchema JSON is the contract

---

## Part 2: Architecture Layers (Clean Design)

### Layer 1: Type System (Pure Declarations)

**Location**: `fraiseql/types/`

```
types/
├── __init__.py           # Public API
├── core.py              # Base classes (Type, Field, Arg)
├── scalars.py           # Scalar types (ID, String, Int, Float, Boolean)
├── decorators.py        # @type, @field, @input decorators
├── metadata.py          # Type metadata storage (no logic)
└── validation.py        # Type validation (pure functions, no state)
```

**Responsibilities**:
- ✅ Define type classes
- ✅ Store field metadata
- ✅ Validate type definitions
- ❌ Execute queries
- ❌ Generate SQL
- ❌ Map results
- ❌ Connect to database

**Key Classes**:
```python
@dataclass
class Field:
    """Field definition - pure metadata."""
    name: str
    field_type: Type
    nullable: bool = False
    default: Any = UNSET
    description: str | None = None
    # Zero logic - just data

@dataclass
class Type:
    """Type definition - pure metadata."""
    name: str
    fields: dict[str, Field]
    sql_source: str | None = None
    description: str | None = None
    # Zero logic - just data
```

### Layer 2: Configuration (Pure Data)

**Location**: `fraiseql/config/`

```
config/
├── __init__.py
├── database.py          # Database config (URLs, pool size, etc)
├── security.py          # Security config (auth, RBAC, policies)
├── server.py            # Server config (host, port, CORS, etc)
├── audit.py             # Audit config (event types, backends, etc)
├── caching.py           # Cache config (TTLs, backends, etc)
├── observability.py     # Tracing, metrics, logging config
└── loader.py            # Load config from env/files
```

**Responsibilities**:
- ✅ Define configuration data classes
- ✅ Load from environment/files
- ✅ Validate configuration
- ✅ Serialize to JSON for Rust
- ❌ Execute anything
- ❌ Connect to external services

**Example**:
```python
@dataclass
class DatabaseConfig:
    """Database connection configuration."""
    url: str
    pool_size: int = 20
    timeout_secs: int = 30
    ssl_mode: str = "prefer"

    def to_json(self) -> dict:
        """Serialize for Rust."""
        return asdict(self)

@dataclass
class SecurityConfig:
    """Security policies."""
    authentication_required: bool = False
    authorization_enabled: bool = True
    rate_limit_requests_per_minute: int = 1000

    def to_json(self) -> dict:
        return asdict(self)

@dataclass
class FraiseQLConfig:
    """Complete FraiseQL configuration."""
    database: DatabaseConfig
    security: SecurityConfig
    server: ServerConfig
    audit: AuditConfig
    caching: CachingConfig
    observability: ObservabilityConfig

    def to_json(self) -> dict:
        """Serialize all config for Rust."""
        return {
            'database': self.database.to_json(),
            'security': self.security.to_json(),
            # ... etc
        }
```

### Layer 3: Schema Compiler (Composition)

**Location**: `fraiseql/schema/`

```
schema/
├── __init__.py
├── compiler.py          # SchemaCompiler class
├── validator.py         # Schema validation rules
└── json_format.py       # JSON schema format specification
```

**Responsibilities**:
- ✅ Collect type definitions from decorators
- ✅ Validate schema integrity
- ✅ Compile to JSON for Rust
- ✅ Version schema format
- ❌ Execute anything
- ❌ Generate SQL
- ❌ Connect to services

**Key Class**:
```python
class SchemaCompiler:
    """Compile Python type definitions to Rust-compatible JSON."""

    def __init__(self):
        self.types: dict[str, Type] = {}
        self.queries: dict[str, Query] = {}
        self.mutations: dict[str, Mutation] = {}
        self.subscriptions: dict[str, Subscription] = {}

    def register_type(self, type_def: Type) -> None:
        """Register a type definition."""
        self.types[type_def.name] = type_def

    def register_query(self, query_def: Query) -> None:
        """Register query root type."""
        self.queries[query_def.name] = query_def

    def compile(self) -> CompiledSchema:
        """Compile to Rust-compatible schema."""
        return CompiledSchema(
            version="1.0",
            types=[self._compile_type(t) for t in self.types.values()],
            queries=[self._compile_query(q) for q in self.queries.values()],
            mutations=[...],
            subscriptions=[...],
        )

    def to_json(self) -> str:
        """Serialize to JSON for Rust."""
        schema = self.compile()
        return json.dumps(schema.to_dict())

    def _compile_type(self, type_def: Type) -> CompiledType:
        """Compile a type definition."""
        # Pure transformation, no logic
        return CompiledType(
            name=type_def.name,
            sql_source=type_def.sql_source,
            fields=[...],
        )
```

### Layer 4: Server Integration (Thin Wrapper)

**Location**: `fraiseql/server/`

```
server/
├── __init__.py
├── axum.py              # Axum (Rust) server integration
├── fastapi.py           # FastAPI server integration (optional)
└── startup.py           # Server startup orchestration
```

**Responsibilities**:
- ✅ Compile schema
- ✅ Load configuration
- ✅ Pass compiled schema to Rust
- ✅ Start Rust server
- ✅ Handle graceful shutdown
- ❌ Handle HTTP requests (Rust does this)
- ❌ Execute GraphQL (Rust does this)
- ❌ Connect to database (Rust does this)

**Example**:
```python
async def create_server(
    schema: CompiledSchema,
    config: FraiseQLConfig,
) -> AxumServer:
    """Create and start a FraiseQL server.

    This is the ONLY time Python is involved in serving.
    After this function returns, Rust handles everything.
    """

    # Compile schema if not already compiled
    if isinstance(schema, SchemaCompiler):
        schema = schema.compile()

    # Pass to Rust
    rust_server = fraiseql_rs.create_server(
        schema_json=schema.to_json(),
        config_json=config.to_json(),
    )

    # Start Rust server (async, non-blocking)
    await rust_server.start()

    return rust_server
```

### Layer 5: Utilities (Pure Functions)

**Location**: `fraiseql/utils/`

```
utils/
├── __init__.py
├── type_helpers.py      # Type conversion helpers
├── validation.py        # Input validation (pure functions)
├── serialization.py     # JSON serialization helpers
└── inspection.py        # Schema inspection/introspection (read-only)
```

**Responsibilities**:
- ✅ Pure helper functions
- ✅ Type conversions
- ✅ Input validation
- ✅ Schema inspection (read-only)
- ❌ Execute anything
- ❌ Modify state
- ❌ Have side effects

---

## Part 3: What Gets Eliminated

### Completely Remove (0 LOC)

1. **sql/** (1.1MB) - SQL generation
   - Why: Rust QueryBuilder handles this
   - Rust equivalent: `fraiseql_rs/core/src/query/`

2. **db/** (304KB) - Database operations
   - Why: Rust tokio-postgres handles this
   - Rust equivalent: `fraiseql_rs/core/src/db/`

3. **core/** (288KB) - Execution engine
   - Why: Rust executor pipeline handles this
   - Rust equivalent: `fraiseql_rs/core/src/pipeline/`

4. **execution/** (~150KB) - Query orchestration
   - Why: Rust orchestration handles this
   - Rust equivalent: Built into Rust pipeline

5. **graphql/** (~120KB) - GraphQL execution
   - Why: Rust GraphQL engine handles this
   - Rust equivalent: `fraiseql_rs/core/src/execution/`

### Severely Reduce (Keep Config Only)

1. **security/** (496KB → 100KB)
   - Keep: Auth config, RBAC definitions, policy data
   - Remove: Auth enforcement (move to Rust)
   - Remove: Permission checking logic

2. **enterprise/** (544KB → 200KB)
   - Keep: Audit event definitions, configuration
   - Remove: Audit capture logic (move to Rust)
   - Remove: Audit storage implementation

3. **monitoring/** (468KB → 150KB)
   - Keep: Metrics/trace definitions, configuration
   - Remove: Actual collection/emission (move to Rust)

4. **cli/** (468KB → 100KB)
   - Keep: Schema validation, schema inspection tools
   - Remove: Query execution tools
   - Remove: Database migration tools

### Keep & Improve

1. **types/** (892KB → 700KB)
   - Keep all type definitions
   - Remove any execution logic
   - Improve documentation
   - Add examples
   - Add validation rules

2. **decorators.py** (40KB)
   - Keep: @type, @query, @mutation decorators
   - Keep: Registry mechanism
   - Remove: Any execution logic

3. **config/** (new, ~200KB)
   - New: Consolidated configuration
   - Replaces scattered config from enterprise/, security/, etc.
   - All serializable to JSON

---

## Part 4: Detailed Implementation Plan

### Phase 0: Foundation (Weeks 1-4)
**Goal**: Establish base infrastructure for clean architecture

#### Week 1: Type System v2
- [ ] Design clean Type/Field/Arg classes (no logic)
- [ ] Implement @type, @field decorators
- [ ] Write comprehensive tests (100+ test cases)
- [ ] Document with examples
- **Deliverable**: Clean, well-tested type system

#### Week 2: Configuration System
- [ ] Design FraiseQLConfig hierarchy
- [ ] Implement all config classes
- [ ] Add environment variable loading
- [ ] JSON serialization
- **Deliverable**: Complete, validated configuration system

#### Week 3: Schema Compiler
- [ ] Design SchemaCompiler class
- [ ] Implement schema compilation
- [ ] Define JSON schema format (versioned)
- [ ] Add validation rules
- **Deliverable**: SchemaCompiler that produces clean JSON

#### Week 4: Server Integration
- [ ] Design thin server wrapper
- [ ] Implement startup orchestration
- [ ] Add graceful shutdown
- [ ] Integration tests
- **Deliverable**: Clean startup flow

### Phase 1: Type System Refactoring (Weeks 5-8)
**Goal**: Replace old types/ with new clean implementation

#### Week 5: Migrate Type Definitions
- [ ] Extract all type definitions from old types/
- [ ] Implement in new clean system
- [ ] Run compatibility tests
- [ ] Update documentation

#### Week 6: Migrate Decorators
- [ ] Convert all @type, @query, @mutation usage
- [ ] Ensure backward compatibility
- [ ] Comprehensive tests

#### Week 7-8: Testing & Polish
- [ ] Run full test suite
- [ ] Fix any issues
- [ ] Complete documentation
- [ ] Code review

**Deliverable**: Fully functional, clean type system

### Phase 2: Configuration System Refactoring (Weeks 9-12)
**Goal**: Centralize and clean all configuration

#### Week 9-10: Config Consolidation
- [ ] Extract config from security/, enterprise/, monitoring/, etc.
- [ ] Implement in clean config/ hierarchy
- [ ] Add environment loading
- [ ] JSON serialization

#### Week 11-12: Integration & Testing
- [ ] Integration tests with Rust
- [ ] Environment variable resolution
- [ ] Error handling
- [ ] Documentation

**Deliverable**: Single source of truth for configuration

### Phase 3: Remove Execution Code (Weeks 13-24)
**Goal**: Eliminate all Python execution logic

#### Week 13-16: Remove sql/ (1.1MB)
- [ ] Audit what sql/ does
- [ ] Verify Rust equivalents exist
- [ ] Remove Python implementations
- [ ] Update tests (move to Rust)

#### Week 17-20: Remove db/ (304KB)
- [ ] Extract config classes
- [ ] Move to config/
- [ ] Remove execution code
- [ ] Update tests

#### Week 21-24: Remove core/ & execution/ (438KB)
- [ ] Eliminate execution orchestration
- [ ] Remove query planning
- [ ] Update request pipeline
- [ ] Final integration tests

**Deliverable**: Zero execution logic in Python

### Phase 4: Enterprise Features (Weeks 25-32)
**Goal**: Keep config, move enforcement to Rust

#### Week 25-28: Security Refactoring
- [ ] Extract auth config/policies
- [ ] Keep: RBAC definitions, policy data
- [ ] Remove: Auth enforcement
- [ ] Remove: Permission checking

#### Week 29-32: Audit/Monitoring
- [ ] Extract event definitions
- [ ] Extract configuration
- [ ] Remove: Capture/emission logic
- [ ] Remove: Storage implementation

**Deliverable**: Config-only security & audit layers

### Phase 5: API Polish (Weeks 33-36)
**Goal**: Create perfect developer experience

#### Week 33: Documentation
- [ ] Architecture guide
- [ ] API reference
- [ ] Migration guide
- [ ] Examples

#### Week 34: Examples & Tutorials
- [ ] Complete example projects
- [ ] Tutorial documentation
- [ ] Video walkthroughs (if desired)

#### Week 35-36: Testing & QA
- [ ] Comprehensive test suite
- [ ] Performance validation
- [ ] Integration validation
- [ ] User acceptance testing

**Deliverable**: Production-ready, well-documented system

---

## Part 5: Code Quality Standards

### Architecture Principles

1. **No Execution Logic in Python**
   - Python declares, doesn't do
   - All logic is data transformation
   - No side effects

2. **Single Responsibility**
   - Types: Define schema
   - Config: Define settings
   - Compiler: Produce JSON
   - Server: Start Rust

3. **No Duplication with Rust**
   - If Rust can do it, Python doesn't
   - Only data/declarations in Python

4. **Clear Boundaries**
   - CompiledSchema JSON is the contract
   - Python → JSON once at startup
   - Rust reads JSON, runs forever

### Code Style

**Type Hints**: Full coverage (Python 3.13+)
```python
def compile(self) -> CompiledSchema: ...
def to_json(self) -> str: ...
def register_type(self, type_def: Type) -> None: ...
```

**Docstrings**: Comprehensive, with examples
```python
def compile(self) -> CompiledSchema:
    """Compile Python schema to Rust-compatible JSON.

    This converts all type definitions, queries, and mutations
    into a single CompiledSchema that Rust can load at startup.

    Returns:
        CompiledSchema: Compiled schema ready for Rust

    Example:
        >>> compiler = SchemaCompiler()
        >>> compiler.register_type(User)
        >>> schema = compiler.compile()
        >>> json_str = schema.to_json()
    """
```

**Testing**: 95%+ coverage minimum
```python
def test_compile_simple_schema():
    """Test compiling a simple schema."""
    compiler = SchemaCompiler()
    compiler.register_type(User)
    schema = compiler.compile()
    assert schema.types[0].name == 'User'

def test_json_output_valid():
    """Test JSON output is valid and parseable."""
    schema = compiler.compile()
    json_str = schema.to_json()
    parsed = json.loads(json_str)
    assert parsed['version'] == '1.0'
```

**No Exceptions in Type System**
- Use dataclasses, not custom classes
- Use pure functions, not methods with state
- No try/except in hot paths

### File Organization

```
fraiseql/
├── types/                  # Type definitions (700KB)
│   ├── __init__.py        # Public API
│   ├── core.py            # Base classes
│   ├── scalars.py         # Scalar types
│   ├── decorators.py      # Decorators
│   ├── metadata.py        # Metadata storage
│   └── validation.py      # Pure validation functions
│
├── config/                 # Configuration (200KB)
│   ├── __init__.py
│   ├── database.py        # Database config
│   ├── security.py        # Security config
│   ├── server.py          # Server config
│   ├── audit.py           # Audit config
│   ├── caching.py         # Cache config
│   ├── observability.py   # Tracing/metrics
│   └── loader.py          # Config loading
│
├── schema/                 # Schema compilation (150KB)
│   ├── __init__.py
│   ├── compiler.py        # SchemaCompiler
│   ├── validator.py       # Validation rules
│   └── json_format.py     # JSON spec
│
├── server/                 # Server integration (100KB)
│   ├── __init__.py
│   ├── axum.py            # Axum integration
│   ├── fastapi.py         # FastAPI integration
│   └── startup.py         # Startup orchestration
│
├── utils/                  # Pure utilities (100KB)
│   ├── __init__.py
│   ├── type_helpers.py    # Type helpers
│   ├── validation.py      # Input validation
│   ├── serialization.py   # JSON helpers
│   └── inspection.py      # Schema inspection
│
├── auth/                   # Auth configuration (100KB)
│   ├── __init__.py
│   ├── policies.py        # RBAC policies
│   ├── roles.py           # Role definitions
│   └── models.py          # Auth data models
│
└── testing/                # Testing utilities (50KB)
    ├── __init__.py
    ├── fixtures.py        # Test fixtures
    ├── factories.py       # Test object factories
    └── assertions.py      # Custom assertions
```

**Total**: ~1.5MB (from 13MB - 89% reduction)

---

## Part 6: Success Criteria

### Functionality
- [ ] All types compile to valid JSON
- [ ] Config serializes correctly to JSON
- [ ] Rust can parse all JSON output
- [ ] Server starts and serves requests
- [ ] Zero Python in request path

### Code Quality
- [ ] 95%+ test coverage
- [ ] Zero linting errors
- [ ] Full type hints throughout
- [ ] Comprehensive docstrings
- [ ] No duplication with Rust

### Performance
- [ ] Startup time < 1 second
- [ ] Compilation time < 100ms
- [ ] Zero runtime overhead
- [ ] No memory leaks

### Documentation
- [ ] Architecture guide complete
- [ ] API reference complete
- [ ] 3+ example projects
- [ ] Migration guide (if needed)
- [ ] Video tutorials (optional)

### Compatibility
- [ ] All existing tests pass
- [ ] PrintOptim compatible
- [ ] Backward compatibility maintained
- [ ] Clear upgrade path

---

## Part 7: Timeline

### Estimated Duration: 9 months (36 weeks)

| Phase | Duration | Work |
|-------|----------|------|
| Phase 0 | 4 weeks | Foundation infrastructure |
| Phase 1 | 4 weeks | Type system refactoring |
| Phase 2 | 4 weeks | Configuration refactoring |
| Phase 3 | 12 weeks | Remove all execution code |
| Phase 4 | 8 weeks | Enterprise features |
| Phase 5 | 4 weeks | Polish & release |

**Resources**:
- 1 Senior Python Architect (part-time oversight)
- 1-2 Python Developers (full-time implementation)
- Rust team (validate JSON compatibility)

**Flexibility**: This is NOT a fixed timeline. Quality > speed.

---

## Part 8: Risk Mitigation

### Risk 1: Rust Compatibility
- **Mitigation**: Validate JSON with Rust team weekly
- **Test**: Rust tests parse all JSON outputs

### Risk 2: Breaking Changes
- **Mitigation**: Maintain backward compatibility throughout
- **Deprecation**: Old APIs get deprecation warnings, not immediate removal

### Risk 3: Performance Regression
- **Mitigation**: Benchmark each phase
- **Validation**: Performance tests for startup, compilation

### Risk 4: Incomplete Feature Coverage
- **Mitigation**: Audit Rust layer completeness first
- **Build**: Any missing Rust features before removing Python

---

## Part 9: Guardrails & Commitments

### What We Will NOT Do

❌ Ship broken code
❌ Sacrifice quality for speed
❌ Leave Python execution code in "just in case"
❌ Create technical debt
❌ Break PrintOptim
❌ Reduce test coverage below 95%

### What We WILL Do

✅ Build the ideal architecture
✅ Comprehensive testing at every step
✅ Clear, complete documentation
✅ Regular code reviews
✅ Architectural decisions documented
✅ Zero execution logic in Python
✅ All utilities are pure functions
✅ Clear boundaries and contracts

---

## Part 10: Next Steps

### Immediate (This Week)
1. [ ] Review and approve this plan
2. [ ] Identify team members
3. [ ] Set up architecture review process
4. [ ] Create Phase 0 detailed tasks

### Phase 0 Preparation (Week 1)
1. [ ] Design clean Type system
2. [ ] Design FraiseQLConfig
3. [ ] Design SchemaCompiler
4. [ ] Design server integration
5. [ ] Review designs with Rust team

### Phase 0 Execution (Weeks 2-4)
1. [ ] Implement Type system v2
2. [ ] Implement FraiseQLConfig
3. [ ] Implement SchemaCompiler
4. [ ] Implement server integration
5. [ ] Comprehensive testing

---

## Appendix: The Ideal Result

### What Python Becomes

After this refactoring, Python is:

```python
# 1. Pure schema authoring
@type
class User:
    id: ID
    name: str

# 2. Configuration
config = FraiseQLConfig(
    database=DatabaseConfig(url="..."),
    security=SecurityConfig(authentication_required=True),
)

# 3. Startup
schema = SchemaCompiler().register_type(User).compile()
server = create_server(schema, config)
# Python is done
```

**That's it.** That's what Python does. Nothing more.

All the:
- SQL generation → Rust
- Query execution → Rust
- Result mapping → Rust
- Database connection → Rust
- HTTP serving → Rust
- Authentication → Rust
- Audit logging → Rust
- Caching → Rust
- Monitoring → Rust

Handled by Rust. Python is clean, simple, focused.

---

**Status**: Architecture Plan Complete
**Quality Focus**: Excellence over speed
**Timeline**: 9 months (no rush)
**Recommendation**: Proceed with confidence
**Next Action**: Architecture review and team assignment
