# ADR-0001: Three-Layer Architecture (Authoring → Compilation → Runtime)

## Status: Accepted

## Context

FraiseQL needs to support schema authoring in Python and TypeScript for developer ergonomics while executing as a high-performance Rust server in production. Supporting FFI bindings (PyO3, NAPI) at runtime adds complexity, maintenance burden, and dependency management issues. Requiring Rust-only authoring alienates Python/TypeScript developers.

## Decision

Separate FraiseQL into three distinct layers:

1. **Authoring** (Python/TypeScript): Schema definition via decorators/classes
2. **Compilation** (Rust CLI): `fraiseql compile schema.json` validates and generates optimized SQL templates
3. **Runtime** (Rust Server): Loads compiled schema, executes GraphQL queries, zero Python/TS dependencies

No FFI calls at runtime. Schema authoring languages are strictly build-time tools.

## Consequences

**Positive:**

- Zero runtime overhead from Python/TS
- Clean separation of concerns
- Deployment is pure Rust binary
- Schema validation at compile time

**Negative:**

- Adds compilation step to deployment pipeline
- Schema changes require recompilation
- Developers must understand three-layer model

## Alternatives Considered

1. **Runtime FFI (PyO3/NAPI)**: Eliminates compilation step but adds 50-100ms startup overhead and complex dependency management
2. **Rust-only authoring**: Eliminates Python/TS but reduces developer accessibility
3. **Code generation in Python/TS**: Generates Rust code; more complex than JSON output and compilation
