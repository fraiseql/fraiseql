# Migrating from FraiseQL v1 to v2

This guide covers the breaking changes and migration steps from FraiseQL v1 to v2.

## Overview

FraiseQL v2 is a ground-up rewrite. The core concepts (schema authoring, compiled SQL, GraphQL serving) are the same, but the implementation, configuration format, and API surface have changed significantly.

## Key Changes

### Schema Format

**v1**: Schema was defined inline in TOML configuration.
**v2**: Schema is authored in Python/TypeScript, exported to `schema.json`, then compiled to `schema.compiled.json`.

```bash
# v2 workflow
python schema.py              # generates schema.json
fraiseql-cli compile schema.json  # generates schema.compiled.json
fraiseql-server --schema schema.compiled.json
```

### Configuration

**v1**: Single `fraiseql.toml` with schema + config mixed.
**v2**: Separate concerns:

- Schema authoring: Python/TypeScript decorators
- Server config: `fraiseql.toml` (security, rate limiting, observability)
- Compiled output: `schema.compiled.json` (types + config + SQL)

### Database Support

**v1**: PostgreSQL only.
**v2**: PostgreSQL (primary), MySQL, SQLite, SQL Server.

### Authentication

**v1**: Basic token authentication.
**v2**: Full OAuth2/OIDC with PKCE, API key authentication, JWT validation, field-level authorization.

### Crate Structure

**v1**: Single `fraiseql` crate.
**v2**: Modular workspace with 16 crates (`fraiseql-core`, `fraiseql-server`, `fraiseql-db`, etc.).

## Migration Steps

### 1. Install v2 CLI

```bash
cargo install fraiseql-cli
```

### 2. Convert Schema Definitions

Translate your v1 TOML type definitions to Python decorators:

```python
# v1 (TOML)
# [types.User]
# sql_source = "users"
# fields = { id = "int", name = "string", email = "string" }

# v2 (Python)
import fraiseql

@fraiseql.type(sql_source="users")
class User:
    id: int
    name: str
    email: str
```

### 3. Update Configuration

Move security and server configuration to the new `fraiseql.toml` format. See `fraiseql.toml.example` in the repository root for all available options.

### 4. Compile and Test

```bash
python schema.py
fraiseql-cli compile schema.json -o schema.compiled.json
fraiseql-server --schema schema.compiled.json
```

### 5. Update Client Queries

GraphQL query syntax is unchanged. However:

- Relay cursor pagination is now available (if configured)
- REST endpoints are available alongside GraphQL
- Subscription support requires WebSocket configuration

## Breaking Changes Reference

| v1 Feature | v2 Equivalent | Notes |
|-----------|---------------|-------|
| TOML schema | Python/TS decorators + compile | Two-step process |
| `fraiseql serve` | `fraiseql-server` binary | Separate binary |
| Basic auth | OAuth2/OIDC/API keys | Much richer auth |
| PostgreSQL only | Multi-database | Feature flags per DB |
| Single crate | 16-crate workspace | Import `fraiseql` umbrella crate |

## Getting Help

- [Architecture Documentation](../architecture/README.md)
- [Getting Started Guide](getting-started.md)
- [GitHub Issues](https://github.com/fraiseql/fraiseql/issues)
