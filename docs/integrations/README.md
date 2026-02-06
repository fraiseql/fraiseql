<!-- Skip to main content -->
---

title: Integrations
description: Integration guides for connecting FraiseQL with external services and databases.
keywords: ["framework", "sdk", "monitoring", "database", "authentication"]
tags: ["documentation", "reference"]
---

# Integrations

Integration guides for connecting FraiseQL with external services and databases.

## Quick Navigation

### Federation (Multi-Database)

- **[Federation Quick Start](federation/guide.md)** — Get started with federation
- **[Federation API Reference](federation/api-reference.md)** — API reference for federation
- **[Deployment](federation/deployment.md)** — Deploy federated systems
- **[SAGA Patterns](federation/sagas.md)** — Distributed transactions across databases
- **[Readiness Checklist](federation/readiness-checklist.md)** — Pre-deployment checklist

### Authentication

- **[Authentication Guide](authentication/README.md)** — Overview and setup
- **[Auth0 Setup](authentication/setup-auth0.md)**
- **[Google OAuth Setup](authentication/setup-google-oauth.md)**
- **[Keycloak Setup](authentication/setup-keycloak.md)**
- **[SCRAM Authentication](authentication/scram.md)**
- **[Deployment](authentication/deployment.md)**
- **[Security Checklist](authentication/security-checklist.md)**
- **[Troubleshooting](authentication/troubleshooting.md)**

### Arrow Flight (Analytics)

- **[Arrow Flight Guide](arrow-flight/getting-started.md)** — Get started
- **[Architecture](arrow-flight/architecture.md)** — Design and architecture
- **[Migration Guide](arrow-flight/migration-guide.md)** — Migrate from GraphQL

### Monitoring & Visualization

- **[Grafana Dashboard](monitoring/grafana-dashboard-8.7.json)** — Pre-built dashboard

## Federation Features

FraiseQL's federation implementation enables:

- **Multi-database composition** — Compose queries across PostgreSQL, MySQL, SQLite, SQL Server
- **Direct database federation** — No HTTP gateway, direct connections
- **Distributed transactions** — SAGA pattern for consistency
- **Real-time observability** — Federation-aware metrics and tracing

See [Federation Quick Start](federation/guide.md) for detailed guide.

## Authentication Providers Supported

- **Auth0** — SaaS identity platform
- **Google OAuth 2.0** — Social login
- **Keycloak** — Self-hosted identity server
- **SCRAM** — SASL/SCRAM credential authentication
- **Custom JWT** — Bring your own token issuer

See [Authentication Guide](authentication/README.md) for setup.

## Arrow Flight Integration

Arrow Flight enables high-performance analytics by:

- **Columnar data transfer** — Binary Arrow format (10-100x faster than JSON)
- **Direct database access** — No GraphQL query overhead
- **BI tool integration** — Connect Tableau, Power BI, Apache Superset directly

See [Arrow Flight Guide](arrow-flight/getting-started.md).

---

**Version**: v2.0.0-alpha.1
**Last Updated**: February 5, 2026
