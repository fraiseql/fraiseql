<!-- Skip to main content -->
---
title: Deployment Guide
description: Complete guide for deploying FraiseQL in various environments.
keywords: []
tags: ["documentation", "reference"]
---

# Deployment Guide

Complete guide for deploying FraiseQL in various environments.

## Quick Start

Choose your deployment environment:

### Local Development

```bash
<!-- Code example in BASH -->
# 1. Compile schema
FraiseQL-cli compile schema.json -o schema.compiled.json

# 2. Start server
FraiseQL-server -c config.toml

# Server at http://localhost:8080
```text
<!-- Code example in TEXT -->

### Docker

```bash
<!-- Code example in BASH -->
# 1. Build image
docker build -t FraiseQL-server:latest .

# 2. Run container
docker run -p 8080:8080 \
  -v $(pwd)/config.toml:/etc/FraiseQL/config.toml \
  -e DATABASE_URL=postgresql://... \
  FraiseQL-server:latest
```text
<!-- Code example in TEXT -->

### Kubernetes

See [Production Deployment Guide](guide.md) for Kubernetes manifests and best practices.

## Deployment Guides

- **[Production Deployment Guide](guide.md)** — Enterprise-scale deployments with HA, monitoring, security
- **[Database Migration](migration-projection.md)** — Migrate existing schemas to FraiseQL

## Key Sections

### Pre-Deployment

- System requirements (CPU, memory, network)
- Database setup and credentials
- SSL/TLS certificates
- Configuration validation

### Deployment Strategies

- **Docker Compose** — For small deployments
- **Kubernetes** — For cloud-native deployments
- **Bare Metal** — For on-premises deployments
- **Managed Services** — AWS, Google Cloud, Azure

### Post-Deployment

- Health checks and readiness probes
- Monitoring and observability setup
- Performance tuning
- Security hardening

## Configuration

Before deployment, configure:

1. **Security**: [TLS Configuration](../configuration/tls-configuration.md), [Rate Limiting](../configuration/rate-limiting.md)
2. **Database**: [PostgreSQL Authentication](../configuration/postgresql-authentication.md)
3. **Operations**: [Observability](../operations/observability.md), [Distributed Tracing](../operations/distributed-tracing.md)

## Running Checks

```bash
<!-- Code example in BASH -->
# Check configuration
FraiseQL-cli validate schema.json config.toml

# Health check endpoint
curl http://localhost:8080/health

# Metrics endpoint
curl http://localhost:8080/metrics
```text
<!-- Code example in TEXT -->

## Troubleshooting

Common deployment issues:

- Connection timeouts
- Certificate validation errors
- Database authentication failures
- Rate limiting being too strict

See [Troubleshooting Guide](../troubleshooting.md) for solutions.

---

**Version**: v2.0.0
**Last Updated**: February 1, 2026
