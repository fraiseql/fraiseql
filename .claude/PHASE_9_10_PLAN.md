# Phase 9.10: Cross-Language Arrow Flight SDK

**Objective**: Make Arrow Flight implementable in ANY programming language

**Duration**: 2 weeks | **Effort**: 10 days of implementation | **Priority**: High

---

## Problem Statement

### Current State
- Arrow Flight implemented in Rust (fraiseql-arrow crate)
- Python/R/Rust clients exist
- Schema definitions hardcoded in Rust
- New language support = rewrite everything

### Gap
- **No language-agnostic schema definition**
- **No code generation for clients/servers**
- **No formal specification for interop**
- **Hard to extend to Java, Go, C#, C++, Node.js**

### Impact
- Teams can't implement Arrow Flight in their preferred language
- Vendor lock-in to Rust ecosystem
- Difficult to integrate with legacy systems

---

## Solution: Language-Neutral SDK

### Three Core Components

```
┌─────────────────────────────────────────────────────────┐
│  Arrow Schema IDL (.arrow-schema files)                  │
│  - Define tables, fields, types, indexes                 │
│  - Language-agnostic YAML/JSON format                    │
│  - Example: EntityEvent, User, Order schemas             │
└─────────────┬───────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────────┐
│  Code Generators (Template-Driven)                        │
│  - Parse .arrow-schema → Generate language-specific code │
│  - 5 languages: Go, Java, C#, Node.js, C++              │
│  - 2 templates each: Serialization + RPC client         │
└─────────────┬───────────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────────┐
│  Protocol Specification & Examples                        │
│  - Formal Arrow Flight wire format spec                  │
│  - Cross-language interop guide                          │
│  - Example servers: Go, Java, Node.js                    │
└─────────────────────────────────────────────────────────┘
```

---

## Implementation Plan (10 Days)

### Day 1-3: Arrow Schema IDL Design & Tooling

**Goal**: Define schema language, create parser/validator

**Deliverables**:

1. **Arrow Schema Specification** (`docs/arrow-flight/schema-spec.md`)
   - JSON schema format (similar to Avro/Protobuf)
   - Supported field types:
     - Scalars: `bool`, `int8-64`, `uint8-64`, `float32-64`, `string`, `bytes`
     - Temporal: `date`, `time_us`, `timestamp_us`, `duration_us`
     - Complex: `list`, `struct`, `map`, `union`
   - Field attributes: `required`, `nullable`, `default`, `doc`
   - Table attributes: `namespace`, `version`, `description`, `indexes`
   - TTL configuration: `expires_after_days`, `partition_key`

2. **Example Schema Files** (`crates/fraiseql-arrow/schemas/`)
   - `EntityEvent.arrow-schema` (Observer events)
   - `User.arrow-schema` (Example entity)
   - `Order.arrow-schema` (Example with complex types)

3. **Schema Parser** (`crates/fraiseql-codegen/src/parser.rs`, ~200 lines)
   - Parse `.arrow-schema` files
   - Validate field types, required fields
   - Error messages for invalid schemas

4. **Schema Validator** (`crates/fraiseql-codegen/src/validator.rs`, ~150 lines)
   - Check for naming conventions
   - Verify index fields exist
   - Detect conflicting field names across tables

**Effort**: 3 days

**Key File**: `EntityEvent.arrow-schema`
```yaml
namespace: fraiseql.events
name: EntityEvent
version: 1.0
description: >
  Observer event: represents state change in entity.
  Streaming via Arrow Flight, storage in ClickHouse.

fields:
  - name: event_id
    type: string
    required: true
    doc: "Unique event identifier (UUID v4)"

  - name: entity_type
    type: string
    required: true
    doc: "Entity type from schema (e.g., 'User', 'Order')"
    index: true

  - name: entity_id
    type: string
    required: true
    doc: "Entity primary key"

  - name: event_type
    type: string
    required: true
    doc: "Event type (created, updated, deleted)"
    index: true

  - name: timestamp
    type: timestamp_us
    required: true
    doc: "Event timestamp in microseconds UTC"
    index: true

  - name: data
    type: string
    required: true
    doc: "Event payload as JSON string"

  - name: user_id
    type: string
    required: false
    doc: "User who triggered event"

  - name: org_id
    type: string
    required: false
    doc: "Organization context"

indexes:
  - name: idx_entity_timestamp
    fields: [entity_type, timestamp]
    description: "Partition by entity type, sort by timestamp"

ttl:
  expires_after_days: 90
  partition_strategy: monthly

metadata:
  storage:
    primary: clickhouse
    table_name: fraiseql_events
    materialized_views:
      - fraiseql_events_hourly
      - fraiseql_org_daily
```

---

### Day 4-6: Code Generators (5 Languages)

**Goal**: Generate client + serialization code for 5 languages

**Architecture**: Template-driven using Handlebars/Tera

**Process**:
1. Parse schema → AST
2. Map types to language-specific types
3. Render templates → language code
4. Output ready-to-use modules

#### Language Targets & Templates

| Language | Client Library | Serialization | Output |
|----------|----------------|---------------|--------|
| **Go** | `grpc-go` + `apache-arrow/go` | Manual struct marshaling | `.go` files |
| **Java** | `grpc-java` + `apache-arrow-java` | Protobuf serialization | `.java` files |
| **C#** | `grpc-dotnet` + `Apache.Arrow` | JSON serialization | `.cs` files |
| **Node.js** | `grpc-js` + `apache-arrow` | JSON + Arrow IPC | `.js` files |
| **C++** | `grpc-c++` + `apache-arrow-cpp` | Manual struct marshaling | `.h`/`.cpp` |

**Implementation**:

1. **Type Mapping** (`crates/fraiseql-codegen/src/type_mapping.rs`, ~300 lines)
   ```rust
   // Maps Arrow types to language-specific types
   enum ArrowType {
       String → "string" (Go/Java), "str" (Rust), "string" (C#), "string" (JS), "std::string" (C++)
       Int64 → "int64" (Go), "long" (Java), "long" (C#), "BigInt" (JS), "int64_t" (C++)
       Timestamp → "time.Time", "Instant", "DateTime", "Date", "std::chrono::time_point"
       ...
   }
   ```

2. **Template Engine** (`crates/fraiseql-codegen/src/generator.rs`, ~400 lines)
   - Tera templates in `crates/fraiseql-codegen/templates/`
   - One template set per language:
     - `go/entity.tera` → struct definition
     - `go/serializer.tera` → MarshalBinary/UnmarshalBinary
     - `go/client.tera` → Flight client code
     - (Same for Java, C#, Node.js, C++)

3. **CLI Command** (`crates/fraiseql-cli/src/generate.rs`)
   ```bash
   fraiseql generate arrow-flight \
     --schema EntityEvent.arrow-schema \
     --language go \
     --output generated/go/
   ```

**Effort**: 5 days

**Outputs**:
- `templates/go/entity.tera`
- `templates/java/entity.tera`
- `templates/csharp/entity.tera`
- `templates/nodejs/entity.tera`
- `templates/cpp/entity.tera`
- Generator logic in `crates/fraiseql-codegen/`

---

### Day 7-8: Protocol Specification & Examples

**Goal**: Document Arrow Flight protocol, provide example servers

**Deliverables**:

1. **Arrow Flight Protocol Specification** (`docs/arrow-flight/protocol-spec.md`)
   - Message framing (gRPC + Arrow IPC)
   - Ticket format for querying
   - Streaming batch format
   - Backpressure & flow control
   - Error handling & status codes
   - Example wire traces

2. **Example Server Implementations**

   a. **Go Server** (`examples/go/server/main.go`, ~200 lines)
   ```go
   // Generated from EntityEvent.arrow-schema
   type EntityEvent struct {
       EventID   string    `arrow:"event_id"`
       EventType string    `arrow:"event_type"`
       Timestamp time.Time `arrow:"timestamp"`
       Data      string    `arrow:"data"`
   }

   // Flight GetFlightInfo handler
   func (s *Server) GetFlightInfo(ctx context.Context, descriptor *flight.FlightDescriptor) (*flight.FlightInfo, error) {
       // Query preparation
   }

   // Flight DoGet handler - stream data
   func (s *Server) DoGet(request *flight.Ticket, stream flight.FlightService_DoGetServer) error {
       // Stream Arrow batches
   }
   ```

   b. **Java Server** (`examples/java/ArrowFlightServer.java`, ~250 lines)
   - Same flight handlers, Java Arrow API

   c. **Node.js Server** (`examples/nodejs/server.js`, ~200 lines)
   - Express.js wrapper around gRPC server
   - For ease of JavaScript integration

3. **Interop Testing Guide** (`docs/arrow-flight/interop-testing.md`)
   - Cross-language client-server testing
   - Docker Compose setup (Rust, Go, Java servers)
   - Test matrix: all clients × all servers

**Effort**: 2 days

---

### Day 9-10: Integration & Testing

**Goal**: Verify all generators work, examples compile, interop tests pass

**Deliverables**:

1. **Generated Code Verification**
   - Run generator for all 5 languages
   - Verify compilation (Go, Java, C#, Node.js, C++)
   - Check for syntax errors, missing imports

2. **Cross-Language Interop Tests** (`tests/arrow-flight-interop/`)
   - Matrix: 5 servers × 5 clients = 25 combinations
   - Template test:
     ```
     1. Start server (Go/Java/Node.js/Rust/C++)
     2. Run client (same language or different)
     3. Send query with EntityEvent schema
     4. Verify batch deserialization matches expected values
     5. Check performance (latency, throughput)
     ```
   - Docker Compose for easy orchestration

3. **Documentation Finalization**
   - README per language with installation instructions
   - Migration guide: moving from HTTP/JSON to Arrow Flight
   - Benchmarks: language comparison (throughput, memory, latency)

**Effort**: 2 days

---

## Detailed Specification

### File Structure

```
crates/fraiseql-codegen/                    # NEW CRATE
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── parser.rs                          # Parse .arrow-schema
│   ├── validator.rs                        # Validate schemas
│   ├── type_mapping.rs                     # Arrow → language types
│   ├── generator.rs                        # Template rendering
│   └── cli.rs                              # CLI integration
├── templates/
│   ├── go/
│   │   ├── entity.tera
│   │   ├── serializer.tera
│   │   └── client.tera
│   ├── java/
│   ├── csharp/
│   ├── nodejs/
│   └── cpp/
└── tests/
    ├── parser_tests.rs
    ├── generator_tests.rs
    └── integration_tests.rs

crates/fraiseql-arrow/schemas/              # Schema definitions
├── EntityEvent.arrow-schema
├── User.arrow-schema
└── Order.arrow-schema

examples/
├── go/
│   ├── server/
│   └── client/
├── java/
├── csharp/
├── nodejs/
└── cpp/

docs/arrow-flight/
├── schema-spec.md
├── protocol-spec.md
├── interop-testing.md
└── language-guides/
    ├── go.md
    ├── java.md
    ├── csharp.md
    ├── nodejs.md
    └── cpp.md

tests/arrow-flight-interop/
├── docker-compose.yml
├── Makefile
└── test_cases/
    ├── basic_query.sh
    ├── stress_test.sh
    └── cross_lang_matrix.sh
```

---

## Success Criteria

- ✅ `.arrow-schema` format specified and documented
- ✅ Schema parser compiles and passes all tests
- ✅ Code generators produce valid code for 5 languages
- ✅ Generated Go, Java, C# code compiles without errors
- ✅ Generated Node.js code runs without errors
- ✅ Example servers run and accept Flight connections
- ✅ Cross-language interop tests (5×5 matrix) all pass
- ✅ Benchmarks show <5% overhead vs hand-written code
- ✅ Complete documentation with examples
- ✅ CLI command: `fraiseql generate arrow-flight --language <lang>`

---

## Benefits

| Benefit | Impact |
|---------|--------|
| **Language-agnostic** | Implement Arrow Flight in Java, Go, C#, Node.js, C++ |
| **Code generation** | Zero-boilerplate client/server code |
| **Standardized schema** | Single source of truth for all languages |
| **Interoperability** | Clients in any language talk to servers in any language |
| **Documentation** | Clear protocol spec enables custom implementations |
| **Scalability** | Teams can choose their preferred language |
| **Maintainability** | Schema changes generate new code automatically |

---

## Timeline & Milestones

| Week | Days | Milestone | Output |
|------|------|-----------|--------|
| **W1** | 1-3 | Schema IDL + Parser | `EntityEvent.arrow-schema` + parser crate |
| **W1** | 4-6 | Code Generators | 5 language templates + generator CLI |
| **W2** | 7-8 | Protocol Spec + Examples | Spec doc + Go/Java/Node.js servers |
| **W2** | 9-10 | Integration & Testing | Interop tests + all docs complete |

**Total**: 2 weeks, 10 implementation days

---

## Risks & Mitigations

| Risk | Probability | Mitigation |
|------|-------------|-----------|
| Template bugs for 5 languages | Medium | Test each template with generated code compilation |
| Generated code performance | Low | Benchmarks in integration tests |
| Schema version compatibility | Low | Version field + migration guide |
| Interop testing complexity | Medium | Docker Compose + Makefile automation |

---

## Dependencies

- ✅ Phase 9.1-9.8 complete (Arrow Flight working)
- ✅ Phase 9.9 testing done (confidence in Arrow Flight)
- No external blocker - can start immediately after Phase 9.9

---

## Questions for Decision

1. **Priority**: Start immediately after Phase 9.9 testing, or after Phase 10 security?
   - **Recommendation**: Immediately after Phase 9.9 (unblocks multi-language teams)

2. **Scope**: Support all 5 languages or start with 2-3?
   - **Recommendation**: All 5 (or 3 primary: Go, Java, Node.js)

3. **Examples**: Minimal (100 lines) or comprehensive (1000 lines)?
   - **Recommendation**: Comprehensive with error handling, retries, auth

---

## References

- Arrow Flight spec: https://arrow.apache.org/docs/format/FlightRpc.html
- Avro schema format: https://avro.apache.org/docs/current/spec.html
- Protobuf code generation: https://developers.google.com/protocol-buffers/docs/reference/go-generated
- Tera template engine: https://tera.netlify.app/

---

**Next Step**: Review this plan, then decide if Phase 9.10 starts after Phase 9.9 testing or Phase 10 completion.
