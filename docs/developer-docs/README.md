# FraiseQL Developer Documentation

**Audience**: FraiseQL contributors and maintainers

This section contains **internal development documentation** for FraiseQL. If you're a **user** looking to use FraiseQL in your application, see the [main documentation](../README.md) instead.

---

## What's in This Section

### Development Phases (`phases/`)

Historical documentation of FraiseQL's development phases. Each phase represents a major implementation milestone:

- **Phase 7**: WHERE clause and ORDER BY improvements
- **Phase 10**: Rust-based JWT authentication
- **Phase 11**: Rust-based RBAC (Role-Based Access Control)
- **Phase 12**: Security constraints (rate limiting, IP filtering, query complexity)
- **Phase 14**: Audit logging with PostgreSQL backend

**Note**: Phase docs contain:
- Implementation details and architecture
- Performance benchmarks and optimization strategies
- Database schemas and migration strategies
- Rust code examples and PyO3 bindings
- Task lists and development checklists

### Architecture (`../architecture/`)

System design and architectural decisions (located in main docs because referenced by user guides):

- [Architecture Overview](../architecture/README.md)
- [Mutation Pipeline](../architecture/mutation-pipeline.md)
- [CQRS Design](../architecture/cqrs-design.md)
- [Architectural Decisions](../architecture/decisions/README.md)

---

## For FraiseQL Users

If you're looking for **how to use** these features (not how they're built), see:

- **Authentication**: [User Guide](../guides/authentication.md) *(to be created)*
- **RBAC**: [User Guide](../guides/rbac.md) *(to be created)*
- **Security**: [User Guide](../guides/security.md) *(to be created)*
- **Audit Logging**: [User Guide](../guides/audit-logging.md) *(to be created)*

---

## For FraiseQL Contributors

### Understanding the Codebase

1. **Start with phases** to understand implementation history
2. **Read architecture docs** for system design
3. **Check planning docs** in `../archive/planning/` for future work

### Development Workflow

See the main [CLAUDE.md](../../.claude/CLAUDE.md) for:
- Code standards
- Testing requirements
- Commit conventions
- Release process

### Key Implementation Details

#### Phase 10: Authentication
- Rust JWT validation (5-10x faster than Python)
- LRU caching for JWKS and user contexts
- SHA256 token hashing (security)
- Auth0 and custom JWT provider support

#### Phase 11: RBAC
- PostgreSQL-based permission storage
- Hierarchical role inheritance
- Tenant-scoped permissions
- Rust permission resolution

#### Phase 12: Security Constraints
- Token bucket rate limiting (governor crate)
- CIDR-based IP filtering (ipnetwork crate)
- Query complexity analysis (optional)

#### Phase 14: Audit Logging
- PostgreSQL audit log table with JSONB
- 5 optimized indexes for query patterns
- Multi-tenant isolation
- 10-100x faster than Python logging

---

## Documentation Philosophy

### Developer Docs (this section)
- **Why**: Explain design decisions and trade-offs
- **How it works**: Implementation details and internals
- **Performance**: Benchmarks and optimization strategies
- **Code**: Rust implementation examples

### User Docs (main docs)
- **What**: Feature description and capabilities
- **How to use**: Configuration and usage examples
- **When**: Use cases and patterns
- **Examples**: Practical code snippets

---

## Contributing to Documentation

### Adding Phase Documentation

When implementing a new phase:

1. Create `phases/phaseN_feature_name.md`
2. Include:
   - Objective and context
   - Architecture and design
   - Implementation details
   - Verification and testing
   - Performance benchmarks
3. Reference from this README
4. Later: Extract user guide from phase doc

### Updating Architecture Docs

Architecture docs are shared between developer and user docs because users need to understand high-level design. Keep them:
- High-level (not too detailed)
- Diagram-heavy (visual understanding)
- User-focused where possible

---

## Questions?

- **Using FraiseQL?** → See [main docs](../README.md)
- **Contributing?** → See [CONTRIBUTING.md](../../CONTRIBUTING.md)
- **Building FraiseQL?** → See [.claude/CLAUDE.md](../../.claude/CLAUDE.md)
