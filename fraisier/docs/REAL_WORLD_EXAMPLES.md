# Fraisier Real-World Examples

**Complete configurations for common deployment scenarios**

This guide provides 4 production-ready example configurations covering typical deployment patterns.

---

## Table of Contents

1. [Example 1: Simple Web Service](#example-1-simple-web-service)
2. [Example 2: Microservices with Monitoring](#example-2-microservices-with-monitoring)
3. [Example 3: Multi-Environment Deployment](#example-3-multi-environment-deployment)
4. [Example 4: HA Setup with Disaster Recovery](#example-4-ha-setup-with-disaster-recovery)

---

## Example 1: Simple Web Service

**Scenario**: Single API service with PostgreSQL database running on bare metal.

**Time to Production**: 15-20 minutes

**Architecture**:

```
┌─────────────────┐
│ fraisier-cli    │
│ (Deployment)    │
└────────┬────────┘
         │ SSH
         ↓
┌─────────────────┐
│ Production Host │
│  (ubuntu-22)    │
│ ┌─────────────┐ │
│ │ fraisier    │ │
│ │ (systemd)   │ │
│ └─────────────┘ │
│ ┌─────────────┐ │
│ │ PostgreSQL  │ │
│ └─────────────┘ │
└─────────────────┘
```

### Prerequisites

```bash
# Production server (ubuntu-22.04)
# - SSH access as 'deploy' user
# - PostgreSQL 14+ installed
# - 2GB+ RAM, 10GB+ storage
# - Outbound HTTPS for updates
```

### Step 1: Prepare Application

**Repository structure**:

```
my-api/
├── src/
│   ├── main.py
│   └── app.py
├── requirements.txt
├── Dockerfile
├── fraises.yaml
└── README.md
```

**requirements.txt**:

```
flask==3.0.0
psycopg==3.1.14
python-dotenv==1.0.0
gunicorn==21.2.0
```

**src/app.py**:

```python
from flask import Flask, jsonify
import psycopg
import os
from datetime import datetime

app = Flask(__name__)

def get_db_connection():
    conn = psycopg.connect(os.environ['DATABASE_URL'])
    return conn

@app.route('/health')
def health():
    try:
        conn = get_db_connection()
        conn.execute('SELECT 1')
        conn.close()
        return jsonify({
            'status': 'healthy',
            'timestamp': datetime.utcnow().isoformat(),
            'database': 'connected'
        }), 200
    except Exception as e:
        return jsonify({
            'status': 'unhealthy',
            'error': str(e)
        }), 500

@app.route('/api/v1/status')
def status():
    return jsonify({
        'service': 'my-api',
        'version': '1.0.0',
        'environment': os.environ.get('ENVIRONMENT', 'production')
    }), 200

if __name__ == '__main__':
    app.run(host='0.0.0.0', port=8000)
```

**src/main.py**:

```python
import os
from app import app

if __name__ == '__main__':
    app.run(
        host='0.0.0.0',
        port=int(os.environ.get('PORT', 8000)),
        debug=False
    )
```

**Dockerfile**:

```dockerfile
FROM python:3.11-slim

WORKDIR /app

# Install dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application
COPY src/ .

# Health check
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health').read()"

# Run with gunicorn
CMD ["gunicorn", "--workers=4", "--bind=0.0.0.0:8000", "--timeout=30", "main:app"]
```

### Step 2: Configure Fraisier

**fraises.yaml**:

```yaml
version: '1.0'

fraises:
  my_api:
    type: api
    description: My API Service

    git_provider: github
    git_repo: your-org/my-api
    git_branch: main

    environments:
      production:
        provider: bare_metal

        provider_config:
          # SSH Configuration
          ssh_host: api.example.com
          ssh_user: deploy
          ssh_port: 22
          ssh_key_path: ~/.ssh/production_key

          # Deployment
          working_directory: /opt/my-api
          deployment_strategy: rolling
          max_parallel_deployments: 1

          # Application
          service_name: my-api
          service_file: /etc/systemd/system/my-api.service
          auto_restart: true

          # Health Check
          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 10
            max_retries: 3
            retry_delay: 5

          # Build
          build_command: docker build -t my-api:latest .
          push_command: |
            docker run --rm \
              -e DATABASE_URL="${DATABASE_URL}" \
              -e ENVIRONMENT=production \
              -p 8000:8000 \
              --name my-api-container \
              my-api:latest

          # Environment variables
          env_vars:
            ENVIRONMENT: production
            LOG_LEVEL: info
            WORKERS: "4"

          # Monitoring
          monitoring:
            enabled: true
            metrics_port: 9090
            health_check_interval: 30
```

### Step 3: Setup Production Server

**SSH into server and run**:

```bash
# Create deploy user (if not exists)
sudo useradd -m -s /bin/bash deploy
sudo usermod -aG docker deploy

# Setup directories
sudo mkdir -p /opt/my-api
sudo chown deploy:deploy /opt/my-api
cd /opt/my-api

# Install Docker
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh
sudo usermod -aG docker deploy

# Create systemd service
sudo tee /etc/systemd/system/my-api.service > /dev/null <<EOF
[Unit]
Description=My API Service
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=deploy
WorkingDirectory=/opt/my-api
Environment="DATABASE_URL=postgresql://api_user:secure_password@localhost:5432/api_db"
Environment="ENVIRONMENT=production"
Environment="LOG_LEVEL=info"

# Docker container
ExecStart=/usr/bin/docker run --rm \
  -e DATABASE_URL=\${DATABASE_URL} \
  -e ENVIRONMENT=\${ENVIRONMENT} \
  -e LOG_LEVEL=\${LOG_LEVEL} \
  -p 8000:8000 \
  --name my-api-container \
  my-api:latest

ExecStop=/usr/bin/docker stop my-api-container
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable my-api
```

### Step 4: Deploy

```bash
# From your local machine
fraisier deploy my_api production

# Expected output:
# ✓ Code pulled (3 commits)
# ✓ Docker image built (45 seconds)
# ✓ Docker image pushed (10 seconds)
# ✓ Service updated via SSH (5 seconds)
# ✓ Health checks passed (2 retries, 15 seconds)
#
# ✅ Deployment successful in 77 seconds
#
# Service: my-api
# Status: healthy
# Version: 1.0.0
# URL: http://api.example.com:8000
```

### Step 5: Monitor

```bash
# Check service status
fraisier status my_api production

# View logs
fraisier logs dep_00001

# Follow logs in real-time
ssh deploy@api.example.com "docker logs -f my-api-container"

# Check health
curl http://api.example.com:8000/health

# Manual restart
ssh deploy@api.example.com "sudo systemctl restart my-api"
```

### Rollback Procedure

```bash
# View deployment history
fraisier history my_api production --limit 5

# Rollback to previous version
fraisier rollback my_api production

# Or deploy specific commit
fraisier deploy my_api production --commit abc123def456
```

---

## Example 2: Microservices with Monitoring

**Scenario**: Multiple services (API, Worker, Cache) with Prometheus monitoring and Grafana dashboards.

**Time to Production**: 30-40 minutes

**Architecture**:

```
┌──────────────────────────────────────────┐
│ Docker Host (single machine)             │
├──────────────────────────────────────────┤
│ ┌──────────┐  ┌──────────┐  ┌─────────┐ │
│ │   API    │  │  Worker  │  │  Cache  │ │
│ │ :8000    │  │ :8001    │  │ :6379   │ │
│ └──────────┘  └──────────┘  └─────────┘ │
│      ↓              ↓             ↓      │
│ ┌─────────────────────────────────────┐  │
│ │      PostgreSQL (5432)              │  │
│ └─────────────────────────────────────┘  │
│      ↓                                    │
│ ┌─────────────────────────────────────┐  │
│ │     Prometheus (9090)               │  │
│ └─────────────────────────────────────┘  │
│      ↓                                    │
│ ┌─────────────────────────────────────┐  │
│ │      Grafana (3000)                 │  │
│ └─────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

### docker-compose.yml

```yaml
version: '3.9'

services:
  # API Service
  api:
    build:
      context: ./api
      dockerfile: Dockerfile
    container_name: my-api
    ports:
      - "8000:8000"
    environment:
      - DATABASE_URL=postgresql://postgres:password@db:5432/app
      - REDIS_URL=redis://cache:6379
      - LOG_LEVEL=info
      - ENVIRONMENT=production
    depends_on:
      - db
      - cache
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 20s
    restart: unless-stopped
    networks:
      - app-network

  # Worker Service
  worker:
    build:
      context: ./worker
      dockerfile: Dockerfile
    container_name: my-worker
    environment:
      - DATABASE_URL=postgresql://postgres:password@db:5432/app
      - REDIS_URL=redis://cache:6379
      - LOG_LEVEL=info
    depends_on:
      - db
      - cache
    restart: unless-stopped
    networks:
      - app-network

  # Cache
  cache:
    image: redis:7-alpine
    container_name: my-cache
    ports:
      - "6379:6379"
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 3s
      retries: 3
    restart: unless-stopped
    volumes:
      - cache-data:/data
    networks:
      - app-network

  # Database
  db:
    image: postgres:15-alpine
    container_name: my-db
    environment:
      - POSTGRES_DB=app
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=password
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 10s
      timeout: 5s
      retries: 3
    restart: unless-stopped
    volumes:
      - db-data:/var/lib/postgresql/data
    networks:
      - app-network

  # Prometheus
  prometheus:
    image: prom/prometheus:latest
    container_name: my-prometheus
    ports:
      - "9090:9090"
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml
      - ./monitoring/rules.yml:/etc/prometheus/rules.yml
      - prometheus-data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'
    restart: unless-stopped
    networks:
      - app-network

  # Grafana
  grafana:
    image: grafana/grafana:latest
    container_name: my-grafana
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_USER=admin
      - GF_SECURITY_ADMIN_PASSWORD=admin123
      - GF_INSTALL_PLUGINS=grafana-clock-panel
    volumes:
      - ./monitoring/grafana/provisioning:/etc/grafana/provisioning
      - grafana-data:/var/lib/grafana
    depends_on:
      - prometheus
    restart: unless-stopped
    networks:
      - app-network

volumes:
  db-data:
  cache-data:
  prometheus-data:
  grafana-data:

networks:
  app-network:
    driver: bridge
```

### fraises.yaml

```yaml
version: '1.0'

fraises:
  microservices:
    type: service_group
    description: Microservices with Monitoring

    git_provider: github
    git_repo: your-org/my-microservices
    git_branch: main

    environments:
      staging:
        provider: docker_compose

        provider_config:
          docker_compose_file: ./docker-compose.yml
          services:
            - api
            - worker
            - cache
            - db
            - prometheus
            - grafana

          # Health Check
          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 10
            max_retries: 3

          # Deployment
          deployment_strategy: rolling
          build_cache: true
          pull_images: true

          # Post-deployment hooks
          hooks:
            post_deployment:
              - command: docker-compose exec db psql -U postgres -d app -c "SELECT 1"
                on_failure: warn
              - command: curl -f http://localhost:8000/health
                on_failure: fail
```

### Monitoring Setup

**monitoring/prometheus.yml**:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'api'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/metrics'

  - job_name: 'worker'
    static_configs:
      - targets: ['localhost:8001']
    metrics_path: '/metrics'

  - job_name: 'cache'
    static_configs:
      - targets: ['localhost:6379']

  - job_name: 'postgres'
    static_configs:
      - targets: ['localhost:5432']
```

**monitoring/rules.yml**:

```yaml
groups:
  - name: application
    interval: 1m
    rules:
      - alert: ApiHighErrorRate
        expr: |
          (sum(rate(http_requests_total{status=~"5.."}[5m])) /
           sum(rate(http_requests_total[5m]))) > 0.05
        for: 5m
        annotations:
          summary: "API error rate > 5%"

      - alert: ApiResponseTimeHigh
        expr: histogram_quantile(0.95, http_request_duration_seconds) > 1
        for: 5m
        annotations:
          summary: "API P95 response time > 1s"

      - alert: WorkerQueueLarge
        expr: worker_queue_depth > 1000
        for: 5m
        annotations:
          summary: "Worker queue backed up ({{ $value }} items)"

      - alert: DatabaseConnectionPoolLow
        expr: db_connection_pool_available < 2
        for: 2m
        annotations:
          summary: "Database connection pool nearly exhausted"
```

### Deployment

```bash
# Deploy entire stack
fraisier deploy microservices staging

# Access services
open http://localhost:8000        # API
open http://localhost:3000        # Grafana (admin/admin123)
open http://localhost:9090        # Prometheus
```

### Grafana Dashboard Creation

1. Navigate to http://localhost:3000
2. Add Prometheus data source (http://prometheus:9090)
3. Create dashboard with panels:
   - API Request Rate (rate(http_requests_total[5m]))
   - API Error Rate (rate(http_requests_total{status=~"5.."}[5m]))
   - API Response Time (histogram_quantile(0.95, http_request_duration_seconds))
   - Worker Queue Depth (worker_queue_depth)
   - Database Connections (db_connection_pool_available)

---

## Example 3: Multi-Environment Deployment

**Scenario**: Same application deployed to development, staging, and production with different configurations.

**Time to Production**: 25-30 minutes per environment

**Architecture**:

```
Development (Local/Docker)
    ↓
Staging (Docker Compose on remote)
    ↓
Production (Kubernetes/Bare Metal)
```

### fraises.yaml

```yaml
version: '1.0'

fraises:
  my_app:
    type: api
    description: Multi-environment application

    git_provider: github
    git_repo: your-org/my-app
    git_branch: main

    environments:
      # Development - Local Docker
      development:
        provider: docker_compose

        provider_config:
          docker_compose_file: ./docker-compose.dev.yml
          service: app

          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 5
            max_retries: 2

      # Staging - Remote Docker Compose
      staging:
        provider: docker_compose

        provider_config:
          docker_compose_file: ./docker-compose.staging.yml
          working_directory: /opt/my-app

          remote:
            enabled: true
            ssh_host: staging.example.com
            ssh_user: deploy
            ssh_key_path: ~/.ssh/staging_key
            ssh_port: 22

          health_check:
            type: http
            url: https://staging-api.example.com/health
            timeout: 10
            max_retries: 3

          env_vars:
            ENVIRONMENT: staging
            LOG_LEVEL: debug

      # Production - Bare Metal
      production:
        provider: bare_metal

        provider_config:
          ssh_host: prod-api.example.com
          ssh_user: deploy
          ssh_key_path: ~/.ssh/production_key

          service_name: my-app
          service_file: /etc/systemd/system/my-app.service

          health_check:
            type: http
            url: https://api.example.com/health
            timeout: 15
            max_retries: 5

          deployment_strategy: blue_green
          auto_rollback_on_failure: true

          env_vars:
            ENVIRONMENT: production
            LOG_LEVEL: info
            DATABASE_URL: ${DATABASE_URL_PROD}
            CACHE_URL: ${CACHE_URL_PROD}

          monitoring:
            enabled: true
            metrics_port: 9090
```

### Environment-Specific docker-compose Files

**docker-compose.dev.yml**:

```yaml
version: '3.9'

services:
  app:
    build: .
    ports:
      - "8000:8000"
    environment:
      - DATABASE_URL=sqlite:///app.db
      - DEBUG=true
      - LOG_LEVEL=debug
    volumes:
      - .:/app
    command: flask run --host=0.0.0.0
```

**docker-compose.staging.yml**:

```yaml
version: '3.9'

services:
  app:
    image: my-app:latest
    ports:
      - "8000:8000"
    environment:
      - DATABASE_URL=postgresql://user:pass@db-staging.example.com/app
      - CACHE_URL=redis://cache-staging.example.com:6379
      - LOG_LEVEL=debug
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
    restart: unless-stopped
```

### Deployment Workflow

```bash
# 1. Deploy to development (local)
fraisier deploy my_app development

# 2. Verify in development
curl http://localhost:8000/health

# 3. Deploy to staging
fraisier deploy my_app staging

# 4. Run integration tests
fraisier test my_app staging

# 5. Monitor staging for 24-48 hours
fraisier status my_app staging --watch

# 6. Deploy to production
fraisier deploy my_app production

# 7. Monitor production
fraisier status my_app production --watch

# 8. If issues arise, rollback
fraisier rollback my_app production
```

---

## Example 4: HA Setup with Disaster Recovery

**Scenario**: Production deployment with high availability, automated backups, and disaster recovery.

**Time to Production**: 45-60 minutes

**Architecture**:

```
┌─────────────────────────────────────────┐
│       Load Balancer (HAProxy)           │
└────────────────┬────────────────────────┘
                 │
    ┌────────────┼────────────┐
    ↓            ↓            ↓
┌────────┐  ┌────────┐  ┌────────┐
│ API-1  │  │ API-2  │  │ API-3  │
│:8000   │  │:8000   │  │:8000   │
└───┬────┘  └───┬────┘  └───┬────┘
    └───────────┼───────────┘
                ↓
┌─────────────────────────────────┐
│ PostgreSQL Primary (Master)     │
│ with Replication to 2 replicas  │
└──────────┬──────────────────────┘
           │
    ┌──────┴──────┐
    ↓             ↓
┌─────────┐  ┌─────────┐
│ Replica │  │ Replica │
│   (Hot) │  │ (Warm)  │
└─────────┘  └─────────┘

Backup System:
┌──────────────┐
│ Automated    │
│ Backups      │
│ (Every 4h)   │
└──────────────┘
    │
    ↓
┌──────────────┐
│ S3 Storage   │
│ (2 regions)  │
└──────────────┘
```

### Infrastructure Setup

**Production servers** (3x API + 1x DB Primary + 2x DB Replica):

```bash
# Server 1: API with Load Balancer
prod-lb.example.com
  - HAProxy for load balancing
  - 2 vCPU, 4GB RAM

# Servers 2-4: API instances
prod-api-1.example.com  (ip: 10.0.1.10)
prod-api-2.example.com  (ip: 10.0.1.11)
prod-api-3.example.com  (ip: 10.0.1.12)
  - Each: 4 vCPU, 8GB RAM, 50GB storage

# Server 5: PostgreSQL Primary
prod-db.example.com (ip: 10.0.2.10)
  - 8 vCPU, 16GB RAM, 200GB SSD

# Servers 6-7: PostgreSQL Replicas
prod-db-replica-1.example.com (ip: 10.0.2.11, hot standby)
prod-db-replica-2.example.com (ip: 10.0.2.12, warm standby)
  - Each: 8 vCPU, 16GB RAM, 200GB SSD
```

### HAProxy Configuration

**On prod-lb.example.com**:

```bash
sudo apt-get install haproxy

# /etc/haproxy/haproxy.cfg
cat > /etc/haproxy/haproxy.cfg <<'EOF'
global
    log 127.0.0.1 local0
    chroot /var/lib/haproxy
    stats socket /run/haproxy/admin.sock mode 660 level admin
    stats timeout 30s
    user haproxy
    group haproxy
    daemon
    maxconn 4096

defaults
    mode http
    log global
    option httplog
    option dontlognull
    timeout connect 5000
    timeout client 50000
    timeout server 50000

frontend my_app_frontend
    bind *:80
    bind *:443 ssl crt /etc/ssl/certs/api.example.com.pem
    redirect scheme https code 301 if !{ ssl_fc }
    option httpchk GET /health HTTP/1.1\r\nHost:\ api.example.com
    default_backend my_app_backend

backend my_app_backend
    balance roundrobin

    server api1 10.0.1.10:8000 check inter 5s fall 3 rise 2
    server api2 10.0.1.11:8000 check inter 5s fall 3 rise 2
    server api3 10.0.1.12:8000 check inter 5s fall 3 rise 2

    # Session persistence (if needed for stateful sessions)
    cookie SERVERID insert indirect nocache

listen stats
    bind *:8404
    stats enable
    stats uri /stats
    stats refresh 30s
EOF

sudo systemctl restart haproxy
```

### PostgreSQL Replication Setup

**On prod-db.example.com (Primary)**:

```bash
# /etc/postgresql/15/main/postgresql.conf

# Replication settings
max_wal_senders = 10
wal_level = replica
max_replication_slots = 10
synchronous_commit = on

# Backups
wal_keep_size = 1GB
archive_mode = on
archive_command = 'test ! -f /archive/%f && cp %p /archive/%f'
```

**On replica servers**:

```bash
# Basebackup from primary
pg_basebackup -h 10.0.2.10 -D /var/lib/postgresql/15/main -U replication -v -P

# /etc/postgresql/15/main/recovery.conf
standby_mode = 'on'
primary_conninfo = 'host=10.0.2.10 port=5432 user=replication password=repl_password'
recovery_target_timeline = 'latest'
```

### fraises.yaml for HA Deployment

```yaml
version: '1.0'

fraises:
  my_app_ha:
    type: api
    description: HA Production Deployment

    git_provider: github
    git_repo: your-org/my-app
    git_branch: main

    environments:
      production_ha:
        provider: bare_metal

        # Deploy to multiple API servers simultaneously
        deployment_targets:
          - name: api-1
            ssh_host: prod-api-1.example.com
            ssh_user: deploy
            ssh_key_path: ~/.ssh/production_key

          - name: api-2
            ssh_host: prod-api-2.example.com
            ssh_user: deploy
            ssh_key_path: ~/.ssh/production_key

          - name: api-3
            ssh_host: prod-api-3.example.com
            ssh_user: deploy
            ssh_key_path: ~/.ssh/production_key

        provider_config:
          # Parallel deployment (rolling)
          deployment_strategy: rolling
          max_parallel_deployments: 2  # Deploy to 2 servers simultaneously
          auto_rollback_on_failure: true

          # Service configuration
          service_name: my-app
          service_file: /etc/systemd/system/my-app.service

          # Health Check with higher thresholds for HA
          health_check:
            type: http
            url: https://api.example.com/health
            timeout: 15
            max_retries: 5
            retry_delay: 3

          # Database configuration (Primary + Replicas)
          database:
            type: postgresql
            primary_host: 10.0.2.10:5432
            replica_hosts:
              - 10.0.2.11:5432   # Hot standby
              - 10.0.2.12:5432   # Warm standby
            read_replica_enabled: true
            failover_timeout: 30

          # Backup configuration
          backups:
            enabled: true
            schedule: "0 */4 * * *"  # Every 4 hours
            method: pg_basebackup
            storage:
              type: s3
              bucket: my-app-backups
              region: us-east-1
              prefix: production/
            retention:
              days: 30
              copies: 3  # Keep 3 copies for redundancy

          # Monitoring for HA
          monitoring:
            enabled: true
            metrics_port: 9090
            alerting:
              - name: api_down
                condition: "up{job='api'} == 0"
                duration: 2m
              - name: db_replication_lag
                condition: "pg_replication_lag_seconds > 10"
                duration: 5m

          # Environment variables
          env_vars:
            ENVIRONMENT: production
            LOG_LEVEL: info
            DATABASE_URL: ${DATABASE_URL_PRIMARY}
            DATABASE_REPLICA_URLS: ${DATABASE_REPLICA_URLS}
            CACHE_CLUSTER: ${CACHE_CLUSTER_HA}
```

### Disaster Recovery Procedure

```bash
# 1. Automated backup verification
fraisier backup verify my_app_ha production_ha

# 2. Full system health check
fraisier status my_app_ha production_ha --detailed

# 3. Manual backup before major changes
fraisier backup create my_app_ha production_ha --tag "pre-migration"

# 4. Promote replica to primary (if primary fails)
ssh deploy@prod-db.example.com "pg_ctl promote -D /var/lib/postgresql/15/main"

# 5. Full recovery from S3 backup
fraisier restore my_app_ha production_ha --from s3 --backup-date 2024-01-22

# 6. Verify recovery by running tests
fraisier test my_app_ha production_ha --suite integration

# 7. Monitor system for 24 hours
fraisier logs my_app_ha production_ha --follow --since 1h
```

### Monitoring & Alerting

**Key metrics to monitor**:

```promql
# API Health
up{job="api"}

# Replication Lag
pg_replication_lag_seconds > 10

# Database Connections
pg_stat_activity_count > 80

# Cache Hit Ratio
redis_hits / (redis_hits + redis_misses) < 0.8

# API Response Time (P95)
histogram_quantile(0.95, http_request_duration_seconds) > 1

# Error Rate
rate(http_requests_total{status=~"5.."}[5m]) > 0.01
```

**Alert rules**:

```yaml
groups:
  - name: ha_production
    rules:
      - alert: ApiServerDown
        expr: up{job="api"} == 0
        for: 2m
        annotations:
          summary: "API server {{ $labels.instance }} is down"
          severity: critical

      - alert: DatabaseReplicationLag
        expr: pg_replication_lag_seconds > 10
        for: 5m
        annotations:
          summary: "Database replication lag > 10s"
          severity: warning

      - alert: AutomaticFailoverTriggered
        expr: pg_server_role == 0 and pg_is_in_recovery == 0
        for: 1m
        annotations:
          summary: "Database failover occurred"
          severity: critical
```

---

## Common Deployment Patterns Checklist

### Simple Web Service

- [ ] Single API server with PostgreSQL
- [ ] SSH + systemd deployment
- [ ] Health checks configured
- [ ] Monitoring enabled
- [ ] Backup strategy defined
- [ ] Rollback procedure tested

### Microservices

- [ ] Multiple services defined
- [ ] Service discovery working
- [ ] Network isolation configured
- [ ] Monitoring for each service
- [ ] Health checks for dependencies
- [ ] Graceful shutdown handling

### Multi-Environment

- [ ] Dev environment working
- [ ] Staging environment mirrors production
- [ ] Promotion workflow defined
- [ ] Secrets management configured
- [ ] Environment variables separated
- [ ] Smoke tests for each environment

### HA/DR

- [ ] Load balancer configured
- [ ] Database replication working
- [ ] Automated backups scheduled
- [ ] Failover tested
- [ ] Recovery procedure documented
- [ ] Monitoring alerts configured
- [ ] Incident response plan created

---

## Reference

- [GETTING_STARTED_DOCKER.md](GETTING_STARTED_DOCKER.md) - Docker setup guide
- [PROVIDER_BARE_METAL.md](PROVIDER_BARE_METAL.md) - Bare metal deployment
- [PROVIDER_DOCKER_COMPOSE.md](PROVIDER_DOCKER_COMPOSE.md) - Docker Compose deployment
- [MONITORING_SETUP.md](MONITORING_SETUP.md) - Monitoring and alerting
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues
- [API_REFERENCE.md](API_REFERENCE.md) - API documentation
