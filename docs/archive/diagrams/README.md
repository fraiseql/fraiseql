# Architecture Diagrams

This directory contains visual diagrams explaining FraiseQL's architecture and data flow patterns. All diagrams are provided in both ASCII art (for terminal viewing) and Mermaid format (for web rendering).

## Diagram Index

### Core Architecture

| Diagram | Description | Key Concepts |
|---------|-------------|--------------|
| [**Request Flow**](request-flow.md) | Complete request lifecycle from client to database | GraphQL → FastAPI → PostgreSQL → Response |
| [**CQRS Pattern**](cqrs-pattern.md) | Read vs Write separation | Queries vs Mutations, v_* vs fn_* |
| [**Database Schema Conventions**](database-schema-conventions.md) | Naming patterns and object roles | tb_*, v_*, tv_*, fn_* conventions |

### Advanced Features

| Diagram | Description | Key Concepts |
|---------|-------------|--------------|
| [**Multi-Tenant Isolation**](multi-tenant-isolation.md) | Tenant data isolation mechanisms | RLS, Context passing, Security layers |
| [**APQ Cache Flow**](apq-cache-flow.md) | Automatic Persisted Queries caching | Query hashing, Cache storage, Performance |
| [**Rust Pipeline**](rust-pipeline.md) | High-performance data transformation | JSONB processing, Field projection, Memory optimization |

## Diagram Formats

### ASCII Art
All diagrams include ASCII art versions that render correctly in:
- Terminal/command line interfaces
- Plain text editors
- GitHub README files
- Documentation systems without Mermaid support

### Mermaid Diagrams
Interactive diagrams using Mermaid syntax for:
- Web documentation
- IDE preview
- Documentation generators
- Enhanced readability

## Usage Guidelines

### When to Use Each Diagram

**For New Users:**
1. Start with [Request Flow](request-flow.md) - understand the big picture
2. Read [CQRS Pattern](cqrs-pattern.md) - learn read vs write separation
3. Study [Database Schema Conventions](database-schema-conventions.md) - understand naming patterns

**For Developers:**
1. [Multi-Tenant Isolation](multi-tenant-isolation.md) - implementing multi-tenant apps
2. [APQ Cache Flow](apq-cache-flow.md) - optimizing query performance
3. [Rust Pipeline](rust-pipeline.md) - advanced performance tuning

**For Architects:**
- All diagrams provide comprehensive understanding of system design
- Use as reference for design decisions and troubleshooting

### Reading the Diagrams

**Flow Direction:**
- Left to right: Data flow through the system
- Top to bottom: Layered architecture
- Arrows: Transformation or processing steps

**Color Coding:**
- Blue: Client-side components
- Green: Database and storage
- Red: Processing and transformation
- Orange: Caching and optimization

## Contributing

### Adding New Diagrams
1. Create diagram file in this directory
2. Include both ASCII art and Mermaid versions
3. Add comprehensive explanations
4. Update this README with the new diagram
5. Test rendering in both terminal and web formats

### Diagram Standards
- **ASCII Art**: Use box-drawing characters, keep lines under 80 characters
- **Mermaid**: Use flowchart syntax, include styling for clarity
- **Explanations**: Provide context, examples, and code samples
- **Consistency**: Follow existing naming and formatting patterns

## Quick Reference

### Most Important Diagrams (Start Here)
1. **[Request Flow](request-flow.md)** - System overview
2. **[CQRS Pattern](cqrs-pattern.md)** - Core architectural pattern
3. **[Database Schema Conventions](database-schema-conventions.md)** - Naming system

### Performance & Scaling
1. **[APQ Cache Flow](apq-cache-flow.md)** - Query optimization
2. **[Rust Pipeline](rust-pipeline.md)** - High-performance processing
3. **[Multi-Tenant Isolation](multi-tenant-isolation.md)** - Scaling considerations

### Troubleshooting
- Check [Request Flow](request-flow.md) for general issues
- Review [CQRS Pattern](cqrs-pattern.md) for read/write problems
- Consult [Multi-Tenant Isolation](multi-tenant-isolation.md) for data access issues

## Related Documentation

- [Understanding FraiseQL](../guides/understanding-fraiseql.md) - Conceptual overview
- [Core Concepts](../core/concepts-glossary.md) - Terminology reference
- [Performance Guide](../performance/index.md) - Optimization strategies
- [Multi-Tenancy Guide](../advanced/multi-tenancy.md) - Tenant implementation

---

*These diagrams are automatically updated with architecture changes. Last updated: 2025-10-23*
