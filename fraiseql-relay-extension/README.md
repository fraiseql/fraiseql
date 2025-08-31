# FraiseQL Relay Extension

A PostgreSQL extension implementing GraphQL Relay specification compliance for FraiseQL with database-native performance optimization.

## Overview

This PostgreSQL extension provides:
- **Global Object Identification**: Node interface with UUID-based global IDs
- **Registry-Driven Entity Management**: Dynamic entity registration and view generation
- **Multi-Layer Cache Integration**: TurboRouter, lazy cache, materialized tables support
- **C-Optimized Performance**: Critical path functions implemented in C
- **Python Integration Layer**: Seamless FraiseQL integration

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│ Python FraiseQL Layer                                   │
│ - GraphQL Schema & Resolvers                            │
│ - Type System Integration                               │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│ PostgreSQL Extension (fraiseql_relay)                   │
│ - Entity Registry (core.tb_entity_registry)             │
│ - Dynamic View Generation (core.refresh_v_nodes_view)   │
│ - Node Resolution (core.resolve_node_fast) [C]          │
│ - Cache Layer Routing (core.get_optimal_data_source)    │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│ Multi-Layer Cache Architecture                          │
│ - TurboRouter (turbo.* functions)                       │
│ - Lazy Cache (lazy cache patterns)                      │
│ - Materialized Tables (tv_*)                            │
│ - Materialized Views (mv_*)                             │
│ - Real-time Views (v_*)                                 │
└─────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Install Extension

```bash
# Build and install
make install

# Enable in your database
psql -d your_db -c "CREATE EXTENSION fraiseql_relay;"
```

### 2. Register Your Entities

```sql
-- Register entities with their corresponding views/tables
SELECT core.register_entity(
    p_entity_name := 'User',
    p_graphql_type := 'User',
    p_pk_column := 'pk_user',
    p_v_table := 'v_user',
    p_tv_table := 'tv_user',
    p_turbo_function := 'turbo.fn_get_users'
);

SELECT core.register_entity('Contract', 'Contract', 'pk_contract', 'v_contract', 'tv_contract');
SELECT core.register_entity('Post', 'Post', 'pk_post', 'v_post', NULL);
```

### 3. Python Integration

```python
# pip install fraiseql[relay]
from fraiseql.extensions.relay import enable_relay_support

# Enable Relay support in your schema
schema = enable_relay_support(existing_schema, db_pool)

# Auto-discover and register existing entities
await schema.discover_and_register_entities()
```

### 4. Use Node Resolution

```python
# GraphQL Query
query = """
  query GetNode($id: UUID!) {
    node(id: $id) {
      __typename
      ... on User {
        name
        email
      }
      ... on Contract {
        title
        status
      }
    }
  }
"""

# Resolves through high-performance C function
result = await client.execute(query, {"id": "550e8400-e29b-41d4-a716-446655440000"})
```

## Features

### ✅ Core Functionality
- [x] Entity registry with metadata
- [x] Dynamic v_nodes view generation
- [x] Fast node resolution (C implementation)
- [x] Multi-layer cache integration
- [x] Python FraiseQL integration
- [x] Auto-discovery of existing views

### 🚧 Planned Features
- [ ] Relay Connection pagination helpers
- [ ] Mutation clientMutationId support
- [ ] Global ID encoding options
- [ ] Performance monitoring functions
- [ ] Schema introspection helpers

## File Structure

```
fraiseql-relay-extension/
├── README.md                           # This file
├── docs/
│   ├── technical-specification.md      # Complete technical spec
│   ├── graphql-specialist-review.md    # Expert review request
│   ├── migration-guide.md             # How to integrate with existing FraiseQL
│   └── performance-benchmarks.md      # Performance analysis
├── sql/
│   ├── fraiseql_relay--1.0.sql        # Extension schema
│   ├── fraiseql_relay.control         # Extension control file
│   └── migrations/                    # Schema migration files
├── src/
│   ├── fraiseql_relay.c               # C implementation (performance-critical)
│   ├── fraiseql_relay.h               # C headers
│   └── Makefile                       # Build configuration
├── python-integration/
│   ├── __init__.py
│   ├── relay.py                       # Python integration layer
│   ├── discovery.py                   # Auto-discovery of entities
│   └── types.py                       # GraphQL type integration
├── examples/
│   ├── basic-setup.sql                # Basic extension usage
│   ├── multi-tenant.sql               # Multi-tenant patterns
│   └── performance-optimization.sql    # Advanced optimization
└── tests/
    ├── sql/                           # PostgreSQL extension tests
    ├── python/                        # Python integration tests
    └── performance/                   # Performance benchmarks
```

## Development

### Build Requirements
- PostgreSQL 14+ development headers
- gcc/clang with C99 support
- make
- Python 3.11+ (for integration layer)

### Build and Test
```bash
# Build extension
make clean && make

# Install (requires PostgreSQL admin rights)
sudo make install

# Run tests
make test

# Performance benchmarks
make benchmark
```

## Performance

The extension provides significant performance improvements:

| Operation | Standard GraphQL | FraiseQL + Extension | Improvement |
|-----------|------------------|---------------------|-------------|
| Node Resolution | 50-100ms | 1-5ms | 10-50x |
| Entity Registration | N/A | <1ms | - |
| View Refresh | Manual | Automatic | ∞ |

## License

MIT License - same as FraiseQL core.

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) in the main FraiseQL repository.
