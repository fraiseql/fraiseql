# Phase 10: Complete Documentation System

## Comprehensive Guide for v0.1.0 Release

**Goal**: Create production-quality documentation that enables users to understand, deploy, and operate Fraisier at scale.

**Scope**: 7 subphases covering all documentation aspects
**Effort**: ~25-30 hours total
**Value**: Critical - Users cannot effectively use Fraisier without this

---

## Phase Overview

This phase creates a complete documentation system covering:

1. **Phase 10.1**: API Reference Documentation (REST endpoints, webhooks, CLI commands)
2. **Phase 10.2**: Getting Started Guides (SQLite, PostgreSQL, MySQL setups)
3. **Phase 10.3**: Provider Setup Guides (Bare Metal, Docker Compose, Coolify)
4. **Phase 10.4**: Monitoring Setup Guide (Prometheus, Grafana, alerting)
5. **Phase 10.5**: Troubleshooting Guide (50+ scenarios, solutions, debug commands)
6. **Phase 10.6**: Real-World Examples (complete configurations for common use cases)
7. **Phase 10.7**: FAQ and Advanced Topics (deep dives, best practices)

---

## Phase 10.1: API Reference Documentation

**Time**: ~4 hours
**Value**: High - Essential for developers

### Deliverables

#### 1. REST API Reference (`docs/API_REFERENCE.md`)

- All HTTP endpoints with examples
- Request/response formats
- Status codes and error handling
- Rate limiting information
- Example curl commands

**Structure**:

```
## Deployments API

### List All Deployments
GET /api/v1/deployments

**Parameters**:

- environment (optional): Filter by environment
- status (optional): Filter by status (pending, in_progress, success, failed)
- limit (optional): Max results (default 100)
- offset (optional): Pagination offset (default 0)

**Response** (200 OK):
```json
{
  "deployments": [
    {
      "id": "dep_123",
      "fraise": "my_api",
      "environment": "production",
      "status": "success",
      "started_at": "2024-01-22T10:00:00Z",
      "completed_at": "2024-01-22T10:05:30Z",
      "version": "2.0.0"
    }
  ],
  "total": 150,
  "limit": 100,
  "offset": 0
}
```

**Example**:

```bash
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8000/api/v1/deployments?environment=production&limit=10"
```

### Sections to Cover

1. **Authentication**
   - Bearer token format
   - Token generation
   - Token expiration
   - Refreshing tokens

2. **Deployments**
   - List deployments
   - Get deployment details
   - Create deployment (trigger)
   - Cancel deployment
   - Rollback deployment
   - Get deployment logs
   - Get deployment events

3. **Fraises**
   - List fraises
   - Get fraise details
   - Get fraise status
   - Get fraise history
   - Get fraise metrics

4. **Environments**
   - List environments
   - Get environment details
   - Environment configuration
   - Environment health

5. **Health & Status**
   - System health check
   - Database status
   - NATS connection status
   - Provider status

#### 2. Webhook Reference (`docs/WEBHOOK_REFERENCE.md`)

- Webhook events and payloads
- Signature verification
- Retry logic
- Example handlers

**Structure**:

```
## Webhook Events

### deployment.started
Fired when a deployment begins

**Payload**:
```json
{
  "event": "deployment.started",
  "deployment_id": "dep_123",
  "fraise": "my_api",
  "environment": "production",
  "triggered_by": "user_id_456",
  "timestamp": "2024-01-22T10:00:00Z"
}
```

### deployment.completed

Fired when a deployment finishes

**Payload**:

```json
{
  "event": "deployment.completed",
  "deployment_id": "dep_123",
  "fraise": "my_api",
  "environment": "production",
  "status": "success",
  "version": "2.0.0",
  "duration_seconds": 330,
  "timestamp": "2024-01-22T10:05:30Z"
}
```

```

#### 3. CLI Reference (`docs/CLI_REFERENCE.md`)

- All CLI commands with options
- Common usage patterns
- Exit codes

**Structure**:
```

## fraisier Command Reference

### fraisier deploy

Deploy a fraise to an environment

**Usage**:

```bash
fraisier deploy [OPTIONS] FRAISE ENVIRONMENT
```

**Options**:

- `--version VERSION`: Specific version to deploy (default: latest)
- `--strategy STRATEGY`: Deployment strategy (rolling, blue-green, canary)
- `--health-check-delay SECONDS`: Wait time before health checks
- `--wait`: Wait for deployment to complete
- `--timeout SECONDS`: Max time to wait
- `--no-backup`: Skip backup before deployment
- `--dry-run`: Show what would happen, don't actually deploy

**Examples**:

```bash
# Deploy latest version to production
fraisier deploy my_api production

# Deploy specific version with blue-green
fraisier deploy my_api production --version 2.0.0 --strategy blue-green

# Dry-run before actual deployment
fraisier deploy my_api staging --dry-run

# Deploy and wait for completion
fraisier deploy my_api production --wait --timeout 600
```

**Exit Codes**:

- 0: Success
- 1: General error
- 2: Invalid arguments
- 3: Fraise not found
- 4: Environment not found
- 5: Deployment failed

```

#### 4. Event Reference (`docs/EVENT_REFERENCE.md`)

- All event types (deployment, health check, metrics)
- Event structure
- Event filtering
- Subscription examples

**Structure**:
```

## NATS Event Types

### deployment.started

Emitted when deployment begins

**Subject**: `fraisier.deployment.started.{region}`

**Payload**:

```json
{
  "event_type": "deployment.started",
  "deployment_id": "dep_123",
  "service": "my_api",
  "version": "2.0.0",
  "strategy": "rolling",
  "timestamp": "2024-01-22T10:00:00Z",
  "region": "us-east-1",
  "trace_id": "trace_abc123"
}
```

**Subscribe Example**:

```python
from fraisier.nats import get_event_bus, EventFilter, EventSubscriberRegistry

registry = EventSubscriberRegistry()

def handle_deployment_started(event):
    print(f"Deployment started: {event.data['service']}")

registry.register(
    handle_deployment_started,
    EventFilter(event_type="deployment.started")
)
```

```

---

## Phase 10.2: Getting Started Guides

**Time**: ~5 hours
**Value**: Very High - First thing users read

### Deliverables

#### 1. SQLite Quick Start (`docs/GETTING_STARTED_SQLITE.md`)

- Installation
- Configuration
- First deployment
- Common tasks

**Structure**:
```markdown
# Getting Started with Fraisier + SQLite

Perfect for: Local development, testing, small deployments

## Installation

1. **Clone repository**:
```bash
git clone https://github.com/your-org/fraisier.git
cd fraisier
```

2. **Install Fraisier**:

```bash
pip install -e .
```

3. **Initialize SQLite database**:

```bash
fraisier db init --database sqlite --path fraisier.db
```

## Configuration

Create `fraises.yaml`:

```yaml
fraises:
  my_api:
    type: api
    git_provider: github
    git_repo: my-org/my-api
    git_branch: main
    environments:
      development:
        provider: docker-compose
        provider_config:
          docker_compose_file: ./docker-compose.yml
          service: api
```

## First Deployment

```bash
# List available fraises
fraisier list

# Deploy to development
fraisier deploy my_api development

# Check status
fraisier status my_api development

# View deployment history
fraisier history my_api development
```

## Next Steps

- Set up webhooks for automated deployments
- Configure monitoring
- Add more fraises

```

#### 2. PostgreSQL Production Setup (`docs/GETTING_STARTED_POSTGRES.md`)

- Installation
- Configuration for production
- Performance tuning
- Backup strategy

#### 3. MySQL Setup (`docs/GETTING_STARTED_MYSQL.md`)

- Installation
- Configuration
- MySQL-specific settings
- Compatibility notes

#### 4. Docker Compose Setup (`docs/GETTING_STARTED_DOCKER.md`)

- Full stack (Fraisier + PostgreSQL + Prometheus + Grafana)
- Single command setup
- Common customizations

**Structure**:
```bash
# One command full stack
docker-compose up -d

# Verify everything is running
docker-compose ps

# Access Fraisier
curl http://localhost:8000/health

# Access Grafana dashboards
open http://localhost:3000  # admin/admin

# View NATS events
nats sub "fraisier.>"
```

---

## Phase 10.3: Provider Setup Guides

**Time**: ~4 hours
**Value**: High - Required for production deployments

### Deliverables

#### 1. Bare Metal Provider Guide (`docs/PROVIDER_BARE_METAL.md`)

- SSH setup and keys
- systemd service creation
- Health check configuration
- Log retrieval
- Troubleshooting

**Structure**:

```markdown
# Deploying to Bare Metal

## Setup

1. **SSH Key Configuration**
```bash
# Generate SSH key if not exists
ssh-keygen -t ed25519 -f ~/.ssh/fraisier

# Add to authorized_keys on target machine
ssh-copy-id -i ~/.ssh/fraisier user@production-server.com
```

2. **Configure in fraises.yaml**

```yaml
fraises:
  my_api:
    environments:
      production:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: production-1.example.com
              port: 22
              username: deploy
              ssh_key_path: ~/.ssh/fraisier
          service_name: my-api
          app_path: /opt/my-api
          systemd_service: my-api.service
          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 10
```

3. **Create systemd service on target**

```ini
[Unit]
Description=My API Service
After=network.target

[Service]
Type=simple
User=deploy
ExecStart=/opt/my-api/bin/start.sh
ExecStop=/bin/kill -s TERM $MAINPID
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

## Deployment Flow

1. SSH to target machine
2. Pull latest code
3. Stop systemd service
4. Update application
5. Start systemd service
6. Verify health check passes

## Troubleshooting

- Check SSH connectivity: `ssh -i ~/.ssh/fraisier user@host`
- View systemd logs: `journalctl -u my-api.service -f`

```

#### 2. Docker Compose Provider Guide (`docs/PROVIDER_DOCKER_COMPOSE.md`)

- docker-compose file setup
- Service configuration
- Port mapping
- Volume management
- Network setup

#### 3. Coolify Provider Guide (`docs/PROVIDER_COOLIFY.md`)

- Coolify API setup
- Project and service configuration
- Authentication
- Deployment triggering
- Status monitoring

---

## Phase 10.4: Monitoring Setup Guide

**Time**: ~4 hours
**Value**: Very High - Essential for production

### File: `docs/MONITORING_SETUP.md`

**Sections**:

1. **Prometheus Configuration**
   - Scrape config for Fraisier metrics
   - NATS metrics
   - Database metrics
   - Example prometheus.yml

2. **Grafana Dashboards**
   - Dashboard JSON for deployment metrics
   - Health check dashboard
   - System metrics dashboard
   - Provider-specific dashboards

3. **Alerting Rules**
   - Deployment failures alert
   - Health check failures alert
   - NATS connection issues alert
   - Database performance alert

4. **Example**:
```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'fraisier'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'

  - job_name: 'nats'
    static_configs:
      - targets: ['localhost:8222']
    metrics_path: '/metrics'

  - job_name: 'postgres'
    static_configs:
      - targets: ['localhost:9187']  # postgres_exporter
```

---

## Phase 10.5: Troubleshooting Guide

**Time**: ~4 hours
**Value**: Very High - Users will need this

### File: `docs/TROUBLESHOOTING.md`

**Structure** (similar to NATS_TROUBLESHOOTING.md pattern):

1. **Connection Issues** (10+ scenarios)
   - Cannot connect to database
   - Cannot SSH to bare metal host
   - Cannot reach Coolify API
   - NATS connection timeout

2. **Deployment Issues** (15+ scenarios)
   - Deployment stuck in progress
   - Health check failing
   - Version mismatch
   - Rollback failures
   - Webhook not triggering

3. **Observability Issues** (8+ scenarios)
   - Metrics not appearing
   - Logs missing
   - NATS events not received
   - Grafana dashboards empty

4. **Performance Issues** (5+ scenarios)
   - Slow deployments
   - High database load
   - Memory usage issues
   - CPU spikes

Each scenario includes:

- Error message
- Causes
- Solutions (3-5 steps)
- Debugging commands
- When to escalate

---

## Phase 10.6: Real-World Examples

**Time**: ~3 hours
**Value**: High - Users learn by example

### Deliverables

#### 1. Simple Web Service (`docs/examples/simple-web-service/`)

- Flask API deployed to Docker Compose
- Health checks
- Environment configuration
- Webhook setup

**Files**:

```
simple-web-service/
├── README.md
├── fraises.yaml
├── docker-compose.yml
├── app.py
├── requirements.txt
├── Dockerfile
└── .env.example
```

#### 2. Microservices with Monitoring (`docs/examples/microservices-monitoring/`)

- Multiple services (API, Worker, Database)
- Full monitoring stack (Prometheus, Grafana)
- NATS integration
- Alerting configured

#### 3. Multi-Environment Deployment (`docs/examples/multi-environment/`)

- Dev, staging, production
- Different configurations per environment
- Progressive rollout strategy
- Automated testing in each environment

#### 4. High-Availability Setup (`docs/examples/ha-setup/`)

- Load-balanced deployment
- Database replication
- Backup and recovery
- Disaster recovery procedures

---

## Phase 10.7: FAQ and Advanced Topics

**Time**: ~3 hours
**Value**: Medium - Deep-dives for advanced users

### File: `docs/FAQ_AND_ADVANCED.md`

**FAQ Sections**:

1. **General Questions** (10+ Q&A)
   - What databases does Fraisier support?
   - Can I use Fraisier with existing deployments?
   - How is Fraisier different from other tools?
   - What's the licensing model?

2. **Deployment Questions** (15+ Q&A)
   - How do I rollback a failed deployment?
   - Can I deploy multiple services simultaneously?
   - How are health checks configured?
   - What happens if a provider goes down?

3. **Monitoring Questions** (8+ Q&A)
   - How do I set up alerting?
   - Can I export metrics to Datadog/New Relic?
   - How long are events retained?
   - Can I replicate events to another Fraisier instance?

**Advanced Topics**:

1. **Custom Provider Development**
   - Provider interface
   - Health check implementation
   - Event emission
   - Example provider

2. **Event-Driven Architecture**
   - NATS subject patterns
   - Filtering strategies
   - Building custom handlers
   - Event replay scenarios

3. **Performance Tuning**
   - Database connection pooling
   - Query optimization
   - Caching strategies
   - Deployment acceleration

4. **Security Hardening**
   - TLS/SSL configuration
   - API authentication
   - Provider credential management
   - NATS authentication

5. **Operational Best Practices**
   - Monitoring checklist
   - Backup and recovery
   - Scaling strategies
   - Disaster recovery

---

## Implementation Strategy

### Timeline

**Day 1** (8 hours):

- Phase 10.1: API Reference (4 hours)
- Phase 10.2: Getting Started Guides (4 hours)

**Day 2** (8 hours):

- Phase 10.3: Provider Guides (4 hours)
- Phase 10.4: Monitoring Setup (4 hours)

**Day 3** (8 hours):

- Phase 10.5: Troubleshooting Guide (4 hours)
- Phase 10.6: Real-World Examples (3 hours)
- Phase 10.7: FAQ and Advanced Topics (1 hour - will finish next)

### File Structure

```
docs/
├── README.md (updated index)
├── API_REFERENCE.md
├── CLI_REFERENCE.md
├── WEBHOOK_REFERENCE.md
├── EVENT_REFERENCE.md
├── GETTING_STARTED_SQLITE.md
├── GETTING_STARTED_POSTGRES.md
├── GETTING_STARTED_MYSQL.md
├── GETTING_STARTED_DOCKER.md
├── PROVIDER_BARE_METAL.md
├── PROVIDER_DOCKER_COMPOSE.md
├── PROVIDER_COOLIFY.md
├── MONITORING_SETUP.md
├── TROUBLESHOOTING.md
├── FAQ_AND_ADVANCED.md
├── NATS_INTEGRATION_GUIDE.md (already exists)
├── NATS_EXAMPLES.md (already exists)
├── NATS_TROUBLESHOOTING.md (already exists)
└── examples/
    ├── simple-web-service/
    ├── microservices-monitoring/
    ├── multi-environment/
    └── ha-setup/
```

### Quality Standards

- All examples tested and working
- All code snippets executable
- All commands verified on target systems
- Consistent formatting and style
- Cross-references between guides
- Version compatibility noted

---

## Success Criteria

✅ Completion checklist:

- [ ] API Reference complete (all endpoints documented)
- [ ] CLI Reference complete (all commands documented)
- [ ] Getting Started guides for all 3 databases + Docker
- [ ] Provider guides for all 3 providers
- [ ] Monitoring setup guide with alerting
- [ ] Troubleshooting guide with 50+ scenarios
- [ ] 4 real-world examples working end-to-end
- [ ] FAQ with 40+ answers
- [ ] All examples have README and configuration files
- [ ] All documentation links verified
- [ ] Consistent formatting throughout
- [ ] No broken links or references
- [ ] All commits follow standard format

---

## What Comes After

Once Phase 10 is complete, you'll have:

- ✅ Complete, production-ready Fraisier
- ✅ Comprehensive documentation for all use cases
- ✅ Real-world examples users can copy
- ✅ Troubleshooting guides for common issues
- ✅ Ready for v0.1.0 release

Next opportunity: **Phase 11** (Production Hardening, Performance Optimization, Security Audit, Enterprise Features) - for enterprise-grade release
