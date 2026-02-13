# Fraisier Troubleshooting Guide

**Version**: 0.1.0
**Format**: Issue → Symptoms → Causes → Solutions → Debug Commands

---

## Table of Contents

1. [Connection Issues](#connection-issues)
2. [Deployment Issues](#deployment-issues)
3. [Health Check Issues](#health-check-issues)
4. [Database Issues](#database-issues)
5. [Docker Issues](#docker-issues)
6. [SSH Issues](#ssh-issues)
7. [NATS Issues](#nats-issues)
8. [Monitoring Issues](#monitoring-issues)
9. [Performance Issues](#performance-issues)
10. [Data Issues](#data-issues)

---

## Connection Issues

### Issue: Cannot connect to Fraisier API

**Symptoms**:

- `Connection refused` error
- API endpoint unreachable
- Cannot list deployments or services

**Causes**:

1. Fraisier service not running
2. Wrong hostname/port
3. Firewall blocking connection
4. TLS/SSL certificate issues

**Solutions**:

1. **Check if Fraisier is running**:

```bash
# Local
curl http://localhost:8000/health

# Remote
curl https://fraisier.example.com/health

# Via Docker
docker-compose ps fraisier
docker logs fraisier
```

2. **Verify hostname/port**:

```bash
# Test connectivity
nc -zv localhost 8000
nc -zv fraisier.example.com 8000

# Check DNS
nslookup fraisier.example.com
```

3. **Check firewall**:

```bash
# On server
sudo ufw status
sudo ufw allow 8000

# Test from client
telnet localhost 8000
```

4. **Verify SSL certificate**:

```bash
# Check certificate
openssl s_client -connect fraisier.example.com:8443

# Verify certificate validity
curl -vI https://fraisier.example.com
```

**Debug Commands**:

```bash
fraisier --version
fraisier config show
export FRAISIER_LOG_LEVEL=DEBUG
fraisier list
```

---

### Issue: Authentication token invalid or expired

**Symptoms**:

- `401 Unauthorized` error
- Token rejected
- Need to re-authenticate

**Causes**:

1. Token expired
2. Wrong token format
3. Token revoked
4. Credentials incorrect

**Solutions**:

1. **Get new token**:

```bash
fraisier auth login
fraisier auth token
```

2. **Verify token format**:

```bash
# Should be Bearer token
curl -H "Authorization: Bearer $TOKEN" http://localhost:8000/health

# Check token environment variable
echo $FRAISIER_TOKEN
```

3. **Check token expiration**:

```bash
# Decode JWT token (requires jq)
echo $FRAISIER_TOKEN | cut -d'.' -f2 | base64 -d | jq .exp
```

4. **Re-login**:

```bash
fraisier auth logout
fraisier auth login
```

---

## Deployment Issues

### Issue: Deployment stuck in "pending" state

**Symptoms**:

- Deployment status shows "pending" for hours
- No progress in logs
- Cannot cancel deployment

**Causes**:

1. Provider temporarily unavailable
2. Network connectivity issues
3. Resource constraints
4. Webhook not responding

**Solutions**:

1. **Check deployment status**:

```bash
fraisier status my_api production
fraisier logs dep_00001

# Get detailed deployment info
curl http://localhost:8000/api/v1/deployments/dep_00001
```

2. **Check provider health**:

```bash
# SSH access (for bare metal)
ssh deploy@prod-1.example.com "systemctl status my-api"

# Docker (for Docker Compose)
docker-compose ps

# Coolify API
curl https://coolify.example.com/api/system/status
```

3. **Cancel and retry**:

```bash
fraisier cancel dep_00001
# Wait 30 seconds
fraisier deploy my_api production --wait
```

4. **Check resource usage**:

```bash
# Memory, CPU, disk
fraisier metrics

# Docker stats
docker stats

# Server resources
ssh deploy@prod-1.example.com "free -h && df -h"
```

**Debug Commands**:

```bash
fraisier history my_api production --status pending
fraisier logs dep_00001 --level error
curl http://localhost:9090/api/v1/query?query=fraisier_active_deployments
```

---

### Issue: Deployment fails with "Health check failed"

**Symptoms**:

- Deployment progresses but stops at health checks
- Error: `Health check failed: connection refused`
- Application started but not responding

**Causes**:

1. Application not starting properly
2. Port configuration mismatch
3. Health check endpoint not implemented
4. Network issues between checker and service

**Solutions**:

1. **Verify application is running**:

```bash
# Bare metal
ssh deploy@prod-1.example.com "ps aux | grep myapp"

# Docker
docker-compose ps api
docker logs api
```

2. **Test health check manually**:

```bash
# From client
curl -v http://localhost:8000/health

# From server
ssh deploy@prod-1.example.com "curl -v http://localhost:8000/health"

# From container
docker-compose exec api curl http://localhost:8000/health
```

3. **Check port binding**:

```bash
# Verify port is listening
netstat -tlnp | grep 8000
# or
ss -tlnp | grep 8000

# On server
ssh deploy@prod-1.example.com "netstat -tlnp | grep 8000"
```

4. **Review application logs**:

```bash
# Bare metal
ssh deploy@prod-1.example.com "sudo journalctl -u my-api -f"

# Docker
docker-compose logs -f api

# Coolify
# Via UI or API
```

5. **Increase health check timeout**:

```yaml
# In fraises.yaml
health_check:
  timeout: 30  # Increase from default 10
  max_retries: 5
  retry_delay: 5
```

**Debug Commands**:

```bash
fraisier logs dep_00001 --level error
fraisier logs dep_00001 --component health_check
curl -v http://localhost:8000/health
```

---

### Issue: Deployment succeeds but application not working

**Symptoms**:

- Deployment shows success
- Health checks pass
- But application has errors
- Users report issues

**Causes**:

1. Health check too simplistic
2. Database not initialized
3. Migrations not run
4. Environment variables not set

**Solutions**:

1. **Check application logs**:

```bash
# View real-time logs
fraisier logs dep_00001 --follow

# View specific errors
fraisier logs dep_00001 --level error

# Check component logs
fraisier logs dep_00001 --component deployment
```

2. **Verify environment variables**:

```bash
# Check on server
ssh deploy@prod-1.example.com "env | grep APP"

# In Docker
docker-compose exec api env | grep APP

# Verify they match config
fraisier config show
```

3. **Run database migrations**:

```bash
# Manual migration
ssh deploy@prod-1.example.com "cd /opt/my-api && python migrate.py"

# Or from fraisier
fraisier coolify:exec --command "python migrate.py"
```

4. **Check service dependencies**:

```bash
# Test database connection
curl http://db.example.com:5432

# Test external API
curl https://external-api.example.com/health

# Check DNS
nslookup db.example.com
```

---

## Health Check Issues

### Issue: Health checks flaky/intermittent failures

**Symptoms**:

- Health checks sometimes pass, sometimes fail
- Random failures without code changes
- Deployment succeeds eventually after retries

**Causes**:

1. Application slow to start
2. Database not ready
3. External service timeout
4. Network latency

**Solutions**:

1. **Increase retry configuration**:

```yaml
health_check:
  max_retries: 5  # More retries
  retry_delay: 10  # Longer wait between retries
  timeout: 30  # Longer timeout
```

2. **Add startup delay**:

```yaml
health_check:
  startup_delay: 30  # Wait 30 seconds before first check
```

3. **Improve health check endpoint**:

```python
# Better health check
@app.route('/health')
def health():
    try:
        # Test database
        db.connection.ping()
        # Test cache
        cache.get('health_check')
        return {'status': 'healthy'}, 200
    except Exception as e:
        return {'status': 'unhealthy', 'error': str(e)}, 503
```

4. **Debug individual checks**:

```bash
# Run health check 10 times and see results
for i in {1..10}; do
  curl -w "\n" http://localhost:8000/health
  sleep 2
done
```

**Debug Commands**:

```bash
fraisier logs dep_00001 --component health_check --level debug
curl -v http://localhost:8000/health
```

---

## Database Issues

### Issue: Cannot connect to database

**Symptoms**:

- Deployment fails with `database connection failed`
- Application logs show connection refused
- Cannot query database

**Causes**:

1. Database server not running
2. Wrong connection string
3. Firewall blocking access
4. Authentication failed

**Solutions**:

1. **Check database is running**:

```bash
# PostgreSQL
psql postgresql://user:password@localhost/fraisier

# MySQL
mysql -u user -p -h localhost fraisier

# SQLite
sqlite3 fraisier.db
```

2. **Verify connection string**:

```bash
# Check environment variable
echo $DATABASE_URL

# Test connection with correct string
psql postgresql://fraisier:password@db.example.com:5432/fraisier
```

3. **Check network access**:

```bash
# Test connectivity
nc -zv db.example.com 5432
telnet db.example.com 5432

# From server
ssh deploy@prod-1.example.com "nc -zv db.example.com 5432"
```

4. **Check credentials**:

```bash
# Verify username/password
psql -U fraisier -d fraisier -c "SELECT 1;"

# Check user permissions
psql -U postgres -c "SELECT * FROM pg_user WHERE usename='fraisier';"
```

5. **Enable connection logging**:

```yaml
database:
  type: postgresql
  url: postgresql://user:password@localhost/fraisier
  pool_pre_ping: true
  echo: true  # Log all queries
```

**Debug Commands**:

```bash
fraisier db status
fraisier logs dep_00001 --level error | grep -i database
psql postgresql://user:password@localhost/fraisier -c "SELECT 1;"
```

---

### Issue: Database queries slow

**Symptoms**:

- Deployments take longer than expected
- High database CPU usage
- Logs show slow query warnings

**Causes**:

1. Missing indexes
2. Inefficient queries
3. Too many connections
4. Large data volumes

**Solutions**:

1. **Check slow queries**:

```bash
# PostgreSQL
psql -U fraisier -d fraisier -c "
SELECT query, calls, mean_time
FROM pg_stat_statements
ORDER BY mean_time DESC
LIMIT 10;
"
```

2. **Analyze table**:

```bash
psql -U fraisier -d fraisier -c "ANALYZE tb_deployment;"
```

3. **Create missing indexes**:

```bash
psql -U fraisier -d fraisier << 'EOF'
CREATE INDEX idx_deployment_created ON tb_deployment(created_at);
CREATE INDEX idx_deployment_status ON tb_deployment(status);
EOF
```

4. **Check connection pool**:

```bash
# View active connections
fraisier db status | grep connections

# Kill long-running queries
psql -U fraisier -d fraisier -c "
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE duration > interval '5 minutes';
"
```

---

## Docker Issues

### Issue: Docker image build fails

**Symptoms**:

- Deployment fails during build phase
- Error in Dockerfile
- Build times out

**Causes**:

1. Dockerfile syntax error
2. Missing dependencies
3. Network issues during build
4. Build timeout

**Solutions**:

1. **Check Dockerfile syntax**:

```bash
docker build --no-cache .
# Check error output
```

2. **Test build locally**:

```bash
# Build with verbose output
docker build -v .

# Build specific stage
docker build --target builder .
```

3. **Check dependencies**:

```bash
# Verify all packages exist
docker pull python:3.11
docker pull node:18

# Test image manually
docker run -it python:3.11 pip install requests
```

4. **Increase build timeout**:

```yaml
provider_config:
  build_timeout: 1800  # 30 minutes
```

5. **Use build cache**:

```yaml
provider_config:
  build_cache: true
  build_cache_from: registry.example.com/my-api:latest
```

**Debug Commands**:

```bash
docker build --no-cache -t my-api .
docker logs $(docker ps -lq)
```

---

### Issue: Container exits immediately

**Symptoms**:

- Container starts then stops
- Status shows `Exited (1) 2 minutes ago`
- No application logs visible

**Causes**:

1. Application crashes on startup
2. Missing required file
3. Permission denied
4. Environment variable not set

**Solutions**:

1. **Check container logs**:

```bash
docker logs container_name
docker logs -f container_name  # Follow logs
docker logs --tail 100 container_name  # Last 100 lines
```

2. **Run container interactively**:

```bash
# Start with shell instead
docker run -it my-api /bin/bash

# Run startup command manually
docker run -it my-api python -m myapp.server
```

3. **Check permissions**:

```bash
# Inside container
docker run -it my-api ls -la /app/

# Fix permissions in Dockerfile
RUN chmod +x /app/start.sh
```

4. **Verify environment variables**:

```bash
docker run -e DATABASE_URL=postgresql://... -it my-api
```

**Debug Commands**:

```bash
docker logs my-api
docker inspect my-api | grep -A 5 State
docker run -it my-api env
```

---

## SSH Issues

### Issue: SSH key authentication fails

**Symptoms**:

- Permission denied (publickey)
- Cannot SSH to server
- Key not accepted

**Causes**:

1. Wrong SSH key
2. Key not added to authorized_keys
3. Key permissions incorrect
4. SSH config wrong

**Solutions**:

1. **Verify SSH key exists**:

```bash
ls -la ~/.ssh/fraisier
# Should be: -rw------- (600)

# Check public key
ls -la ~/.ssh/fraisier.pub
# Should be: -rw-r--r-- (644)
```

2. **Add key to server**:

```bash
ssh-copy-id -i ~/.ssh/fraisier deploy@prod-1.example.com

# Or manually
cat ~/.ssh/fraisier.pub | ssh deploy@prod-1.example.com \
  "mkdir -p ~/.ssh && cat >> ~/.ssh/authorized_keys"
```

3. **Fix key permissions**:

```bash
# On client
chmod 600 ~/.ssh/fraisier
chmod 644 ~/.ssh/fraisier.pub

# On server
ssh deploy@prod-1.example.com "chmod 700 ~/.ssh && chmod 600 ~/.ssh/authorized_keys"
```

4. **Test SSH**:

```bash
ssh -i ~/.ssh/fraisier -v deploy@prod-1.example.com "echo SSH working"

# Debug output
ssh -vvv -i ~/.ssh/fraisier deploy@prod-1.example.com
```

**Debug Commands**:

```bash
ssh-keygen -l -f ~/.ssh/fraisier
ssh-keyscan prod-1.example.com
ssh -v -i ~/.ssh/fraisier deploy@prod-1.example.com
```

---

### Issue: SSH connection timeout

**Symptoms**:

- `Connection timed out` after long wait
- Cannot reach server via SSH
- Network seems offline

**Causes**:

1. Server offline or unreachable
2. Firewall blocking port 22
3. SSH service not running
4. Network routing issue

**Solutions**:

1. **Test network connectivity**:

```bash
ping prod-1.example.com
traceroute prod-1.example.com
```

2. **Verify SSH port open**:

```bash
nc -zv prod-1.example.com 22
telnet prod-1.example.com 22
```

3. **Check firewall on server**:

```bash
ssh deploy@prod-1.example.com "sudo ufw status | grep 22"

# Allow SSH
ssh deploy@prod-1.example.com "sudo ufw allow 22"
```

4. **Verify SSH service running**:

```bash
ssh deploy@prod-1.example.com "sudo systemctl status ssh"

# Restart if needed
ssh deploy@prod-1.example.com "sudo systemctl restart ssh"
```

5. **Increase timeout**:

```yaml
provider_config:
  ssh_timeout: 30  # seconds
  ssh_retries: 3
```

**Debug Commands**:

```bash
ping -c 1 prod-1.example.com
nc -zv prod-1.example.com 22
ssh -v prod-1.example.com
```

---

## NATS Issues

### Issue: NATS connection failed

**Symptoms**:

- Error: `Failed to connect to NATS`
- Events not being published
- No connection to event bus

**Causes**:

1. NATS server not running
2. Wrong NATS URL
3. Firewall blocking port
4. Authentication failed

**Solutions**:

1. **Check NATS is running**:

```bash
docker-compose ps nats
curl http://localhost:8222/varz

# Or check directly
nc -zv localhost 4222
```

2. **Verify NATS URL**:

```bash
echo $NATS_SERVERS
# Should be: nats://localhost:4222 or similar

# Test connection
nats rtt
```

3. **Test connectivity**:

```bash
nc -zv localhost 4222
telnet localhost 4222
```

4. **Check credentials**:

```bash
nats -s nats://user:password@localhost:4222 server info
```

**Debug Commands**:

```bash
nats server info
nats stream list
nats sub "fraisier.>"
echo $NATS_SERVERS
```

---

### Issue: Events not being received

**Symptoms**:

- Deployment events published but not received
- No subscribers receiving messages
- Event log shows published but not delivered

**Causes**:

1. Subscriber not connected
2. Filter too restrictive
3. Event type mismatch
4. Consumer not tracking

**Solutions**:

1. **Verify subscriber is listening**:

```bash
# Check subscriptions
nats consumer list DEPLOYMENT_EVENTS

# Subscribe and test
nats sub "fraisier.deployment.>"
```

2. **Test event publish**:

```bash
nats pub "fraisier.deployment.started.default" '{"test":"data"}'

# Check if received
nats sub "fraisier.deployment.started.>" --max-msgs=1
```

3. **Check consumer lag**:

```bash
nats consumer info DEPLOYMENT_EVENTS CONSUMER_NAME

# View pending messages
nats stream view DEPLOYMENT_EVENTS --last 10
```

4. **Verify filter settings**:

```python
# Check filter in code
registry = EventSubscriberRegistry()
# Ensure filter matches event type exactly
registry.register(handler, EventFilter(event_type="deployment.started"))
```

**Debug Commands**:

```bash
nats stream list
nats consumer list DEPLOYMENT_EVENTS
nats sub "fraisier.>" --verbose
```

---

## Monitoring Issues

### Issue: Metrics not appearing in Prometheus

**Symptoms**:

- Grafana dashboards empty
- No metrics collected
- Prometheus scrape jobs failing

**Causes**:

1. Fraisier metrics endpoint not responding
2. Prometheus scrape config wrong
3. Firewall blocking metrics port
4. Application not exposing metrics

**Solutions**:

1. **Check metrics endpoint**:

```bash
curl http://localhost:9090/metrics
curl http://localhost:8000/metrics  # Fraisier metrics
```

2. **Verify Prometheus scrape config**:

```yaml
# In prometheus.yml
scrape_configs:
  - job_name: 'fraisier'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
```

3. **Check Prometheus targets**:

```bash
# Go to Prometheus UI
open http://localhost:9090/targets

# Check via API
curl http://localhost:9090/api/v1/targets
```

4. **Restart Prometheus**:

```bash
docker-compose restart prometheus

# Check logs
docker logs prometheus
```

**Debug Commands**:

```bash
curl http://localhost:8000/metrics | grep fraisier_deployments
curl http://localhost:9090/api/v1/query?query=up
```

---

## Performance Issues

### Issue: Deployments very slow

**Symptoms**:

- Deployments take 10+ minutes
- No obvious errors
- System resources not maxed out

**Causes**:

1. Slow network
2. Large code repository
3. Database queries slow
4. Health checks timing out

**Solutions**:

1. **Profile deployment**:

```bash
fraisier deploy my_api production --verbose
# Track each step's time
```

2. **Check network speed**:

```bash
# Test download speed
curl -O https://github.com/your-org/my-api/archive/main.zip
# Time how long it takes
```

3. **Optimize Git clone**:

```yaml
provider_config:
  git_depth: 1  # Shallow clone
  git_branch: main
```

4. **Pre-warm cache**:

```bash
# Warm container image cache
docker pull my-api:latest

# Pre-load database
fraisier db init  # Parallel initialization
```

5. **Parallel health checks**:

```yaml
health_check:
  parallel: true  # Check all instances in parallel
```

---

## Data Issues

### Issue: Old deployments/logs taking up too much space

**Symptoms**:

- Disk usage growing
- Database large
- Deployment logs accumulating

**Causes**:

1. Long retention period
2. Too many deployments recorded
3. Large log files
4. No cleanup policy

**Solutions**:

1. **Check database size**:

```bash
fraisier db status  # See database size

# PostgreSQL
psql -U fraisier -d fraisier -c "SELECT pg_database_size('fraisier') / 1024 / 1024;"
```

2. **Reduce retention**:

```yaml
database:
  retention_days: 30  # Keep only 30 days
  max_deployments_kept: 10000
```

3. **Archive old logs**:

```bash
# Export old deployments
fraisier history my_api production --since 1y > old_deployments.json

# Delete from database
fraisier db purge --before 2024-01-01
```

4. **Enable log rotation**:

```yaml
logging:
  rotation:
    max_size: 100M
    max_backups: 5
    max_age_days: 30
```

**Debug Commands**:

```bash
fraisier db status
du -sh fraisier.db
sqlite3 fraisier.db "SELECT COUNT(*) FROM tb_deployment;"
```

---

## Getting Help

If you can't find your issue here:

1. **Check logs**:

   ```bash
   fraisier logs dep_00001 --follow
   docker logs fraisier
   ```

2. **Enable debug mode**:

   ```bash
   FRAISIER_LOG_LEVEL=DEBUG fraisier deploy my_api production
   ```

3. **Run diagnostics**:

   ```bash
   fraisier health
   fraisier config show
   fraisier db status
   ```

4. **Check documentation**:
   - [CLI_REFERENCE.md](CLI_REFERENCE.md)
   - [API_REFERENCE.md](API_REFERENCE.md)
   - [PROVIDER_*.md](PROVIDER_BARE_METAL.md)

5. **Report issue**:
   - GitHub: https://github.com/your-org/fraisier/issues
   - Discord: https://discord.gg/your-invite
   - Email: support@fraisier.dev

---

**Remember**: Most issues are solvable with proper logging and debugging. Enable debug mode and check logs first!
