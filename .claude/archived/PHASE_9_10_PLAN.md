# Phase 9.10: Language-Agnostic Arrow Flight Schema (REVISED)

**Objective**: Enable Arrow schemas to be authored in ANY programming language

**Duration**: 1.5 weeks | **Effort**: 6 days of implementation | **Priority**: High

---

## Core Principle (Clarified)

**The Arrow Flight RUNTIME runs in Rust. The AUTHORING is language-agnostic.**

```
Arrow Schema Authoring (Python/TypeScript/YAML/JSON)
    ↓
fraiseql compile-arrow (CLI command)
    ↓
schema.arrow.json (Language-agnostic artifact)
    ↓
Arrow Flight Server (Rust - fraiseql-arrow crate)
    ↓
Clients in Any Language (PyArrow, R, Go, Java, C#, Node.js, etc.)
```

**Key Point**: We are NOT building Arrow Flight servers in Go, Java, C#, etc. We are building authoring tools in those languages that compile to a common schema format for the single Rust runtime.

---

## Problem Statement

### Current State

- Arrow schemas defined in Rust code (type definitions)
- EntityEvent, User schemas hardcoded in `fraiseql-arrow`
- No standard schema format for other languages to reference

### Gap

- **Python developer**: Can't define Arrow schema without writing Rust
- **Go backend**: Can't validate Arrow schemas without Rust tooling
- **Java system**: No way to introspect Arrow schema format
- **TypeScript web app**: Can't understand Arrow table structure

### Impact

- Schema validation happens only in Rust
- Other languages guess schema structure
- Schema changes require Rust recompilation
- No language-neutral schema registry

---

## Solution: Language-Agnostic Schema Definition

### Three Components

```
┌─────────────────────────────────────────────────────────┐
│  Arrow Schema Format (.arrow-schema)                     │
│  - JSON-based, language-neutral                          │
│  - Defines tables, fields, types, constraints           │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────┐
│  Schema Authoring Libraries (Language SDKs)              │
│  - Python: fraiseql_arrow library                        │
│  - TypeScript: @fraiseql/arrow library                   │
│  - Go, Java, C#, etc.: Generate code from .arrow-schema │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────┐
│  Single Arrow Flight Server (Rust)                       │
│  - Loads .arrow-schema artifacts                         │
│  - Serves data to any language client                    │
│  - No language-specific implementations                  │
└─────────────────────────────────────────────────────────┘
```

---

## Implementation Plan (6 Days)

### Day 1-2: Arrow Schema Format & Standard Library

**Goal**: Define schema format, create Python/TypeScript libraries

**Deliverables**:

1. **Arrow Schema Specification** (`docs/arrow-flight/arrow-schema-format.md`)
   - JSON schema format (similar to Avro)
   - Types: scalars, temporal, complex (list, struct, map, union)
   - Field attributes: required, nullable, default, index, doc
   - Table attributes: namespace, version, ttl, partition_strategy
   - Example:
     ```json
     {
       "namespace": "fraiseql.events",
       "name": "EntityEvent",
       "version": "1.0",
       "fields": [
         {"name": "event_id", "type": "string", "required": true},
         {"name": "timestamp", "type": "timestamp_us", "required": true},
         {"name": "data", "type": "string", "required": true}
       ]
     }
     ```

2. **Python Authoring Library** (`crates/fraiseql-arrow/python/`)
   - `fraiseql_arrow` pip package (NEW)
   - `@schema` decorator for defining Arrow schemas
   - Export to `.arrow-schema` JSON format
   - Validation and type checking

   ```python
   from fraiseql_arrow import schema, Schema, Field, String, Timestamp

   @schema(namespace="fraiseql.events", version="1.0")
   class EntityEvent(Schema):
       event_id: String(required=True, index=True)
       event_type: String(required=True)
       timestamp: Timestamp(resolution="us", required=True, index=True)
       data: String(required=True)
       user_id: String(required=False)
       org_id: String(required=False)

   # Export to .arrow-schema JSON
   EntityEvent.to_schema_file("EntityEvent.arrow-schema")
   ```

3. **TypeScript Authoring Library** (`crates/fraiseql-arrow/typescript/`)
   - `@fraiseql/arrow` npm package (NEW)
   - TypeScript classes for schema definition
   - JSON schema generation

   ```typescript
   import { ArrowSchema, Field, StringType, TimestampType } from '@fraiseql/arrow';

   class EntityEvent extends ArrowSchema {
     @Field(new StringType({ required: true, index: true }))
     eventId: string;

     @Field(new TimestampType({ resolution: 'us', required: true }))
     timestamp: Date;

     @Field(new StringType({ required: true }))
     data: string;
   }

   // Export
   const schema = new EntityEvent().toSchema();
   fs.writeFileSync('EntityEvent.arrow-schema', JSON.stringify(schema));
   ```

4. **Schema Validator** (`crates/fraiseql-codegen/src/arrow_schema_validator.rs`)
   - Load and validate `.arrow-schema` JSON files
   - Check field types, constraints, naming conventions
   - Report errors clearly

**Effort**: 2 days

---

### Day 3-4: Server Integration & CLI

**Goal**: Load `.arrow-schema` files, serve via Arrow Flight

**Deliverables**:

1. **Schema Registry** (Rust, `fraiseql-arrow/src/schema_registry.rs`, ~200 lines)
   ```rust
   pub struct ArrowSchemaRegistry {
       schemas: HashMap<String, ArrowSchema>,  // namespace/name → schema
   }

   impl ArrowSchemaRegistry {
       // Load from .arrow-schema files
       fn load_from_directory(path: &str) -> Result<Self> { }

       // Serve to Flight clients
       fn get_schema(&self, name: &str) -> Option<&ArrowSchema> { }

       // Generate RecordBatch from schema + data
       fn create_batch(&self, schema_name: &str, data: &[u8]) -> Result<RecordBatch> { }
   }
   ```

2. **Flight GetFlightInfo Handler** (Updated, ~50 lines)
   ```rust
   fn get_flight_info(&self, descriptor: &FlightDescriptor) -> Result<FlightInfo> {
       // descriptor.path[0] = schema name (from .arrow-schema)
       let schema = self.registry.get_schema(&descriptor.path[0])?;

       // Return Arrow schema + endpoints
       Ok(FlightInfo {
           schema: schema.to_arrow_schema(),
           endpoints: /* endpoints */,
           total_records: /* count */,
       })
   }
   ```

3. **CLI Command** (`fraiseql-cli/src/commands/arrow-schema.rs`)
   ```bash
   # Validate schema
   fraiseql arrow-schema validate EntityEvent.arrow-schema

   # Export to Arrow proto format
   fraiseql arrow-schema export EntityEvent.arrow-schema --format arrow-ipc

   # Register schema with server
   fraiseql arrow-schema register EntityEvent.arrow-schema --server http://localhost:8080
   ```

4. **Configuration** (Updated `fraiseql/config.toml`)
   ```toml
   [arrow]
   schemas_dir = "./schemas"  # Load all .arrow-schema files from here
   registry_mode = "file"      # File-based registry (no separate service)
   ```

**Effort**: 2 days

---

### Day 5-6: Integration & Documentation

**Goal**: End-to-end flow: author in Python/TypeScript → run in Rust

**Deliverables**:

1. **End-to-End Example** (`examples/arrow-schema-authoring/`)
   ```bash
   ├── python/
   │   ├── define_schema.py          # Define EntityEvent in Python
   │   └── EntityEvent.arrow-schema  # Generated schema file
   ├── typescript/
   │   ├── define_schema.ts          # Define Order in TypeScript
   │   └── Order.arrow-schema        # Generated schema file
   └── rust/
       ├── main.rs                   # Load both schemas, serve via Flight
       └── config.toml
   ```

2. **How-To Guide** (`docs/arrow-flight/defining-schemas.md`)
   - Define in Python: Python decorators → JSON export
   - Define in TypeScript: TypeScript classes → JSON export
   - Define in YAML: Manual YAML → validate with CLI
   - Load in Rust: Automatic schema discovery from directory
   - Use with Flight: Client queries by schema name

3. **Schema Versioning Guide** (`docs/arrow-flight/schema-versioning.md`)
   - Version field in schemas
   - Backward compatibility rules
   - Migration strategies

4. **Testing** (`tests/arrow_schema_integration.rs`, ~200 lines)
   - Load Python-generated schema in Rust
   - Load TypeScript-generated schema in Rust
   - Verify schema compatibility
   - E2E: Python author schema → Rust load → Python client query

**Effort**: 2 days

---

## Detailed Specification

### Arrow Schema Format (JSON)

```json
{
  "namespace": "fraiseql.events",
  "name": "EntityEvent",
  "version": "1.0",
  "description": "Observer event representing entity state change",

  "fields": [
    {
      "name": "event_id",
      "type": "string",
      "required": true,
      "doc": "Unique event ID (UUID v4)"
    },
    {
      "name": "entity_type",
      "type": "string",
      "required": true,
      "index": true,
      "doc": "Entity type (e.g., User, Order)"
    },
    {
      "name": "entity_id",
      "type": "string",
      "required": true,
      "doc": "Entity primary key"
    },
    {
      "name": "timestamp",
      "type": {
        "precision": "microsecond",
        "timezone": "UTC"
      },
      "required": true,
      "index": true,
      "doc": "Event timestamp"
    },
    {
      "name": "data",
      "type": "string",
      "required": true,
      "doc": "JSON payload"
    },
    {
      "name": "user_id",
      "type": "string",
      "required": false,
      "nullable": true,
      "doc": "User who triggered event"
    }
  ],

  "metadata": {
    "storage": {
      "primary": "clickhouse",
      "table": "fraiseql_events"
    },
    "ttl_days": 90,
    "partition_strategy": "monthly"
  }
}
```

---

## File Structure

```
crates/fraiseql-arrow/
├── src/
│   └── schema_registry.rs          # Load and serve schemas
│
├── python/                         # NEW - Python library
│   ├── pyproject.toml
│   ├── fraiseql_arrow/
│   │   ├── __init__.py
│   │   ├── schema.py               # @schema decorator
│   │   ├── fields.py               # Field types
│   │   ├── exporter.py             # Export to .arrow-schema
│   │   └── validator.py            # Validate JSON schema
│   └── examples/
│       └── define_entity_event.py
│
├── typescript/                     # NEW - TypeScript library
│   ├── package.json
│   ├── src/
│   │   ├── index.ts
│   │   ├── schema.ts               # ArrowSchema base class
│   │   ├── fields.ts               # Field decorators
│   │   ├── types.ts                # Type definitions
│   │   └── exporter.ts             # Export to .arrow-schema
│   └── examples/
│       └── define-order-schema.ts
│
└── schemas/                        # Directory of .arrow-schema files
    ├── EntityEvent.arrow-schema
    ├── User.arrow-schema
    └── Order.arrow-schema

crates/fraiseql-cli/
└── src/
    └── commands/
        └── arrow_schema.rs         # CLI: validate, export, register

docs/arrow-flight/
├── arrow-schema-format.md          # Format specification
├── defining-schemas.md             # How-to guide (Python/TypeScript/YAML)
└── schema-versioning.md            # Versioning strategy

tests/
└── arrow_schema_integration.rs     # E2E tests
```

---

## Benefits

| Benefit | Impact |
|---------|--------|
| **Language-agnostic authoring** | Python/TypeScript devs can define schemas without Rust |
| **Single server runtime** | One Arrow Flight server in Rust, all clients connect to it |
| **Schema as artifact** | `.arrow-schema` files are portable, versionable, shareable |
| **Auto-discovery** | Server loads all schemas from directory automatically |
| **Type validation** | Each language validates schema types at author time |
| **Standardized format** | Schema format is language-neutral JSON |
| **Client flexibility** | Clients in any language can query any schema |

---

## Timeline & Milestones

| Days | Milestone | Output |
|------|-----------|--------|
| **1-2** | Python + TypeScript libraries | `fraiseql_arrow` + `@fraiseql/arrow` packages |
| **3-4** | Server integration + CLI | Schema registry, Flight handler, CLI commands |
| **5-6** | Examples + documentation | E2E examples, how-to guides, tests |

**Total**: 6 implementation days (1.5 weeks)

---

## Success Criteria

- ✅ Python library can define Arrow schemas
- ✅ TypeScript library can define Arrow schemas
- ✅ Both export to standardized `.arrow-schema` JSON
- ✅ Rust server loads `.arrow-schema` files from directory
- ✅ Flight GetFlightInfo returns correct schema
- ✅ Python-authored schema works with Rust server
- ✅ TypeScript-authored schema works with Rust server
- ✅ YAML-defined schema works with Rust server
- ✅ Complete documentation with examples
- ✅ All tests pass (Python, TypeScript, Rust integration)

---

## Dependencies

- ✅ Phase 9.1-9.8 complete (Arrow Flight working)
- ✅ Phase 9.9 testing done (confidence in Arrow Flight)
- No external blocker - can start immediately after Phase 9.9

---

## Key Difference from Original Plan

### Original Phase 9.10 (INCORRECT)

- Implement Arrow Flight servers in 5 languages (Go, Java, C#, Node.js, C++)
- Generate client code in 5 languages
- Cross-language server interop

### Revised Phase 9.10 (CORRECT)

- Author Arrow schemas in ANY language (Python, TypeScript, YAML, etc.)
- Compile to `.arrow-schema` format
- Single Rust Arrow Flight server
- Any-language clients connect to single server

**This matches the core FraiseQL architecture principle:**
- Authoring: Language-agnostic
- Compilation: Standardized artifacts
- Runtime: Single Rust implementation

---

## Next Step

Review this revised approach. Does this align with the authoring/compilation/runtime separation principle?

If yes, proceed with Phase 9.10 implementation after Phase 9.9 testing complete.
