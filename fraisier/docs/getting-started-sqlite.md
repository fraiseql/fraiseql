# Getting Started with Fraisier + SQLite

**Perfect For**: Local development, testing, small deployments, embedded systems

**Database**: SQLite (serverless, file-based, zero configuration)

**Time to Production**: 5-10 minutes

---

## Overview

SQLite is ideal for:

- ✅ Local development on laptop
- ✅ CI/CD testing environments
- ✅ Small production deployments (< 100k requests/day)
- ✅ Learning and experimenting with Fraisier
- ✅ Microservices on edge devices

It's not ideal for:

- ❌ High-concurrency applications (> 1000 concurrent users)
- ❌ Deployments requiring automatic failover
- ❌ Applications needing complex transactions

---

## Installation

### Prerequisites

- Python 3.11+
- Git
- pip or uv

### Step 1: Clone Repository

```bash
git clone https://github.com/your-org/fraisier.git
cd fraisier
```

### Step 2: Create Virtual Environment

```bash
# Using venv
python3 -m venv venv
source venv/bin/activate

# Or using uv (faster)
uv venv
source .venv/bin/activate
```

### Step 3: Install Fraisier

```bash
# Development install
pip install -e ".[dev]"

# Or just runtime
pip install -e .
```

### Step 4: Verify Installation

```bash
fraisier --version
# Output: fraisier 0.1.0
```

---

## Configuration

### Create Configuration File

Create `fraises.yaml` in your project root:

```yaml
# fraises.yaml
database:
  type: sqlite
  path: ./fraisier.db

fraises:
  my_api:
    type: api
    description: My Python API service
    git_provider: github
    git_repo: your-org/my-api
    git_branch: main

    environments:
      development:
        provider: docker_compose
        provider_config:
          docker_compose_file: ./docker-compose.dev.yml
          service: api
          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 10
            max_retries: 3

      staging:
        provider: docker_compose
        provider_config:
          docker_compose_file: ./docker-compose.staging.yml
          service: api

      production:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: prod-server.example.com
              username: deploy
              ssh_key_path: ~/.ssh/id_ed25519
          service_name: my-api
          app_path: /opt/my-api
          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 10

  my_worker:
    type: etl
    description: Background job worker
    git_provider: github
    git_repo: your-org/my-worker
    git_branch: main

    environments:
      staging:
        provider: docker_compose
        provider_config:
          docker_compose_file: ./docker-compose.staging.yml
          service: worker

      production:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: prod-server.example.com
          service_name: my-worker
```

### Environment Variables

Create `.env` file (optional, for overrides):

```bash
# Database
FRAISIER_DATABASE=sqlite
FRAISIER_DB_PATH=./fraisier.db

# NATS (optional, for event bus)
NATS_SERVERS=nats://localhost:4222
NATS_TIMEOUT=5

# Logging
FRAISIER_LOG_LEVEL=INFO

# Output
FRAISIER_JSON_OUTPUT=false
FRAISIER_VERBOSE=false
```

Load environment variables:

```bash
set -a
source .env
set +a
```

---

## Database Setup

### Initialize Database

```bash
# Create SQLite database with schema
fraisier db init

# Output:
# ✓ Database initialized: fraisier.db
# ✓ Schema created
# ✓ Ready for deployments
```

### Verify Database

```bash
# Check database status
fraisier db status

# Output:
# Database Type: SQLite
# Path: ./fraisier.db
# Size: 256 KB
# Status: ✓ Healthy
```

### Inspect Database

```bash
# View deployment history using sqlite3
sqlite3 fraisier.db "SELECT * FROM tb_deployment LIMIT 5;"

# View using Fraisier
fraisier history my_api
```

---

## First Deployment

### Verify Configuration

```bash
# Validate fraises.yaml
fraisier config validate

# Output:
# Configuration valid ✓
#
# Services Found: 2
# ├─ my_api (api)
# └─ my_worker (etl)
#
# Environments: 3
# ├─ development
# ├─ staging
# └─ production
```

### List Services

```bash
fraisier list

# Output:
# Services (2 total):
#
# 1. my_api ........... api service
#    │ Environments: development, staging, production
#    │ Status: Not deployed
#
# 2. my_worker ....... background job worker
#    │ Environments: staging, production
#    │ Status: Not deployed
```

### Perform Test Deployment

```bash
# Dry-run to see what would happen
fraisier deploy my_api development --dry-run

# Output:
# Deployment Plan for my_api → development
#
# Provider: docker_compose
# Strategy: rolling
# Health Checks: http://localhost:8000/health
#
# Steps:
# 1. Pull latest code from GitHub
# 2. Build Docker image
# 3. Stop running containers
# 4. Start new containers
# 5. Run health checks (up to 3 attempts)
# 6. Complete deployment
#
# Estimated time: 2-3 minutes
#
# No changes would be made (dry-run mode)
```

### Execute Actual Deployment

```bash
# Deploy with confirmation
fraisier deploy my_api development

# Output (interactive):
# Deploy my_api to development? (yes/no): yes
#
# Starting deployment...
# ✓ Code pulled (3 commits)
# ✓ Docker image built (2.5s)
# ✓ Containers stopped (1.2s)
# ✓ Containers started (4.3s)
# ✓ Health checks passed (50ms)
#
# ✅ Deployment successful in 11.1 seconds
#
# Deployment ID: dep_00001
# Version: 2.0.0
# Status: success
```

### View Deployment Details

```bash
# Check deployment status
fraisier status my_api development

# Output:
# my_api / development
# ├─ Status: Healthy
# ├─ Current Version: 2.0.0
# ├─ Previous Version: 1.9.5
# ├─ Deployment Strategy: rolling
# ├─ Last Deployment: just now (success)
# ├─ Health Checks: 3/3 passing
# └─ Instances: 1/1 healthy

# View logs
fraisier logs dep_00001

# View history
fraisier history my_api development
```

---

## Common Workflows

### Deploy Latest Version

```bash
# Deploy to development
fraisier deploy my_api development

# Deploy to staging
fraisier deploy my_api staging

# Deploy to production
fraisier deploy my_api production --wait --timeout 600
```

### Deploy Specific Version

```bash
# Deploy v2.0.0 specifically
fraisier deploy my_api production --version 2.0.0

# Verify version
fraisier status my_api production
```

### Rollback to Previous Version

```bash
# Rollback if something goes wrong
fraisier rollback my_api production

# Rollback to specific version
fraisier rollback my_api production --to-version 1.9.5 --reason "Bug in 2.0.0"
```

### Deploy Multiple Services

```bash
#!/bin/bash
# Deploy all services to staging

for service in $(fraisier list --json | jq -r '.services[].name'); do
  echo "Deploying $service..."
  fraisier deploy $service staging --wait
done
```

### Watch Deployment Progress

```bash
# Deploy and wait
fraisier deploy my_api production --wait --timeout 600

# Or watch status in another terminal
fraisier status my_api production --watch
```

---

## Health Checks

### Configure Health Checks

In `fraises.yaml`:

```yaml
environments:
  production:
    provider: docker_compose
    provider_config:
      service: api
      health_check:
        type: http                    # or tcp
        url: http://localhost:8000/health
        timeout: 10                   # seconds
        max_retries: 3                # attempt up to 3 times
        retry_delay: 5                # wait 5s between retries
```

### Implementation

Your service should respond to health checks:

```python
# Flask example
from flask import Flask

app = Flask(__name__)

@app.route('/health')
def health():
    return {
        'status': 'healthy',
        'version': '2.0.0'
    }, 200
```

### Test Health Checks

```bash
# Manually test
curl http://localhost:8000/health
# Output: {"status":"healthy","version":"2.0.0"}

# Test during deployment
fraisier deploy my_api development
# Health checks: 3/3 passing ✓
```

---

## Monitoring & Logs

### View Recent Logs

```bash
# Last 50 lines
fraisier logs dep_00001

# Last 100 lines with errors only
fraisier logs dep_00001 --lines 100 --level error

# Follow logs in real-time
fraisier logs dep_00001 --follow
```

### Check Service Status

```bash
# Quick status
fraisier status my_api development

# Detailed status
fraisier status my_api development --long

# All services
fraisier health

# Watch with auto-update
fraisier health --watch
```

### View Database

```bash
# Via Fraisier
fraisier history my_api                    # All environments
fraisier history my_api development        # Specific environment
fraisier history my_api development --limit 50

# Via SQLite CLI
sqlite3 fraisier.db
sqlite> SELECT * FROM tb_deployment;
sqlite> SELECT * FROM tb_fraise_state;
```

---

## Backup & Recovery

### Backup Database

```bash
# Simple file copy
cp fraisier.db fraisier.backup.db

# Using Fraisier
fraisier db backup --output backup.sql

# Automated daily backups (cron)
0 2 * * * fraisier db backup --output /backups/fraisier-$(date +\%Y-\%m-\%d).sql
```

### Restore from Backup

```bash
# Restore from backup
fraisier db restore --input backup.sql

# Verify restoration
fraisier history my_api
```

---

## Upgrading Fraisier

### Check Version

```bash
fraisier --version
# fraisier 0.1.0
```

### Upgrade to Latest

```bash
pip install --upgrade fraisier

# Verify
fraisier --version
```

### Database Migrations

```bash
# Run any pending migrations
fraisier db migrate

# Check status
fraisier db status
```

---

## Troubleshooting

### Issue: "fraisier" command not found

```bash
# Ensure venv is activated
source venv/bin/activate

# Or install using full path
/path/to/venv/bin/fraisier deploy my_api development
```

### Issue: Database locked

```bash
# SQLite locks database during writes
# This is normal - wait a moment and retry

# If stuck, restart Fraisier
pkill -f fraisier

# Remove lock file if exists
rm fraisier.db-wal
rm fraisier.db-shm
```

### Issue: Deployment timeout

```bash
# Increase timeout
fraisier deploy my_api production --timeout 1200

# Or check if service is actually running
curl http://localhost:8000/health
```

### Issue: Health checks failing

```bash
# Verify endpoint is responding
curl http://localhost:8000/health

# Check service logs
docker-compose logs api

# Increase health check timeout
# In fraises.yaml, increase timeout value
```

### View Detailed Logs

```bash
# Enable debug logging
FRAISIER_LOG_LEVEL=DEBUG fraisier deploy my_api development

# Check SQLite database
sqlite3 fraisier.db "SELECT * FROM tb_deployment ORDER BY created_at DESC LIMIT 1;"
```

---

## Next Steps

### 1. Set Up Webhooks (Optional)

Integrate with Slack, Discord, or PagerDuty:

```bash
fraisier webhook add \
  --event deployment.completed \
  --event deployment.failed \
  https://hooks.slack.com/services/YOUR/WEBHOOK/URL
```

### 2. Configure NATS (Optional)

For event-driven integrations:

```bash
docker run -d -p 4222:4222 nats:latest -js

export NATS_SERVERS=nats://localhost:4222
fraisier deploy my_api development
```

### 3. Add Monitoring (Optional)

Set up Prometheus and Grafana:

```bash
docker-compose up -d prometheus grafana
```

### 4. Automate Deployments

Add to GitHub Actions:

```yaml
# .github/workflows/deploy.yml
name: Deploy

on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-python@v4
      - run: pip install fraisier
      - run: fraisier deploy my_api production --wait
```

### 5. Move to PostgreSQL

When you outgrow SQLite:

See [getting-started-postgres.md](getting-started-postgres.md)

---

## Reference

- **Configuration**: [fraises.yaml](fraises.example.yaml)
- **CLI Commands**: [cli-reference.md](cli-reference.md)
- **API Documentation**: [api-reference.md](api-reference.md)
- **Troubleshooting**: [troubleshooting.md](troubleshooting.md)

---

## Tips & Best Practices

1. **Use Dry-Run Before Production**

   ```bash
   fraisier deploy my_api production --dry-run
   ```

2. **Always Wait for Staging**

   ```bash
   fraisier deploy my_api staging --wait --timeout 600
   ```

3. **Keep Database Backed Up**

   ```bash
   # Daily backups
   0 2 * * * cp /path/to/fraisier.db /backups/fraisier-$(date +\%Y-\%m-\%d).db
   ```

4. **Monitor Health Checks**

   ```bash
   fraisier status my_api production --watch
   ```

5. **Review Deployment History**

   ```bash
   fraisier history my_api production --limit 100
   ```

---

## Getting Help

- **Documentation**: Read the docs in `docs/` directory
- **Issues**: Report on GitHub: https://github.com/your-org/fraisier/issues
- **Discussion**: Ask on Discord: https://discord.gg/your-invite
- **Email**: support@fraisier.dev

---

**Ready to deploy?**

```bash
fraisier deploy my_api production
```

Happy deploying! 🚀
