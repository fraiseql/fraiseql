# FraiseQL Feature Matrix

Complete overview of all FraiseQL capabilities.

## 🎯 Quick Feature Lookup

**Looking for a specific feature?** Use the tables below to find what you need.

---

## Core Features

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **GraphQL Types** | ✅ Stable | [Types Guide](../core/types-and-schema.md) | [blog_simple](../../examples/blog_simple/) |
| **Queries** | ✅ Stable | [Queries Guide](../core/queries-and-mutations.md) | [blog_api](../../examples/blog_api/) |
| **Mutations** | ✅ Stable | [Mutations Guide](../core/queries-and-mutations.md) | [mutations_demo](../../examples/mutations_demo/) |
| **Mutation Result Formats** | ✅ Stable | [Result Reference](mutation-result-reference.md) | [mutations_demo](../../examples/mutations_demo/) |
| **Input Types** | ✅ Stable | [Types Guide](../core/types-and-schema.md#input-types) | [blog_simple](../../examples/blog_simple/) |
| **Success/Failure Responses** | ✅ Stable | [Mutations Guide](../core/queries-and-mutations.md#success-failure-pattern) | [mutations_demo](../../examples/mutations_demo/) |
| **Nested Relations** | ✅ Stable | [Database API](../core/database-api.md#nested-relations) | [blog_api](../../examples/blog_api/) |
| **Pagination** | ✅ Stable | [Database API](../core/database-api.md) | [ecommerce](../../examples/ecommerce/) |
| **Filtering (Where Input)** | ✅ Stable | [Where Input Guide](../advanced/where-input-types.md) | [filtering](../../examples/filtering/) |

---

## Database Features

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **JSONB Views (v_*)** | ✅ Stable | [Core Concepts](../core/concepts-glossary.md#jsonb-views) | [blog_simple](../../examples/blog_simple/) |
| **Table Views (tv_*)** | ✅ Stable | [Explicit Sync](../core/explicit-sync.md) | [complete_cqrs_blog](../../examples/complete_cqrs_blog/) |
| **PostgreSQL Functions** | ✅ Stable | [Database API](../core/database-api.md#calling-functions) | [blog_api](../../examples/blog_api/) |
| **Connection Pooling** | ✅ Stable | [Database API](../core/database-api.md#connection-pool) | All examples |
| **Transaction Support** | ✅ Stable | [Database API](../core/database-api.md#transactions) | [enterprise_patterns](../../examples/enterprise_patterns/) |
| **Trinity Identifiers** | ✅ Stable | [Trinity Pattern](../database/trinity-identifiers.md) | [saas-starter](../../examples/saas-starter/) |
| **CQRS Pattern** | ✅ Stable | [Patterns Guide](../patterns/README.md#cqrs) | [blog_enterprise](../../examples/blog_enterprise/) |

---

## Advanced Query Features

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **Nested Array Filtering** | ✅ Stable | [Nested Arrays](../guides/nested-array-filtering.md) | [specialized_types](../../examples/specialized_types/) |
| **Logical Operators (AND/OR/NOT)** | ✅ Stable | [Where Input Types](../advanced/where-input-types.md#logical-operators) | [filtering](../../examples/filtering/) |
| **Network Types (IPv4/IPv6/CIDR)** | ✅ Stable | [Specialized Types](../advanced/where-input-types.md#network-types) | [specialized_types](../../examples/specialized_types/) |
| **Hierarchical Data (ltree)** | ✅ Stable | [Hierarchical Guide](../advanced/database-patterns.md#ltree) | [ltree-hierarchical-data](../../examples/ltree-hierarchical-data/) |
| **Date/Time Ranges** | ✅ Stable | [Range Types](../advanced/where-input-types.md#range-types) | [specialized_types](../../examples/specialized_types/) |
| **Full-Text Search** | ✅ Stable | [Search Guide](../advanced/database-patterns.md#full-text-search) | [ecommerce](../../examples/ecommerce/) |
| **Geospatial Queries (PostGIS)** | 🚧 Beta | Coming soon | - |

---

## Performance Features

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **Rust Pipeline Acceleration** | ✅ Stable | [Rust Pipeline](../performance/rust-pipeline-optimization.md) | All examples (automatic) |
| **Zero N+1 Queries** | ✅ Stable | [Performance Guide](../performance/index.md#n-plus-one-prevention) | [blog_api](../../examples/blog_api/) |
| **Automatic Persisted Queries (APQ)** | ✅ Stable | [APQ Guide](../performance/apq-optimization-guide.md) | [apq_multi_tenant](../../examples/apq_multi_tenant/) |
| **PostgreSQL Caching** | ✅ Stable | [Caching Guide](../performance/index.md#postgresql-caching) | [ecommerce](../../examples/ecommerce/) |
| **Query Batching** | ✅ Stable | [Database API](../core/database-api.md#batching) | [turborouter](../../examples/turborouter/) |
| **Connection Pooling** | ✅ Stable | [Database API](../core/database-api.md#connection-pool) | All examples |

---

## Security Features

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **Row-Level Security (RLS)** | ✅ Stable | [Security Guide](../production/security.md#rls) | [security](../../examples/security/) |
| **Field-Level Authorization** | ✅ Stable | [Authentication](../advanced/authentication.md#field-authorization) | [security](../../examples/security/) |
| **@authorized Decorator** | ✅ Stable | [Authentication](../advanced/authentication.md#authorized-decorator) | [security](../../examples/security/) |
| **JWT Authentication** | ✅ Stable | [Authentication](../advanced/authentication.md#jwt) | [native-auth-app](../../examples/native-auth-app/) |
| **OAuth2 Integration** | ✅ Stable | [Authentication](../advanced/authentication.md#oauth2) | [saas-starter](../../examples/saas-starter/) |
| **Audit Logging** | ✅ Stable | [Security Guide](../production/security.md#audit-logging) | [blog_enterprise](../../examples/blog_enterprise/) |
| **Cryptographic Audit Chain** | ✅ Stable | [Security Guide](../production/security.md#crypto-audit) | [enterprise_patterns](../../examples/enterprise_patterns/) |
| **SQL Injection Prevention** | ✅ Stable | [Security Guide](../production/security.md#sql-injection) | Built-in (automatic) |
| **CORS Configuration** | ✅ Stable | [Configuration](../core/configuration.md#cors) | All examples |
| **Rate Limiting** | ✅ Stable | [Security Guide](../production/security.md#rate-limiting) | [saas-starter](../../examples/saas-starter/) |

---

## Enterprise Features

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **Multi-Tenancy** | ✅ Stable | [Multi-Tenancy Guide](../advanced/multi-tenancy.md) | [saas-starter](../../examples/saas-starter/) |
| **Bounded Contexts** | ✅ Stable | [Bounded Contexts](../advanced/bounded-contexts.md) | [blog_enterprise](../../examples/blog_enterprise/) |
| **Event Sourcing** | ✅ Stable | [Event Sourcing](../advanced/event-sourcing.md) | [complete_cqrs_blog](../../examples/complete_cqrs_blog/) |
| **Domain Events** | ✅ Stable | [Event Sourcing](../advanced/event-sourcing.md#domain-events) | [blog_enterprise](../../examples/blog_enterprise/) |
| **CQRS Architecture** | ✅ Stable | [Patterns Guide](../patterns/README.md#cqrs) | [blog_enterprise](../../examples/blog_enterprise/) |
| **Compliance (GDPR/SOC2/HIPAA)** | ✅ Stable | [Enterprise Guide](../enterprise/enterprise.md) | [saas-starter](../../examples/saas-starter/) |

---

## Real-Time Features

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **GraphQL Subscriptions** | ✅ Stable | See examples | [real_time_chat](../../examples/real_time_chat/) |
| **WebSocket Support** | ✅ Stable | See examples | [real_time_chat](../../examples/real_time_chat/) |
| **Presence Tracking** | ✅ Stable | See examples | [real_time_chat](../../examples/real_time_chat/) |
| **LISTEN/NOTIFY (PostgreSQL)** | ✅ Stable | [Database Patterns](../advanced/database-patterns.md) | [real_time_chat](../../examples/real_time_chat/) |

---

## Monitoring & Observability

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **Built-in Error Tracking** | ✅ Stable | [Monitoring Guide](../production/monitoring.md) | [saas-starter](../../examples/saas-starter/) |
| **PostgreSQL-based Monitoring** | ✅ Stable | [Monitoring Guide](../production/monitoring.md#postgresql-monitoring) | [saas-starter](../../examples/saas-starter/) |
| **OpenTelemetry Integration** | ✅ Stable | [Observability Guide](../production/observability.md) | [saas-starter](../../examples/saas-starter/) |
| **Grafana Dashboards** | ✅ Stable | [Monitoring Guide](../production/monitoring.md#grafana) | [grafana/](../../grafana/) |
| **Health Checks** | ✅ Stable | [Health Checks](../production/health-checks.md) | All examples |
| **Custom Metrics** | ✅ Stable | [Observability Guide](../production/observability.md#metrics) | [analytics_dashboard](../../examples/analytics_dashboard/) |

---

## Integration Features

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **FastAPI Integration** | ✅ Stable | See examples | [fastapi](../../examples/fastapi/) |
| **Starlette Integration** | ✅ Stable | See examples | [fastapi](../../examples/fastapi/) |
| **ASGI Applications** | ✅ Stable | Built-in | All examples |
| **TypeScript Client Generation** | ✅ Stable | See examples | [documented_api](../../examples/documented_api/) |

---

## Development Tools

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **GraphQL Playground** | ✅ Stable | Built-in | All examples |
| **Schema Introspection** | ✅ Stable | Built-in | All examples |
| **Hot Reload** | ✅ Stable | Built-in | All examples |
| **CLI Commands** | ✅ Stable | [CLI Reference](../reference/cli.md) | - |
| **Type Generation** | ✅ Stable | [CLI Reference](../reference/cli.md#type-generation) | - |
| **Schema Export** | ✅ Stable | [CLI Reference](../reference/cli.md#schema-export) | - |

---

## Deployment Support

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **Docker Support** | ✅ Stable | [Deployment Guide](../deployment/README.md#docker) | All examples |
| **Kubernetes Support** | ✅ Stable | [Deployment Guide](../deployment/README.md#kubernetes) | [deployment/k8s/](../../deployment/k8s/) |
| **AWS Deployment** | ✅ Stable | [Deployment Guide](../deployment/README.md#aws) | - |
| **GCP Deployment** | ✅ Stable | [Deployment Guide](../deployment/README.md#gcp) | - |
| **Azure Deployment** | ✅ Stable | [Deployment Guide](../deployment/README.md#azure) | - |
| **Environment Configuration** | ✅ Stable | [Configuration Guide](../core/configuration.md) | All examples |

---

## AI & Vector Features (v1.5.0)

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **pgvector Integration** | ✅ Stable | [pgvector Guide](pgvector.md) | [vector_search](../../examples/vector_search/) |
| **Vector Similarity Search** | ✅ Stable | [pgvector Guide](pgvector.md#distance-operators) | [vector_search](../../examples/vector_search/) |
| **GraphQL Cascade** | ✅ Stable | [Cascade Guide](graphql-cascade.md) | [graphql-cascade](../../examples/graphql-cascade/) |
| **SQL Function Return Format** | ✅ Stable | [SQL Function Guide](sql-function-return-format.md) | [mutations_demo](../../examples/mutations_demo/) |
| **LangChain Integration** | ✅ Stable | [LangChain Guide](../guides/langchain-integration.md) | [Documentation](../guides/langchain-integration.md) |
| **AI-Native Architecture** | ✅ Stable | [AI-Native Guide](ai-native.md) | [Documentation](ai-native.md) |

### Vector Distance Operators

| Operator | PostgreSQL | Use Case | Documentation |
|----------|------------|----------|---------------|
| `cosine_distance` | `<=>` | Text similarity, semantic search | [pgvector](pgvector.md#cosine-distance-cosine_distance) |
| `l2_distance` | `<->` | Euclidean distance, spatial | [pgvector](pgvector.md#l2-distance-l2_distance) |
| `inner_product` | `<#>` | Learned similarity metrics | [pgvector](pgvector.md#inner-product-inner_product) |
| `l1_distance` | `<+>` | Manhattan distance, sparse vectors | [pgvector](pgvector.md#l1-distance-l1_distance) |
| `hamming_distance` | `<~>` | Binary vectors, hashing | [pgvector](pgvector.md#hamming-distance-hamming_distance) |
| `jaccard_distance` | `<%>` | Set similarity, sparse binary | [pgvector](pgvector.md#jaccard-distance-jaccard_distance) |

---

## Cache & Invalidation Features (v1.5.0)

| Feature | Status | Documentation | Example |
|---------|--------|---------------|---------|
| **CASCADE Invalidation** | ✅ Stable | [Cascade Guide](graphql-cascade.md) | [complete_cqrs_blog](../../examples/complete_cqrs_blog/) |
| **PostgreSQL Function Pattern** | ✅ Stable | [PostgreSQL Pattern](graphql-cascade.md#postgresql-function-pattern) | - |
| **Cascade Structure** | ✅ Stable | [Cascade Structure](graphql-cascade.md#cascade-structure) | - |
| **Apollo Client Integration** | ✅ Stable | [Client Integration](graphql-cascade.md#apollo-client) | - |
| **Relay Integration** | ✅ Stable | [Client Integration](graphql-cascade.md#relay) | - |

---

## Legend

- ✅ **Stable**: Production-ready, fully documented
- 🚧 **Beta**: Functional but API may change
- 🔬 **Experimental**: Early stage, feedback welcome
- 📋 **Planned**: On roadmap, not yet implemented

---

## Feature Request?

Don't see a feature you need? [Open a GitHub issue](https://github.com/fraiseql/fraiseql/issues/new) with:
- **Use case**: What are you trying to achieve?
- **Current workaround**: How are you solving it today?
- **Proposed solution**: How should FraiseQL support this?

We prioritize features based on:
1. Number of user requests
2. Alignment with FraiseQL's philosophy (database-first, performance, security)
3. Implementation complexity vs. value

---

## Quick Links

- **[Getting Started](../getting-started/quickstart.md)** - Build your first API in 5 minutes
- **[Core Concepts](../core/concepts-glossary.md)** - Understand FraiseQL's mental model
- **[Examples](../../examples/)** - Learn by example
- **[Production Deployment](../production/)** - Deploy to production
