# FraiseQL HTTP Servers: Choose Your Framework

**Version**: 2.0.0+
**Last Updated**: 2026-01-05
**Reading Time**: 10 minutes

---

## Welcome to FraiseQL v2.0.0

In v2.0.0, FraiseQL introduces a **pluggable HTTP server architecture**. This means you can choose the HTTP framework that best fits your needs, all while benefiting from the same high-performance Rust-based GraphQL pipeline.

This guide helps you understand your options and choose the right server for your project.

---

## Three HTTP Server Options

FraiseQL now supports three production-ready HTTP servers:

### 1. 🚀 **Axum** (Rust) - Maximum Performance

**Best for**: Performance-critical applications, microservices, large-scale deployments

```
┌─────────────────────┐
│  Your Python Code   │
│  (Types, Resolvers) │
└──────────┬──────────┘
           │
           ↓
┌──────────────────────────────┐
│  High-Performance Rust HTTP  │
│  Server (Axum Native)        │
│  • 7-10x faster than Python  │
│  • HTTP/2 native             │
│  • WebSocket subscriptions   │
│  • Advanced observability    │
└──────────┬───────────────────┘
           │
           ↓
┌──────────────────────────────┐
│  Rust GraphQL Pipeline       │
│  (Exclusive to FraiseQL)     │
└──────────────────────────────┘
```

**Quick Comparison**:
- **Performance**: ⭐⭐⭐⭐⭐ (Fastest)
- **Setup Time**: 30-60 minutes (requires Rust)
- **Customization**: Rust code
- **Recommended for**: New projects, performance critical apps

**Getting Started**: [Axum Guide →](./axum/01-getting-started.md)

---

### 2. 🐍 **Starlette** (Python) - Lightweight Alternative

**Best for**: New Python projects, teams that prefer Python, simple APIs

```
┌──────────────────────────────┐
│  Your Python Code            │
│  (Types, Resolvers, Routes)  │
└──────────┬───────────────────┘
           │
           ↓
┌──────────────────────────────┐
│  Lightweight Python HTTP     │
│  Server (Starlette ASGI)     │
│  • Same features as Axum     │
│  • WebSocket subscriptions   │
│  • Custom middleware         │
│  • Easy to extend            │
└──────────┬───────────────────┘
           │
           ↓
┌──────────────────────────────┐
│  Rust GraphQL Pipeline       │
│  (Exclusive to FraiseQL)     │
└──────────────────────────────┘
```

**Quick Comparison**:
- **Performance**: ⭐⭐⭐⭐ (Very good)
- **Setup Time**: 5-10 minutes (Python only)
- **Customization**: Python code
- **Recommended for**: New projects, teams that prefer Python

**Getting Started**: [Starlette Guide →](./starlette/01-getting-started.md)

---

### 3. 🔄 **FastAPI** (Python) - Legacy Support

**Best for**: Existing FastAPI applications, teams wanting to migrate gradually

```
┌──────────────────────────────┐
│  Your Existing FastAPI Code  │
│  (Types, Decorators, Routes) │
└──────────┬───────────────────┘
           │
           ↓
┌──────────────────────────────┐
│  FastAPI HTTP Server         │
│  • Works with existing code  │
│  • Same GraphQL features     │
│  • Migration path available  │
│  • Being phased out (v3.0)   │
└──────────┬───────────────────┘
           │
           ↓
┌──────────────────────────────┐
│  Rust GraphQL Pipeline       │
│  (Exclusive to FraiseQL)     │
└──────────────────────────────┘
```

**Quick Comparison**:
- **Performance**: ⭐⭐⭐ (Good, but slower than others)
- **Setup Time**: 0 minutes (already running)
- **Customization**: Existing FastAPI patterns
- **Status**: Deprecated (still fully functional)

**Why Still Supported?**: Backward compatibility. We don't break existing projects.

**Migration Path**: [FastAPI → Starlette →](./migration/fastapi-to-starlette.md)

---

## Decision Matrix: Which Server Should You Use?

### I'm Starting a New Project

**Start here:**

```
┌─────────────────────────────────┐
│   Performance is critical?      │
├─────────────────────────────────┤
│  YES (microservices, API-heavy) │ → Use AXUM
│  NO (small API, simple queries) │ → Use STARLETTE
└─────────────────────────────────┘
```

**Why?**
- **Axum** if you expect high traffic, complex queries, or real-time features
- **Starlette** if you want rapid development in Python, simplicity, or learning

### I Have Existing FastAPI Code

**You have options:**

```
┌──────────────────────────────────┐
│  Do you want to migrate?         │
├──────────────────────────────────┤
│  NO (works fine, leave as-is)    │ → Stay with FASTAPI
│  YES (want new features)         │ → Migrate to STARLETTE
│  YES (need high performance)     │ → Migrate to AXUM
└──────────────────────────────────┘
```

**Key Point**: Migrating is optional and has zero breaking changes for your schema.

### I Want Maximum Performance

```
Use AXUM (Rust)

Features:
✓ HTTP/2 native support
✓ Advanced multiplexing
✓ WebSocket subscriptions
✓ Operation monitoring
✓ 7-10x faster than FastAPI
✓ Perfect for microservices
```

### I Prefer Python Everything

```
Use STARLETTE (Python)

Features:
✓ Pure Python codebase
✓ Easy to customize
✓ WebSocket subscriptions (like Axum!)
✓ Same GraphQL performance as Axum
✓ Lightweight ASGI framework
✓ Simple to understand and modify
```

---

## Feature Comparison Matrix

| Feature | Axum | Starlette | FastAPI |
|---------|------|-----------|---------|
| **GraphQL Queries** | ✅ | ✅ | ✅ |
| **GraphQL Mutations** | ✅ | ✅ | ✅ |
| **WebSocket Subscriptions** | ✅ | ✅ | ❌ |
| **Automatic Persisted Queries (APQ)** | ✅ | ✅ | ✅ |
| **Query Result Caching** | ✅ | ✅ | ✅ |
| **CORS Configuration** | ✅ | ✅ | ✅ |
| **Authentication Middleware** | ✅ | ✅ | ✅ |
| **Request Logging** | ✅ | ✅ | ✅ |
| **Rate Limiting** | ✅ | ✅ | ❌ |
| **Operation Monitoring** | ✅ | ✅ | ❌ |
| **HTTP/2 Support** | ✅ | ✅ | ✅ |
| **Batch Request Processing** | ✅ | ✅ | ❌ |
| **Setup Time** | 30-60 min | 5-10 min | 0 min |
| **Language** | Rust | Python | Python |
| **Performance** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐ |
| **Learning Curve** | Moderate | Easy | Easy |
| **Customization** | Rust knowledge | Python only | Python only |
| **Production Ready** | ✅ Yes | ✅ Yes | ✅ Yes |
| **Status** | ✅ Recommended | ✅ Recommended | ⚠️ Deprecated |

---

## Performance Comparison

All servers use the same **Rust GraphQL pipeline**, so the performance difference is in the HTTP layer.

**Throughput (queries/second)**:

```
Axum:     5,000-10,000 qps  ⭐⭐⭐⭐⭐
Starlette: 2,000-4,000 qps  ⭐⭐⭐⭐
FastAPI:   1,500-2,500 qps  ⭐⭐⭐
```

**Latency (simple query)**:

```
Axum:      0.5-1ms   ⭐⭐⭐⭐⭐
Starlette: 2-3ms     ⭐⭐⭐⭐
FastAPI:   4-5ms     ⭐⭐⭐
```

**Real-world Impact**:
- **Under 100 QPS**: All three perform identically (no noticeable difference)
- **100-1,000 QPS**: Starlette and Axum perform well, FastAPI may need tuning
- **1,000+ QPS**: Axum recommended for best performance
- **10,000+ QPS**: Axum strongly recommended

---

## Getting Started: The Three Paths

### Path 1: Choose Axum (Recommended for new projects)

```
1. Install Rust
2. Set up development environment (5 min)
3. Follow Axum Getting Started (30 min)
4. Deploy to production (1 hour)
```

**Next**: [Axum Getting Started Guide →](./axum/01-getting-started.md)

### Path 2: Choose Starlette (Recommended for Python teams)

```
1. Install Python (you have it!)
2. Follow Starlette Getting Started (5 min)
3. Deploy to production (30 min)
```

**Next**: [Starlette Getting Started Guide →](./starlette/01-getting-started.md)

### Path 3: Migrate from FastAPI

```
1. Review migration guide (10 min)
2. Update imports and config (15 min)
3. Test thoroughly (varies)
4. Deploy (30 min)
```

**Next**: [FastAPI → Starlette Migration Guide →](./migration/fastapi-to-starlette.md)

---

## Common Questions

### Q: Do I need Rust knowledge to use Axum?

**A**: No! You write your GraphQL types and resolvers in Python. FraiseQL handles the Rust HTTP layer for you. However, customizing the HTTP server behavior requires Rust knowledge.

### Q: Can I switch servers later?

**A**: Yes! Your GraphQL schema, types, and resolvers are identical across all servers. Switching is a low-risk operation.

### Q: What about performance differences in practice?

**A**: For queries under 100 QPS, the difference is negligible. The HTTP layer is the difference, not the GraphQL pipeline. Choose based on developer experience, not raw performance (unless you're at scale).

### Q: Is FastAPI still supported?

**A**: Yes, fully. It works exactly as before. However, it's being phased out in favor of Starlette (which is simpler) and Axum (which is faster).

### Q: What's the migration path from FastAPI?

**A**: Simple:
- **To Starlette**: 30-60 minutes of code changes
- **To Axum**: 1-2 hours (if learning Rust) or leverage existing Rust team

**Zero schema changes required.** Your types and resolvers work identically.

### Q: Can I run multiple servers with the same schema?

**A**: Yes! The same schema can power Axum, Starlette, and FastAPI simultaneously. This is useful for gradual migration.

### Q: What about WebSocket subscriptions?

**A**: Fully supported in Axum and Starlette. Not in FastAPI (limitation of the framework).

### Q: How do I choose between Axum and Starlette?

**Simple rule of thumb:**
- **New project, performance matters**: Axum
- **New project, developer velocity matters**: Starlette
- **Existing FastAPI code**: Stay put, or migrate to Starlette when ready
- **Microservices, high traffic**: Axum
- **Internal tools, simple APIs**: Starlette

---

## Full Documentation Structure

This documentation is organized as follows:

```
docs/http-servers/
├─ README.md (you are here)
│  └─ Overview and decision guide
│
├─ COMPARISON.md
│  └─ Detailed feature comparison, performance analysis
│
├─ axum/
│  ├─ 01-getting-started.md
│  ├─ 02-configuration.md
│  ├─ 03-deployment.md
│  ├─ 04-performance.md
│  ├─ 05-troubleshooting.md
│  └─ examples/
│     └─ [hello-world, auth, docker, k8s, etc.]
│
├─ starlette/
│  ├─ 01-getting-started.md
│  ├─ 02-configuration.md
│  ├─ 03-deployment.md
│  ├─ 04-performance.md
│  ├─ 05-troubleshooting.md
│  └─ examples/
│     └─ [hello-world, auth, docker, k8s, etc.]
│
└─ migration/
   ├─ fastapi-to-starlette.md
   ├─ fastapi-to-axum.md
   └─ FASTAPI-DEPRECATION.md
```

---

## Next Steps

Choose your path:

**👉 I'm starting a new project:**
- [Read the Decision Matrix](#decision-matrix-which-server-should-you-use) (above)
- [Axum Getting Started →](./axum/01-getting-started.md) (recommended)
- [Starlette Getting Started →](./starlette/01-getting-started.md) (if Python-first)

**👉 I have existing FastAPI code:**
- [FastAPI → Starlette Migration →](./migration/fastapi-to-starlette.md)
- [Stay with FastAPI](../getting-started/quickstart.md) (totally fine!)

**👉 I want to understand all options:**
- [Detailed Comparison Guide →](./COMPARISON.md)
- [Axum vs Starlette Deep Dive →](./AXUM-VS-STARLETTE.md)

**👉 I want to see examples:**
- [Axum Examples →](./axum/examples/)
- [Starlette Examples →](./starlette/examples/)

---

## Key Takeaways

1. **You have choices**: Axum (fast), Starlette (simple), FastAPI (legacy)
2. **They all work**: Same GraphQL engine, different HTTP frameworks
3. **Zero lock-in**: Switch servers without changing your schema
4. **Choose wisely**: Performance vs. developer experience trade-off
5. **Migration is possible**: Move from FastAPI to Starlette/Axum anytime
6. **All production-ready**: No alpha or beta, all fully tested

---

## Support & Help

Having trouble choosing? Questions?

- **Decision help**: Read [COMPARISON.md](./COMPARISON.md)
- **Setup issues**: See [Troubleshooting Guides](./axum/05-troubleshooting.md) or [Starlette Troubleshooting](./starlette/05-troubleshooting.md)
- **Architecture questions**: Check [Decision Matrices](#decision-matrix-which-server-should-you-use) above

---

**Ready to build?** Pick a server and get started! 🚀

- [Axum →](./axum/01-getting-started.md)
- [Starlette →](./starlette/01-getting-started.md)
- [FastAPI (existing) →](../getting-started/quickstart.md)
