# Deployment Guide

Complete guide for deploying FraiseQL in various environments.

## Quick Start

Choose your deployment environment:

### Local Development

```bash
# 1. Compile schema
fraiseql-cli compile schema.json -o schema.compiled.json

# 2. Start server
fraiseql-server -c config.toml

# Server at http://localhost:8080
```

### Docker

```bash
# 1. Build image
docker build -t fraiseql-server:latest .

# 2. Run container
docker run -p 8080:8080 \
  -v $(pwd)/config.toml:/etc/fraiseql/config.toml \
  -e DATABASE_URL=postgresql://... \
  fraiseql-server:latest
```

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

1. **Security**: [TLS Configuration](../configuration/TLS_CONFIGURATION.md), [Rate Limiting](../configuration/RATE_LIMITING.md)
2. **Database**: [PostgreSQL Authentication](../configuration/POSTGRESQL_AUTHENTICATION.md)
3. **Operations**: [Observability](../operations/observability.md), [Distributed Tracing](../operations/distributed-tracing.md)

## Running Checks

```bash
# Check configuration
fraiseql-cli validate schema.json config.toml

# Health check endpoint
curl http://localhost:8080/health

# Metrics endpoint
curl http://localhost:8080/metrics
```

## Troubleshooting

Common deployment issues:

- Connection timeouts
- Certificate validation errors
- Database authentication failures
- Rate limiting being too strict

See [Troubleshooting Guide](../TROUBLESHOOTING.md) for solutions.

---

**Version**: v2.0.0
**Last Updated**: February 1, 2026
