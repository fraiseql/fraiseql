# Phase 0: Foundation Infrastructure
## Weeks 1-4: Build the Base for Clean Architecture

**Status**: Detailed Execution Plan
**Duration**: 4 weeks (20 working days)
**Objective**: Establish the foundation infrastructure that Phases 1-5 will build upon
**Quality Level**: Production-ready, well-tested code

---

## Overview

Phase 0 is **critical and foundational**. We're not refactoring existing code; we're **building new, clean systems from first principles** that will replace the old ones.

By the end of Phase 0, we'll have:
1. ✅ Clean Type/Field/Arg system (with zero execution logic)
2. ✅ Complete FraiseQLConfig hierarchy
3. ✅ Working SchemaCompiler
4. ✅ Thin server integration layer

Everything will be **well-tested, documented, and ready** for Phases 1-5 to build upon.

---

## Week 1: Type System v2 Design & Implementation

### Objective
Create a **clean, data-focused type system** with no execution logic whatsoever.

### Day 1-2: Design Type System Architecture

#### Task 1.1: Document Design Decisions
Create: `DESIGN_DECISIONS.md`

```python
# Decision 1: Use dataclasses, not custom classes
# Why: Pure data, no methods/logic, serializable
@dataclass
class Field:
    name: str
    field_type: str  # "ID", "String", "Int", etc.
    nullable: bool = False
    default: Any = UNSET
    description: str | None = None

# Decision 2: Type is just metadata container
@dataclass
class Type:
    name: str
    fields: dict[str, Field]
    sql_source: str | None = None
    description: str | None = None
    # Zero methods - just data

# Decision 3: Decorators register, don't execute
def type(cls):
    """Decorator that registers a type with SchemaCompiler."""
    # Just register, don't execute
    SchemaCompiler.get_default().register_type(Type.from_class(cls))
    return cls
```

**Deliverable**: Design document with decisions and rationale

#### Task 1.2: Design Public API
Create: `fraiseql/types/__init__.py` (skeleton)

```python
# What users will import
from fraiseql import (
    type,              # @type decorator
    query,             # @query decorator
    mutation,          # @mutation decorator
    subscription,      # @subscription decorator
    field,             # @field decorator (optional)
    ID,                # Scalar type
    String,            # Scalar type
    Int,               # Scalar type
    Float,             # Scalar type
    Boolean,           # Scalar type
)

# That's it. Nothing else.
# No query execution, no database, no SQL
```

**Deliverable**: Clean, minimal public API

### Day 3-5: Implement Type System

#### Task 1.3: Create base classes (pure data)
File: `fraiseql/types/core.py` (50 LOC, no logic)

```python
from dataclasses import dataclass, field as dc_field
from typing import Any
from enum import Enum

# Sentinel value for "no default"
class UNSET:
    pass

# Scalar type enumeration
class ScalarType(Enum):
    ID = "ID"
    String = "String"
    Int = "Int"
    Float = "Float"
    Boolean = "Boolean"

# Pure data classes - zero methods
@dataclass
class Field:
    """Field definition - pure metadata."""
    name: str
    field_type: str  # "ID", "String", "User", "[User]", etc.
    nullable: bool = False
    default: Any = UNSET
    description: str | None = None

@dataclass
class Argument:
    """Argument definition - pure metadata."""
    name: str
    arg_type: str
    required: bool = False
    default: Any = UNSET
    description: str | None = None

@dataclass
class Type:
    """Type definition - pure metadata."""
    name: str
    fields: dict[str, Field] = dc_field(default_factory=dict)
    sql_source: str | None = None
    description: str | None = None

@dataclass
class QueryDef:
    """Query definition - pure metadata."""
    name: str
    return_type: str
    arguments: dict[str, Argument] = dc_field(default_factory=dict)
    description: str | None = None

@dataclass
class MutationDef:
    """Mutation definition - pure metadata."""
    name: str
    return_type: str
    arguments: dict[str, Argument] = dc_field(default_factory=dict)
    description: str | None = None
```

**Key Principle**: These are **pure data containers**. Zero logic.

**Tests**:
```python
def test_field_creation():
    f = Field(name="id", field_type="ID", nullable=False)
    assert f.name == "id"

def test_type_creation():
    t = Type(name="User", fields={"id": Field(...)})
    assert t.name == "User"
    assert len(t.fields) == 1
```

**Deliverable**: Pure data classes with 100% test coverage

#### Task 1.4: Implement decorators
File: `fraiseql/types/decorators.py` (100 LOC)

```python
from fraiseql.schema.compiler import SchemaCompiler

def type(cls):
    """Decorator that registers a type with the schema compiler.

    Pure registration - no execution logic.
    """
    compiler = SchemaCompiler.get_default()

    # Extract fields from class annotations
    type_def = Type(
        name=cls.__name__,
        fields={...},  # from cls.__annotations__
        description=cls.__doc__,
    )

    compiler.register_type(type_def)
    return cls

def query(cls):
    """Decorator that registers a query root type."""
    compiler = SchemaCompiler.get_default()
    # Similar registration
    compiler.register_query(...)
    return cls

def mutation(cls):
    """Decorator that registers a mutation root type."""
    compiler = SchemaCompiler.get_default()
    # Similar registration
    compiler.register_mutation(...)
    return cls
```

**Tests**:
```python
def test_type_decorator():
    @type
    class User:
        id: ID
        name: str

    compiler = SchemaCompiler.get_default()
    assert "User" in compiler.types

def test_query_decorator():
    @query
    class Query:
        @field
        def users() -> list[User]:
            pass

    compiler = SchemaCompiler.get_default()
    assert "Query" in compiler.queries
```

**Deliverable**: Working decorators, no execution logic

#### Task 1.5: Implement scalar types
File: `fraiseql/types/scalars.py` (30 LOC)

```python
# Scalar type classes for type hints
class ID(str):
    """GraphQL ID scalar."""
    pass

class String(str):
    """GraphQL String scalar."""
    pass

class Int(int):
    """GraphQL Int scalar."""
    pass

class Float(float):
    """GraphQL Float scalar."""
    pass

class Boolean(bool):
    """GraphQL Boolean scalar."""
    pass
```

**Deliverable**: Scalar types for type hints

#### Task 1.6: Add type utilities
File: `fraiseql/types/utils.py` (100 LOC, pure functions)

```python
def is_list_type(type_str: str) -> bool:
    """Check if type is a list (e.g., '[User]')."""
    return type_str.startswith('[') and type_str.endswith(']')

def get_inner_type(type_str: str) -> str:
    """Get inner type of list (e.g., '[User]' -> 'User')."""
    if is_list_type(type_str):
        return type_str[1:-1]
    return type_str

def is_nullable(type_str: str) -> bool:
    """Check if type can be null (e.g., 'User' vs 'User!')."""
    return not type_str.endswith('!')

def make_nullable(type_str: str) -> str:
    """Remove ! from type (if present)."""
    return type_str.rstrip('!')

def make_non_nullable(type_str: str) -> str:
    """Add ! to type (if not present)."""
    if type_str.endswith('!'):
        return type_str
    return type_str + '!'
```

**All pure functions - zero state.**

### Week 1 Deliverables
- [ ] Design decisions documented
- [ ] Public API defined
- [ ] Core data classes (Field, Type, Argument, etc.)
- [ ] Decorators (@type, @query, @mutation)
- [ ] Scalar types (ID, String, Int, Float, Boolean)
- [ ] Type utilities (all pure functions)
- [ ] 200+ tests, 100% coverage
- [ ] Complete docstrings with examples

---

## Week 2: Configuration System

### Objective
Create a **centralized, serializable configuration system** with no execution logic.

### Day 1-2: Design Configuration Hierarchy

#### Task 2.1: Design config structure
Create: `fraiseql/config/__init__.py` (skeleton)

```python
# Users will use it like:
from fraiseql.config import FraiseQLConfig, DatabaseConfig, SecurityConfig

config = FraiseQLConfig(
    database=DatabaseConfig(url="postgresql://..."),
    security=SecurityConfig(authentication_required=True),
    server=ServerConfig(host="0.0.0.0", port=8000),
    audit=AuditConfig(enabled=True),
)

# Serialize for Rust
json_str = config.to_json()
```

#### Task 2.2: Implement database config
File: `fraiseql/config/database.py` (80 LOC)

```python
from dataclasses import dataclass

@dataclass
class DatabaseConfig:
    """Database connection configuration."""
    url: str
    pool_size: int = 20
    timeout_secs: int = 30
    ssl_mode: str = "prefer"
    statement_cache_size: int = 100

    def validate(self) -> list[str]:
        """Validate configuration. Return list of errors."""
        errors = []
        if not self.url.startswith(('postgresql://', 'postgres://')):
            errors.append("Database URL must be PostgreSQL")
        if self.pool_size < 1:
            errors.append("pool_size must be >= 1")
        return errors

    def to_dict(self) -> dict:
        """Serialize to dict for JSON."""
        return {
            'url': self.url,
            'pool_size': self.pool_size,
            'timeout_secs': self.timeout_secs,
            'ssl_mode': self.ssl_mode,
            'statement_cache_size': self.statement_cache_size,
        }
```

**Tests**:
```python
def test_database_config_valid():
    config = DatabaseConfig(url="postgresql://localhost/test")
    assert len(config.validate()) == 0

def test_database_config_invalid_url():
    config = DatabaseConfig(url="mysql://localhost/test")
    errors = config.validate()
    assert len(errors) == 1
```

#### Task 2.3: Implement security config
File: `fraiseql/config/security.py` (100 LOC)

```python
@dataclass
class SecurityConfig:
    """Security and authorization configuration."""
    authentication_required: bool = False
    authorization_enabled: bool = True
    rate_limit_requests_per_minute: int = 1000
    enable_introspection: bool = True
    cors_allowed_origins: list[str] = field(default_factory=list)
    jwt_secret: str | None = None
    oauth_provider: str | None = None

    def validate(self) -> list[str]:
        errors = []
        if self.authentication_required and not self.jwt_secret:
            errors.append("jwt_secret required when authentication_required=True")
        if self.rate_limit_requests_per_minute < 1:
            errors.append("rate_limit must be >= 1")
        return errors

    def to_dict(self) -> dict:
        return asdict(self)
```

#### Task 2.4: Implement server config
File: `fraiseql/config/server.py` (60 LOC)

```python
@dataclass
class ServerConfig:
    """HTTP server configuration."""
    host: str = "0.0.0.0"
    port: int = 8000
    workers: int = 4
    log_level: str = "info"
    enable_metrics: bool = True
    enable_tracing: bool = False

    def validate(self) -> list[str]:
        errors = []
        if not (0 <= self.port <= 65535):
            errors.append("port must be 0-65535")
        if self.workers < 1:
            errors.append("workers must be >= 1")
        return errors

    def to_dict(self) -> dict:
        return asdict(self)
```

#### Task 2.5: Implement audit config
File: `fraiseql/config/audit.py` (80 LOC)

```python
@dataclass
class AuditConfig:
    """Audit logging configuration."""
    enabled: bool = False
    backends: list[str] = field(default_factory=list)  # ["database", "file", etc]
    event_types: list[str] = field(default_factory=list)  # What events to capture
    retention_days: int = 90
    sample_rate: float = 1.0  # 0.0 to 1.0

    def validate(self) -> list[str]:
        errors = []
        if not (0.0 <= self.sample_rate <= 1.0):
            errors.append("sample_rate must be 0.0-1.0")
        return errors

    def to_dict(self) -> dict:
        return asdict(self)
```

#### Task 2.6: Implement caching config
File: `fraiseql/config/caching.py` (60 LOC)

```python
@dataclass
class CachingConfig:
    """Query result caching configuration."""
    enabled: bool = False
    backend: str = "memory"  # "memory", "redis", etc
    ttl_seconds: int = 300
    max_size_mb: int = 100

    def validate(self) -> list[str]:
        errors = []
        if self.backend not in ["memory", "redis"]:
            errors.append(f"Unknown backend: {self.backend}")
        return errors

    def to_dict(self) -> dict:
        return asdict(self)
```

#### Task 2.7: Implement observability config
File: `fraiseql/config/observability.py` (80 LOC)

```python
@dataclass
class ObservabilityConfig:
    """Tracing, metrics, and logging configuration."""
    tracing_enabled: bool = False
    tracing_backend: str = "jaeger"  # "jaeger", "datadog", etc
    metrics_enabled: bool = True
    metrics_backend: str = "prometheus"
    logging_format: str = "json"  # "json" or "text"

    def validate(self) -> list[str]:
        errors = []
        if self.tracing_backend not in ["jaeger", "datadog"]:
            errors.append(f"Unknown tracing backend: {self.tracing_backend}")
        return errors

    def to_dict(self) -> dict:
        return asdict(self)
```

### Day 3: Create FraiseQLConfig (main config class)

#### Task 2.8: Implement main config
File: `fraiseql/config/main.py` (100 LOC)

```python
@dataclass
class FraiseQLConfig:
    """Complete FraiseQL configuration."""
    database: DatabaseConfig
    security: SecurityConfig
    server: ServerConfig
    audit: AuditConfig
    caching: CachingConfig
    observability: ObservabilityConfig

    def validate(self) -> list[str]:
        """Validate all configuration sections."""
        errors = []
        errors.extend(self.database.validate())
        errors.extend(self.security.validate())
        errors.extend(self.server.validate())
        errors.extend(self.audit.validate())
        errors.extend(self.caching.validate())
        errors.extend(self.observability.validate())
        return errors

    def to_json(self) -> str:
        """Serialize to JSON for Rust."""
        config_dict = {
            'database': self.database.to_dict(),
            'security': self.security.to_dict(),
            'server': self.server.to_dict(),
            'audit': self.audit.to_dict(),
            'caching': self.caching.to_dict(),
            'observability': self.observability.to_dict(),
        }
        return json.dumps(config_dict, indent=2)

    @staticmethod
    def from_env() -> "FraiseQLConfig":
        """Load configuration from environment variables."""
        return FraiseQLConfig(
            database=DatabaseConfig(
                url=os.getenv('DATABASE_URL', ''),
                pool_size=int(os.getenv('DB_POOL_SIZE', '20')),
            ),
            security=SecurityConfig(
                authentication_required=os.getenv('AUTH_REQUIRED', 'false').lower() == 'true',
                jwt_secret=os.getenv('JWT_SECRET'),
            ),
            # ... etc
        )
```

### Day 4-5: Environment loading & tests

#### Task 2.9: Config loader
File: `fraiseql/config/loader.py` (80 LOC)

```python
def load_config(
    env_file: str | None = None,
    override: dict | None = None,
) -> FraiseQLConfig:
    """Load configuration from environment and optional override."""

    # Load from .env file if provided
    if env_file:
        load_dotenv(env_file)

    # Load from environment
    config = FraiseQLConfig.from_env()

    # Apply overrides
    if override:
        # Merge overrides
        config = config.merge(override)

    # Validate
    errors = config.validate()
    if errors:
        raise ConfigurationError(f"Invalid configuration: {errors}")

    return config
```

#### Task 2.10: Comprehensive tests
File: `tests/unit/config/` (500+ LOC)

```python
def test_database_config_from_env(monkeypatch):
    monkeypatch.setenv('DATABASE_URL', 'postgresql://localhost/test')
    config = DatabaseConfig.from_env()
    assert config.url == 'postgresql://localhost/test'

def test_fraiseql_config_valid():
    config = FraiseQLConfig(
        database=DatabaseConfig(url='postgresql://localhost/test'),
        # ... etc
    )
    errors = config.validate()
    assert len(errors) == 0

def test_fraiseql_config_to_json():
    config = FraiseQLConfig(...)
    json_str = config.to_json()
    parsed = json.loads(json_str)
    assert 'database' in parsed
    assert 'security' in parsed

def test_load_config_from_env():
    config = load_config()
    assert config is not None
```

### Week 2 Deliverables
- [ ] DatabaseConfig (with validation, serialization)
- [ ] SecurityConfig
- [ ] ServerConfig
- [ ] AuditConfig
- [ ] CachingConfig
- [ ] ObservabilityConfig
- [ ] FraiseQLConfig (main)
- [ ] Config loader from environment
- [ ] 150+ tests, 100% coverage
- [ ] Complete docstrings with examples

---

## Week 3: Schema Compiler

### Objective
Create the **SchemaCompiler** that converts Python decorators to Rust-compatible JSON.

### Day 1-2: Design & Core Implementation

#### Task 3.1: Design schema format
Create: `fraiseql/schema/format_spec.md`

```markdown
# FraiseQL Schema JSON Format v1.0

## Overall Structure
{
  "version": "1.0",
  "types": [...],
  "queries": [...],
  "mutations": [...],
  "subscriptions": [...]
}

## Type Definition
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
      "name": "email",
      "field_type": "String",
      "nullable": true
    }
  ]
}

## Query Definition
{
  "name": "users",
  "return_type": "User",
  "returns_list": true,
  "arguments": [...]
}
```

#### Task 3.2: Implement SchemaCompiler
File: `fraiseql/schema/compiler.py` (200 LOC)

```python
from fraiseql.types import Type, QueryDef, MutationDef
from typing import Optional

class SchemaCompiler:
    """Compile Python type definitions to Rust-compatible JSON schema."""

    _instance: Optional["SchemaCompiler"] = None

    def __init__(self):
        self.types: dict[str, Type] = {}
        self.queries: dict[str, QueryDef] = {}
        self.mutations: dict[str, MutationDef] = {}
        self.subscriptions: dict = {}

    @classmethod
    def get_default(cls) -> "SchemaCompiler":
        """Get or create the default instance."""
        if cls._instance is None:
            cls._instance = SchemaCompiler()
        return cls._instance

    def register_type(self, type_def: Type) -> "SchemaCompiler":
        """Register a type definition."""
        self.types[type_def.name] = type_def
        return self

    def register_query(self, query_def: QueryDef) -> "SchemaCompiler":
        """Register a query definition."""
        self.queries[query_def.name] = query_def
        return self

    def register_mutation(self, mutation_def: MutationDef) -> "SchemaCompiler":
        """Register a mutation definition."""
        self.mutations[mutation_def.name] = mutation_def
        return self

    def compile(self) -> "CompiledSchema":
        """Compile to Rust-compatible schema."""
        return CompiledSchema(
            version="1.0",
            types=[self._compile_type(t) for t in self.types.values()],
            queries=[self._compile_query(q) for q in self.queries.values()],
            mutations=[self._compile_mutation(m) for m in self.mutations.values()],
            subscriptions=[],
        )

    def to_json(self) -> str:
        """Serialize to JSON for Rust."""
        schema = self.compile()
        return json.dumps(schema.to_dict(), indent=2)

    def _compile_type(self, type_def: Type) -> dict:
        """Compile a type definition."""
        return {
            'name': type_def.name,
            'sql_source': type_def.sql_source,
            'fields': [
                {
                    'name': f.name,
                    'field_type': f.field_type,
                    'nullable': f.nullable,
                    'description': f.description,
                }
                for f in type_def.fields.values()
            ],
            'description': type_def.description,
        }

    def _compile_query(self, query_def: QueryDef) -> dict:
        """Compile a query definition."""
        return {
            'name': query_def.name,
            'return_type': query_def.return_type,
            'description': query_def.description,
            'arguments': [...],
        }
```

#### Task 3.3: Implement CompiledSchema
File: `fraiseql/schema/compiled.py` (80 LOC)

```python
from dataclasses import dataclass

@dataclass
class CompiledSchema:
    """Schema compiled from Python to Rust-compatible format."""
    version: str = "1.0"
    types: list = None
    queries: list = None
    mutations: list = None
    subscriptions: list = None

    def to_dict(self) -> dict:
        """Convert to dictionary for JSON serialization."""
        return {
            'version': self.version,
            'types': self.types or [],
            'queries': self.queries or [],
            'mutations': self.mutations or [],
            'subscriptions': self.subscriptions or [],
        }

    def to_json(self) -> str:
        """Serialize to JSON string."""
        return json.dumps(self.to_dict(), indent=2)
```

### Day 3-4: Schema validation

#### Task 3.4: Implement validator
File: `fraiseql/schema/validator.py` (150 LOC)

```python
class SchemaValidator:
    """Validate compiled schema."""

    @staticmethod
    def validate(schema: CompiledSchema) -> list[str]:
        """Validate schema integrity. Return list of errors."""
        errors = []

        # Check required fields
        if not schema.version:
            errors.append("Schema must have a version")

        # Validate types
        type_names = {t['name'] for t in schema.types}

        # Validate queries
        for query in schema.queries:
            if query['return_type'] not in type_names:
                errors.append(
                    f"Query '{query['name']}' references unknown type '{query['return_type']}'"
                )

        # Validate mutations
        for mutation in schema.mutations:
            if mutation['return_type'] not in type_names:
                errors.append(
                    f"Mutation '{mutation['name']}' references unknown type '{mutation['return_type']}'"
                )

        return errors

    @staticmethod
    def validate_json(json_str: str) -> list[str]:
        """Validate JSON schema format."""
        try:
            data = json.loads(json_str)
        except json.JSONDecodeError as e:
            return [f"Invalid JSON: {e}"]

        schema = CompiledSchema(**data)
        return SchemaValidator.validate(schema)
```

### Day 5: Integration & Tests

#### Task 3.5: Integration & comprehensive tests
File: `tests/unit/schema/` (400+ LOC)

```python
def test_compiler_register_type():
    compiler = SchemaCompiler()
    compiler.register_type(Type(name="User", sql_source="users"))
    assert "User" in compiler.types

def test_compiler_compile():
    compiler = SchemaCompiler()
    compiler.register_type(Type(name="User", sql_source="users"))
    schema = compiler.compile()
    assert schema.version == "1.0"
    assert len(schema.types) == 1

def test_compiler_to_json():
    compiler = SchemaCompiler()
    compiler.register_type(Type(name="User", sql_source="users"))
    json_str = compiler.to_json()

    # Validate JSON is valid
    parsed = json.loads(json_str)
    assert parsed['version'] == "1.0"
    assert len(parsed['types']) == 1

def test_schema_validator_detects_missing_type():
    schema = CompiledSchema(
        types=[],
        queries=[{'name': 'users', 'return_type': 'User'}],
    )
    errors = SchemaValidator.validate(schema)
    assert len(errors) > 0
    assert "unknown type" in errors[0]

def test_full_workflow():
    """Test complete workflow: decorator → compiler → JSON."""
    @type
    class User:
        id: ID
        name: str

    compiler = SchemaCompiler.get_default()
    json_str = compiler.to_json()

    # Validate it's valid JSON
    parsed = json.loads(json_str)
    assert parsed['version'] == "1.0"
```

### Week 3 Deliverables
- [ ] Schema format specification (versioned)
- [ ] SchemaCompiler class (full implementation)
- [ ] CompiledSchema class
- [ ] SchemaValidator
- [ ] Integration with decorators
- [ ] 200+ tests, 100% coverage
- [ ] Format specification document
- [ ] Complete docstrings

---

## Week 4: Server Integration & Polish

### Objective
Create thin **server startup layer** that passes schema/config to Rust.

### Day 1-2: Server integration

#### Task 4.1: Implement server module
File: `fraiseql/server/startup.py` (80 LOC)

```python
import fraiseql_rs  # Rust FFI

async def create_server(
    schema: CompiledSchema | SchemaCompiler,
    config: FraiseQLConfig,
) -> "AxumServer":
    """Create and configure a FraiseQL server.

    This is the ONLY place Python creates a server.
    After this, Rust handles everything.
    """

    # Compile schema if needed
    if isinstance(schema, SchemaCompiler):
        schema = schema.compile()

    # Validate everything
    schema_errors = SchemaValidator.validate(schema)
    config_errors = config.validate()

    if schema_errors or config_errors:
        errors = schema_errors + config_errors
        raise StartupError(f"Configuration errors: {errors}")

    # Serialize for Rust
    schema_json = schema.to_json()
    config_json = config.to_json()

    # Create Rust server
    rust_server = fraiseql_rs.create_server(
        schema_json=schema_json,
        config_json=config_json,
    )

    # Start Rust server
    await rust_server.start()

    return rust_server
```

#### Task 4.2: Implement Axum integration
File: `fraiseql/server/axum.py` (50 LOC)

```python
async def run_axum_server(
    schema: CompiledSchema,
    config: FraiseQLConfig,
) -> None:
    """Run FraiseQL on Axum (Rust HTTP server)."""

    server = await create_server(schema, config)

    # Server is running in Rust, wait for shutdown signal
    await server.wait_for_shutdown()
```

#### Task 4.3: Create startup utilities
File: `fraiseql/server/utils.py` (100 LOC)

```python
class StartupError(Exception):
    """Raised when server startup fails."""
    pass

def validate_startup(
    schema: CompiledSchema,
    config: FraiseQLConfig,
) -> list[str]:
    """Validate everything before starting server."""
    errors = []
    errors.extend(SchemaValidator.validate(schema))
    errors.extend(config.validate())
    return errors

def log_startup_info(
    schema: CompiledSchema,
    config: FraiseQLConfig,
) -> None:
    """Log server startup information."""
    print(f"FraiseQL Server Starting")
    print(f"  Host: {config.server.host}:{config.server.port}")
    print(f"  Types: {len(schema.types)}")
    print(f"  Queries: {len(schema.queries)}")
    print(f"  Mutations: {len(schema.mutations)}")
    print(f"  Auth: {'Required' if config.security.authentication_required else 'Optional'}")
```

### Day 3: Documentation & Examples

#### Task 4.4: Create example usage
File: `examples/basic_server.py` (50 LOC)

```python
from fraiseql import type, query, ID
from fraiseql.config import FraiseQLConfig, DatabaseConfig
from fraiseql.server import create_server

# Define types
@type
class User:
    """A user in the system."""
    id: ID
    name: str
    email: str | None = None

# Define queries
@query
class Query:
    @staticmethod
    def users() -> list[User]:
        """Get all users."""
        pass

# Create config
config = FraiseQLConfig(
    database=DatabaseConfig(url="postgresql://localhost/fraiseql"),
    # ... rest of config
)

# Start server
import asyncio
asyncio.run(create_server(Query, config))
```

#### Task 4.5: Create comprehensive documentation
File: `docs/PHASE_0_COMPLETE.md` (200 LOC)

Complete guide covering:
- Type system architecture
- Configuration system
- Schema compiler
- Server startup flow
- Examples and tutorials

### Day 4-5: Integration tests & validation

#### Task 4.6: End-to-end tests
File: `tests/integration/phase_0/` (300+ LOC)

```python
def test_full_startup_flow():
    """Test complete startup: types → compiler → config → server."""

    @type
    class User:
        id: ID
        name: str

    @query
    class Query:
        @staticmethod
        def users() -> list[User]:
            pass

    compiler = SchemaCompiler.get_default()
    schema = compiler.compile()

    config = FraiseQLConfig(
        database=DatabaseConfig(url="postgresql://localhost/test"),
    )

    # Should not raise
    errors = validate_startup(schema, config)
    assert len(errors) == 0

def test_schema_to_json_validity():
    """Test schema JSON is valid and Rust-compatible."""
    # ... test

def test_config_to_json_validity():
    """Test config JSON is valid."""
    # ... test
```

#### Task 4.7: Quality checks
- [ ] Run full test suite: `pytest tests/ -v`
- [ ] Check coverage: `pytest --cov=fraiseql tests/`
- [ ] Run type checks: `mypy fraiseql/`
- [ ] Run linter: `ruff check fraiseql/`
- [ ] Code review with Rust team

### Week 4 Deliverables
- [ ] Server startup module
- [ ] Axum integration (thin wrapper)
- [ ] Startup utilities & validation
- [ ] Example usage code
- [ ] Comprehensive documentation
- [ ] 250+ integration tests
- [ ] Full type checking (mypy)
- [ ] Full test coverage (95%+)
- [ ] All linting passes

---

## Phase 0 Completion Checklist

### Code Quality
- [ ] All code has type hints (Python 3.13+)
- [ ] All modules have docstrings
- [ ] 95%+ test coverage
- [ ] All tests pass
- [ ] Zero linting errors
- [ ] Zero type checking errors
- [ ] All examples run without errors

### Architecture
- [ ] Zero execution logic in Python
- [ ] All classes are pure data or pure functions
- [ ] Clear separation of concerns
- [ ] CompiledSchema JSON is Rust-compatible
- [ ] FraiseQLConfig JSON is Rust-compatible

### Documentation
- [ ] API reference complete
- [ ] Architecture guide complete
- [ ] 3+ working examples
- [ ] Configuration guide
- [ ] Schema format specification

### Testing
- [ ] Unit tests for all modules
- [ ] Integration tests for startup flow
- [ ] JSON validity tests
- [ ] Validation tests
- [ ] End-to-end tests

### Validation
- [ ] Rust team validates JSON formats
- [ ] Backward compatibility (PrintOptim)
- [ ] Performance baselines established

---

## Success Criteria

✅ **All code is production-ready**
✅ **Zero technical debt introduced**
✅ **95%+ test coverage**
✅ **Complete documentation**
✅ **Rust team validates JSON outputs**
✅ **Ready for Phase 1**

---

## Timeline

| Week | Deliverables | Hours |
|------|--------------|-------|
| 1 | Type system v2 | 40 |
| 2 | Configuration system | 40 |
| 3 | Schema compiler | 40 |
| 4 | Server integration | 30 |
| **Total** | **Phase 0 Complete** | **150** |

---

## Team Structure

**Phase 0 Development**:
- 1 Senior Python Architect (oversight, architecture decisions)
- 1 Python Developer (implementation)
- Rust team (weekly validation of JSON outputs)

**Effort**: ~150 developer-hours (3-4 weeks at full-time)

---

## Next Phase

Once Phase 0 is complete:
- All foundation infrastructure is ready
- Phases 1-5 can proceed smoothly
- No rework needed
- Clean, sustainable codebase established

**Phase 0 → Phase 1**: Type System Refactoring

---

**Status**: Ready for execution
**Quality Focus**: Excellence (95%+ coverage minimum)
**Timeline**: 4 weeks
**Team**: 1-2 Python developers + oversight
**Next Action**: Begin Week 1 tasks
