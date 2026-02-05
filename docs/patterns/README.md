# Real-World Application Patterns

**Status:** ✅ Production Ready
**Audience:** Architects, senior developers
**Last Updated:** 2026-02-05

Complete architectural patterns and implementation guides for production FraiseQL applications.

---

## Overview

This section contains production-tested patterns for building scalable applications with FraiseQL. Each pattern includes:

- Complete schema design
- Architecture diagrams
- Database structure
- Query optimization strategies
- Security considerations
- Scaling guidelines

---

## SaaS & Multi-Tenancy

### [Multi-Tenant SaaS with Row-Level Security](./saas-multi-tenant.md)

**Use Case:** Build a SaaS platform where customers are isolated at the database row level.

**Architecture:**

- Single PostgreSQL instance with tenant ID per row
- Automatic tenant context injection from JWT claims
- Row-level security policies in database
- Billing integration with Stripe

**Includes:**

- Tenant isolation schema design
- Dynamic data filtering at query time
- Subscription management
- Usage-based billing calculations
- Audit logging per tenant

---

## Analytics & Data Warehousing

### [Analytics Platform with OLAP](./analytics-olap-platform.md)

**Use Case:** Build a BI/analytics dashboard querying large datasets efficiently.

**Architecture:**

- Fact tables for events (millions of rows)
- Dimension tables for context
- Materialized views for aggregations
- Real-time and batch query support
- Arrow Flight for bulk exports

**Includes:**

- Star schema design
- Aggregation query patterns
- Time-series data handling
- Drill-down analytics queries
- Performance optimization for large datasets
- Export to Excel/CSV/Parquet

---

## Database Federation

### [Multi-Database Federation](./federation-patterns.md)

**Use Case:** Query across multiple databases (PostgreSQL, MySQL, SQLite) as a unified GraphQL API.

**Architecture:**

- PostgreSQL as primary (customer data)
- MySQL as secondary (historical data)
- SQLite for local caching
- Cross-database joins
- Transaction coordination

**Includes:**

- Federation setup and configuration
- Cross-database query execution
- Consistency guarantees
- Failover handling
- Performance considerations

---

## Real-Time Collaborative Apps

### [Real-Time Collaboration with Subscriptions](./realtime-collaboration.md)

**Use Case:** Build collaborative tools (document editor, project management) with real-time updates.

**Architecture:**

- WebSocket subscriptions for live updates
- Operational transformation for conflict resolution
- Event sourcing for audit trail
- Presence tracking (who's online)
- Optimistic concurrency control

**Includes:**

- Subscription patterns for live data
- Conflict resolution strategies
- Presence management
- Activity feeds
- Notification system

---

## IoT & Time-Series Data

### [IoT Platform with Time-Series Data](./iot-timeseries.md)

**Use Case:** Collect and query IoT sensor data efficiently (millions of data points).

**Architecture:**

- Time-partitioned tables for sensor readings
- Aggregation tables for rollups (hourly, daily)
- Retention policies
- Real-time alerts
- Streaming ingestion

**Includes:**

- Schema design for high-cardinality data
- Efficient time-range queries
- Downsampling strategies
- Alert query patterns
- Scaling to billions of points

---

## E-Commerce Platform

### [E-Commerce with Complex Workflows](./ecommerce-workflows.md)

**Use Case:** Build e-commerce with orders, inventory, fulfillment workflows.

**Architecture:**

- Product catalog with variants
- Order management with state machine
- Inventory tracking and reservations
- Fulfillment pipeline
- Returns and refunds

**Includes:**

- Schema for products/variants/SKUs
- Order state machine design
- Inventory allocation queries
- Reporting dashboards
- Integration with payment processors

---

## Content Management System (CMS)

### [Headless CMS with Versioning](./cms-headless.md)

**Use Case:** Build a headless CMS with content versioning, drafts, and publishing workflows.

**Architecture:**

- Content models (dynamic types)
- Version history tracking
- Draft/published states
- Media management
- Content relationship graphs

**Includes:**

- Polymorphic content types
- Version control and rollback
- Publication workflows
- SEO metadata management
- API-first design

---

## Social Network Platform

### [Social Network with Activity Feeds](./social-network.md)

**Use Case:** Build a social network with feeds, followers, messaging, and notifications.

**Architecture:**

- User graph with follower relationships
- Activity feed generation
- Real-time notifications
- Messaging with threading
- Privacy controls per post

**Includes:**

- Graph queries for friends/followers
- Feed algorithms (chronological, algorithmic)
- Notification delivery
- Private messaging
- Content visibility rules

---

## Common Patterns Across All Applications

### Data Validation

- Pre-insert validation on mutation
- Custom business rule validation
- Referential integrity checks
- Type coercion and sanitization

### Error Handling

- Validation error responses with field-level details
- Business logic error codes
- User-friendly error messages
- Error tracking and monitoring

### Performance Optimization

- Query result caching strategies
- N+1 query prevention
- Pagination for large result sets
- Connection pooling at backend
- Database index strategy

### Security

- Authentication with JWT tokens
- Authorization per object and field
- SQL injection prevention (prepared statements)
- Rate limiting per user/IP
- Audit logging for sensitive operations

### Monitoring & Observability

- Query execution time tracking
- Error rate monitoring
- Database query metrics
- Application performance monitoring (APM)
- Distributed tracing

---

## Pattern Selection Guide

Choose a pattern based on your application needs:

| Pattern | Best For | Scale |
|---------|----------|-------|
| **Multi-Tenant SaaS** | B2B SaaS platforms, white-label solutions | 10K-100K+ customers |
| **Analytics OLAP** | BI dashboards, business intelligence, reporting | 100GB-100TB+ data |
| **Database Federation** | Legacy system migration, data warehouse queries | Multiple data sources |
| **Real-Time Collaboration** | Google Docs-like, Figma-like, project management | 100-10K+ concurrent users |
| **IoT Time-Series** | Sensor networks, monitoring systems, metrics | Billions of data points |
| **E-Commerce** | Online stores, marketplaces, product catalogs | 1M-100M+ products |
| **Headless CMS** | Content platforms, publishing, websites | 10K-100K+ content items |
| **Social Network** | User communities, social platforms, forums | 1M-1B+ users |

---

## Common Challenges & Solutions

### Challenge: N+1 Queries

**Problem:** Fetching parent then iterating children causes excessive queries

**Solution:** Use FraiseQL's nested query syntax or batch queries

```graphql
# ✅ Good: Single query with nested relationships
query GetPostsWithAuthors {
  posts {
    id
    title
    author {
      id
      name
      email
    }
    comments {
      id
      content
      author { name }
    }
  }
}
```text

### Challenge: Large Result Sets

**Problem:** Querying millions of rows causes memory issues

**Solution:** Implement cursor-based pagination

```graphql
query GetPostsPaginated($first: Int!, $after: String) {
  posts(first: $first, after: $after) {
    edges {
      cursor
      node {
        id
        title
      }
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```text

### Challenge: Complex Authorization Rules

**Problem:** Some users can see only subset of data

**Solution:** Use row-level security in database + custom claims in JWT

```text
JWT Claims: { userId: "123", role: "admin", tenantId: "tenant_456" }
              ↓
Database RLS Policy: WHERE tenant_id = JWT.tenantId AND (is_public OR owner_id = JWT.userId)
```text

### Challenge: Real-Time Updates

**Problem:** Need live data updates for subscriptions

**Solution:** Use WebSocket subscriptions with intelligent filtering

```graphql
subscription OnUserOnline {
  userStatusChanged {
    userId
    status  # online, idle, offline
    lastSeen
  }
}
```text

---

## See Also

**Detailed Patterns:**

- [Multi-Tenant SaaS with RLS](./saas-multi-tenant.md)
- [Analytics Platform with OLAP](./analytics-olap-platform.md)
- [Database Federation](./federation-patterns.md)
- [Real-Time Collaboration](./realtime-collaboration.md)
- [IoT Time-Series Data](./iot-timeseries.md)
- [E-Commerce Workflows](./ecommerce-workflows.md)
- [Headless CMS](./cms-headless.md)
- [Social Network Platform](./social-network.md)

**Related Guides:**

- [Production Deployment](../guides/production-deployment.md)
- [Performance Optimization](../guides/performance-optimization.md)
- [Schema Design Best Practices](../guides/schema-design-best-practices.md)
- [Security Checklist](../guides/production-security-checklist.md)

**Full-Stack Examples:**

- [Python + React Example](../examples/fullstack-python-react.md)
- [TypeScript + Vue Example](../examples/fullstack-typescript-vue.md)
- [Go + Flutter Example](../examples/fullstack-go-flutter.md)
- [Java + Next.js Example](../examples/fullstack-java-nextjs.md)

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
