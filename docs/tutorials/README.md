# FraiseQL Tutorials

**Status:** ✅ Production Ready
**Audience:** Developers, Architects
**Reading Time:** Varies by tutorial (30-90 minutes total for all)
**Last Updated:** 2026-02-05

Complete, hands-on tutorials for building real-world applications with FraiseQL. Each tutorial focuses on practical schema authoring patterns in different programming languages.

---

## Available Tutorials

### Full-Stack: Python + React Blog Application

**[📖 Read the full tutorial](fullstack-python-react.md)**

A comprehensive end-to-end guide building a complete full-stack blog application with:

- **Python schema authoring** - FraiseQL decorators for types, queries, mutations
- **Schema compilation** - From Python to optimized SQL with fraiseql-cli
- **FraiseQL server deployment** - Rust-based GraphQL backend in Docker
- **React frontend** - Apollo Client integration with full UI
- **PostgreSQL database** - Complete schema with views and functions
- **Docker orchestration** - Multi-container development environment
- **Complete workflow** - From authoring to deployed application

**Contents:**

- Python schema definition with decorators
- PostgreSQL database DDL (tables, views, functions)
- FraiseQL configuration and compilation
- Docker Compose for local development
- React components with Apollo Client
- GraphQL queries and mutations
- End-to-end testing and workflow
- Deployment to production
- Troubleshooting guide

**Time estimate:** 60-90 minutes (or read sections selectively)

**Prerequisites:**

- Python 3.10+
- Node.js 18+
- PostgreSQL 14+ (or Docker)
- FraiseQL CLI
- Docker & Docker Compose

**What you'll learn:**

- How to structure a full-stack application with FraiseQL
- Python to JSON schema authoring patterns
- Deployment patterns for FraiseQL server
- React integration with GraphQL APIs
- Multi-container orchestration
- Complete development workflow

**Perfect for:** Anyone wanting to see how all the pieces fit together, from schema authoring to running frontend

**Best way to use this tutorial:**

1. Read the Architecture Overview to understand the flow
2. Follow the step-by-step setup sections
3. Run the full stack locally with Docker Compose
4. Reference specific sections for patterns and troubleshooting

---

### TypeScript: Build a Blog API with Schema Authoring

**[📖 Read the full tutorial](typescript-blog-api.md)**

A comprehensive guide to building a complete Blog API using FraiseQL with TypeScript decorators. Learn:

- **TypeScript decorators for schema definition** - Using `@Type`, `@Query`, `@Mutation` decorators
- **Type registration** - Mapping TypeScript properties to GraphQL types with `registerTypeFields`
- **Query definition** - Building read operations with `registerQuery`
- **Mutation definition** - Building write operations with `registerMutation`
- **Schema export and compilation** - Generating and validating schemas
- **Testing strategies** - Unit and integration tests for GraphQL queries
- **Deployment patterns** - Docker, Docker Compose, health checks
- **Common patterns** - Pagination, filtering, sorting, computed fields
- **Troubleshooting** - Common errors and solutions

**Contents:**

- Database schema setup (PostgreSQL DDL)
- TypeScript project configuration
- Type definitions with decorators
- Query registration and configuration
- Mutation registration and configuration
- Schema export to JSON
- Compilation and validation
- GraphQL IDE testing
- Docker deployment
- 8+ common patterns
- Comprehensive troubleshooting guide
- Complete working example code

**Time estimate:** 40-50 minutes

**Prerequisites:**

- Node.js 18+
- TypeScript 5.0+
- PostgreSQL 14+
- FraiseQL CLI
- Basic GraphQL knowledge

**Code examples included:**

- ✅ Complete type definitions
- ✅ Query registration examples
- ✅ Mutation registration examples
- ✅ Export tool implementation
- ✅ Integration tests
- ✅ Docker Compose configuration
- ✅ PostgreSQL DDL
- ✅ Health check examples
- ✅ React client example

---

### Go: Build a Blog API with Schema Authoring

**[📖 Read the full tutorial](go-blog-api.md)**

A comprehensive guide to building a complete Blog API using FraiseQL with Go. Learn:

- **Struct tags for schema definition** - Map Go types to GraphQL schema declaratively
- **Builder pattern for queries** - Fluent API for query configuration
- **Builder pattern for mutations** - Creating type-safe mutations
- **Schema export and compilation** - Generating and validating schemas
- **Testing strategies** - Unit and integration tests for schema authoring
- **Deployment patterns** - Docker, Docker Compose, health checks
- **Common patterns** - Pagination, filtering, sorting, relationships
- **Troubleshooting** - Common errors and solutions

**Contents:**

- Database schema setup (PostgreSQL DDL)
- Type definitions with struct tags
- Query builder patterns
- Mutation builder patterns
- Schema export tooling
- Compilation and validation
- Testing (unit and integration)
- Docker deployment
- 7+ common patterns
- Comprehensive troubleshooting guide

**Time estimate:** 30-45 minutes

**Prerequisites:**

- Go 1.22+
- PostgreSQL 14+
- FraiseQL CLI
- Basic GraphQL knowledge

**Code examples included:**

- ✅ Complete type definitions
- ✅ Query builder examples
- ✅ Mutation builder examples
- ✅ Export tool implementation
- ✅ Unit tests
- ✅ Integration tests
- ✅ Docker Compose configuration
- ✅ PostgreSQL DDL
- ✅ Health check examples

---

## Tutorial Structure

Each tutorial follows this consistent format:

1. **Overview** - What you'll learn and time estimate
2. **Architecture** - High-level system design
3. **Project Setup** - Creating and organizing your project
4. **Database Schema** - SQL DDL for PostgreSQL
5. **Schema Definition** - Language-specific type definitions
6. **Query Builders** - Building GraphQL queries
7. **Mutation Builders** - Building GraphQL mutations
8. **Schema Export** - Generating schema.json
9. **Compilation** - Using fraiseql-cli
10. **Testing** - Unit and integration tests
11. **Deployment** - Docker, Docker Compose, health checks
12. **Common Patterns** - 7+ reusable patterns
13. **Next Steps** - Advanced topics
14. **Troubleshooting** - Common issues and solutions
15. **Code Reference** - Complete working code

---

## Quick Reference

### By Use Case

**I want to...**

- **See a complete full-stack application** → [Full-Stack: Python + React Blog](fullstack-python-react.md)
- **Run a working example locally** → [Full-Stack Deployment section](fullstack-python-react.md#part-9-running-the-full-stack)
- **Understand the complete architecture** → [Full-Stack Architecture Overview](fullstack-python-react.md#overview)
- **Deploy to production** → [Full-Stack Production Deployment](fullstack-python-react.md#part-11-deployment-to-production)
- **Learn TypeScript schema authoring** → [TypeScript: Build a Blog API](typescript-blog-api.md)
- **Understand TypeScript decorators** → [TypeScript: Build a Blog API - Part 3](typescript-blog-api.md#part-3-fraiseql-schema-definition)
- **Learn @Type decorator** → [TypeScript: Build a Blog API - Understanding Decorators](typescript-blog-api.md#32-understanding-the-decorators)
- **Learn Query registration** → [TypeScript: Build a Blog API - Queries](typescript-blog-api.md#queries-read-operations)
- **Learn Mutation registration** → [TypeScript: Build a Blog API - Mutations](typescript-blog-api.md#mutations-write-operations)
- **Deploy with Docker** → [TypeScript: Build a Blog API - Deployment](typescript-blog-api.md#part-10-deployment)
- **Test GraphQL queries** → [TypeScript: Build a Blog API - Testing](typescript-blog-api.md#part-8-testing-your-schema)
- **Understand common patterns** → [TypeScript: Build a Blog API - Patterns](typescript-blog-api.md#part-9-common-patterns)
- **Troubleshoot errors** → [TypeScript: Build a Blog API - Troubleshooting](typescript-blog-api.md#part-11-troubleshooting)
- **Learn Go schema authoring** → [Go: Build a Blog API](go-blog-api.md)
- **Understand struct tags** → [Go: Build a Blog API - Step 3](go-blog-api.md#step-3-fraiseql-schema-definition)
- **Learn Go builder pattern** → [Go: Build a Blog API - Queries](go-blog-api.md#32-query-definitions) and [Mutations](go-blog-api.md#33-mutation-definitions)

### By Topic

**Schema Authoring:**

- [Type Definitions with Struct Tags](go-blog-api.md#31-type-definitions-with-struct-tags)
- [Understanding Struct Tags](go-blog-api.md#understanding-struct-tags)
- [Type Mapping](go-blog-api.md#type-mapping-examples)

**Query Building:**

- [Query Definitions](go-blog-api.md#32-query-definitions)
- [Understanding the Query Builder](go-blog-api.md#understanding-the-query-builder)
- [Query Pattern Examples](go-blog-api.md#common-patterns)

**Mutation Building:**

- [Mutation Definitions](go-blog-api.md#33-mutation-definitions)
- [Understanding Mutations](go-blog-api.md#understanding-mutations)
- [Mutation Patterns](go-blog-api.md#pattern-6-optional-mutation-arguments)

**Deployment:**

- [Docker Deployment](go-blog-api.md#docker-deployment)
- [Docker Compose](go-blog-api.md#docker-compose)
- [Health Checks](go-blog-api.md#health-checks)

**Testing:**

- [Unit Tests](go-blog-api.md#unit-tests-for-type-definitions)
- [Integration Tests](go-blog-api.md#integration-tests-for-schema-export)
- [Testing Patterns](go-blog-api.md#testing-your-schema)

---

## Learning Path

### Quick Start (90 minutes) - Full-Stack Recommended

Want to see everything work together end-to-end? Start here:

1. Read [Full-Stack Architecture Overview](fullstack-python-react.md#overview)
2. Follow [Project Setup](fullstack-python-react.md#part-1-project-setup)
3. Follow [Database Setup](fullstack-python-react.md#part-2-database-schema-postgresql)
4. Create [Python Schema](fullstack-python-react.md#part-3-python-schema-definition)
5. [Compile with FraiseQL CLI](fullstack-python-react.md#part-4-compile-with-fraiseql-cli)
6. [Deploy with Docker Compose](fullstack-python-react.md#part-5-fraiseql-server-deployment)
7. [Build React Frontend](fullstack-python-react.md#part-6-react-frontend-setup)
8. [Launch Full Stack](fullstack-python-react.md#part-9-running-the-full-stack)

**Outcome:** Full working blog application running locally

---

### Beginner (30 minutes) - TypeScript Recommended

1. Read [Overview](typescript-blog-api.md#overview)
2. Follow [Project Setup](typescript-blog-api.md#part-2-typescript-project-setup)
3. Review [Database Schema](typescript-blog-api.md#part-1-database-schema)
4. Complete [Type Definitions](typescript-blog-api.md#part-3-fraiseql-schema-definition)
5. Run [Export Tool](typescript-blog-api.md#part-4-exporting-the-schema)

### Intermediate (50 minutes) - TypeScript Recommended

1. Follow entire Beginner path
2. Build [Query Definitions](typescript-blog-api.md#queries-read-operations)
3. Build [Mutation Definitions](typescript-blog-api.md#mutations-write-operations)
4. Review [Common Patterns](typescript-blog-api.md#part-9-common-patterns)
5. Study [Testing](typescript-blog-api.md#part-8-testing-your-schema)

### Advanced (60+ minutes) - TypeScript Recommended

1. Follow entire Intermediate path
2. Set up [Docker Deployment](typescript-blog-api.md#101-docker-deployment)
3. Implement [Health Checks](typescript-blog-api.md#102-health-checks)
4. Study [Troubleshooting](typescript-blog-api.md#part-11-troubleshooting)
5. Explore [Next Steps](typescript-blog-api.md#part-13-next-steps)

---

### Alternative: Learning with Go

1. Beginner - Follow [Go Tutorial Overview](go-blog-api.md#overview)
2. Intermediate - Complete [Go Query/Mutation Builders](go-blog-api.md)
3. Advanced - Deploy with [Go Docker Setup](go-blog-api.md#docker-deployment)

---

## Code Examples

All tutorials include working code examples you can copy and run:

- **TypeScript** - Full working blog API with decorators and tests
- **Go** - Full working blog API example with builder patterns
- **PostgreSQL** - Complete DDL for database setup (used by all tutorials)
- **Docker** - Ready-to-use Dockerfile and docker-compose.yml
- **Tests** - Unit and integration test examples
- **Node.js/TypeScript** - npm scripts and tsconfig.json
- **Go** - Makefiles and project structure

---

## Getting Started

Choose your language and start building:

```bash
# TypeScript (Recommended for beginners)
mkdir blog-api && cd blog-api
npm init -y
npm install --save-dev typescript ts-node @types/node
npm install fraiseql

# Follow: docs/tutorials/typescript-blog-api.md
# Then: npm run export && npm run compile && npm run dev
```

```bash
# Go - Complete blog API tutorial
cd fraiseql-blog-api  # Create new directory
go mod init fraiseql-blog-api
go get github.com/fraiseql/fraiseql-go

# Follow: docs/tutorials/go-blog-api.md
```

---

## Prerequisites for All Tutorials

**Required:**

- Your chosen language runtime (Go 1.22+, Python 3.10+, TypeScript 5+, etc.)
- PostgreSQL 14+ database
- FraiseQL CLI tool
- Code editor with syntax highlighting
- Bash or equivalent shell

**Optional but Recommended:**

- Docker and Docker Compose
- IDE extensions for your language
- GraphQL client (Postman, Insomnia, etc.)
- Version control (Git)

---

## Common Questions

### How long does each tutorial take?

- **TypeScript Blog API**: 40-50 minutes for basics, 60+ minutes with deployment
- **Go Blog API**: 30-45 minutes for basics, 60+ minutes with deployment
- All tutorials are designed for 1-2 hour sessions

### Can I use these tutorials as templates?

Yes! Each tutorial provides complete, production-ready code you can:

- Copy and modify for your own projects
- Use as reference implementations
- Build upon with your own extensions

### Are the tutorials kept up to date?

Yes, all tutorials are updated with each FraiseQL release and tested regularly.

### Can I follow multiple tutorials?

Absolutely! Each tutorial is independent and uses the same patterns, so learning one language helps with others.

---

## Feedback & Contributions

Have questions about these tutorials?

- Check the [Troubleshooting section](go-blog-api.md#troubleshooting)
- Review the [FAQ](../FAQ.md)
- Open an issue on [GitHub](https://github.com/fraiseql/fraiseql)

Want to contribute a tutorial?

- Submit a PR with a new tutorial following this format
- Include working code examples
- Add comprehensive troubleshooting section
- Update this README with links to your tutorial

---

## Related Documentation

- **[Full-Stack Architecture](fullstack-python-react.md)** - Python → FraiseQL → React end-to-end
- **[Language Generators Guide](../guides/language-generators.md)** - Overview of all schema authoring languages
- **[Schema Design Best Practices](../guides/schema-design-best-practices.md)** - Design patterns and principles
- **[Frontend Integration Guide](../guides/frontend-integration.md)** - React, Apollo Client, and other frontends
- **[Testing Strategy](../guides/testing-strategy.md)** - Comprehensive testing approaches
- **[Deployment Guide](../deployment/)** - Production deployment strategies
- **[CLI Reference](../reference/)** - fraiseql-cli command reference
- **[TOML Configuration Reference](../TOML_REFERENCE.md)** - Complete fraiseql.toml options

---

**Back to:** [Documentation Home](../README.md)
