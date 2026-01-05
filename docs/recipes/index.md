---
title: Recipes & Examples
description: Cookbook of copy-paste solutions for common FraiseQL use cases
tags:
  - recipes
  - examples
  - cookbook
  - how-to
  - patterns
---

# Recipes & Examples

**Quick solutions for common FraiseQL tasks.** Each recipe is a complete, tested example you can copy and adapt for your needs.

---

## 🚀 Quick Start

New to FraiseQL? Start here:

- **[5-Minute Quickstart](../getting-started/quickstart.md)** - Build a notes API in 5 minutes
- **[First Hour Guide](../getting-started/first-hour.md)** - Progressive tutorial
- **[Blog API Tutorial](../tutorials/blog-api.md)** - Production-ready blog with auth

---

## 🔍 Filtering & Querying

### Basic Filtering
- **[Filtering Guide](../guides/filtering.md)** - Complete filtering reference with runnable example
- **[Where Input Types](../advanced/where-input-types.md)** - Type-safe filtering
- **[Filter Operators](../advanced/filter-operators.md)** - All available operators

### Advanced Filtering
- **[Advanced Filtering Examples](../examples/advanced-filtering.md)** - Real-world recipes:
  - E-commerce product search (full-text, price ranges, tags)
  - CMS content filtering (status, dates, categories)
  - User management (roles, permissions, activity)
  - Log analysis (time ranges, severity, JSON queries)
  - Multi-tenant SaaS (tenant isolation, cross-tenant queries)

### Nested & Array Filtering
- **[Nested Array Filtering](../guides/nested-array-filtering.md)** - Filter array elements
- **[Nested Filters Guide](../filtering/nested-filters.md)** - Complex nested objects

---

## ✏️ Mutations & Data Changes

- **[Mutation SQL Requirements](../guides/mutation-sql-requirements.md)** - Complete runnable example
- **[Error Handling Patterns](../guides/error-handling-patterns.md)** - Production error handling
- **[GraphQL Cascade](../guides/cascade-best-practices.md)** - Automatic cache invalidation

---

## 🔐 Authentication & Security

- **[Authentication Guide](../advanced/authentication.md)** - Complete JWT example with roles
- **[Multi-Tenancy](../advanced/multi-tenancy.md)** - Tenant isolation patterns

---

## 🤖 AI & Vector Search

- **[pgvector Integration](../features/pgvector.md)** - Similarity search with pgvector
- **[RAG Tutorial](../ai-ml/rag-tutorial.md)** - Build a RAG app with LangChain
- **[LangChain Integration](../guides/langchain-integration.md)** - Document ingestion & semantic search

---

## ⚡ Performance & Optimization

- **[Rust Pipeline Architecture](../core/rust-pipeline-integration.md)** - 5-7x performance boost explained
- **[Performance Guide](../performance/index.md)** - Optimization strategies
- **[APQ Optimization](../performance/apq-optimization-guide.md)** - Automatic persisted queries
- **[Caching Strategies](../performance/caching.md)** - Query result caching

---

## 🏗️ Architecture & Patterns

- **[Trinity Pattern Guide](../guides/trinity-pattern-guide.md)** - Three-tier identifier system
- **[Database Patterns](../advanced/database-patterns.md)** - PostgreSQL best practices
- **[CQRS Pattern](../core/concepts-glossary.md#cqrs-pattern)** - Separate reads from writes
- **[Bounded Contexts](../advanced/bounded-contexts.md)** - Domain separation

---

## 🔧 Troubleshooting

- **[Troubleshooting Guide](../guides/troubleshooting.md)** - Common issues and solutions
- **[Troubleshooting Decision Tree](../guides/troubleshooting-decision-tree.md)** - Diagnostic flowchart
- **[Common Mistakes](../guides/common-mistakes.md)** - Avoid these pitfalls
- **[Troubleshooting Mutations](../guides/troubleshooting-mutations.md)** - Mutation-specific issues

---

## 🚀 Production & Deployment

- **[Production Deployment](../tutorials/production-deployment.md)** - Complete production setup
- **[Health Checks](../production/health-checks.md)** - Monitoring endpoints
- **[Observability](../production/observability.md)** - Metrics and logging

---

## 📚 By Use Case

### Building a Blog/CMS
1. [Blog API Tutorial](../tutorials/blog-api.md) - Complete blog with posts, comments, tags
2. [Authentication](../advanced/authentication.md) - User auth and permissions
3. [Cascade Best Practices](../guides/cascade-best-practices.md) - Auto cache invalidation

### E-commerce Platform
1. [Advanced Filtering](../examples/advanced-filtering.md#e-commerce-product-catalog) - Product search
2. [Multi-Tenancy](../advanced/multi-tenancy.md) - Vendor isolation
3. [Performance Guide](../performance/index.md) - Handle high traffic

### SaaS Application
1. [Multi-Tenancy](../advanced/multi-tenancy.md) - Customer data isolation
2. [Authentication](../advanced/authentication.md) - JWT + roles
3. [Bounded Contexts](../advanced/bounded-contexts.md) - Domain separation

### AI/ML Application
1. [RAG Tutorial](../ai-ml/rag-tutorial.md) - Retrieval augmented generation
2. [pgvector](../features/pgvector.md) - Similarity search
3. [LangChain Integration](../guides/langchain-integration.md) - Document processing

---

## 🎯 Quick Lookups

**Need to:**
- **Filter data?** → [Filtering Guide](../guides/filtering.md)
- **Create/update data?** → [Mutation SQL Requirements](../guides/mutation-sql-requirements.md)
- **Add authentication?** → [Authentication Guide](../advanced/authentication.md)
- **Improve performance?** → [Performance Guide](../performance/index.md)
- **Deploy to production?** → [Production Deployment](../tutorials/production-deployment.md)
- **Debug an issue?** → [Troubleshooting Guide](../guides/troubleshooting.md)

---

## 💡 Contributing Recipes

Have a useful recipe to share? We'd love to include it!

1. Create a markdown file with your recipe
2. Include: problem statement, complete code, expected output
3. Submit a PR to the [FraiseQL repository](https://github.com/fraiseql/fraiseql)

**Recipe Template:**
```markdown
## Recipe: [Short Title]

**Problem**: What does this solve?
**Use Case**: When should you use this?

**Complete Example:**
\`\`\`python
# Runnable code here
\`\`\`

**Expected Output:**
\`\`\`
Result here
\`\`\`
```

---

**Can't find what you need?**
- Search the docs (top right)
- Check [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- [Open an issue](https://github.com/fraiseql/fraiseql/issues)
