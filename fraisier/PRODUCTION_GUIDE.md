# Fraisier v0.1.0 Production Guide

Welcome to Fraisier - a deployment orchestrator for the FraiseQL ecosystem. This guide covers production deployment, operation, troubleshooting, and best practices.

## Table of Contents

1. [Overview](#overview)
2. [Installation](#installation)
3. [Configuration](#configuration)
4. [Deployment Providers](#deployment-providers)
5. [Deployment Workflows](#deployment-workflows)
6. [Monitoring & Observability](#monitoring--observability)
7. [Troubleshooting](#troubleshooting)
8. [Best Practices](#best-practices)
9. [Advanced Patterns](#advanced-patterns)
10. [FAQ](#faq)

---

## Overview

Fraisier is a deployment orchestrator that manages deployments across multiple infrastructure providers:

- **Bare Metal** - SSH + systemd deployments to VMs/physical servers
- **Docker Compose** - Containerized deployments for local/test environments
- **Coolify** - PaaS deployments via Coolify self-hosted platform

### Key Features

- ✅ Multi-provider support with unified interface
- ✅ Complete deployment lifecycle management
- ✅ Automatic health checking and rollback
- ✅ Production-grade monitoring and logging
- ✅ Database migrations and schema management
- ✅ Git integration for webhook-based deployments

---

## Installation

### Requirements

- Python 3.11+
- `uv` package manager
- Provider-specific requirements (see below)

### Installation Steps

```bash
# Clone the repository
git clone https://github.com/fraiseql/fraisier.git
cd fraisier

# Install for production
pip install -e ".[prod]"

# Or with uv
uv pip install -e ".[prod]"
```

### Provider-Specific Installation

#### Bare Metal Provider
```bash
# Install SSH support
pip install asyncssh

# Ensure SSH key access is configured
ssh-keygen -t ed25519 -f ~/.ssh/id_fraisier
ssh-copy-id -i ~/.ssh/id_fraisier deploy@prod.example.com
```

#### Docker Compose Provider
```bash
# Install Docker and docker-compose
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER

# Verify installation
docker-compose --version
```

#### Coolify Provider
```bash
# Coolify is self-hosted
# See: https://coolify.io/docs/installation

# Install HTTP client
pip install httpx
```

---

## Configuration

### Configuration File Structure

Create a `fraises.yaml` file in your project root:

```yaml
# Git provider configuration
git:
  provider: github  # github, gitlab, gitea, bitbucket
  github:
    base_url: https://github.com
    webhook_secret: ${FRAISIER_WEBHOOK_SECRET}

# Define your services (fraises)
fraises:
  my_api:
    type: api
    description: My GraphQL API
    environments:
      production:
        name: api.example.com
        branch: main
        provider: bare_metal
        bare_metal:
          host: prod.example.com
          username: deploy
          port: 22
          key_path: ~/.ssh/id_fraisier
        systemd_service: my-api.service
        health_check:
          url: https://api.example.com/health
          timeout: 30
          retries: 3

# Environment-specific configuration
environments:
  production:
    log_dir: /var/log/fraisier
    notifications:
      enabled: true
      slack_channel: "#deployments"
```

### Environment Variables

```bash
# Authentication
export FRAISIER_WEBHOOK_SECRET="your-webhook-secret"
export FRAISIER_API_TOKEN="coolify-api-token"

# Database
export DATABASE_URL="postgresql://user:pass@localhost/fraisier"

# Git providers
export GITHUB_TOKEN="ghp_..."
export GITLAB_TOKEN="glpat-..."

# Logging
export FRAISIER_LOG_LEVEL="INFO"  # DEBUG, INFO, WARNING, ERROR
```

### Database Setup

```bash
# Run migrations
fraisier db-check

# This will:
# - Verify database connectivity
# - Create tables and views
# - Initialize schema
```

---

## Deployment Providers

### Bare Metal Provider (SSH + systemd)

**Configuration:**
```yaml
bare_metal:
  host: prod.example.com
  username: deploy
  port: 22
  key_path: ~/.ssh/id_fraisier
  known_hosts_path: ~/.ssh/known_hosts
```

**Prerequisites:**
- SSH key-based authentication
- systemd service for your application
- Git repository accessible from server

**Example systemd Service:**
```ini
[Unit]
Description=My Fraise API
After=network.target

[Service]
User=deploy
WorkingDirectory=/var/www/api
ExecStart=/usr/bin/python -m uvicorn main:app
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

**Operations:**
```bash
# Deploy
fraisier deploy my_api production

# Check status
fraisier status my_api production

# View deployment history
fraisier history my_api production
```

### Docker Compose Provider

**Configuration:**
```yaml
docker_compose:
  compose_file: docker-compose.prod.yml
  project_name: myapp
  timeout: 600
```

**Prerequisites:**
- Docker and docker-compose installed
- docker-compose.yml in deployment directory
- Docker daemon running and accessible

**Example docker-compose.yml:**
```yaml
version: '3.9'
services:
  api:
    image: myregistry/api:latest
    ports:
      - "8000:8000"
    environment:
      DATABASE_URL: postgresql://db:5432/myapp
    depends_on:
      - db
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 10s
      timeout: 5s
      retries: 3

  db:
    image: postgres:15
    environment:
      POSTGRES_PASSWORD: secret
```

**Operations:**
```bash
# Deploy
fraisier deploy my_api production

# View logs
fraisier logs my_api production

# Scale service
docker-compose -f docker-compose.prod.yml up -d --scale api=3
```

### Coolify Provider (PaaS)

**Configuration:**
```yaml
coolify:
  api_url: https://coolify.example.com/api
  api_token: ${COOLIFY_API_TOKEN}
  application_id: app-123456
  timeout: 600
```

**Prerequisites:**
- Coolify instance running and accessible
- Application already created in Coolify
- API token with deployment permissions

**Operations:**
```bash
# Deploy
fraisier deploy my_api production

# View recent deployments
fraisier history my_api production --limit 10

# Manual rollback
fraisier rollback my_api production --to previous
```

---

## Deployment Workflows

### Basic Deployment Flow

```
1. Connect to infrastructure
   └─ Verify connectivity
   └─ Check credentials
   └─ Validate configuration

2. Pre-deployment checks
   └─ Get current version
   └─ Get latest version
   └─ Compare (skip if same)

3. Execute deployment
   └─ Pull code/images
   └─ Run migrations
   └─ Restart services

4. Post-deployment validation
   └─ Run health checks
   └─ Verify logs
   └─ Record in database

5. Notify stakeholders
   └─ Slack notification
   └─ Email notification
   └─ Webhook callback
```

### Blue-Green Deployment

This pattern runs two complete environments and switches traffic:

```bash
# 1. Deploy to green environment
export DEPLOY_ENV=green
fraisier deploy my_api staging

# 2. Verify green is healthy
fraisier status my_api staging

# 3. Switch traffic (via load balancer)
# Update LB config to point to green

# 4. Decommission blue (or keep for quick rollback)
```

### Canary Deployment

Deploy to a subset first, monitor, then full deployment:

```bash
# 1. Deploy to test environment
fraisier deploy my_api test

# 2. Run smoke tests
pytest tests/smoke_tests.py

# 3. Deploy to production with low traffic percentage
# (via load balancer or feature flags)

# 4. Monitor metrics
fraisier stats my_api production --hours 1

# 5. Increase traffic percentage if stable
# 6. Full deployment if no issues
```

### Rollback Procedure

```bash
# 1. Identify the deployment to rollback from
fraisier history my_api production --limit 5

# 2. Rollback to previous version
fraisier rollback my_api production --to previous

# 3. Verify rollback succeeded
fraisier status my_api production
fraisier logs my_api production --lines 50
```

---

## Monitoring & Observability

### Health Checks

Fraisier supports multiple health check types:

```yaml
health_check:
  # HTTP endpoint
  type: http
  url: https://api.example.com/health
  timeout: 30
  retries: 3
  retry_delay: 2

  # TCP port connectivity
  type: tcp
  port: 5432
  timeout: 10

  # Custom command
  type: exec
  command: curl -f http://localhost:8000/health || exit 1

  # systemd service status
  type: systemd
  service: my-api.service
```

### Metrics Endpoint

Fraisier exposes Prometheus metrics:

```bash
# Start metrics server
fraisier metrics --port 8001 --address 0.0.0.0

# Metrics available at http://localhost:8001/metrics
```

**Key Metrics:**
- `fraisier_deployments_total` - Total deployments by status
- `fraisier_deployment_duration_seconds` - Deployment duration
- `fraisier_deployment_errors_total` - Deployment errors
- `fraisier_db_queries_total` - Database queries
- `fraisier_active_db_connections` - Active database connections

### Logging

**Log Levels:**
```bash
export FRAISIER_LOG_LEVEL=DEBUG  # Verbose logging
export FRAISIER_LOG_LEVEL=INFO   # Standard logging (default)
export FRAISIER_LOG_LEVEL=WARNING # Warnings only
export FRAISIER_LOG_LEVEL=ERROR  # Errors only
```

**Log Output:**
```json
{
  "timestamp": "2026-01-22T12:00:00.000Z",
  "level": "INFO",
  "message": "Deployment completed",
  "fraise": "my_api",
  "environment": "production",
  "deployment_id": "deploy-123",
  "duration_seconds": 45.2,
  "status": "success"
}
```

---

## Troubleshooting

### SSH Connection Issues (Bare Metal)

**Problem:** SSH connection timeout
```
ConnectionError: Failed to connect to prod.example.com:22
```

**Solutions:**
1. Verify connectivity: `ssh deploy@prod.example.com`
2. Check firewall: `telnet prod.example.com 22`
3. Verify SSH key: `ssh-keygen -l -f ~/.ssh/id_fraisier`
4. Check permissions: `ls -la ~/.ssh/` (should be 0700)

### Docker Connection Issues (Docker Compose)

**Problem:** Docker daemon not responding
```
RuntimeError: Failed to connect to Docker daemon
```

**Solutions:**
1. Check Docker status: `systemctl status docker`
2. Verify Docker socket: `ls -la /var/run/docker.sock`
3. Add user to group: `sudo usermod -aG docker $USER`
4. Restart Docker: `sudo systemctl restart docker`

### Health Check Failures

**Problem:** Health check fails after deployment
```
Error: Health check failed after deployment
```

**Solutions:**
1. Check application logs: `fraisier logs my_api production`
2. Verify health endpoint: `curl -v http://localhost:8000/health`
3. Check connectivity: `netstat -tuln | grep 8000`
4. Increase retry count in configuration

### Database Issues

**Problem:** Database migration fails
```
Error: Migration 001_create_tables.sql failed
```

**Solutions:**
1. Check database connectivity: `fraisier db-check`
2. Verify credentials in DATABASE_URL
3. Check migration files: `ls fraisier/db/migrations/sqlite/`
4. Manual migration: `psql -U user -d database -f migration.sql`

### Provider-Specific Issues

#### Bare Metal: systemd Service Not Found
```bash
# List all services
systemctl list-units --all | grep my-api

# Reload systemd daemon
sudo systemctl daemon-reload

# Start service
sudo systemctl start my-api.service
```

#### Docker: Image Pull Failures
```bash
# Check registry credentials
docker login registry.example.com

# Manually pull image
docker pull registry.example.com/api:latest

# Check disk space
docker system df
```

#### Coolify: API Authentication Errors
```bash
# Verify token is valid
curl -H "Authorization: Bearer $COOLIFY_API_TOKEN" \
  https://coolify.example.com/api/applications

# Check token expiration
# Regenerate token in Coolify UI if needed
```

---

## Best Practices

### 1. Pre-Deployment Checklist

- [ ] Code is merged to main branch
- [ ] Tests pass: `pytest tests/`
- [ ] Linting passes: `ruff check .`
- [ ] Database migrations are tested
- [ ] Configuration is correct for environment
- [ ] Health check endpoint is working
- [ ] Backup exists (if applicable)

### 2. Deployment Timing

- Deploy during low-traffic periods
- Avoid deployments near business-critical times
- Schedule maintenance windows for major changes
- Have a rollback plan ready

### 3. Monitoring During Deployment

```bash
# In one terminal: watch deployment
watch fraisier status my_api production

# In another: monitor logs
tail -f /var/log/fraisier/deployments.log

# In third: monitor metrics
curl http://localhost:8001/metrics | grep deployment
```

### 4. Notification Setup

```yaml
notifications:
  enabled: true
  slack_channel: "#deployments"
  slack_webhook: ${SLACK_WEBHOOK_URL}
  email_recipients:
    - ops@example.com
    - team@example.com
```

### 5. Version Management

- Always tag releases: `git tag v0.1.0`
- Keep `main` deployable
- Use feature branches for changes
- Require code review before merge

### 6. Database Management

```bash
# Always backup before major operations
pg_dump -U user database > backup.sql

# Test migrations in staging first
fraisier deploy my_api staging

# Then deploy to production
fraisier deploy my_api production
```

### 7. Security Best Practices

- **Secrets**: Use environment variables, never hardcode
- **SSH Keys**: Use strong keys, rotate regularly
- **API Tokens**: Store in secure vault, rotate regularly
- **Database**: Use separate credentials per environment
- **Network**: Use VPN/firewall to restrict access

---

## Advanced Patterns

### Progressive Deployment

Deploy gradually to minimize impact:

```bash
# Stage 1: Deploy to 25% of servers
# (Scale Docker containers or use load balancer)

# Stage 2: Monitor for 30 minutes
# Check error rates, latency, resource usage

# Stage 3: If all metrics normal, deploy to 100%
# If issues detected, rollback to previous version
```

### Canary Releases with Metrics

```bash
# Deploy new version
fraisier deploy my_api production

# Monitor for 1 hour
sleep 3600

# Check error rate (should be < 0.1%)
fraisier stats my_api production --hours 1 | grep error_rate

# Check latency (p99 should be < 500ms)
fraisier stats my_api production --hours 1 | grep latency
```

### Automated Rollback

Configure automatic rollback if health check fails:

```bash
# fraisier will automatically rollback if:
# 1. Health check fails 3 times
# 2. Error rate > 5% for 5 minutes
# 3. CPU usage > 90% for 10 minutes
# 4. Memory usage > 85% for 5 minutes
```

### Multi-Environment Promotion

Deploy through environments:

```bash
# 1. Deploy to staging
fraisier deploy my_api staging

# 2. Run integration tests
pytest tests/integration/

# 3. If tests pass, deploy to production
fraisier deploy my_api production
```

---

## FAQ

### Q: How do I rollback a deployment?
A: Use the rollback command:
```bash
fraisier rollback my_api production --to previous
```

### Q: Can I deploy to multiple environments at once?
A: No, but you can script it:
```bash
for env in staging production; do
  fraisier deploy my_api $env
done
```

### Q: How do I skip health checks?
A: You can't (by design). Health checks are critical for safety. Fix the health check endpoint instead.

### Q: What happens if a deployment fails?
A: Fraisier automatically:
1. Stops the failed deployment
2. Reverts any changes (git, migrations rolled back)
3. Restarts the previous version
4. Sends notification

### Q: How do I deploy a specific branch?
A: Set the branch in configuration:
```yaml
environments:
  production:
    branch: develop  # Deploy from develop instead of main
```

### Q: Can I test deployments without actually deploying?
A: Yes, use dry-run mode:
```bash
fraisier deploy my_api production --dry-run
```

### Q: How do I monitor a deployment?
A: Use the status and logs commands:
```bash
fraisier status my_api production
fraisier logs my_api production --lines 100
```

### Q: What if I need to manually intervene?
A: You can SSH to the server and fix issues manually:
```bash
ssh deploy@prod.example.com
sudo systemctl restart my-api.service
```

Then update deployment status in database.

---

## Support & Contributing

For issues, questions, or contributions:

- **GitHub Issues**: https://github.com/fraiseql/fraisier/issues
- **Documentation**: https://github.com/fraiseql/fraisier/docs
- **Discussions**: https://github.com/fraiseql/fraisier/discussions

---

## Version Info

- **Fraisier**: v0.1.0
- **FraiseQL**: v2.0.0+
- **Python**: 3.11+
- **Last Updated**: 2026-01-22
