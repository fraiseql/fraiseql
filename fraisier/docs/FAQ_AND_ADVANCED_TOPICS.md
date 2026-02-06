# Fraisier FAQ & Advanced Topics

**Frequently Asked Questions and In-Depth Technical Guides**

---

## Table of Contents

1. [FAQ - General](#faq---general)
2. [FAQ - Deployment](#faq---deployment)
3. [FAQ - Troubleshooting](#faq---troubleshooting)
4. [Advanced Topics](#advanced-topics)
5. [Performance Tuning](#performance-tuning)
6. [Custom Providers](#custom-providers)

---

## FAQ - General

### Q: What is Fraisier?

**A:** Fraisier is a lightweight deployment orchestrator that manages application deployments across multiple infrastructure providers (Bare Metal, Docker Compose, Coolify, Kubernetes). It handles versioning, health checks, rollbacks, and monitoring.

**Key features**:

- Multi-provider support (Bare Metal SSH+systemd, Docker Compose, Coolify)
- Deployment strategies (rolling, blue-green, canary)
- Health checks and automatic rollback
- Event-driven architecture with NATS
- Monitoring and alerting integration
- Webhook notifications
- APQ (Automatic Persistent Query) support

### Q: What databases does Fraisier support?

**A:** Fraisier supports these databases for storing deployment state and metadata:

- **SQLite** - Development and testing (default)
- **PostgreSQL** - Production recommended (HA, replication support)
- **MySQL** - Secondary option for production

**Note**: Fraisier is a **deployment tool**, not a database abstraction layer. Your applications can use any database they need.

### Q: Can I use Fraisier without NATS?

**A:** No. NATS with JetStream is required for:

- Event streaming (deployment events, health check results)
- Real-time monitoring and alerting
- Event sourcing and audit trails
- Cross-service communication

You can use a simple NATS deployment (single server) for development and small deployments.

### Q: How does Fraisier compare to Kubernetes?

**A:**

| Aspect | Fraisier | Kubernetes |
|--------|----------|-----------|
| **Setup complexity** | 15-30 min | Days to weeks |
| **Operational overhead** | Low | High |
| **Scaling** | Manual or auto (cloud) | Automatic, dynamic |
| **Multi-provider** | Yes (built-in) | Requires distro choice |
| **Learning curve** | 1-2 hours | 2-4 weeks |
| **Best for** | <100 deployments/day | 1000+ deployments/day |
| **Cost** | Minimal | Significant (requires k8s expertise) |

**Choose Fraisier if**:

- You have < 10 services
- You deploy < 100 times/day
- You need multi-provider support
- You want fast setup (< 30 min)
- You prefer operational simplicity

**Choose Kubernetes if**:

- You have 50+ services
- You deploy 1000+ times/day
- You need automatic scaling
- You have dedicated k8s engineers
- You need complex networking policies

### Q: Does Fraisier support rollbacks?

**A:** Yes. Three rollback mechanisms:

1. **Automatic rollback** (on deployment failure):

   ```yaml
   provider_config:
     auto_rollback_on_failure: true
   ```

2. **Manual rollback** (command):

   ```bash
   fraisier rollback my_service production
   ```

3. **Commit-based deployment** (deploy specific version):

   ```bash
   fraisier deploy my_service production --commit abc123def456
   ```

### Q: Can I deploy multiple services together?

**A:** Yes, using service groups:

```yaml
fraises:
  microservices:
    type: service_group
    services:
      - api
      - worker
      - scheduler

# Deploy all
fraisier deploy microservices production

# Deploy specific service
fraisier deploy microservices production --service api
```

### Q: What happens if a deployment fails?

**A:** Fraisier performs these steps:

1. **Health check fails** → Detect issue
2. **Automatic rollback** → Revert to previous version
3. **Notify** → Send webhook/alert
4. **Log** → Record failure in event stream
5. **Stop** → Prevent further deployments until manual intervention

View failure details:

```bash
fraisier logs dep_00001
fraisier status my_service production
```

### Q: How do I keep secrets secure?

**A:** Best practices:

1. **Never commit secrets** to Git:

   ```bash
   echo ".env" >> .gitignore
   echo ".env.local" >> .gitignore
   ```

2. **Use environment variables**:

   ```bash
   export DATABASE_PASSWORD="secure_password"
   fraisier deploy my_service production
   ```

3. **Use secret management systems**:
   - HashiCorp Vault
   - AWS Secrets Manager
   - Google Secret Manager
   - Azure Key Vault

4. **Store in fraises.yaml**:

   ```yaml
   provider_config:
     env_vars:
       DATABASE_URL: ${DATABASE_URL}  # From environment
       API_KEY: ${API_KEY}            # From environment
   ```

### Q: Can I run Fraisier in the cloud?

**A:** Yes. Fraisier runs on any Linux server with:

- Docker installed
- PostgreSQL access
- NATS access
- SSH access to deployment targets

**Cloud deployment options**:

- **AWS**: EC2 instance with RDS (PostgreSQL) and managed NATS
- **GCP**: Compute Engine with Cloud SQL and NATS
- **Azure**: Virtual Machine with Azure Database for PostgreSQL
- **DigitalOcean**: Droplet with managed databases

---

## FAQ - Deployment

### Q: What's the difference between rolling, blue-green, and canary deployments?

**A:**

**Rolling Deployment**:

```
v1 instance 1  →  v2 instance 1
v1 instance 2  →  v2 instance 2
v1 instance 3  →  v2 instance 3
```

- **Pros**: Simple, no downtime, fast
- **Cons**: Temporary mixed versions, errors affect old users
- **Best for**: Stateless services, backward-compatible changes

**Blue-Green Deployment**:

```
Blue  (v1): instance 1, 2, 3
  ↓
Green (v2): instance 1, 2, 3  (created)
  ↓
Switch traffic (single operation)
  ↓
Red   (v1): kept as rollback
```

- **Pros**: Zero downtime, atomic switch, instant rollback
- **Cons**: 2x resources needed, complex switch logic
- **Best for**: Breaking changes, high-risk updates

**Canary Deployment**:

```
v1: 100% traffic
  ↓
v2: 5% traffic (canaries, watch for issues)
  ↓
v2: 25% traffic (if no errors)
  ↓
v2: 100% traffic (gradual rollout)
```

- **Pros**: Risk-minimized, catches errors early, gradual rollout
- **Cons**: Requires traffic splitting capability, longer deployment
- **Best for**: Large deployments, risky changes, production-critical

### Q: How do I configure automatic deployments on Git push?

**A:** Use webhooks:

1. **GitHub webhook**:

   ```bash
   # Add to repository Settings → Webhooks
   Payload URL: https://fraisier.example.com/webhooks/github
   Content type: application/json
   Events: Push
   Active: ✓
   ```

2. **Configure Fraisier**:

   ```yaml
   provider_config:
     auto_deploy: true
     auto_deploy_branch: main
     webhook_secret: ${WEBHOOK_SECRET}
   ```

3. **Push to trigger**:

   ```bash
   git push origin main  # Automatically triggers deployment
   ```

### Q: How long does a typical deployment take?

**A:** Varies by infrastructure:

| Provider | Time | Breakdown |
|----------|------|-----------|
| **Bare Metal** | 30-60s | Git pull (5s) + Docker build (20s) + SSH deploy (5s) + Health checks (10-15s) |
| **Docker Compose** | 15-30s | Pull images (5s) + Restart services (3s) + Health checks (10-15s) |
| **Coolify** | 45-90s | Git clone (10s) + nixpacks build (30s) + Deploy (5s) + Health checks (10-15s) |
| **Kubernetes** | 60-300s | Image build (30s) + Push to registry (10s) + Rolling update (20-30s) + Health checks (60-240s) |

### Q: Can I deploy to multiple regions simultaneously?

**A:** Yes. Define multiple environments:

```yaml
environments:
  us-east:
    provider: bare_metal
    provider_config:
      ssh_host: api-us-east.example.com

  us-west:
    provider: bare_metal
    provider_config:
      ssh_host: api-us-west.example.com

  eu-west:
    provider: bare_metal
    provider_config:
      ssh_host: api-eu-west.example.com

# Deploy to all regions
fraisier deploy my_service us-east us-west eu-west

# Or sequentially with validation
fraisier deploy my_service us-east
fraisier test my_service us-east
fraisier deploy my_service us-west
fraisier deploy my_service eu-west
```

### Q: How do I handle database migrations during deployment?

**A:** Use post-deployment hooks:

```yaml
provider_config:
  hooks:
    pre_deployment:
      - command: npm run migrate
        on_failure: fail  # Stop if migrations fail

    post_deployment:
      - command: npm run seed  # Optional: seed data
        on_failure: warn
```

**For safety**:

1. Always run migrations **before** service restart
2. Make migrations **idempotent** (safe to run multiple times)
3. Use database transactions for safety
4. Test migrations in staging first

### Q: Can I deploy with zero downtime?

**A:** Yes, for stateless services using blue-green or rolling deployments:

```yaml
# Blue-green (atomic switch)
deployment_strategy: blue_green

# Rolling (gradual replacement)
deployment_strategy: rolling
max_parallel_deployments: 1  # Replace 1 at a time
```

For stateful services (databases, persistent sessions):

- Use connection draining
- Implement graceful shutdown (30s timeout)
- Migrate state before switching

---

## FAQ - Troubleshooting

### Q: Deployment is stuck. What do I do?

**A:** Step-by-step:

1. **Check status**:

   ```bash
   fraisier status my_service production
   ```

2. **View logs**:

   ```bash
   fraisier logs dep_12345 --tail 100
   ```

3. **Check health checks**:

   ```bash
   curl http://my-service.example.com/health
   ```

4. **SSH to server and check service**:

   ```bash
   ssh deploy@production.example.com
   sudo systemctl status my-service
   sudo journalctl -u my-service -n 50 --no-pager
   ```

5. **Force restart** (if safe):

   ```bash
   ssh deploy@production.example.com "sudo systemctl restart my-service"
   ```

6. **Rollback if needed**:

   ```bash
   fraisier rollback my_service production
   ```

See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for detailed scenarios.

### Q: Health checks keep failing. Why?

**A:** Common causes:

1. **Service not ready yet**:

   ```yaml
   health_check:
     start_period: 30  # Give 30s to start
     initial_delay: 5
   ```

2. **Wrong health check URL**:

   ```bash
   # Test manually
   curl -v http://localhost:8000/health
   ```

3. **Port not listening**:

   ```bash
   # Check port
   netstat -tlnp | grep 8000
   docker ps | grep my-service
   ```

4. **Timeout too short**:

   ```yaml
   health_check:
     timeout: 15  # Increase from 5s
   ```

### Q: Docker build is failing. What's the issue?

**A:** Check:

1. **Dockerfile syntax**:

   ```bash
   docker build -f Dockerfile --dry-run .
   ```

2. **Build context**:

   ```bash
   docker build --progress=plain .
   ```

3. **Docker daemon**:

   ```bash
   docker ps  # If this fails, daemon issue
   ```

4. **Disk space**:

   ```bash
   docker system df
   docker system prune -a  # Clean unused images
   ```

5. **Build logs**:

   ```bash
   docker build --progress=plain --no-cache . 2>&1 | tail -50
   ```

### Q: Service crashes after deployment. How do I debug?

**A:** Steps:

1. **Check exit code**:

   ```bash
   docker ps -a | grep my-service
   docker inspect <container_id> | grep ExitCode
   ```

2. **View logs**:

   ```bash
   docker logs <container_id> --tail 100
   ```

3. **Check environment**:

   ```bash
   docker inspect <container_id> | grep -A 20 Env
   ```

4. **Check resource limits**:

   ```bash
   docker stats <container_id>
   ```

5. **Test locally**:

   ```bash
   docker run -it my-service:latest /bin/bash
   # Inside container, test manually
   ```

---

## Advanced Topics

### Custom Providers

**Create a custom provider for your infrastructure**:

**Step 1: Define provider interface**

```python
# custom_provider.py
from abc import ABC, abstractmethod
from typing import Optional, Dict, Any

class CustomProvider(ABC):
    @abstractmethod
    async def deploy(
        self,
        service: str,
        version: str,
        config: Dict[str, Any]
    ) -> bool:
        """Deploy service to target infrastructure."""
        pass

    @abstractmethod
    async def rollback(
        self,
        service: str,
        config: Dict[str, Any]
    ) -> bool:
        """Rollback to previous version."""
        pass

    @abstractmethod
    async def health_check(self, config: Dict[str, Any]) -> bool:
        """Check service health."""
        pass

    @abstractmethod
    async def get_status(self, service: str) -> Dict[str, Any]:
        """Get current service status."""
        pass
```

**Step 2: Implement for your infrastructure**

```python
# custom_provider_impl.py
import httpx
from custom_provider import CustomProvider

class MyCloudProvider(CustomProvider):
    def __init__(self):
        self.api_url = "https://api.mycloud.example.com"

    async def deploy(self, service: str, version: str, config):
        async with httpx.AsyncClient() as client:
            response = await client.post(
                f"{self.api_url}/deployments",
                json={
                    "service": service,
                    "version": version,
                    "config": config
                }
            )
            return response.status_code == 201

    async def health_check(self, config):
        async with httpx.AsyncClient() as client:
            try:
                response = await client.get(
                    config["health_check_url"],
                    timeout=config.get("timeout", 10)
                )
                return response.status_code == 200
            except Exception:
                return False

    # ... implement other methods
```

**Step 3: Register with Fraisier**

```yaml
# fraises.yaml
fraises:
  my_service:
    environments:
      production:
        provider: custom_provider  # Reference your provider
        provider_config:
          api_url: https://api.mycloud.example.com
          health_check_url: https://my-service.example.com/health
```

**Step 4: Install and use**

```bash
# Install as plugin
fraisier plugin install ./custom_provider_impl.py

# Deploy
fraisier deploy my_service production
```

### Event-Driven Deployments

**Listen to NATS events and trigger custom logic**:

```python
import nats
import json

async def deployment_event_handler():
    nc = await nats.connect("nats://localhost:4222")

    async def message_handler(msg):
        event = json.loads(msg.data)

        if event['type'] == 'deployment.started':
            print(f"Deployment started: {event['service']}")
            # Send notification

        elif event['type'] == 'deployment.succeeded':
            print(f"Deployment succeeded: {event['service']}")
            # Run smoke tests

        elif event['type'] == 'deployment.failed':
            print(f"Deployment failed: {event['service']}")
            # Trigger rollback alert

        elif event['type'] == 'health_check.failed':
            print(f"Health check failed: {event['service']}")
            # Trigger incident

    # Subscribe to all deployment events
    await nc.subscribe("fraisier.events.deployment.>", message_handler)

    # Keep alive
    while True:
        await asyncio.sleep(1)
```

### Building Custom Webhooks

**Integrate Fraisier with your tools**:

```python
# Custom Slack webhook
async def slack_webhook(event):
    if event['type'] == 'deployment.succeeded':
        message = f"""
🚀 **Deployment Successful**
Service: {event['service']}
Version: {event['version']}
Duration: {event['duration']}s
"""
    elif event['type'] == 'deployment.failed':
        message = f"""
❌ **Deployment Failed**
Service: {event['service']}
Error: {event['error']}
Check: {event['logs_url']}
"""

    await post_to_slack(message)
```

Configure in fraises.yaml:

```yaml
provider_config:
  webhooks:
    - type: slack
      url: ${SLACK_WEBHOOK_URL}
      events: ["deployment.succeeded", "deployment.failed"]

    - type: custom
      url: https://myapp.example.com/webhooks/fraisier
      events: ["*"]  # All events
```

### Advanced Health Checks

**Custom health check logic**:

```python
# Custom health check script
import subprocess
import time

async def advanced_health_check():
    checks = [
        # HTTP endpoint
        {
            "type": "http",
            "url": "http://localhost:8000/health",
            "timeout": 5,
            "expected_status": 200
        },

        # TCP port
        {
            "type": "tcp",
            "host": "localhost",
            "port": 8000,
            "timeout": 5
        },

        # Custom command
        {
            "type": "command",
            "command": "docker exec my-service npm run health-check",
            "expected_exit_code": 0
        },

        # Database query
        {
            "type": "database",
            "connection_string": "${DATABASE_URL}",
            "query": "SELECT 1",
            "timeout": 10
        }
    ]

    results = []
    for check in checks:
        if check["type"] == "http":
            result = await http_check(check)
        elif check["type"] == "tcp":
            result = await tcp_check(check)
        elif check["type"] == "command":
            result = await command_check(check)
        elif check["type"] == "database":
            result = await database_check(check)

        results.append(result)

    # All checks must pass
    return all(results)
```

---

## Performance Tuning

### Deployment Performance

**Optimize deployment speed**:

```yaml
provider_config:
  # 1. Build cache
  build_cache: true  # Use Docker layer cache

  # 2. Parallel deployments
  max_parallel_deployments: 3  # Deploy to 3 servers simultaneously

  # 3. Pre-built images
  deployment_type: docker_image  # Skip build, use pre-built
  docker_image: my-registry.com/my-service:latest

  # 4. Skip health checks (if not critical)
  health_check:
    enabled: false  # For fast deployments (not recommended)

  # 5. Reduce retry delays
  health_check:
    retry_delay: 1  # 1 second between retries
```

**Typical optimizations**:

```
Before: 120s
  - Git pull: 10s
  - Docker build: 60s        ← Can optimize with pre-built images
  - Push image: 15s          ← Network dependent
  - Deploy: 10s
  - Health checks: 25s       ← Can optimize with readiness probes

After: 45s (with pre-built images)
  - Deploy: 10s
  - Push image: 15s
  - Health checks: 20s
```

### Database Query Performance

**Optimize deployment health checks**:

```python
# Bad: Full database query
SELECT * FROM users WHERE status = 'active';  # Expensive

# Good: Simple connection check
SELECT 1;  # Fast (10ms)

# Good: Health check endpoint
GET /health  # Returns cached status (1ms)
```

### Monitoring Performance

**Reduce monitoring overhead**:

```yaml
# Reduce metric cardinality
monitoring:
  scrape_interval: 60s      # From 15s (if not critical)
  retention: 7d             # From 30d (storage cost)

  # Sample high-cardinality metrics
  sample_metrics:
    - metric_name: http_request_duration_seconds
      sample_rate: 0.1      # Sample 10% of requests
```

---

## Frequently Asked Configuration Questions

### Q: How do I configure resource limits?

**A:** Set limits in provider config:

```yaml
provider_config:
  resources:
    cpu:
      limit: 2           # 2 CPU cores
      request: 1         # Guarantee 1 core

    memory:
      limit: 2Gi         # 2GB max
      request: 1Gi       # Guarantee 1GB

    disk:
      limit: 50Gi        # 50GB storage
```

### Q: How do I handle secrets rotation?

**A:** Use secret management system:

```yaml
provider_config:
  secrets:
    method: vault        # HashiCorp Vault
    vault_addr: https://vault.example.com
    vault_token: ${VAULT_TOKEN}
    secret_path: secret/fraisier/prod

  # Rotation schedule
  secret_rotation:
    enabled: true
    interval: 30d        # Rotate every 30 days
    notification:
      type: slack
      url: ${SLACK_WEBHOOK_URL}
```

### Q: How do I implement gradual traffic shifting?

**A:** Use canary deployment:

```yaml
deployment_strategy: canary

canary_config:
  initial_percentage: 5    # 5% traffic to new version
  increment: 10            # Increase by 10% every step
  interval: 5m             # Every 5 minutes
  error_threshold: 1       # Rollback if error rate > 1%
  latency_threshold: 500   # Rollback if P95 > 500ms
```

### Q: How do I monitor across multiple regions?

**A:** Use centralized monitoring:

```yaml
monitoring:
  type: prometheus

  scrape_targets:
    - region: us-east
      targets: ["api-us-east.example.com:9090"]

    - region: us-west
      targets: ["api-us-west.example.com:9090"]

    - region: eu-west
      targets: ["api-eu-west.example.com:9090"]

  # Global alerting
  alerting:
    enabled: true
    rule_files:
      - global_rules.yml    # Rules for all regions
      - region_rules.yml    # Region-specific rules
```

---

## Best Practices Summary

### Before Going to Production

- [ ] Set up automated backups
- [ ] Configure monitoring and alerting
- [ ] Test rollback procedure
- [ ] Document runbooks for common issues
- [ ] Set up incident response process
- [ ] Perform load testing
- [ ] Security audit completed
- [ ] Disaster recovery plan documented

### During Deployments

- [ ] Check health before deployment
- [ ] Review changes being deployed
- [ ] Have rollback plan ready
- [ ] Monitor error rate for 10 minutes post-deploy
- [ ] Check application logs
- [ ] Verify data integrity

### After Deployments

- [ ] Confirm all health checks passing
- [ ] Review deployment metrics
- [ ] Check error rate is normal
- [ ] Update runbooks if needed
- [ ] Archive logs for audit trail
- [ ] Post deployment review if issues occurred

---

## Glossary

**APQ (Automatic Persistent Query)**: Query optimization technique that caches parsed query results.

**Blue-Green Deployment**: Running two identical production environments (blue and green). Deploy to inactive environment, switch traffic atomically.

**Canary Deployment**: Gradually rolling out new version to small percentage of users, monitoring for issues before full rollout.

**Health Check**: Endpoint or script that verifies service is running and healthy.

**JetStream**: NATS message persistence and stream management system.

**Rollback**: Reverting to previous working version after failed deployment.

**Webhook**: HTTP callback triggered by deployment events.

---

## Additional Resources

### Documentation

- [Getting Started Guide](GETTING_STARTED_DOCKER.md)
- [API Reference](API_REFERENCE.md)
- [Troubleshooting Guide](TROUBLESHOOTING.md)
- [Real-World Examples](REAL_WORLD_EXAMPLES.md)

### External Resources

- [NATS Documentation](https://docs.nats.io)
- [Prometheus Documentation](https://prometheus.io/docs)
- [Docker Documentation](https://docs.docker.com)
- [PostgreSQL Documentation](https://www.postgresql.org/docs)

### Community

- GitHub Issues: https://github.com/fraiseql/fraisier/issues
- Discussions: https://github.com/fraiseql/fraisier/discussions
- Email: support@fraiseql.io

---

**Last Updated**: 2024-01-22
**Version**: 0.1.0
