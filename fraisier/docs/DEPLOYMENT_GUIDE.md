# Fraisier Deployment Guide

Guide for deploying services using Fraisier in production.

---

## Prerequisites

- Linux server (Ubuntu 22.04+, Debian 12+, or similar)
- Python 3.11+
- Git access to your repositories
- SSH access to deployment targets

---

## Installation

### 1. System Dependencies

```bash
sudo apt-get update
sudo apt-get install -y python3.11 python3-pip git curl

# Optional: For advanced deployments
sudo apt-get install -y docker.io postgresql-client
```

### 2. Install Fraisier

```bash
# From PyPI (when released)
pip install fraisier

# Or from source
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql/fraisier
pip install .
```

### 3. Create Deployment User

```bash
# Create dedicated user for Fraisier
sudo useradd -m -s /bin/bash fraisier
sudo usermod -aG docker fraisier  # If using Docker

# Allow sudo without password for deployment commands
echo "fraisier ALL=(ALL) NOPASSWD: /bin/systemctl" | sudo tee /etc/sudoers.d/fraisier-systemctl
```

### 4. Directory Structure

```bash
# Create Fraisier home
sudo mkdir -p /opt/fraisier/{config,logs,data}
sudo chown -R fraisier:fraisier /opt/fraisier
sudo chmod 700 /opt/fraisier

# Directories:
# config/      - fraises.yaml, secrets, SSL certs
# logs/        - Deployment logs
# data/        - fraisier.db (state database)
```

---

## Configuration

### 1. Create fraises.yaml

```bash
sudo vim /opt/fraisier/config/fraises.yaml
```

**Example**:

```yaml
git:
  provider: github
  github:
    webhook_secret: ${FRAISIER_WEBHOOK_SECRET}

fraises:
  my_api:
    type: api
    description: My API Service
    environments:
      production:
        name: my-api
        branch: main
        app_path: /opt/services/my-api
        git_repo: https://github.com/user/my-api.git
        systemd_service: my-api.service
        database:
          name: my_api_prod
          strategy: apply  # Never rebuild production DB
          backup_before_deploy: true
        health_check:
          url: https://api.mycompany.com/health
          timeout: 30
```

### 2. Environment Secrets

```bash
# Create .env file
sudo vim /opt/fraisier/config/.env
```

**Content**:

```bash
# Git provider webhook secret
FRAISIER_WEBHOOK_SECRET=your-secret-here

# Database credentials (if needed)
DATABASE_URL=postgresql://user:pass@localhost/fraisier

# Notifications (optional)
SLACK_WEBHOOK_URL=https://hooks.slack.com/services/...
```

### 3. Load Environment

```bash
# For systemd service
# Add to [Service] section:
# EnvironmentFile=/opt/fraisier/config/.env

# For manual runs
sudo -u fraisier bash -c 'source /opt/fraisier/config/.env && fraisier list'
```

---

## Deployment Targets Setup

### 1. Systemd Service (Bare Metal)

For each service managed by Fraisier:

```bash
sudo vim /etc/systemd/system/my-api.service
```

**Example**:

```ini
[Unit]
Description=My API Service
After=network.target

[Service]
Type=simple
User=www-data
WorkingDirectory=/opt/services/my-api
ExecStart=/usr/bin/python /opt/services/my-api/app.py
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable my-api.service
```

### 2. Docker Compose

For containerized deployments:

```bash
mkdir -p /opt/services/my-api
cd /opt/services/my-api

# Create docker-compose.yml
cat > docker-compose.yml << EOF
version: '3.8'
services:
  api:
    image: my-api:latest
    ports:
      - "8000:8000"
    environment:
      - DATABASE_URL=postgres://...
    restart: unless-stopped
EOF
```

### 3. Coolify Integration (Future)

Setup when Phase 2 is complete.

---

## Fraisier Service Setup

### 1. Create Systemd Service for Fraisier

```bash
sudo vim /etc/systemd/system/fraisier.service
```

**Content**:

```ini
[Unit]
Description=Fraisier Deployment Orchestrator
After=network.target postgresql.service

[Service]
Type=simple
User=fraisier
Group=fraisier
WorkingDirectory=/opt/fraisier
EnvironmentFile=/opt/fraisier/config/.env

# Webhook server
ExecStart=/usr/local/bin/fraisier-webhook

StandardOutput=journal
StandardError=journal
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

### 2. Create Deployment Watcher (Optional)

For background deployment processing:

```bash
sudo vim /etc/systemd/system/fraisier-watcher.service
```

**Content**:

```ini
[Unit]
Description=Fraisier Deployment Watcher
After=fraisier.service

[Service]
Type=simple
User=fraisier
WorkingDirectory=/opt/fraisier
EnvironmentFile=/opt/fraisier/config/.env

# Watch for deployment requests and execute
ExecStart=/usr/local/bin/fraisier-watcher

StandardOutput=journal
StandardError=journal
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

### 3. Enable Services

```bash
sudo systemctl daemon-reload
sudo systemctl enable fraisier.service fraisier-watcher.service
sudo systemctl start fraisier.service
sudo systemctl status fraisier.service
```

---

## Nginx Reverse Proxy

Setup Nginx to expose Fraisier webhook endpoint:

```bash
sudo vim /etc/nginx/sites-available/fraisier
```

**Content**:

```nginx
upstream fraisier {
    server 127.0.0.1:8000;
}

server {
    listen 80;
    listen [::]:80;
    server_name deploy.mycompany.com;

    # Redirect to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name deploy.mycompany.com;

    ssl_certificate /etc/ssl/certs/fraisier.crt;
    ssl_certificate_key /etc/ssl/private/fraisier.key;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-Frame-Options "DENY" always;

    # Fraisier webhook
    location /webhook {
        proxy_pass http://fraisier;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Timeout for long deployments
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 300s;
    }

    # Health check
    location /health {
        proxy_pass http://fraisier;
        access_log off;
    }
}
```

Enable site:

```bash
sudo ln -s /etc/nginx/sites-available/fraisier /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

---

## Git Webhook Configuration

### GitHub

1. Go to repository settings → Webhooks
2. Create webhook:
   - **Payload URL**: `https://deploy.mycompany.com/webhook`
   - **Content type**: `application/json`
   - **Secret**: Set to `FRAISIER_WEBHOOK_SECRET`
   - **Events**: Push events
3. Click "Add webhook"

### GitLab

1. Go to repository → Settings → Webhooks
2. Create webhook:
   - **URL**: `https://deploy.mycompany.com/webhook`
   - **Trigger**: Push events
   - **Secret token**: Set to `FRAISIER_WEBHOOK_SECRET`
3. Click "Add webhook"

### Gitea

1. Go to repository → Settings → Webhooks
2. Create webhook:
   - **Target URL**: `https://deploy.mycompany.com/webhook`
   - **HTTP method**: POST
   - **Content type**: JSON
   - **Secret**: Set to `FRAISIER_WEBHOOK_SECRET`
3. Click "Add webhook"

---

## Testing Deployment

### 1. Verify Configuration

```bash
# Switch to fraisier user
sudo -u fraisier bash

# Set environment
source /opt/fraisier/config/.env

# Validate config
fraisier config validate

# List fraises
fraisier list
```

### 2. Dry-Run Deployment

```bash
# Test deployment without actually deploying
fraisier deploy my_api production --dry-run
```

### 3. Manual Deployment

```bash
# Actually deploy
fraisier deploy my_api production

# Check deployment history
fraisier history --fraise my_api

# Check status
fraisier status my_api production
```

### 4. Test Webhook

```bash
# Simulate a push to main branch
curl -X POST https://deploy.mycompany.com/webhook \
  -H "Content-Type: application/json" \
  -H "X-GitHub-Event: push" \
  -H "X-Hub-Signature-256: sha256=test" \
  -d '{
    "ref": "refs/heads/main",
    "repository": {"full_name": "user/my-api"},
    "pusher": {"name": "user"},
    "head_commit": {"id": "abc123"}
  }'

# Check deployment status
fraisier history --limit 5
```

---

## Monitoring & Logs

### View Logs

```bash
# Fraisier service logs
sudo journalctl -u fraisier.service -f

# Deployment logs
sudo journalctl -u fraisier-watcher.service -f

# Application logs
tail -f /opt/fraisier/logs/fraisier.log
```

### Database Inspection

```bash
sudo -u fraisier sqlite3 /opt/fraisier/data/fraisier.db

# View recent deployments
SELECT * FROM tb_deployment LIMIT 10;

# View deployment statistics
SELECT * FROM v_deployment_stats;

# Exit
.quit
```

### Monitoring with Prometheus (Future)

Fraisier will expose Prometheus metrics at `/metrics` (Phase 3).

---

## Health Checks

### Application Health

```bash
# Check if Fraisier is running
curl https://deploy.mycompany.com/health

# Response (success):
{"status": "healthy", "version": "0.1.0"}

# Response (failure):
HTTP 503 Service Unavailable
```

### Deployment Health

```bash
# Check if recent deployments succeeded
fraisier stats --days 7

# Should show high success rate
```

---

## Backup & Disaster Recovery

### Database Backup

```bash
# Manual backup
sudo -u fraisier cp /opt/fraisier/data/fraisier.db /opt/fraisier/backups/fraisier.db.$(date +%Y%m%d)

# Automated backup (cron)
sudo crontab -e

# Add:

0 2 * * * /usr/bin/sqlite3 /opt/fraisier/data/fraisier.db ".backup /opt/fraisier/backups/fraisier-$(date +\%Y\%m\%d).db"
```

### Configuration Backup

```bash
# Backup configuration files
tar -czf /opt/backups/fraisier-config-$(date +%Y%m%d).tar.gz /opt/fraisier/config/

# Store off-server
scp fraisier-config-*.tar.gz backup@backup.server:/backups/
```

### Restore from Backup

```bash
# Stop Fraisier
sudo systemctl stop fraisier.service fraisier-watcher.service

# Restore database
sudo cp /opt/fraisier/backups/fraisier.db.backup /opt/fraisier/data/fraisier.db
sudo chown fraisier:fraisier /opt/fraisier/data/fraisier.db

# Restart
sudo systemctl start fraisier.service fraisier-watcher.service
```

---

## Troubleshooting

### Deployment Fails

```bash
# Check Fraisier logs
sudo journalctl -u fraisier-watcher.service -n 50

# Check deployment details
fraisier history --fraise my_api --limit 1

# Check service status
fraisier status my_api production

# Try manual deployment
fraisier deploy my_api production -v  # Verbose output
```

### Webhook Not Triggering

```bash
# Check webhook events were received
fraisier webhooks --limit 20

# If processed=0, webhook wasn't matched
# Check branch_mapping in fraises.yaml

# Test webhook manually
curl -X POST http://localhost:8000/webhook ...
```

### Database Locked

```bash
# If "database is locked" error:
# Kill any hanging processes
sudo pkill -f fraisier

# Check database integrity
sudo -u fraisier sqlite3 /opt/fraisier/data/fraisier.db "PRAGMA integrity_check;"

# If corrupted, restore from backup
```

### Service Won't Start

```bash
# Check for port conflicts
sudo lsof -i :8000  # Fraisier webhook port

# Check logs
sudo journalctl -u fraisier.service -n 20

# Manually test
sudo -u fraisier fraisier-webhook
```

---

## Upgrading

### Backup First

```bash
sudo -u fraisier cp /opt/fraisier/data/fraisier.db /opt/fraisier/backups/fraisier-pre-upgrade.db
```

### Update Package

```bash
# Stop Fraisier
sudo systemctl stop fraisier.service fraisier-watcher.service

# Upgrade
pip install --upgrade fraisier

# Restart
sudo systemctl start fraisier.service fraisier-watcher.service

# Verify
fraisier --version
```

### Check for Breaking Changes

See [RELEASE_NOTES.md](../RELEASE_NOTES.md) for breaking changes between versions.

---

## Security Hardening

### File Permissions

```bash
# Restrict configuration access
sudo chmod 700 /opt/fraisier/config
sudo chmod 600 /opt/fraisier/config/.env
sudo chmod 600 /opt/fraisier/config/fraises.yaml
```

### Firewall Rules

```bash
# Allow webhook traffic only
sudo ufw allow 443/tcp  # HTTPS
sudo ufw allow 22/tcp   # SSH
sudo ufw default deny incoming
sudo ufw enable
```

### SSL/TLS Certificates

```bash
# Using Let's Encrypt
sudo certbot certonly --nginx -d deploy.mycompany.com

# Auto-renewal
sudo systemctl enable certbot.timer
sudo systemctl start certbot.timer
```

### Rate Limiting (Nginx)

```nginx
# Add to nginx config
limit_req_zone $binary_remote_addr zone=webhook_limit:10m rate=10r/m;

location /webhook {
    limit_req zone=webhook_limit burst=20 nodelay;
    proxy_pass http://fraisier;
}
```

---

## Performance Tuning

### Database Optimization

```bash
# Analyze and optimize database
sudo -u fraisier sqlite3 /opt/fraisier/data/fraisier.db << EOF
PRAGMA optimize;
VACUUM;
EOF

# Schedule weekly optimization
sudo crontab -e
# Add: 0 3 * * 0 /usr/bin/sqlite3 /opt/fraisier/data/fraisier.db "PRAGMA optimize; VACUUM;"
```

### Connection Pooling

For PostgreSQL (Phase 2+):
```bash
# Use pgBouncer for connection pooling
sudo apt-get install pgbouncer
```

---

## Next Steps

1. See [DEVELOPMENT.md](../DEVELOPMENT.md) for development setup
2. See [ARCHITECTURE.md](./ARCHITECTURE.md) for technical details
3. See [../ROADMAP.md](../ROADMAP.md) for upcoming features

---

**Last Updated**: 2026-01-22
