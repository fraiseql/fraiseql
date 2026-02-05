# Fraisier Deployment Examples

This document provides real-world deployment examples for common scenarios.

## Table of Contents

1. [Simple API Deployment (Bare Metal)](#simple-api-deployment-bare-metal)
2. [Container Deployment (Docker Compose)](#container-deployment-docker-compose)
3. [PaaS Deployment (Coolify)](#paas-deployment-coolify)
4. [Multi-Service Deployment](#multi-service-deployment)
5. [Rollback Scenarios](#rollback-scenarios)
6. [CI/CD Integration](#cicd-integration)

---

## Simple API Deployment (Bare Metal)

### Configuration

```yaml
fraises:
  api:
    type: api
    description: GraphQL API
    environments:
      production:
        name: api.prod.example.com
        branch: main
        provider: bare_metal
        bare_metal:
          host: prod.example.com
          username: deploy
          port: 22
          key_path: ~/.ssh/id_fraisier
        systemd_service: api.service
        database:
          name: api_production
          strategy: apply
          backup_before_deploy: true
        health_check:
          type: http
          url: https://api.prod.example.com/graphql
          timeout: 30
          retries: 3
```

### systemd Service File

Create `/etc/systemd/system/api.service`:

```ini
[Unit]
Description=Fraisier API Service
After=network.target postgresql.service

[Service]
Type=simple
User=deploy
WorkingDirectory=/var/www/api
ExecStart=/opt/venv/bin/python -m uvicorn main:app --host 0.0.0.0 --port 8000
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
RestartSec=10

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=api

# Resource limits
LimitNOFILE=65536
LimitNPROC=4096

[Install]
WantedBy=multi-user.target
```

### Deployment Execution

```bash
# 1. Deploy
$ fraisier deploy api production
✓ Connecting to prod.example.com:22
✓ Verifying credentials
✓ Getting current version (abc123)
✓ Getting latest version (def456)
✓ Pulling code from git
✓ Running database migrations
  - 001_create_tables.sql
  - 002_create_views.sql
✓ Restarting service
✓ Running health checks (attempt 1/3)
  HTTP GET https://api.prod.example.com/graphql
  Status: 200 OK
✓ Deployment successful
  Duration: 45 seconds
  Old version: abc123
  New version: def456

# 2. Verify
$ fraisier status api production
api (production)
  Status: healthy
  Version: def456
  Last deployed: 2 minutes ago
  Uptime: 45s

# 3. View logs
$ fraisier logs api production --lines 50
api.service[12345]: Starting GraphQL API
api.service[12345]: Connected to PostgreSQL
api.service[12345]: Listening on 0.0.0.0:8000
```

---

## Container Deployment (Docker Compose)

### docker-compose.prod.yml

```yaml
version: '3.9'

services:
  web:
    image: registry.example.com/api:latest
    pull_policy: always
    restart: always
    ports:
      - "8000:8000"
    environment:
      DATABASE_URL: postgresql://db/api
      ENVIRONMENT: production
      LOG_LEVEL: info
    depends_on:
      - db
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 10s
      timeout: 5s
      retries: 3
      start_period: 10s
    volumes:
      - ./logs:/app/logs
    networks:
      - app-network

  db:
    image: postgres:15-alpine
    restart: always
    environment:
      POSTGRES_DB: api
      POSTGRES_USER: api
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - db-data:/var/lib/postgresql/data
    networks:
      - app-network
    healthcheck:
      test: ["CMD", "pg_isready", "-U", "api"]
      interval: 10s
      timeout: 5s
      retries: 3

  redis:
    image: redis:7-alpine
    restart: always
    networks:
      - app-network
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 3

volumes:
  db-data:

networks:
  app-network:
    driver: bridge
```

### Configuration

```yaml
fraises:
  api:
    type: api
    environments:
      production:
        provider: docker_compose
        docker_compose:
          compose_file: docker-compose.prod.yml
          project_name: api
          timeout: 300
        health_check:
          type: http
          url: http://localhost:8000/health
          timeout: 10
          retries: 5
```

### Deployment Execution

```bash
# 1. Deploy
$ fraisier deploy api production
✓ Verifying Docker and docker-compose
✓ Pulling latest images
  registry.example.com/api:latest
  postgres:15-alpine
  redis:7-alpine
✓ Starting containers
✓ Waiting for services to be ready
  web: running ✓
  db: running ✓
  redis: running ✓
✓ Running health checks (attempt 1/5)
  HTTP GET http://localhost:8000/health
  Status: 200 OK
✓ Deployment successful
  Duration: 30 seconds

# 2. Scale service (increase replicas)
$ docker-compose -f docker-compose.prod.yml up -d --scale web=3
api_web_1
api_web_2
api_web_3

# 3. Check logs
$ fraisier logs api production
api_web_1  | 2026-01-22 12:00:00 INFO Starting application
api_web_1  | 2026-01-22 12:00:01 INFO Connected to database
api_web_1  | 2026-01-22 12:00:02 INFO Listening on 0.0.0.0:8000
```

---

## PaaS Deployment (Coolify)

### Coolify Application Setup

1. Create application in Coolify UI
2. Connect Git repository
3. Configure environment variables
4. Set up webhooks

### Configuration

```yaml
fraises:
  api:
    type: api
    environments:
      production:
        provider: coolify
        coolify:
          api_url: https://coolify.example.com/api
          api_token: ${COOLIFY_API_TOKEN}
          application_id: app-123456
          timeout: 600
        health_check:
          type: http
          url: https://api.prod.example.com/health
          timeout: 30
          retries: 3
```

### Deployment Execution

```bash
# 1. Deploy
$ COOLIFY_API_TOKEN=token-xxx fraisier deploy api production
✓ Authenticating with Coolify API
✓ Triggering deployment for app-123456
  Deployment ID: deploy-789
  Branch: main
✓ Waiting for deployment to complete (polling every 10s)
  Status: queued
  Status: running
  Status: building
  Status: deploying
  Status: success ✓
✓ Duration: 2 minutes 15 seconds
✓ Deployment successful

# 2. View recent deployments
$ fraisier history api production --limit 5
  deploy-789 | success    | main    | 2026-01-22 12:00:00 | 2m15s
  deploy-788 | success    | main    | 2026-01-22 11:30:00 | 2m10s
  deploy-787 | failed     | main    | 2026-01-22 11:00:00 | 1m45s
  deploy-786 | success    | main    | 2026-01-22 10:30:00 | 2m00s

# 3. View logs
$ fraisier logs api production
[Stage 1/4] Building...
[Stage 2/4] Running migrations...
[Stage 3/4] Starting services...
[Stage 4/4] Running health checks...
✓ All health checks passed
✓ Deployment complete
```

---

## Multi-Service Deployment

### Configuration

```yaml
fraises:
  api:
    type: api
    environments:
      production:
        provider: bare_metal
        bare_metal:
          host: api.prod.example.com
          username: deploy
        systemd_service: api.service
        health_check:
          type: http
          url: https://api.prod.example.com/health

  etl:
    type: etl
    environments:
      production:
        provider: bare_metal
        bare_metal:
          host: etl.prod.example.com
          username: deploy
        script_path: /var/www/etl/run.py

  scheduled:
    type: scheduled
    environments:
      production:
        provider: bare_metal
        bare_metal:
          host: jobs.prod.example.com
          username: deploy
        jobs:
          daily_stats:
            systemd_service: stats.timer
            schedule: "0 2 * * *"
          weekly_report:
            systemd_service: report.timer
            schedule: "0 8 * * 1"
```

### Coordinated Deployment

```bash
# Deploy all services in order
$ ./deploy-all.sh

#!/bin/bash
set -e

echo "=== Deploying API ==="
fraisier deploy api production || exit 1

echo "=== Deploying ETL ==="
fraisier deploy etl production || exit 1

echo "=== Deploying Scheduled Jobs ==="
fraisier deploy scheduled production || exit 1

echo "=== Verifying all services ==="
fraisier status api production
fraisier status etl production
fraisier status scheduled production

echo "✓ All services deployed successfully"
```

---

## Rollback Scenarios

### Quick Rollback (Previous Version)

```bash
# Something went wrong, rollback immediately
$ fraisier rollback api production --to previous

# Verify
$ fraisier status api production
```

### Specific Version Rollback

```bash
# View history
$ fraisier history api production --limit 10

# Rollback to specific deployment
$ fraisier rollback api production --to deploy-786

# Verify
$ fraisier logs api production --lines 50
```

### Manual Rollback (If Fraisier is broken)

```bash
# SSH to server
$ ssh deploy@prod.example.com

# Check current code
$ cd /var/www/api
$ git log -1

# Revert to previous version
$ git revert HEAD
$ git log -1

# Restart service
$ sudo systemctl restart api.service

# Verify
$ curl https://api.prod.example.com/health
```

---

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Deploy

on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install Fraisier
        run: pip install fraisier

      - name: Configure Fraisier
        env:
          DATABASE_URL: ${{ secrets.DATABASE_URL }}
          FRAISIER_WEBHOOK_SECRET: ${{ secrets.WEBHOOK_SECRET }}
        run: |
          mkdir -p ~/.ssh
          echo "${{ secrets.SSH_KEY }}" > ~/.ssh/id_fraisier
          chmod 600 ~/.ssh/id_fraisier

      - name: Deploy API
        run: fraisier deploy api production

      - name: Verify Deployment
        run: |
          fraisier status api production
          sleep 10
          curl -f https://api.prod.example.com/health

      - name: Notify Slack
        if: success()
        uses: slackapi/slack-github-action@v1
        with:
          webhook-url: ${{ secrets.SLACK_WEBHOOK }}
          payload: |
            {
              "text": "✓ Deployment successful",
              "blocks": [{
                "type": "section",
                "text": {
                  "type": "mrkdwn",
                  "text": "*Fraisier Deployment*\nAPI deployed to production\n${{ github.event.head_commit.message }}"
                }
              }]
            }
```

### GitLab CI Example

```yaml
stages:
  - test
  - deploy

deploy:production:
  stage: deploy
  image: python:3.11
  script:
    - pip install fraisier
    - mkdir -p ~/.ssh && echo "$SSH_KEY" > ~/.ssh/id_fraisier && chmod 600 ~/.ssh/id_fraisier
    - fraisier deploy api production
    - sleep 10
    - curl -f https://api.prod.example.com/health || exit 1
  only:
    - main
  environment:
    name: production
    url: https://api.prod.example.com
```

---

## Common Issues & Solutions

### Issue: "Health check failed after deployment"

```bash
# Check application logs
$ fraisier logs api production

# Check health endpoint directly
$ curl -v https://api.prod.example.com/health

# Increase health check timeout and retries
# Update fraises.yaml:
# health_check:
#   timeout: 60
#   retries: 5
```

### Issue: "Git pull failed"

```bash
# Check SSH access
$ ssh deploy@prod.example.com "cd /var/www/api && git fetch origin"

# Verify SSH key is added to agent
$ ssh-add -l

# Check repository URL
$ ssh deploy@prod.example.com "cd /var/www/api && git remote -v"
```

### Issue: "Database migration failed"

```bash
# Check database connection
$ fraisier db-check

# Manually run migration
$ ssh deploy@prod.example.com "cd /var/www/api && python manage.py migrate"

# Rollback if needed
$ fraisier rollback api production
```

---

## Monitoring After Deployment

```bash
# Watch deployment in real-time
$ watch -n 2 "fraisier status api production"

# Monitor error rate
$ while true; do
    fraisier stats api production --hours 0.1 | grep error_rate
    sleep 30
done

# Monitor resource usage
$ ssh deploy@prod.example.com "top -b -n 1 | head -20"

# Check disk space
$ ssh deploy@prod.example.com "df -h /"

# View recent logs
$ fraisier logs api production --lines 100 | tail -20
```

