<!-- Skip to main content -->
---
title: Configuration Guide
description: Complete configuration reference for FraiseQL security, networking, and operational settings.
keywords: []
tags: ["documentation", "reference"]
---

# Configuration Guide

Complete configuration reference for FraiseQL security, networking, and operational settings.

## Quick Navigation

### Security Configuration

- **[Security Configuration](SECURITY_CONFIGURATION.md)** — Overview of all security settings
- **[TLS/SSL Configuration](TLS_CONFIGURATION.md)** — Configure HTTPS and mutual TLS
- **[Rate Limiting](RATE_LIMITING.md)** — Brute-force protection and request throttling
- **[Runtime Security Initialization](SECURITY_RUNTIME_INITIALIZATION.md)** — Initialize security subsystems at startup

### Database Configuration

- **[PostgreSQL Authentication](POSTGRESQL_AUTHENTICATION.md)** — PostgreSQL connection and authentication

## Environment Variables

FraiseQL uses `FRAISEQL_` prefixed environment variables:

```bash
<!-- Code example in BASH -->
# Security
FRAISEQL_ENABLE_TLS=true
FRAISEQL_TLS_CERT=/path/to/cert.pem
FRAISEQL_TLS_KEY=/path/to/key.pem

# Rate Limiting
FRAISEQL_RATE_LIMIT_ENABLED=true
FRAISEQL_RATE_LIMIT_REQUESTS_PER_MINUTE=100

# Database
FRAISEQL_DATABASE_URL=postgresql://user:pass@localhost/db
FRAISEQL_DATABASE_POOL_SIZE=20
FRAISEQL_DATABASE_POOL_TIMEOUT=30
```text
<!-- Code example in TEXT -->

## Configuration Priority

1. **Compiled schema** (`schema.compiled.json` security section) — Default values
2. **Config file** (`FraiseQL.toml`) — Override defaults
3. **Environment variables** — Override config file (for production secrets)

## Common Scenarios

### Production Setup

1. Enable TLS: [TLS Configuration](TLS_CONFIGURATION.md)
2. Set rate limits: [Rate Limiting](RATE_LIMITING.md)
3. Configure security: [Security Configuration](SECURITY_CONFIGURATION.md)
4. Database connection: [PostgreSQL Authentication](POSTGRESQL_AUTHENTICATION.md)

### Development Setup

1. Disable TLS (use HTTP)
2. Increase rate limits for testing
3. Use minimal security hardening
4. Local database connection

### Enterprise Deployment

1. mTLS for service-to-service communication
2. Strict rate limiting
3. Security audit logging enabled
4. KMS for field-level encryption

---

**Version**: v2.0.0-alpha.1
**Last Updated**: February 5, 2026
