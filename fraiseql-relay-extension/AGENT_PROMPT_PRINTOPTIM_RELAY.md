# Agent Prompt: Create PrintOptim Backend Relay API

## Mission

Create a complete, production-ready **PrintOptim Backend Relay API** from scratch that demonstrates the full power of GraphQL Relay specification compliance using FraiseQL and the PostgreSQL Relay extension.

**Target Location**: `/home/lionel/code/printoptim_backend_relay/`
**Initial Focus**: DNS Server domain only (as proof of concept)
**Architecture**: Clean Architecture + CQRS + FraiseQL + GraphQL Relay

## Reference Codebases

You have access to two reference implementations:

1. **FraiseQL Relay Extension** (this directory): Complete PostgreSQL extension with Python integration
2. **PrintOptim Backend Fresh** (`/home/lionel/code/printoptim_backend_fresh/`): Production-grade PrintOptim backend with DNS server implementation

## Core Requirements

### 1. Architecture Foundation

Implement a modern Python backend with these architectural patterns:

```
┌─────────────────────────────────────────────────────────┐
│ GraphQL Relay Layer (FraiseQL + Relay Extension)       │
│ - Node interface with global ID resolution             │
│ - Relay-compliant mutations and queries                │
│ - High-performance batch node resolution               │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│ Application Layer (Clean Architecture)                 │
│ - GraphQL resolvers and mutations                      │
│ - Domain services and use cases                        │
│ - Input validation and error handling                  │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│ PostgreSQL + Relay Extension                           │
│ - Entity registry (core.tb_entity_registry)            │
│ - Multi-layer cache (TurboRouter, tv_, mv_, v_)        │
│ - CQRS pattern (command/query separation)              │
└─────────────────────────────────────────────────────────┘
```

### 2. Project Structure

Create the following directory structure:

```
printoptim_backend_relay/
├── README.md                                # Project overview and setup
├── pyproject.toml                          # Modern Python configuration (uv)
├── Makefile                                # Development commands
├── .env.example                            # Environment configuration
├── docker-compose.yml                      # PostgreSQL + development setup
├── .github/workflows/                      # CI/CD configuration
├── src/printoptim_backend_relay/
│   ├── __init__.py
│   ├── main.py                             # FastAPI application entry
│   ├── config.py                           # Configuration management
│   ├── domain/                             # Domain entities and services
│   │   ├── __init__.py
│   │   ├── dns_server/                     # DNS Server domain
│   │   │   ├── __init__.py
│   │   │   ├── entities.py                 # Domain entities
│   │   │   ├── services.py                 # Domain services
│   │   │   └── types.py                    # GraphQL types with Node interface
│   │   └── shared/                         # Shared domain logic
│   ├── infrastructure/                     # External concerns
│   │   ├── __init__.py
│   │   ├── database/                       # Database connections
│   │   ├── relay/                          # Relay extension integration
│   │   └── repositories/                   # Data access layer
│   ├── application/                        # Application services
│   │   ├── __init__.py
│   │   ├── dns_server/                     # DNS Server use cases
│   │   │   ├── __init__.py
│   │   │   ├── queries.py                  # Query resolvers
│   │   │   └── mutations.py                # Mutation resolvers
│   │   └── shared/                         # Shared application logic
│   └── entrypoints/                        # API interfaces
│       ├── __init__.py
│       ├── graphql/                        # GraphQL endpoint
│       │   ├── __init__.py
│       │   ├── schema.py                   # Schema assembly
│       │   └── context.py                  # GraphQL context
│       └── rest/                           # REST endpoints (health, etc.)
├── db/                                     # Database schema and migrations
│   ├── schema/                             # SQL schema files
│   │   ├── 01_extensions.sql               # PostgreSQL extensions
│   │   ├── 02_dns_server.sql              # DNS Server tables and views
│   │   └── 03_functions.sql               # Database functions
│   ├── migrations/                         # Schema migrations
│   └── seeds/                             # Test data
└── tests/                                 # Test suite
    ├── __init__.py
    ├── conftest.py                        # Test configuration
    ├── unit/                              # Unit tests
    ├── integration/                       # Integration tests
    └── fixtures/                          # Test data and utilities
```

### 3. DNS Server Domain Implementation

Based on the PrintOptim Backend Fresh reference, implement a complete DNS Server domain:

#### Database Schema

```sql
-- Command Side
CREATE TABLE tenant.tb_dns_server (
    id INTEGER GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    pk_dns_server UUID DEFAULT gen_random_uuid() NOT NULL,
    fk_customer_org UUID NOT NULL,
    identifier TEXT NOT NULL,
    ip_address INET NOT NULL,

    -- Audit fields
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by UUID,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by UUID,
    deleted_at TIMESTAMPTZ,
    deleted_by UUID,

    CONSTRAINT tb_dns_server_identifier_key UNIQUE (identifier),
    CONSTRAINT tb_dns_server_pk_dns_server_key UNIQUE (pk_dns_server)
);

-- Query Side (Relay-compatible)
CREATE OR REPLACE VIEW public.v_dns_server AS
SELECT
    tb_dns_server.pk_dns_server AS id,  -- Global ID for Relay
    tb_dns_server.fk_customer_org AS tenant_id,
    tb_dns_server.deleted_at IS NULL AS is_current,
    jsonb_build_object(
        'id', tb_dns_server.pk_dns_server,
        'identifier', tb_dns_server.identifier,
        'ipAddress', tb_dns_server.ip_address::text,
        'createdAt', tb_dns_server.created_at,
        'updatedAt', tb_dns_server.updated_at
    ) AS data
FROM tenant.tb_dns_server
WHERE tb_dns_server.deleted_at IS NULL
ORDER BY tb_dns_server.created_at DESC;
```

#### GraphQL Types with Node Interface

```python
import fraiseql
from fraiseql_relay_extension import Node
from uuid import UUID
from typing import Optional
from datetime import datetime

@fraiseql.type
class DnsServer(Node):
    """DNS Server with Relay Node interface compliance."""

    id: UUID  # Global ID (required by Node interface)
    identifier: str
    ip_address: str  # Note: camelCase for GraphQL
    created_at: datetime
    updated_at: datetime
    tenant_id: UUID

    @classmethod
    def from_dict(cls, data: dict) -> "DnsServer":
        """Create DnsServer from database JSONB data."""
        return cls(
            id=UUID(data["id"]),
            identifier=data["identifier"],
            ip_address=data["ipAddress"],  # Convert from camelCase
            created_at=data["createdAt"],
            updated_at=data["updatedAt"],
            tenant_id=UUID(data.get("tenantId"))  # If available
        )
```

#### CRUD Operations

Implement complete CRUD with proper error handling:

```python
# Create Mutation
@fraiseql.input
class CreateDnsServerInput:
    identifier: str
    ip_address: str

@fraiseql.success
class CreateDnsServerSuccess:
    message: str = "DNS server created successfully"
    dns_server: DnsServer

@fraiseql.failure
class CreateDnsServerError:
    message: str
    error_code: str
    conflict_dns_server: Optional[DnsServer] = None

@fraiseql.mutation
async def create_dns_server(
    info,
    input: CreateDnsServerInput
) -> CreateDnsServerSuccess | CreateDnsServerError:
    """Create a new DNS server with proper error handling."""
    # Implementation here - call PostgreSQL function
    pass

# Similar patterns for update_dns_server, delete_dns_server
```

#### Query Resolvers

```python
@fraiseql.query
async def dns_servers(
    info,
    where: Optional[DnsServerWhereInput] = None,
    limit: int = 100,
    offset: int = 0,
    order_by: Optional[List[DnsServerOrderByInput]] = None
) -> List[DnsServer]:
    """Query DNS servers with filtering and pagination."""
    # Use the Relay context for high-performance resolution
    relay_context = info.context["relay"]
    # Implementation here
    pass

@fraiseql.query
async def dns_server(info, id: UUID) -> Optional[DnsServer]:
    """Get a single DNS server by ID - leverages Node interface."""
    # This will automatically use the high-performance node resolution
    return await info.context["node_resolver"](id)
```

### 4. PostgreSQL Extension Integration

Set up the FraiseQL Relay Extension:

```python
# In your application startup
from fraiseql_relay_extension import enable_relay_support

async def setup_relay(schema, db_pool):
    """Configure Relay support with entity registration."""

    # Enable Relay support
    relay = await enable_relay_support(
        schema,
        db_pool,
        global_id_format="uuid",  # Use direct UUIDs
        auto_register=False  # We'll register manually for control
    )

    # Register DNS Server entity
    await relay.register_entity_type(
        entity_type=DnsServer,
        entity_name="DnsServer",
        pk_column="pk_dns_server",
        v_table="v_dns_server",
        source_table="tenant.tb_dns_server",
        # Optional performance optimizations:
        # tv_table="tv_dns_server",  # Materialized table
        # turbo_function="turbo.fn_get_dns_servers",  # TurboRouter
    )

    return relay
```

### 5. Testing Framework

Create comprehensive tests following the PrintOptim Backend Fresh patterns:

```python
# tests/integration/test_dns_server_mutations.py
import pytest
from uuid import UUID

@pytest.mark.asyncio
async def test_create_dns_server_success(graphql_client):
    """Test successful DNS server creation."""

    mutation = """
        mutation CreateDnsServer($input: CreateDnsServerInput!) {
            createDnsServer(input: $input) {
                __typename
                ... on CreateDnsServerSuccess {
                    message
                    dnsServer {
                        id
                        identifier
                        ipAddress
                        createdAt
                    }
                }
                ... on CreateDnsServerError {
                    message
                    errorCode
                    conflictDnsServer {
                        id
                        identifier
                        ipAddress
                    }
                }
            }
        }
    """

    input_data = {
        "identifier": "test-dns-001",
        "ipAddress": "192.168.1.1"
    }

    result = await graphql_client.execute(mutation, {"input": input_data})

    # Verify success
    assert result["data"]["createDnsServer"]["__typename"] == "CreateDnsServerSuccess"
    dns_server = result["data"]["createDnsServer"]["dnsServer"]
    assert dns_server["identifier"] == "test-dns-001"
    assert dns_server["ipAddress"] == "192.168.1.1"
    assert dns_server["id"] is not None

@pytest.mark.asyncio
async def test_node_resolution(graphql_client):
    """Test Relay node interface resolution."""

    # First create a DNS server
    # ... creation logic ...

    # Test node resolution
    query = """
        query GetNode($id: UUID!) {
            node(id: $id) {
                __typename
                ... on DnsServer {
                    id
                    identifier
                    ipAddress
                }
            }
        }
    """

    result = await graphql_client.execute(query, {"id": dns_server_id})

    assert result["data"]["node"]["__typename"] == "DnsServer"
    assert result["data"]["node"]["id"] == str(dns_server_id)
```

### 6. Development Setup

Create modern development tooling:

#### pyproject.toml
```toml
[project]
name = "printoptim-backend-relay"
version = "0.1.0"
description = "PrintOptim Backend with GraphQL Relay compliance"
requires-python = ">=3.11"
dependencies = [
    "fastapi>=0.104.1",
    "fraiseql>=0.5.2",
    "psycopg[binary]>=3.1.0",
    "pydantic>=2.4.2",
    "pydantic-settings>=2.0.3",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.4.0",
    "pytest-asyncio>=0.21.1",
    "ruff>=0.1.6",
    "pre-commit>=3.5.0",
]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.ruff]
line-length = 100
target-version = "py311"

[tool.ruff.lint]
select = ["E", "F", "I", "N", "W", "UP"]
ignore = ["E501"]

[tool.pytest.ini_options]
asyncio_mode = "auto"
testpaths = ["tests"]
```

#### Makefile
```makefile
.PHONY: help dev test format lint clean db-setup

help:
	@echo "PrintOptim Backend Relay - Development Commands"
	@echo ""
	@echo "Setup:"
	@echo "  dev          Install development dependencies"
	@echo "  db-setup     Setup PostgreSQL database with extension"
	@echo ""
	@echo "Testing:"
	@echo "  test         Run full test suite"
	@echo "  test-unit    Run unit tests only"
	@echo "  test-integration  Run integration tests only"
	@echo ""
	@echo "Code Quality:"
	@echo "  format       Format code with ruff"
	@echo "  lint         Lint code with ruff"
	@echo ""
	@echo "Development:"
	@echo "  serve        Start development server"
	@echo "  clean        Clean up generated files"

dev:
	uv sync --extra dev
	uv run pre-commit install

db-setup:
	docker-compose up -d postgres
	sleep 5
	uv run python scripts/setup_database.py

test:
	uv run pytest tests/ -v

serve:
	uv run uvicorn src.printoptim_backend_relay.main:app --reload --host 0.0.0.0 --port 8000

format:
	uv run ruff format .

lint:
	uv run ruff check .
```

### 7. Performance Requirements

The implementation must demonstrate:

- **Sub-5ms node resolution** using the PostgreSQL extension
- **Batch resolution with 10x+ improvement** over individual queries
- **Multi-layer caching** with materialized tables and TurboRouter integration
- **Comprehensive benchmarking** comparing with and without Relay extension

### 8. Documentation Requirements

Create comprehensive documentation:

- **README.md**: Quick start, architecture overview, development setup
- **API_DOCUMENTATION.md**: GraphQL schema documentation with examples
- **PERFORMANCE_ANALYSIS.md**: Benchmarking results and optimization guide
- **DEPLOYMENT_GUIDE.md**: Production deployment instructions

## Implementation Steps

Execute this implementation in the following order:

### Phase 1: Foundation (Essential)
1. Create project structure and basic configuration
2. Set up PostgreSQL database with Relay extension
3. Implement basic DNS Server domain entities
4. Create database schema and views

### Phase 2: Core Functionality (Critical)
1. Implement GraphQL types with Node interface
2. Set up FraiseQL schema with Relay integration
3. Create CRUD mutations with proper error handling
4. Implement query resolvers with filtering and pagination

### Phase 3: Testing & Quality (Required)
1. Set up comprehensive test framework
2. Implement integration tests for all CRUD operations
3. Add Node interface resolution tests
4. Create performance benchmarks

### Phase 4: Documentation & Polish (Important)
1. Write comprehensive documentation
2. Add development tooling and scripts
3. Set up CI/CD pipeline
4. Performance optimization and monitoring

## Success Criteria

The implementation is complete when:

✅ **DNS Server CRUD operations** work flawlessly with proper error handling
✅ **Node interface resolution** works via `node(id: UUID!)` query
✅ **Batch node resolution** demonstrates significant performance improvement
✅ **Integration tests** cover all major functionality with >95% success rate
✅ **Performance benchmarks** show sub-5ms node resolution
✅ **Documentation** is comprehensive and enables easy onboarding
✅ **Code quality** meets production standards (linting, formatting, type checking)

## Final Notes

This implementation should serve as a **reference architecture** for building high-performance GraphQL APIs with full Relay specification compliance using FraiseQL and PostgreSQL.

The focus on the DNS Server domain provides a concrete, testable example while demonstrating all the architectural patterns and performance optimizations.

Upon completion, this will be a production-ready foundation that can be extended to support additional domains and entities following the same patterns.

**Remember**: This is not just a proof of concept - build it as a production-grade system that demonstrates the full power and performance potential of the FraiseQL + Relay architecture.
