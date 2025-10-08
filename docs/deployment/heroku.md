# Heroku Deployment Guide

## Overview

Deploy FraiseQL to Heroku in minutes with managed PostgreSQL, automatic SSL, and simple scaling. Perfect for prototypes, MVPs, and small to medium production applications.

## Prerequisites

- Heroku account (free tier available)
- Heroku CLI installed
- Git repository initialized
- Credit card (for add-ons, even free ones)

## Quick Deploy

### One-Click Deploy Button

Add this to your README for instant deployment:

```markdown
[![Deploy to Heroku](https://www.herokucdn.com/deploy/button.svg)](https://heroku.com/deploy?template=https://github.com/your-org/fraiseql)
```

### app.json Configuration

```json
{
  "name": "FraiseQL API",
  "description": "Production-ready GraphQL API with PostgreSQL",
  "repository": "https://github.com/your-org/fraiseql",
  "logo": "https://your-logo-url.png",
  "keywords": ["graphql", "api", "postgresql", "python"],
  "stack": "heroku-22",
  "env": {
    "FRAISEQL_MODE": {
      "description": "Application mode",
      "value": "production"
    },
    "SECRET_KEY": {
      "description": "Secret key for sessions",
      "generator": "secret"
    },
    "JWT_SECRET": {
      "description": "JWT signing secret",
      "generator": "secret"
    },
    "CORS_ORIGINS": {
      "description": "Comma-separated list of allowed origins",
      "value": "https://yourdomain.com",
      "required": false
    },
    "LOG_LEVEL": {
      "description": "Logging level",
      "value": "INFO"
    }
  },
  "addons": [
    {
      "plan": "heroku-postgresql:mini",
      "options": {
        "version": "15"
      }
    },
    {
      "plan": "heroku-redis:mini"
    }
  ],
  "buildpacks": [
    {
      "url": "heroku/python"
    }
  ],
  "formation": {
    "web": {
      "quantity": 1,
      "size": "basic"
    }
  },
  "scripts": {
    "postdeploy": "python -m fraiseql migrate"
  }
}
```

## Manual Deployment

### Step 1: Create Heroku App

```bash
# Login to Heroku
heroku login

# Create new app
heroku create fraiseql-api

# Or create with specific region
heroku create fraiseql-api --region eu
```

### Step 2: Add PostgreSQL

```bash
# Add PostgreSQL (mini plan for production)
heroku addons:create heroku-postgresql:mini

# Wait for provisioning
heroku pg:wait

# Check database info
heroku pg:info
```

### Step 3: Add Redis

```bash
# Add Redis for caching
heroku addons:create heroku-redis:mini

# Get Redis URL
heroku config:get REDIS_URL
```

### Step 4: Configure Environment

```bash
# Set environment variables
heroku config:set FRAISEQL_MODE=production
heroku config:set SECRET_KEY=$(openssl rand -hex 32)
heroku config:set JWT_SECRET=$(openssl rand -hex 32)
heroku config:set LOG_LEVEL=INFO
heroku config:set CORS_ORIGINS=https://yourdomain.com
heroku config:set MAX_CONNECTIONS=20
heroku config:set STATEMENT_TIMEOUT=30000
heroku config:set QUERY_COMPLEXITY_LIMIT=1000

# View all config
heroku config
```

## Application Configuration

### Procfile

```procfile
web: uvicorn src.fraiseql.main:app --host 0.0.0.0 --port $PORT --workers 4
release: python -m fraiseql migrate
worker: celery -A src.fraiseql.worker worker --loglevel=info
```

### runtime.txt

```
python-3.11.7
```

### requirements.txt

```txt
# Core dependencies
uvicorn[standard]==0.24.0
fastapi==0.104.1
graphql-core==3.2.3
sqlalchemy==2.0.23
asyncpg==0.29.0
redis==5.0.1
pydantic==2.5.0

# Production dependencies
gunicorn==21.2.0
psycopg2-binary==2.9.9
python-jose[cryptography]==3.3.0
python-multipart==0.0.6
httpx==0.25.2
sentry-sdk==1.38.0

# Monitoring
prometheus-client==0.19.0
opentelemetry-api==1.21.0
opentelemetry-sdk==1.21.0
```

### Database Configuration

```python
# src/fraiseql/config.py
import os
import dj_database_url

# Parse DATABASE_URL from Heroku
DATABASE_URL = os.environ.get('DATABASE_URL')

# Heroku uses postgres:// but SQLAlchemy needs postgresql://
if DATABASE_URL and DATABASE_URL.startswith('postgres://'):
    DATABASE_URL = DATABASE_URL.replace('postgres://', 'postgresql://', 1)

# Parse database configuration
db_config = dj_database_url.parse(DATABASE_URL)

# Configure SQLAlchemy
SQLALCHEMY_DATABASE_URL = DATABASE_URL
SQLALCHEMY_ENGINE_OPTIONS = {
    'pool_size': 10,
    'max_overflow': 5,
    'pool_timeout': 30,
    'pool_recycle': 1800,
    'pool_pre_ping': True,
    'connect_args': {
        'server_settings': {
            'jit': 'off'
        },
        'command_timeout': 60,
        'options': '-c statement_timeout=30000'
    }
}
```

## Deployment

### Using Git

```bash
# Add Heroku remote
heroku git:remote -a fraiseql-api

# Deploy main branch
git push heroku main

# Deploy different branch
git push heroku feature-branch:main

# View logs
heroku logs --tail
```

### Using GitHub Integration

1. Connect GitHub repo in Heroku Dashboard
2. Enable automatic deploys
3. Optional: Enable review apps

```yaml
# app.json for review apps
{
  "name": "FraiseQL Review App",
  "scripts": {
    "postdeploy": "python -m fraiseql migrate --seed"
  },
  "env": {
    "FRAISEQL_MODE": {
      "value": "staging"
    }
  },
  "formation": {
    "web": {
      "quantity": 1,
      "size": "hobby"
    }
  },
  "addons": [
    "heroku-postgresql:mini",
    "heroku-redis:mini"
  ]
}
```

### Using Docker

```dockerfile
# heroku.yml
build:
  docker:
    web: Dockerfile
    worker: Dockerfile.worker
  config:
    DOCKER_BUILDKIT: 1
release:
  command:

    - python -m fraiseql migrate
  image: web
run:
  web: uvicorn src.fraiseql.main:app --host 0.0.0.0 --port $PORT
  worker: celery -A src.fraiseql.worker worker
```

```bash
# Set stack to container
heroku stack:set container

# Deploy
git push heroku main
```

## Database Management

### Migrations

```bash
# Run migrations
heroku run python -m fraiseql migrate

# Create migration
heroku run python -m fraiseql makemigrations

# Rollback migration
heroku run python -m fraiseql migrate --rollback

# Access database shell
heroku pg:psql
```

### Backup & Restore

```bash
# Create manual backup
heroku pg:backups:capture

# Schedule daily backups
heroku pg:backups:schedule DATABASE_URL --at '02:00 America/New_York'

# List backups
heroku pg:backups

# Download backup
heroku pg:backups:download

# Restore from backup
heroku pg:backups:restore b001 DATABASE_URL

# Copy to another app
heroku pg:copy fraiseql-api::DATABASE_URL fraiseql-staging::DATABASE_URL
```

## Scaling

### Dyno Scaling

```bash
# Scale web dynos
heroku ps:scale web=2:standard-1x

# Scale worker dynos
heroku ps:scale worker=1:standard-1x

# View current scale
heroku ps

# Enable autoscaling (requires Metrics add-on)
heroku autoscale:enable web --min 2 --max 10 --p95 500
```

### Database Scaling

```bash
# Upgrade database plan
heroku addons:upgrade heroku-postgresql:standard-0

# Add follower (read replica)
heroku addons:create heroku-postgresql:standard-0 --follow DATABASE_URL

# Promote follower to primary
heroku pg:promote HEROKU_POSTGRESQL_AMBER_URL
```

## Custom Domain & SSL

```bash
# Add custom domain
heroku domains:add api.example.com

# Add wildcard domain
heroku domains:add *.example.com

# View DNS target
heroku domains

# SSL is automatic with ACM
heroku certs:auto:enable
```

### DNS Configuration

Add CNAME record:
```
Type: CNAME
Name: api
Value: fraiseql-api.herokuapp.com
```

Or for root domain:
```
Type: ALIAS/ANAME
Name: @
Value: fraiseql-api.herokuapp.com
```

## Monitoring

### Application Metrics

```bash
# Enable metrics
heroku labs:enable "runtime-dyno-metadata"
heroku labs:enable "log-runtime-metrics"

# View metrics in dashboard
heroku addons:open metrics
```

### Logging

```bash
# View recent logs
heroku logs --num 100

# Stream logs
heroku logs --tail

# Filter by dyno type
heroku logs --dyno web --tail

# Filter by source
heroku logs --source app --tail
```

### External Monitoring

```bash
# Add New Relic
heroku addons:create newrelic:wayne

# Add Sentry
heroku addons:create sentry:f0

# Add Papertrail for logs
heroku addons:create papertrail:choklad

# Add Scout APM
heroku addons:create scout:chair
```

### Health Checks

```python
# src/fraiseql/health.py
from fastapi import FastAPI
from sqlalchemy import text
import redis

app = FastAPI()

@app.get("/health")
async def health_check():
    """Basic health check for Heroku"""
    return {"status": "healthy", "dyno": os.environ.get("DYNO")}

@app.get("/ready")
async def readiness_check():
    """Detailed readiness check"""
    checks = {}

    # Check database
    try:
        async with get_db() as db:
            await db.execute(text("SELECT 1"))
        checks["database"] = "ok"
    except Exception as e:
        checks["database"] = f"error: {str(e)}"

    # Check Redis
    try:
        r = redis.from_url(os.environ.get("REDIS_URL"))
        r.ping()
        checks["redis"] = "ok"
    except Exception as e:
        checks["redis"] = f"error: {str(e)}"

    status = "ready" if all(v == "ok" for v in checks.values()) else "not ready"
    return {"status": status, "checks": checks}
```

## Performance Optimization

### Preboot

```bash
# Enable preboot for zero-downtime deploys
heroku features:enable preboot
```

### Build Optimization

```python
# setup.cfg
[install]
compile = yes
optimize = 2
```

### Database Connection Pooling

```python
# Optimize for Heroku's connection limits
import os

# Heroku standard-0 has 120 connections
# Reserve some for maintenance
MAX_CONNECTIONS = int(os.environ.get('MAX_CONNECTIONS', '20'))
POOL_SIZE = min(MAX_CONNECTIONS // int(os.environ.get('WEB_CONCURRENCY', '4')), 5)
```

### Static Files with WhiteNoise

```python
# src/fraiseql/main.py
from whitenoise import WhiteNoise

app = FastAPI()
app.mount("/static", WhiteNoise(
    directory="static",
    max_age=31536000,
    compress=True
))
```

## Cost Optimization

### Heroku Pricing Tiers

| Dyno Type | Memory | Price/Month | Use Case |
|-----------|--------|-------------|----------|
| Eco | 512MB | $5 | Development |
| Basic | 512MB | $7 | Low traffic |
| Standard-1X | 512MB | $25 | Production |
| Standard-2X | 1GB | $50 | High traffic |
| Performance-M | 2.5GB | $250 | Enterprise |
| Performance-L | 14GB | $500 | Heavy load |

### Database Plans

| Plan | Connections | Storage | Price/Month |
|------|-------------|---------|-------------|
| Mini | 20 | 1GB | $5 |
| Basic | 20 | 10GB | $9 |
| Standard-0 | 120 | 64GB | $50 |
| Standard-2 | 400 | 256GB | $200 |
| Premium-0 | 500 | 512GB | $350 |

### Total Monthly Cost Estimate

**Small Production:**

- 2× Basic dynos: $14
- Standard-0 database: $50
- Mini Redis: $5
- **Total: ~$69/month**

**Medium Production:**

- 3× Standard-1X dynos: $75
- Standard-2 database: $200
- Premium-0 Redis: $15
- **Total: ~$290/month**

## Troubleshooting

### Common Issues

#### Application Error (H10)

```bash
# Check logs for crash
heroku logs --tail

# Restart dynos
heroku restart

# Check dyno status
heroku ps
```

#### Request Timeout (H12)

```bash
# Increase timeout (max 30s)
heroku config:set STATEMENT_TIMEOUT=30000

# Optimize slow queries
heroku pg:diagnose
heroku pg:outliers
```

#### Memory Quota Exceeded (R14)

```bash
# Check memory usage
heroku logs --dyno web --grep "Memory quota"

# Scale to larger dyno
heroku ps:resize web=standard-2x
```

#### Too Many Connections (R13)

```bash
# Check connection count
heroku pg:info

# Reduce pool size
heroku config:set MAX_CONNECTIONS=10

# Upgrade database plan
heroku addons:upgrade heroku-postgresql:standard-2
```

### Debug Mode

```bash
# Enable debug logging
heroku config:set LOG_LEVEL=DEBUG

# Access shell
heroku run bash

# Run Python shell
heroku run python

# Test database connection
heroku run python -c "
import os
import psycopg2
conn = psycopg2.connect(os.environ['DATABASE_URL'])
print('Connected!')
"
```

## Security Best Practices

1. **Force HTTPS**
   ```python
   from fastapi.middleware.httpsredirect import HTTPSRedirectMiddleware
   app.add_middleware(HTTPSRedirectMiddleware)
   ```

2. **Set Security Headers**
   ```python
   from fastapi.middleware.trustedhost import TrustedHostMiddleware
   app.add_middleware(
       TrustedHostMiddleware,
       allowed_hosts=["*.herokuapp.com", "yourdomain.com"]
   )
   ```

3. **Enable Rate Limiting**
   ```python
   from slowapi import Limiter
   limiter = Limiter(key_func=get_remote_address)
   app.state.limiter = limiter
   ```

4. **Rotate Credentials**
   ```bash
   heroku pg:credentials:rotate
   ```

5. **Enable MFA**

   - Enable in Heroku account settings
   - Require for all team members

## Maintenance Mode

```bash
# Enable maintenance mode
heroku maintenance:on

# Custom maintenance page
heroku config:set MAINTENANCE_PAGE_URL=https://example.com/maintenance

# Disable maintenance mode
heroku maintenance:off
```

## Next Steps

1. Set up [monitoring](./monitoring.md) with add-ons
2. Configure [auto-scaling](./scaling.md)
3. Review [production checklist](./production-checklist.md)
4. Consider upgrading to [enterprise Heroku](https://www.heroku.com/enterprise)
